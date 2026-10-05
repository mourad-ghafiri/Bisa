//! Drawings (19 — Drawings): the canvas's routes.
//!
//! Five routes over [`bisa_store::drawings`] laid out like the notes' — a
//! listing by scope, create, read, a guarded patch, delete — the two the
//! desktop performs an agent's drawing requests through, and the drawings
//! repository (`/drawings/git…`, the notes' shape without a pull: a
//! drawing's record arrives by sync, and the files are an export).
//!
//! # Nothing here announces itself
//!
//! `create` and `patch` from the desktop emit no `drawing_changed` for the
//! writer's own sake — the engine announces a creation (a list elsewhere
//! re-reads) and stays silent on a patch, as it does for a note: the writer
//! holds the answer, and telling it would make it re-read what it just
//! drew. An agent's change is announced by the engine when its parked
//! request is answered.
//!
//! # Two writers, so one guard
//!
//! A drawing has two — the person on the canvas and any agent drawing into
//! it — so `PATCH` carries a `base_hash` and a stale one is a **409** carrying
//! the current scene ([`crate::owner::conflict_or`]).

use crate::dto::{
    DrawingDetail, DrawingRow, FolderRepo, NewDrawingBody, PatchDrawingBody, RepoCommitBody,
    RepoCommitRow, RepoIdentityBody, RepoRemoteBody,
};
use crate::owner::{conflict_or, filter_of, scope_of, ScopeQuery};
use crate::route_docs::RouteDoc;
use crate::Query;
use crate::{bad_request, not_found, repo_git, ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use bisa_core::{Drawing, DrawingId, DrawingSummary};
use bisa_engine::drawings::{self as eng, DrawResult};
use bisa_engine::folder_git::Folder;
use bisa_store::{scene_hash, DrawingPatch, NewDrawing};
use serde_json::json;
use std::str::FromStr;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/drawings", get(list).post(create))
        // The agents' parked requests and the repository — before `/drawings/{id}`,
        // so neither `requests` nor `git` is ever read as an id.
        .route("/drawings/requests", get(pending))
        .route("/drawings/requests/{id}", post(answer))
        .route("/drawings/git", get(repo_status))
        .route("/drawings/git/commit", post(repo_commit))
        .route("/drawings/git/message", post(repo_message))
        .route("/drawings/git/push", post(repo_push))
        .route("/drawings/git/fetch", post(repo_fetch))
        .route("/drawings/git/remote", put(repo_remote))
        .route("/drawings/git/identity", put(repo_identity))
        .route(
            "/drawings/{id}",
            get(detail).patch(patch_drawing).delete(remove),
        )
}

fn drawing_id(raw: &str) -> Result<DrawingId, ApiError> {
    DrawingId::from_str(raw).map_err(|e| {
        bad_request(bisa_core::text!(
            "error-node-drawings-drawing-id",
            e = e.to_string()
        ))
    })
}

fn row_of(s: DrawingSummary) -> DrawingRow {
    DrawingRow {
        id: s.id.to_string(),
        scope: s.scope.kind().to_string(),
        scope_id: s.scope.id(),
        title: s.title,
        pinned: s.pinned,
        hash: s.hash,
        element_count: s.element_count,
        created_at: s.created_at,
        updated_at: s.updated_at,
    }
}

fn detail_of(d: Drawing) -> DrawingDetail {
    DrawingDetail {
        row: DrawingRow {
            id: d.id.to_string(),
            scope: d.scope.kind().to_string(),
            scope_id: d.scope.id(),
            title: d.title,
            pinned: d.pinned,
            hash: scene_hash(&d.scene),
            element_count: d.scene.element_count(),
            created_at: d.created_at,
            updated_at: d.updated_at,
        },
        scene: d.scene,
    }
}

async fn list(
    State(state): State<Shared>,
    Query(q): Query<ScopeQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let filter = filter_of(q.scope.as_deref(), q.id.as_deref())?;
    let drawings: Vec<DrawingRow> = state
        .engine
        .workspace()
        .list_drawings(filter)?
        .into_iter()
        .map(row_of)
        .collect();
    Ok(Json(json!({"drawings": drawings})))
}

async fn create(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<NewDrawingBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let scope = scope_of(&body.scope, body.id.as_deref())?;
    let drawing = eng::create(
        state.engine.inner(),
        NewDrawing {
            scope,
            title: body.title,
            scene: body.scene,
        },
    )?;
    Ok(Json(json!({"drawing": detail_of(drawing)})))
}

async fn detail(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let drawing = state.engine.workspace().get_drawing(drawing_id(&id)?)?;
    Ok(Json(json!({"drawing": detail_of(drawing)})))
}

async fn patch_drawing(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::i18n::Lang(locale): crate::i18n::Lang,
    crate::Body(body): crate::Body<PatchDrawingBody>,
) -> Response {
    let id = match drawing_id(&id) {
        Ok(id) => id,
        Err(e) => return e.into_response(),
    };
    let patched = eng::update(
        state.engine.inner(),
        id,
        DrawingPatch {
            title: body.title,
            pinned: body.pinned,
            scene: body.scene,
        },
        body.base_hash.as_deref(),
    );
    match patched {
        // Silent on the bus on purpose; `bisa_engine::drawings::update` says why.
        Ok(drawing) => Json(json!({"drawing": detail_of(drawing)})).into_response(),
        Err(e) => conflict_or(e, &locale),
    }
}

async fn remove(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    eng::delete(state.engine.inner(), drawing_id(&id)?)?;
    Ok(Json(json!({"ok": true})))
}

