//! The language-server proxy over HTTP (ide/10): document synchronisation
//! from the editor's own buffer, allow-listed requests, the servers' status
//! and a restart. Notifications — diagnostics above all — ride the `engine`
//! stream as `lsp` frames.

use crate::dto::{LspDocumentBody, LspPathBody, LspRequestBody, LspRestartBody};
use crate::route_docs::RouteDoc;
use crate::{ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use bisa_engine::lsp as eng;
use bisa_store::FileScope;
use serde_json::json;
use std::str::FromStr;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/ide/lsp/{scope}/{id}/open", post(open))
        .route("/ide/lsp/{scope}/{id}/change", post(change))
        .route("/ide/lsp/{scope}/{id}/close", post(close))
        .route("/ide/lsp/{scope}/{id}/request", post(request))
        .route("/ide/lsp/{scope}/{id}/status", get(status))
        .route("/ide/lsp/{scope}/{id}/restart", post(restart))
}

fn scope_of(raw: &str) -> Result<FileScope, ApiError> {
    FileScope::from_str(raw).map_err(|_| {
        crate::bad_request(bisa_core::text!(
            "error-node-ide-lsp-not-file-scope",
            raw = format!("{raw:?}")
        ))
    })
}

async fn open(
    State(state): State<Shared>,
    AxPath((scope, id)): AxPath<(String, String)>,
    crate::Body(body): crate::Body<LspDocumentBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let scope = scope_of(&scope)?;
    let opened = eng::open(state.engine.inner(), scope, &id, &body.path, &body.text).await?;
    Ok(Json(json!({
        "path": body.path,
        "language": opened.language,
        "following": opened.following,
    })))
}

async fn change(
    State(state): State<Shared>,
    AxPath((scope, id)): AxPath<(String, String)>,
    crate::Body(body): crate::Body<LspDocumentBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let scope = scope_of(&scope)?;
    eng::change(state.engine.inner(), scope, &id, &body.path, &body.text).await?;
    Ok(Json(json!({"path": body.path})))
}

async fn close(
    State(state): State<Shared>,
    AxPath((scope, id)): AxPath<(String, String)>,
    crate::Body(body): crate::Body<LspPathBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let scope = scope_of(&scope)?;
    eng::close(state.engine.inner(), scope, &id, &body.path).await?;
    Ok(Json(json!({"path": body.path})))
}

async fn request(
    State(state): State<Shared>,
    AxPath((scope, id)): AxPath<(String, String)>,
    crate::Body(body): crate::Body<LspRequestBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let scope = scope_of(&scope)?;
    let result = eng::request(
        state.engine.inner(),
        scope,
        &id,
        &body.path,
        &body.method,
        body.params,
    )
    .await?;
    Ok(Json(json!({"method": body.method, "result": result})))
}

async fn status(
    State(state): State<Shared>,
    AxPath((scope, id)): AxPath<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let scope = scope_of(&scope)?;
    let rows = eng::status(state.engine.inner(), scope, &id).await?;
    Ok(Json(json!({"servers": rows})))
}

async fn restart(
    State(state): State<Shared>,
    AxPath((scope, id)): AxPath<(String, String)>,
    crate::Body(body): crate::Body<LspRestartBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let scope = scope_of(&scope)?;
    eng::restart(state.engine.inner(), scope, &id, &body.language).await?;
    Ok(Json(json!({"language": body.language, "restarted": true})))
}

pub const ROUTES: &[RouteDoc] = &[
    RouteDoc { method: "POST", path: "/ide/lsp/{scope}/{id}/open", summary: "A document opened in the editor: `{path, text}`. Starts the language's server when it is the first; answers `{path, language, following}`: the `language` the path is (`null` when none applies), and `following` — whether a server of it now holds the document. A language named with `following: false` has servers off, none configured for it, or one given up on after a crash loop; the editor opens the document again when that server starts." },
    RouteDoc { method: "POST", path: "/ide/lsp/{scope}/{id}/change", summary: "The buffer changed: `{path, text}` — the whole text, so diagnostics follow what is typed." },
    RouteDoc { method: "POST", path: "/ide/lsp/{scope}/{id}/close", summary: "A document closed: `{path}`. The last of its language arms `lsp.idle_ttl_secs`." },
    RouteDoc { method: "POST", path: "/ide/lsp/{scope}/{id}/request", summary: "Proxy one allow-listed request: `{path, method, params}` — `textDocument/hover`, `textDocument/definition`, `textDocument/references`, `textDocument/documentSymbol`, `textDocument/formatting`, `workspace/symbol`; any other method is a 400, never a guess (no rename, no completion, no signature help). `uri` fields are root-relative both ways." },
    RouteDoc { method: "GET", path: "/ide/lsp/{scope}/{id}/status", summary: "Every configured language: command, whether it is on the login shell's PATH, its install hint, and the server's state for this root." },
    RouteDoc { method: "POST", path: "/ide/lsp/{scope}/{id}/restart", summary: "Forget a crash-loop verdict and stop the server: `{language}`. The next document starts it again." },
];
