//! The desktop's half of the browser bridge (ide/18): the requests agents
//! parked for the embedded browser, and the door their answers come back
//! through. The engine holds the requests (`bisa_engine::browser`); the
//! desktop hears each on the bus (`browser_request`), reads the list here
//! when it opens, performs it in its webview and posts the result.

use crate::route_docs::RouteDoc;
use crate::{not_found, ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::routing::get;
use axum::{Json, Router};
use bisa_engine::browser::BrowserResult;
use serde_json::json;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/browser/requests", get(pending))
        .route("/browser/requests/{id}", axum::routing::post(answer))
}

pub const ROUTES: &[RouteDoc] = &[
    RouteDoc { method: "GET", path: "/browser/requests", summary: "The browser tool calls agents parked for the embedded browser and nobody answered yet, oldest first: `{requests: [{id, request: {action, tab?, url?, target?, query?, text?, format?, all?, until?, timeout_ms?, key?, modifiers?, clear?, submit?, value?, label?, to?, by_x?, by_y?, clear_console?, expression?, headless?}, scope: {home?: {scope, id}, agent?}, headless, asked_at}]}` — `action` one of open · tabs · read · find · snapshot · click · fill · type · press · select · hover · scroll · wait · back · forward · reload · console · eval · close · screenshot; what a desktop that just opened reads to catch up, and reads again every twenty seconds while open: a read is how the engine knows a desktop is home (a request asked with none heard from within 45 s is refused at once as not available). A live desktop hears each request as a `browser_request` frame too. `scope.home` is the engine's word on where the tab is at home — the checkout, the goal, the work item, the workflow, the channel, the direct message or the conversation the asking session speaks in — so the desktop roots it there; `headless` is the engine's decision that a tab this request opens is kept out of sight." },
    RouteDoc { method: "POST", path: "/browser/requests/{id}", summary: "The desktop's answer to one browser request: a `BrowserResult` — `{ok, error?, tab?, url?, title?, text?, tabs?, navigated?, count?, waited_ms?, scroll?, value?, dialogs?, console?, screenshot?, width?, height?}`, the text and the value cut at 16 KiB, at most twenty dialogs and a hundred console lines; a screenshot is the `AttachmentRef` of a PNG the desktop uploaded through `POST /attachments`, never bytes here — the engine answers the agent the path of its named copy. The waiting tool call returns it. 404 when nothing waits under that id (it was answered, or timed out)." },
];

async fn pending(State(state): State<Shared>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        json!({"requests": state.engine.inner().browser.pending()}),
    ))
}

async fn answer(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::Body(result): crate::Body<BrowserResult>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !state.engine.inner().browser.answer(&id, result) {
        return Err(not_found(bisa_core::text!(
            "error-node-browser-nothing-waits-under-id"
        )));
    }
    Ok(Json(json!({"answered": true})))
}