// --- the agents' requests, performed by the desktop ---------------------------

async fn pending(State(state): State<Shared>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        json!({"requests": state.engine.inner().draw.pending()}),
    ))
}

async fn answer(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::Body(result): crate::Body<DrawResult>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // A late answer — the op already said the canvas was silent — is still
    // a 404 to the desktop, but a save it carries is announced all the same
    // (`drawings::answer`): the canvas and the list learn what the store holds.
    if !eng::answer(state.engine.inner(), &id, result) {
        return Err(not_found(bisa_core::text!(
            "error-node-drawings-nothing-waits-under-id"
        )));
    }
    Ok(Json(json!({"answered": true})))
}

// --- the drawings repository: the shared handlers under this prefix ---------

async fn repo_status(State(state): State<Shared>) -> Result<Json<FolderRepo>, ApiError> {
    repo_git::status(&state, Folder::Drawings).await
}

async fn repo_commit(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<RepoCommitBody>,
) -> Result<Json<RepoCommitRow>, ApiError> {
    repo_git::commit(&state, Folder::Drawings, body).await
}

async fn repo_message(State(state): State<Shared>) -> Json<serde_json::Value> {
    repo_git::message(&state, Folder::Drawings).await
}

async fn repo_push(State(state): State<Shared>) -> Result<Json<FolderRepo>, ApiError> {
    repo_git::push(&state, Folder::Drawings).await
}

async fn repo_fetch(State(state): State<Shared>) -> Result<Json<FolderRepo>, ApiError> {
    repo_git::fetch(&state, Folder::Drawings).await
}

async fn repo_remote(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<RepoRemoteBody>,
) -> Result<Json<FolderRepo>, ApiError> {
    repo_git::set_remote(&state, Folder::Drawings, body).await
}

async fn repo_identity(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<RepoIdentityBody>,
) -> Result<Json<FolderRepo>, ApiError> {
    repo_git::set_identity(&state, Folder::Drawings, body).await
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc {
        method: "GET",
        path: "/drawings",
        summary: "Every drawing, every drawing of a kind (`?scope=project`), or one scope's (`?scope=goal&id=`) — `{drawings: [{id, scope, scope_id?, title, pinned, hash, element_count, created_at, updated_at}]}`, no scene: a list never carries one.",
    },
    RouteDoc {
        method: "POST",
        path: "/drawings",
        summary: "Create a drawing: `{title, scope, id?, scene?}` — `scene` a template's `{elements, app_state: {view_background_color, grid}}`, else an empty canvas. Answers the detail with its `hash`.",
    },
    RouteDoc {
        method: "GET",
        path: "/drawings/{id}",
        summary: "One drawing: the row and its `scene` (Excalidraw's elements, vector only), with `hash` for guarded updates.",
    },
    RouteDoc {
        method: "PATCH",
        path: "/drawings/{id}",
        summary: "Edit a drawing — `{title?, pinned?, scene?, base_hash?}`; a scene needs the `base_hash` you last read, and a stale one is a 409 carrying `current` (the scene) and `current_hash`. A scene over 768 KiB, over 4000 elements or holding an image is refused.",
    },
    RouteDoc {
        method: "DELETE",
        path: "/drawings/{id}",
        summary: "Delete a drawing: its record, its snapshot and its file.",
    },
    RouteDoc {
        method: "GET",
        path: "/drawings/requests",
        summary: "The drawing tool calls agents parked for the canvas and nobody answered yet, oldest first: `{requests: [{id, request: {action: draw · mermaid · snapshot, drawing, elements?, text?, replace}, scope: {agent?, conversation?}, asked_at}]}` — what a desktop that just opened reads to catch up, and reads again every twenty seconds while open: a read is how the engine knows a desktop is home (a request asked with none heard from within 45 s is refused at once as not available). A live desktop hears each request as a `drawing_request` frame too.",
    },
    RouteDoc {
        method: "POST",
        path: "/drawings/requests/{id}",
        summary: "The desktop's answer to one drawing request: a `DrawResult` — `{ok, error?, drawing?, hash?, element_count?, snapshot?, width?, height?}`; a snapshot is the `AttachmentRef` of a PNG the desktop uploaded through `POST /attachments`, never bytes here — the engine answers the agent the path of its named copy. The waiting tool call returns it. 404 when nothing waits under that id (it was answered, or timed out).",
    },
    RouteDoc {
        method: "GET",
        path: "/drawings/git",
        summary: "The drawings repository: changes since the last commit, the branch against origin, who commits, the last commit.",
    },
    RouteDoc {
        method: "POST",
        path: "/drawings/git/commit",
        summary: "Commit every change to the drawings as one commit: `{message}`; 400 with nobody set to commit, 409 with nothing changed.",
    },
    RouteDoc {
        method: "POST",
        path: "/drawings/git/message",
        summary: "A commit message drafted by the General Agent from what changed — read-only.",
    },
    RouteDoc {
        method: "POST",
        path: "/drawings/git/push",
        summary: "Push the drawings to origin, setting the upstream the first time — the person's own act, no Publish gate. There is no pull: a drawing's record arrives by sync, and the files are its export.",
    },
    RouteDoc {
        method: "POST",
        path: "/drawings/git/fetch",
        summary: "Read what origin has, moving nothing here.",
    },
    RouteDoc {
        method: "PUT",
        path: "/drawings/git/remote",
        summary: "Set origin's URL: `{url}`.",
    },
    RouteDoc {
        method: "PUT",
        path: "/drawings/git/identity",
        summary: "Who commits drawings — the repository's own pair: `{name, email}`.",
    },
];
