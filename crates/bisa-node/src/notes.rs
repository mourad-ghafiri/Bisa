//! Notes: the scratchpad routes.
//!
//! Six routes over [`bisa_store::notes`], and the notes repository's under
//! `/notes/git`. An agent is asked about a note in a **conversation** whose
//! origin is the note (`POST /conversations` with `{"kind":"note","id":…}`,
//! 13 — Conversations); its only write into the document is the
//! `note_append` op, which the engine points at that note when the call
//! names none.
//!
//! # Nothing here announces itself
//!
//! `create` and `patch` emit no `note_changed`. That event says *somebody else
//! wrote*, and these two callers are the somebody — they get the note back in
//! the response. Emitting on them made the writer its own audience: the frame
//! returned, the overlay refetched, the refetch replaced the buffer somebody
//! was typing into, and the difference that produced was saved as another
//! write. The one path that does emit is the one with a different writer:
//! the `note_append` op an agent calls.
//!
//! # Two writers, so one guard
//!
//! Every other editable thing here has one writer. A note has two — you in the
//! editor and any agent you asked — so `PATCH` carries a `base_hash` and a
//! stale one is a **409** carrying the current body, exactly as
//! [`bisa_store::Workspace::recall_store`] answers an agent that tried to
//! overwrite a memory it had not read. Appending needs no guard, because two
//! appends produce both blocks.

use crate::dto::{
    FolderRepo, NewNoteBody, NoteRow, PatchNoteBody, RepoCommitBody, RepoCommitRow,
    RepoIdentityBody, RepoRemoteBody,
};
use crate::owner::{conflict_or, filter_of, scope_of, ScopeQuery};
use crate::route_docs::RouteDoc;
use crate::Query;
use crate::{bad_request, repo_git, ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use bisa_core::{Note, NoteId};
use bisa_engine::folder_git::Folder;
use bisa_store::{body_hash, NewNote, NotePatch};
use serde_json::json;
use std::str::FromStr;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/notes", get(list).post(create))
        // The notes repository — before `/notes/{id}`, so `git` is never read as an id.
        .route("/notes/git", get(repo_status))
        .route("/notes/git/commit", post(repo_commit))
        .route("/notes/git/message", post(repo_message))
        .route("/notes/git/push", post(repo_push))
        .route("/notes/git/fetch", post(repo_fetch))
        .route("/notes/git/pull", post(repo_pull))
        .route("/notes/git/remote", put(repo_remote))
        .route("/notes/git/identity", put(repo_identity))
        .route("/notes/{id}", get(detail).patch(patch_note).delete(remove))
}

// --- the notes repository: the shared handlers under this prefix -----------

async fn repo_status(State(state): State<Shared>) -> Result<Json<FolderRepo>, ApiError> {
    repo_git::status(&state, Folder::Notes).await
}

async fn repo_commit(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<RepoCommitBody>,
) -> Result<Json<RepoCommitRow>, ApiError> {
    repo_git::commit(&state, Folder::Notes, body).await
}

async fn repo_message(State(state): State<Shared>) -> Json<serde_json::Value> {
    repo_git::message(&state, Folder::Notes).await
}

async fn repo_push(State(state): State<Shared>) -> Result<Json<FolderRepo>, ApiError> {
    repo_git::push(&state, Folder::Notes).await
}

async fn repo_fetch(State(state): State<Shared>) -> Result<Json<FolderRepo>, ApiError> {
    repo_git::fetch(&state, Folder::Notes).await
}

async fn repo_pull(
    State(state): State<Shared>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    repo_git::pull(&state, Folder::Notes, &headers).await
}

async fn repo_remote(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<RepoRemoteBody>,
) -> Result<Json<FolderRepo>, ApiError> {
    repo_git::set_remote(&state, Folder::Notes, body).await
}

async fn repo_identity(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<RepoIdentityBody>,
) -> Result<Json<FolderRepo>, ApiError> {
    repo_git::set_identity(&state, Folder::Notes, body).await
}

fn note_id(raw: &str) -> Result<NoteId, ApiError> {
    NoteId::from_str(raw).map_err(|e| {
        bad_request(bisa_core::text!(
            "error-node-notes-note-id",
            e = e.to_string()
        ))
    })
}

