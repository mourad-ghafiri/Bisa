//! Operator surface: harness catalog, governance, search, session
//! transcripts, engine pause/resume. People and invitations are `collab`'s.

use crate::dto::*;
use crate::route_docs::RouteDoc;
use crate::Query;
use crate::{not_found, ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/harnesses", get(harnesses))
        .route("/harnesses/{id}/usage", get(harness_usage))
        .route("/governance", get(get_governance).put(put_governance))
        .route("/search", get(search))
        .route("/sessions/{id}/transcript", get(transcript))
        .route("/pause", get(paused).post(pause))
        .route("/resume", post(resume))
        .route("/cache/stats", get(cache_stats))
        .route("/cache/clear", post(cache_clear))
}

// --- cache -------------------------------------------------------

/// Every cache's hit/miss counters and live entry count. A read — it changes
/// nothing — that the desktop's Cache panel renders as a table.
async fn cache_stats() -> Json<Vec<CacheStatsDto>> {
    Json(
        bisa_cache::all_stats()
            .into_iter()
            .map(CacheStatsDto::from)
            .collect(),
    )
}

/// Empty every cache. The counters stand; only the held entries are dropped, so
/// the next read of each recomputes. For the panel's "Clear caches" button.
async fn cache_clear() -> Json<serde_json::Value> {
    bisa_cache::clear_all();
    Json(json!({"cleared": true}))
}

/// The harness catalog, probed.
///
/// `launch` is the one field here that is about *this machine* rather than
/// about the workspace: it names the command that would run this harness
/// interactively, and where that binary resolved on `PATH`. It is a **read**,
/// like every other route in this file — the node has never spawned an
/// interactive session and does not gain the ability to here. The desktop shell
/// is the only thing that runs one, behind Tauri's IPC, for the reason
/// `desktop/src-tauri/src/terminal.rs` sets out at length: a PTY behind an
/// unauthenticated control plane is a remote shell.
///
/// `launch` is `null` for every harness with no interactive form — every
/// `acp:*` adapter, whose whole job is to put a binary into protocol mode, and
/// `a2a:*`, which has no local process at all.
async fn harnesses(State(state): State<Shared>) -> Json<serde_json::Value> {
    let listings = state
        .engine
        .inner()
        .catalog
        .list_cached(
            state.engine.cache_settings().harness_listing_ttl(),
            std::time::Duration::from_secs(3),
        )
        .await;
    let rows: Vec<_> = listings
        .into_iter()
        .map(|l| {
            let tier = match l.tier {
                bisa_harness::catalog::HarnessTier::Builtin => "builtin",
                bisa_harness::catalog::HarnessTier::Preset => "preset",
                bisa_harness::catalog::HarnessTier::Custom => "custom",
            };
            json!({
                "id": l.id,
                "label": l.label,
                "tier": tier,
                "installed": l.probe.available,
                "version": l.probe.version,
                "path": l.path,
                "launch": l.launch,
                "detail": l.probe.reason,
                "install_hint": l.install_hint,
                "install": l.install,
                "tool_guard": l.tool_guard,
                "input_rewrite": l.input_rewrite,
            })
        })
        .collect();
    Json(json!({"harnesses": rows}))
}

#[derive(Deserialize)]
struct UsageQuery {
    /// Ask the harness's source again instead of the held answer.
    #[serde(default)]
    refresh: bool,
}

/// What a harness's **account** has left — its usage limits, read from the
/// harness's own source: the provider's usage endpoint with Claude Code's
/// own sign-in, `codex app-server`, `omp usage --json`. A **read**, on
/// demand, cached by the engine (`cache.harness_usage.ttl_ms`); `?refresh=true`
/// asks the source again. A harness that reports no limits says so in its
/// own words; `harness.usage.reads` off answers `off` for every harness.
///
/// The credential the reader used is the harness's own, held for the one
/// request inside the engine and never part of this answer — a report is
/// percentages, labels and reset times.
async fn harness_usage(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    Query(q): Query<UsageQuery>,
) -> Json<HarnessUsage> {
    let usage = state.engine.harness_usage(&id, q.refresh).await;
    Json(HarnessUsage { harness: id, usage })
}

// --- governance -------------------------------------------------------------

async fn get_governance(State(state): State<Shared>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        json!({"governance": state.engine.workspace().governance()?}),
    ))
}

async fn put_governance(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<GovernanceBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let governance = bisa_engine::admin::set_governance(
        state.engine.inner(),
        bisa_engine::admin::GovernancePatch {
            approval: body.approval,
            escalation: body.escalation,
            publish: body.publish,
        },
    )?;
    Ok(Json(json!({"governance": governance})))
}

// --- search -----------------------------------------------------------------

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
}

