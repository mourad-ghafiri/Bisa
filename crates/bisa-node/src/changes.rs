//! What an agent changed in a checkout during a conversation (ide/20): the
//! routes a review is read and settled through, and the asks a turn waits on
//! in the conversation itself.
//!
//! Everything is addressed by the conversation. The node parses and
//! renders; the rule — attribution, the three-way merge that keeps somebody
//! else's edit, the compare-and-swap of a hunk — is
//! [`bisa_engine::changes`]'s. No MCP tool mirrors these: an agent never
//! settles its own change.

use crate::dto::{RestoreChangesBody, SettleChangesBody};
use crate::route_docs::RouteDoc;
use crate::Query;
use crate::{bad_request, ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use bisa_core::Localize as _;
use bisa_core::{ConversationId, RelPath};
use bisa_engine::changes::asks::{self, AskAnswer};
use bisa_engine::changes::{self as eng, settle};
use bisa_engine::ide::files::{self, Disposal};
use serde::Deserialize;
use serde_json::json;
use std::str::FromStr;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/conversations/{id}/changes", get(changes))
        .route("/conversations/{id}/changes/file", get(file))
        .route("/conversations/{id}/changes/settle", post(settle_changes))
        .route("/conversations/{id}/changes/restore", post(restore))
        .route("/conversations/{id}/asks", get(open_asks))
        .route("/conversations/{id}/asks/{ask}", post(answer_ask))
}

fn parse_id(s: &str) -> Result<ConversationId, ApiError> {
    ConversationId::from_str(s).map_err(|_| {
        bad_request(bisa_core::text!(
            "error-node-changes-not-conversation-id",
            s = format!("{s:?}")
        ))
    })
}

/// `editor.delete.trash`, resolved where the conversation stands: how a file
/// the agent made goes when its making is undone.
fn disposal_for(state: &Shared, id: ConversationId) -> Disposal {
    let inner = state.engine.inner();
    let project = bisa_engine::conversations::record(inner, id)
        .ok()
        .and_then(|c| c.origin.project());
    Disposal::from_setting(files::setting_bool(
        inner,
        project,
        "editor.delete.trash",
        true,
    ))
}

async fn changes(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<eng::ChangesView>, ApiError> {
    Ok(Json(eng::view(state.engine.inner(), parse_id(&id)?)?))
}

#[derive(Deserialize)]
struct FileQuery {
    path: String,
}

async fn file(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    Query(q): Query<FileQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let path = RelPath::new(q.path).map_err(|e| bad_request(e.text()))?;
    let review = eng::file_view(state.engine.inner(), parse_id(&id)?, &path)?;
    Ok(Json(json!({ "file": review })))
}

async fn settle_changes(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::Body(body): crate::Body<SettleChangesBody>,
) -> Result<Json<settle::Settled>, ApiError> {
    let id = parse_id(&id)?;
    let disposal = disposal_for(&state, id);
    Ok(Json(settle::settle(
        state.engine.inner(),
        id,
        body.verdict,
        body.target,
        disposal,
        body.force,
    )?))
}

async fn restore(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::Body(body): crate::Body<RestoreChangesBody>,
) -> Result<Json<settle::Settled>, ApiError> {
    let id = parse_id(&id)?;
    let disposal = disposal_for(&state, id);
    Ok(Json(settle::restore(
        state.engine.inner(),
        id,
        body.turn,
        disposal,
    )?))
}

async fn open_asks(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    bisa_engine::conversations::record(state.engine.inner(), id)?;
    Ok(Json(
        json!({ "asks": asks::open(state.engine.inner(), id) }),
    ))
}

async fn answer_ask(
    State(state): State<Shared>,
    AxPath((id, ask)): AxPath<(String, String)>,
    crate::Body(answer): crate::Body<AskAnswer>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    asks::answer(state.engine.inner(), id, &ask, answer)?;
    Ok(Json(
        json!({ "asks": asks::open(state.engine.inner(), id) }),
    ))
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc {
        method: "GET",
        path: "/conversations/{id}/changes",
        summary: "What the conversation's agent changed in its checkout: `{conversation, workstream, mode, pending, owed, turns: [{turn, prompt?, reply?, agent, mode, started_at, ended_at?, files: [{path, kind, state, opaque, overlapped, added, removed}]}]}` — the turns that changed something, oldest first, each under the reply it posted. `pending` is the files waiting for a word; `owed` is true in `manual`. What somebody else wrote since is folded away before the answer. 400 for a conversation that is not about a project or a workstream.",
    },
    RouteDoc {
        method: "GET",
        path: "/conversations/{id}/changes/file",
        summary: "One file under review (`?path=`): `{file: {path, kind, opaque, overlapped, base_text, disk_hash, hunks: [{id, base: {start, len}, disk: {start, len}, removed, added}]}}` — `base_text` is the file before the pending changes, the hunks are cut against the disk whose hash is `disk_hash`. `{file: null}` when nothing of it is pending.",
    },
    RouteDoc {
        method: "POST",
        path: "/conversations/{id}/changes/settle",
        summary: "Keep or undo: `{verdict: keep|undo, target: {grain: all} | {grain: turn, turn} | {grain: file, path} | {grain: hunk, path, hunk, disk_hash}, force?}` → `{files, pending, skipped: [{path, why}]}`. An undo writes through the editor's own write path and never discards somebody else's edit: a file it would conflict with is left alone and named in `skipped`, as is an `overlapped` one unless `force`. A hunk states the `disk_hash` it was read with; 409 when the file moved since. A file the agent made is disposed of as `editor.delete.trash` says.",
    },
    RouteDoc {
        method: "POST",
        path: "/conversations/{id}/changes/restore",
        summary: "Go back to before the message that woke a turn: `{turn}` → `{files, pending, skipped}`. Every file that turn or a later one touched returns to what it was; one somebody else changed since is merged, one that would conflict is left alone and named. The messages stay.",
    },
    RouteDoc {
        method: "GET",
        path: "/conversations/{id}/asks",
        summary: "The asks a turn of this conversation waits on, oldest first: `{asks: [{id, agent, subject, question, grantable, opened_at}]}` — `subject` is what is asked about: `{kind: tool, tool, tier}`, a call the guard put to the person, or `{kind: content, source, url?, reason, excerpt}`, what an agent was about to read from outside. The question is redacted. Empty when nothing waits; an ask lives as long as its session.",
    },
    RouteDoc {
        method: "POST",
        path: "/conversations/{id}/asks/{ask}",
        summary: "Answer one: `{answer: allow, scope?: once|conversation}` or `{answer: deny, note?}` → the asks still waiting. `conversation` allows that tool for the rest of the conversation when the ask is `grantable` — the mode's own ask, never a guard rule's. A `note` is what the agent hears as the reason. 400 when the ask is no longer waiting.",
    },
];