fn row(def: Note) -> NoteRow {
    NoteRow {
        id: def.id.to_string(),
        scope: def.scope.kind().to_string(),
        scope_id: def.scope.id(),
        hash: body_hash(&def.body),
        title: def.title,
        body: def.body,
        created_at: def.created_at,
        updated_at: def.updated_at,
        pinned: def.pinned,
    }
}

async fn list(
    State(state): State<Shared>,
    Query(q): Query<ScopeQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let filter = filter_of(q.scope.as_deref(), q.id.as_deref())?;
    let notes: Vec<NoteRow> = state
        .engine
        .workspace()
        .list_notes(filter)?
        .into_iter()
        .map(row)
        .collect();
    Ok(Json(json!({"notes": notes})))
}

async fn create(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<NewNoteBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let scope = scope_of(&body.scope, body.id.as_deref())?;
    let note = bisa_engine::notes::create(
        state.engine.inner(),
        NewNote {
            scope,
            title: body.title,
            body: body.body,
        },
    )?;
    Ok(Json(json!({"note": row(note)})))
}

async fn detail(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let note = state.engine.workspace().get_note(note_id(&id)?)?;
    Ok(Json(json!({"note": row(note)})))
}

async fn patch_note(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::i18n::Lang(locale): crate::i18n::Lang,
    crate::Body(body): crate::Body<PatchNoteBody>,
) -> Response {
    let id = match note_id(&id) {
        Ok(id) => id,
        Err(e) => return e.into_response(),
    };
    let patched = bisa_engine::notes::update(
        state.engine.inner(),
        id,
        NotePatch {
            title: body.title,
            body: body.body,
            pinned: body.pinned,
        },
        body.base_hash.as_deref(),
    );
    match patched {
        // Silent on the bus on purpose; `bisa_engine::notes::update` says why.
        Ok(note) => Json(json!({"note": row(note)})).into_response(),
        Err(e) => conflict_or(e, &locale),
    }
}

async fn remove(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    bisa_engine::notes::delete(state.engine.inner(), note_id(&id)?)?;
    Ok(Json(json!({"ok": true})))
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc {
        method: "GET",
        path: "/notes",
        summary: "Every note, every note of a kind (`?scope=project`), or one scope's (`?scope=goal&id=`).",
    },
    RouteDoc {
        method: "POST",
        path: "/notes",
        summary: "Create a note: `{title, body, scope}`.",
    },
    RouteDoc {
        method: "GET",
        path: "/notes/{id}",
        summary: "One note, with its hash for guarded updates.",
    },
    RouteDoc {
        method: "PATCH",
        path: "/notes/{id}",
        summary: "Edit a note with `base_hash`; 409 carries the current text.",
    },
    RouteDoc {
        method: "DELETE",
        path: "/notes/{id}",
        summary: "Delete a note.",
    },
    RouteDoc {
        method: "GET",
        path: "/notes/git",
        summary: "The notes repository: changes since the last commit, the branch against origin, who commits, the last commit.",
    },
    RouteDoc {
        method: "POST",
        path: "/notes/git/commit",
        summary: "Commit every change to the notes as one commit: `{message}`; 409 with nobody set to commit or nothing changed.",
    },
    RouteDoc {
        method: "POST",
        path: "/notes/git/message",
        summary: "A commit message drafted by the General Agent from what changed — read-only.",
    },
    RouteDoc {
        method: "POST",
        path: "/notes/git/push",
        summary: "Push the notes to origin, setting the upstream the first time — the person's own act, no Publish gate.",
    },
    RouteDoc {
        method: "POST",
        path: "/notes/git/fetch",
        summary: "Read what origin has, moving nothing here.",
    },
    RouteDoc {
        method: "POST",
        path: "/notes/git/pull",
        summary: "Fast-forward to origin, a safety ref first; anything a fast-forward cannot take is refused.",
    },
    RouteDoc {
        method: "PUT",
        path: "/notes/git/remote",
        summary: "Set origin's URL: `{url}`.",
    },
    RouteDoc {
        method: "PUT",
        path: "/notes/git/identity",
        summary: "Who commits notes — the repository's own pair: `{name, email}`.",
    },
];
