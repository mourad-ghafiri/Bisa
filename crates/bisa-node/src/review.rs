//! Review notes over HTTP (ide/04 §6): annotate a hunk, list,
//! edit, resolve, delete, and hand the notes to the agents.
//!
//! Thin: bounds, identity, and what "send" means are
//! `bisa_engine::ide::review`. This module parses ids and picks codes.

use crate::dto::{ReviewNoteBody, ReviewNoteEditBody, ReviewSendBody};
use crate::projects::parse_project_id;
use crate::route_docs::RouteDoc;
use crate::Query;
use crate::{bad_request, ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use bisa_core::{NoteId, WorkstreamId};
use bisa_engine::ide::review::{self, NewNote};
use serde::Deserialize;
use serde_json::json;
use std::str::FromStr;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/projects/{pid}/review", get(list).post(create))
        .route("/projects/{pid}/review/send", post(send))
        .route("/projects/{pid}/review/{id}", patch(edit).delete(delete))
        .route("/projects/{pid}/review/{id}/resolve", post(resolve))
}

fn parse_note_id(s: &str) -> Result<NoteId, ApiError> {
    NoteId::from_str(s).map_err(|_| {
        bad_request(bisa_core::text!(
            "error-node-review-not-review-note-id",
            s = format!("{s:?}")
        ))
    })
}

#[derive(Deserialize)]
struct ListQuery {
    #[serde(default)]
    workstream: Option<String>,
    /// Include resolved notes. Off by default: a resolved note is a record,
    /// not a task.
    #[serde(default)]
    resolved: bool,
}

async fn list(
    State(state): State<Shared>,
    AxPath(pid): AxPath<String>,
    Query(q): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pid = parse_project_id(&pid)?;
    let workstream = match q.workstream.as_deref() {
        Some(w) => Some(WorkstreamId::from_str(w).map_err(|_| {
            bad_request(bisa_core::text!(
                "error-node-review-not-workstream-id",
                w = format!("{w:?}")
            ))
        })?),
        None => None,
    };
    let notes = review::list(state.engine.inner(), pid, workstream, q.resolved)?;
    Ok(Json(json!({"project": pid.to_string(), "notes": notes})))
}

async fn create(
    State(state): State<Shared>,
    AxPath(pid): AxPath<String>,
    crate::Body(body): crate::Body<ReviewNoteBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let pid = parse_project_id(&pid)?;
    let workstream = match body.workstream.as_deref() {
        Some(w) => Some(WorkstreamId::from_str(w).map_err(|_| {
            bad_request(bisa_core::text!(
                "error-node-review-not-workstream-id",
                w = format!("{w:?}")
            ))
        })?),
        None => None,
    };
    let note = review::create(
        state.engine.inner(),
        NewNote {
            project: pid,
            workstream,
            path: body.path,
            start: body.start,
            end: body.end,
            scope: body.scope,
            hunk: body.hunk,
            body: body.body,
        },
    )?;
    Ok((StatusCode::CREATED, Json(json!({"note": note}))))
}

async fn edit(
    State(state): State<Shared>,
    AxPath((pid, id)): AxPath<(String, String)>,
    crate::Body(body): crate::Body<ReviewNoteEditBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pid = parse_project_id(&pid)?;
    let id = parse_note_id(&id)?;
    let note = review::edit(state.engine.inner(), pid, id, body.body)?;
    Ok(Json(json!({"note": note})))
}

async fn resolve(
    State(state): State<Shared>,
    AxPath((pid, id)): AxPath<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pid = parse_project_id(&pid)?;
    let id = parse_note_id(&id)?;
    let note = review::resolve(state.engine.inner(), pid, id)?;
    Ok(Json(json!({"note": note})))
}

async fn delete(
    State(state): State<Shared>,
    AxPath((pid, id)): AxPath<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let pid = parse_project_id(&pid)?;
    let id = parse_note_id(&id)?;
    review::delete(state.engine.inner(), pid, id)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn send(
    State(state): State<Shared>,
    AxPath(pid): AxPath<String>,
    crate::Body(body): crate::Body<ReviewSendBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pid = parse_project_id(&pid)?;
    let ids = body
        .ids
        .iter()
        .map(|s| parse_note_id(s))
        .collect::<Result<Vec<_>, _>>()?;
    let sent = review::send(state.engine.inner(), pid, ids)?;
    Ok(Json(json!({
        "project": pid.to_string(),
        "notes": sent.notes,
        "posted_to": sent.posted_to,
    })))
}

pub const ROUTES: &[RouteDoc] = &[
    RouteDoc {
        method: "GET",
        path: "/projects/{pid}/review",
        summary: "The project's review notes, oldest first (`?workstream=&resolved=true` to include resolved ones).",
    },
    RouteDoc {
        method: "POST",
        path: "/projects/{pid}/review",
        summary: "Annotate a hunk: `{path, start, end, scope, hunk, body, workstream?}`. The hunk text is hashed into the note's identity.",
    },
    RouteDoc {
        method: "PATCH",
        path: "/projects/{pid}/review/{id}",
        summary: "Edit the body. Clears `sent_at`: the agent has not seen this version.",
    },
    RouteDoc {
        method: "DELETE",
        path: "/projects/{pid}/review/{id}",
        summary: "Delete a note. 204.",
    },
    RouteDoc {
        method: "POST",
        path: "/projects/{pid}/review/{id}/resolve",
        summary: "Mark a note dealt with. It stays on disk, hidden from the default listing.",
    },
    RouteDoc {
        method: "POST",
        path: "/projects/{pid}/review/send",
        summary: "Hand notes to the agents: `{ids}` (empty = every unsent). Marks them sent and posts one message with `DiffHunk` chips into each attached goal's thread.",
    },
];
