//! The MCP server registry over HTTP: named, tagged, reusable — and local.
//!
//! An MCP server used to be a raw JSON blob copied onto each agent and carried
//! inside the public agent snapshot. A stdio transport is a command line plus
//! an environment, so that shape published one machine's paths — and anything
//! an `env` entry named — to every peer. A registry entry is a *local* object
//! with an id; agents reference the id, and whether this machine has a server
//! behind it is this machine's business.
//!
//! One name is refused: the engine injects Bisa's own server into every
//! session, so a second server answering to `bisa` would shadow the tools
//! the platform runs on. The store refuses it, at the door, with a message
//! that says why — and that message is what the caller gets, as a 400.
//!
//! What leaves here is **masked**: every `env` and `headers` value reads as
//! `••••••` (`McpServerConfig::masked`) — the keys stay, so a person editing
//! knows which variables exist — and a value sent back as the mask keeps
//! the stored one. Every entry carries its `health`: what the last probe
//! answered, or `unknown` (`bisa_engine::mcp_health`). A probe is a result,
//! never an HTTP error: a server that would not answer is a 200 whose
//! `report.ok` is false, with the stage it stopped at.

use crate::dto::*;
use crate::route_docs::RouteDoc;
use crate::tags::{parse_tags, TagFilter};
use crate::{mcp_id, ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use bisa_core::tags::TagEntity;
use bisa_core::Localize as _;
use bisa_core::McpServer;
use bisa_store::NewMcp;
use serde_json::json;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/mcp", get(list).post(create))
        .route("/mcp/probe", post(probe_transport))
        .route("/mcp/{id}", get(detail).patch(patch_mcp).delete(remove))
        .route("/mcp/{id}/probe", post(probe_saved))
}

/// An entry as the wire carries it: secrets masked, its health beside it
/// (`dto::McpServerView`).
fn shown(state: &Shared, server: &McpServer) -> serde_json::Value {
    json!(crate::dto::McpServerView {
        server: McpServer {
            transport: server.transport.masked(),
            ..server.clone()
        },
        health: state.engine.inner().mcp_health.view_of(&server.id),
    })
}

async fn list(
    State(state): State<Shared>,
    filter: TagFilter,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    let servers = filter.apply(ws, TagEntity::Mcp, ws.list_mcps()?, |m| m.id.to_string())?;
    let shown: Vec<serde_json::Value> = servers.iter().map(|m| shown(&state, m)).collect();
    Ok(Json(json!({"mcp": shown})))
}

/// Dial a transport that is nobody's entry — the editor's *Test connection*
/// — and answer the report. A transport that fails its own shape is a 400;
/// a server that would not answer is a 200 whose `report.ok` is false.
async fn probe_transport(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<ProbeMcpBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if body.transport.name().trim().is_empty() {
        return Err(crate::bad_request(bisa_core::McpError::EmptyName.text()));
    }
    body.transport
        .validate()
        .map_err(|e| crate::bad_request(e.text()))?;
    let report = state
        .engine
        .inner()
        .mcp_health
        .probe_transport(&body.transport, body.timeout_secs)
        .await;
    Ok(Json(json!({"report": report})))
}

/// Dial a registered server, keep the answer as its health, and answer the
/// report; 404 for an id nobody registered.
async fn probe_saved(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::Body(body): crate::Body<ProbeMcpByIdBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = mcp_id(&id)?;
    let timeout = body.timeout_secs;
    let inner = state.engine.inner();
    let report = inner.mcp_health.probe_mcp(inner, &id, timeout).await?;
    Ok(Json(
        json!({"report": report, "health": inner.mcp_health.view_of(&id)}),
    ))
}

async fn create(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<NewMcpBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let server = bisa_engine::directory::create_mcp(
        state.engine.inner(),
        NewMcp {
            id: mcp_id(&body.id)?,
            description: body.description,
            tags: parse_tags(&body.tags)?,
            transport: body.transport,
        },
    )?;
    Ok(Json(json!({"mcp": shown(&state, &server)})))
}

async fn detail(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    let id = mcp_id(&id)?;
    let server = ws.get_mcp(&id)?;
    // The same second half a skill's detail carries: who uses this server.
    let agents: Vec<serde_json::Value> = ws
        .list_agents()?
        .into_iter()
        .filter(|a| a.mcps.contains(&id))
        .map(|a| json!({"id": a.id, "name": a.name}))
        .collect();
    Ok(Json(
        json!({"mcp": shown(&state, &server), "agents": agents}),
    ))
}

async fn patch_mcp(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::Body(body): crate::Body<PatchMcpBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    let mut server = ws.get_mcp(&mcp_id(&id)?)?;
    if let Some(v) = body.description {
        server.description = v;
    }
    if let Some(v) = &body.tags {
        server.tags = parse_tags(v)?;
    }
    if let Some(v) = body.transport {
        // `name` is derived from the transport by the store, so it is not
        // patched here — one field cannot disagree with the other. A value
        // that came back masked keeps the stored one: the store's rule.
        server.transport = v;
    }
    if let Some(v) = body.enabled {
        server.enabled = v;
    }
    let saved = bisa_engine::directory::update_mcp(state.engine.inner(), server)?;
    Ok(Json(json!({"mcp": shown(&state, &saved)})))
}

/// Remove a server, or refuse while any agent still carries it — the same
/// discipline as deleting a skill, and for the same reason: a delete that
/// quietly edits every agent that named the server is a data loss with no
/// record of itself. `GET /usage/mcp/{id}` lists the holders first.
async fn remove(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    bisa_engine::directory::remove_mcp(state.engine.inner(), &mcp_id(&id)?)?;
    Ok(Json(json!({"ok": true, "mcp": id})))
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc {
        method: "GET",
        path: "/mcp",
        summary: "Every MCP server definition, secrets masked, each with its `health`.",
    },
    RouteDoc {
        method: "POST",
        path: "/mcp",
        summary: "Define an MCP server: `{id, description?, transport, tags?}` — a transport is `stdio` (command, args, env, cwd), `http` (Streamable HTTP: url, headers) or `sse` (the 2024-11-05 transport: url, headers).",
    },
    RouteDoc {
        method: "POST",
        path: "/mcp/probe",
        summary: "Dial a transport that is nobody's entry yet — `{transport, timeout_secs?}` — and answer `{report}`: who answered, the negotiated revision and era, capabilities and tools; a server that would not answer is a 200 whose `report.ok` is false, with the stage it stopped at.",
    },
    RouteDoc {
        method: "GET",
        path: "/mcp/{id}",
        summary: "One MCP server definition, secrets masked, with its `health` and the agents that carry it.",
    },
    RouteDoc {
        method: "PATCH",
        path: "/mcp/{id}",
        summary: "Edit an MCP server; a value sent back as the mask keeps the stored one.",
    },
    RouteDoc {
        method: "POST",
        path: "/mcp/{id}/probe",
        summary: "Dial a registered server (`{timeout_secs?}`), keep the answer as its health, tell the bus (`mcp_probed`), and answer `{report, health}`; a disabled server is answered in words and never dialed.",
    },
    RouteDoc {
        method: "DELETE",
        path: "/mcp/{id}",
        summary: "Remove a server, or 409 while an agent still carries it.",
    },
];