async fn search(
    State(state): State<Shared>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    let mut rows = Vec::new();
    for id in ws.search(&q.q)? {
        if let Ok(goal) = ws.get_goal(id) {
            let status = crate::goals::status_of(ws, &goal);
            rows.push(json!({
                "id": id.to_string(),
                "title": goal.title,
                "statement": goal.statement,
                "status": status,
            }));
        }
    }
    Ok(Json(json!({"results": rows})))
}

// --- transcripts ------------------------------------------------------------

#[derive(Deserialize)]
struct TailQuery {
    #[serde(default)]
    from_byte: Option<u64>,
}

/// Tail a session transcript by byte offset (`{text, next_byte, reset}` —
/// `reset` when the file shrank under the cursor). The harness wrote the
/// file in its own words; the node redacts what it serves of it.
async fn transcript(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    Query(q): Query<TailQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    use std::io::{Read, Seek, SeekFrom};
    let row = state
        .engine
        .workspace()
        .session_by_id(&id)?
        .ok_or_else(|| not_found(bisa_core::text!("error-node-admin-unknown-session")))?;
    let path = row.transcript_path.ok_or_else(|| {
        not_found(bisa_core::text!(
            "error-node-admin-session-has-no-transcript"
        ))
    })?;
    let mut f = std::fs::File::open(&path)
        .map_err(|_| not_found(bisa_core::text!("error-node-admin-transcript-file-missing")))?;
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let from = q.from_byte.unwrap_or(0);
    let (start, reset) = if from > len { (0, true) } else { (from, false) };
    f.seek(SeekFrom::Start(start))
        .map_err(|e| ApiError::internal(&e))?;
    // Cap a single read at 1 MiB — of bytes, then cut on a character: a page
    // that ends inside a multibyte character keeps that character for the
    // next page, and a page that *starts* inside one (a `from_byte` the
    // client made up, a torn write) reads it lossily and moves on. Either
    // way the cursor advances, so a reader never re-asks the same offset for
    // ever.
    let mut bytes = Vec::new();
    f.take(1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|e| ApiError::internal(&e))?;
    let (text, consumed) = match std::str::from_utf8(&bytes) {
        Ok(text) => (text.to_string(), bytes.len()),
        Err(e) if e.error_len().is_none() && e.valid_up_to() > 0 => {
            let cut = e.valid_up_to();
            (String::from_utf8_lossy(&bytes[..cut]).into_owned(), cut)
        }
        Err(_) => (String::from_utf8_lossy(&bytes).into_owned(), bytes.len()),
    };
    let next_byte = start + consumed as u64;
    Ok(Json(json!({
        "text": state.engine.redact(&text),
        "next_byte": next_byte,
        "reset": reset,
    })))
}

// --- pause / resume ---------------------------------------------------------

/// Whether the engine is paused — the read half of `POST /pause`, so a
/// screen that opens mid-pause does not have to guess from silence.
async fn paused(State(state): State<Shared>) -> Json<serde_json::Value> {
    Json(json!({"paused": state.engine.is_paused()}))
}

async fn pause(State(state): State<Shared>) -> Json<serde_json::Value> {
    state.engine.pause();
    Json(json!({"paused": true}))
}

async fn resume(State(state): State<Shared>) -> Json<serde_json::Value> {
    state.engine.resume();
    Json(json!({"paused": false}))
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc {
        method: "GET",
        path: "/harnesses",
        summary: "The harness catalog, each entry probed for whether it is installed.",
    },
    RouteDoc {
        method: "GET",
        path: "/harnesses/{id}/usage",
        summary: "What the harness's account has left — its usage windows from its own source, cached; `?refresh=true` asks again. Never a credential.",
    },
    RouteDoc {
        method: "GET",
        path: "/governance",
        summary: "Who may sign each gate: `approval`, `escalation`, `publish`.",
    },
    RouteDoc {
        method: "PUT",
        path: "/governance",
        summary: "Replace the governance policy. Only the owner may widen a gate.",
    },
    RouteDoc {
        method: "GET",
        path: "/search",
        summary: "Full-text search over goals: `?q=` → `{hits}`.",
    },
    RouteDoc {
        method: "GET",
        path: "/sessions/{id}/transcript",
        summary: "Tail a session transcript by byte offset: `?from=` → `{text, next_byte, reset}`.",
    },
    RouteDoc {
        method: "GET",
        path: "/pause",
        summary: "Whether the engine is paused — the read half of `POST /pause`.",
    },
    RouteDoc {
        method: "POST",
        path: "/pause",
        summary: "Pause scheduling; running sessions finish, nothing new starts.",
    },
    RouteDoc {
        method: "POST",
        path: "/resume",
        summary: "Resume scheduling.",
    },
    RouteDoc {
        method: "GET",
        path: "/cache/stats",
        summary: "Every cache's hit/miss counters and live entry count.",
    },
    RouteDoc {
        method: "POST",
        path: "/cache/clear",
        summary: "Empty every cache; the counters stand and the next read recomputes.",
    },
];
