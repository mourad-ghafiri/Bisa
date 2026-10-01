//! The files a workspace owns, over HTTP.
//!
//! A goal owns real folders — projects, workstreams, the `work` directory its
//! sessions run in, the patches they produced — and until this module the only
//! way to see any of it was a terminal open in the data directory. Three
//! routes, shaped like `/usage/{kind}/{id}`, which is the closest existing
//! thing: a kind and an id in the path, the store's own answer in the body.
//!
//! | Route | Answer |
//! |---|---|
//! | `GET /tree/{scope}/{id}?path=&depth=` | a bounded listing, every entry annotated with what it *is* |
//! | `GET /file/{scope}/{id}?path=` | one bounded read: `{path, size, binary, truncated, text}` |
//! | `GET /placement/{scope}/{id}` | `{path, exists}` — where the scope's files live, whether or not they are there yet |
//!
//! `placement` exists so the other two can refuse cleanly. A project whose
//! folder was never created and a project id that does not exist are different
//! problems, and a route that answered both with the same 400 would leave the
//! caller guessing which one they had.
//!
//! **`work_item` is the scope that is not a directory.** The other three name a
//! folder outright; a work item names a *thing*, and where its work happens —
//! the workstream opened for it, or its goal's `work/` when it never had one —
//! is a rule rather than a lookup. The rule lives in
//! [`bisa_store::Workspace::work_item_root`], which is also what the
//! engine asks before and after a run, so a file route and a criterion cannot
//! disagree about where the work landed.
//!
//! **Nothing here decides anything.** The resolution, the boundary check, the
//! bounds and the annotation all live in [`bisa_store::tree`]; this module
//! parses a scope, parses two query parameters, and renders. That is
//! deliberate: the CLI reads the same files through the same store calls, and a
//! containment check with two implementations is a containment check with one
//! hole.

use crate::dto::*;
use crate::route_docs::RouteDoc;
use crate::Query;
use crate::{bad_request, ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::routing::get;
use axum::{Json, Router};
use bisa_store::FileScope;
use serde::Deserialize;
use std::str::FromStr;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/tree/{scope}/{id}", get(tree))
        .route("/file/{scope}/{id}", get(file))
        .route("/placement/{scope}/{id}", get(placement))
}

/// One of `goal|workstream|work_item`, or a 400 that spells out all three —
/// built from `FileScope::ALL`, so a fourth cannot leave it behind.
///
/// The store's `FromStr` refuses an unknown scope too, but its message repeats
/// what was typed without saying what would have worked, and this is the
/// surface where the typo is made.
fn parse_scope(raw: &str) -> Result<FileScope, ApiError> {
    FileScope::from_str(raw).map_err(|_| {
        let valid: Vec<&str> = FileScope::ALL.iter().map(|s| s.as_str()).collect();
        bad_request(bisa_core::text!(
            "error-node-files-unknown-file-scope-use-one",
            raw = format!("{raw:?}"),
            a0 = (valid.join(", ")).to_string()
        ))
    })
}

#[derive(Deserialize)]
struct ListQuery {
    /// Relative to the scope's root. Absent means the root itself.
    #[serde(default)]
    path: String,
    /// Clamped by the store rather than refused — see
    /// [`bisa_store::TREE_MAX_DEPTH`].
    #[serde(default)]
    depth: Option<usize>,
}

#[derive(Deserialize)]
struct FileQuery {
    #[serde(default)]
    path: String,
}

/// A bounded listing, every entry annotated with what it is.
async fn tree(
    State(state): State<Shared>,
    AxPath((scope, id)): AxPath<(String, String)>,
    Query(q): Query<ListQuery>,
) -> Result<Json<FileTreeDto>, ApiError> {
    let scope = parse_scope(&scope)?;
    let tree = state
        .engine
        .workspace()
        .list_tree(scope, &id, &q.path, q.depth)?;
    Ok(Json(FileTreeDto::new(scope, &id, tree)))
}

/// One file, capped and never served as bytes when it is not text.
async fn file(
    State(state): State<Shared>,
    AxPath((scope, id)): AxPath<(String, String)>,
    Query(q): Query<FileQuery>,
) -> Result<Json<FileContentDto>, ApiError> {
    let scope = parse_scope(&scope)?;
    if q.path.trim().is_empty() {
        // The empty path is the root, and the root is a directory. Saying so
        // here beats letting the store answer "\"\" is a directory".
        return Err(bad_request(bisa_core::text!(
            "error-node-files-get-file-needs-path-file-under-root"
        )));
    }
    let content = state.engine.workspace().read_file(scope, &id, &q.path)?;
    Ok(Json(content.into()))
}

/// Where this scope's files live, and whether the directory is there.
async fn placement(
    State(state): State<Shared>,
    AxPath((scope, id)): AxPath<(String, String)>,
) -> Result<Json<PlacementDto>, ApiError> {
    let scope = parse_scope(&scope)?;
    let placement = state.engine.workspace().placement(scope, &id)?;
    Ok(Json(placement.into()))
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc {
        method: "GET",
        path: "/tree/{scope}/{id}",
        summary: "A bounded listing of a goal, run of the workspace, workstream or work item's files — a project's root is its primary workstream; a run's is its scratch folder.",
    },
    RouteDoc {
        method: "GET",
        path: "/file/{scope}/{id}",
        summary: "One text file (`?path=`), capped at 256 KiB; never bytes.",
    },
    RouteDoc {
        method: "GET",
        path: "/placement/{scope}/{id}",
        summary: "Where this scope's files live, and whether the directory exists.",
    },
];
