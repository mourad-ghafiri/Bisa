//! Work-item routes: every work item of the workspace, one by its id, its
//! result, and forgetting a settled one.
//!
//! A work item is filed at its **home** — its goal, or the run of the
//! workspace that made it — so every route here is addressed by the item's
//! id alone and answers the home beside it, with the words a row is labelled
//! by: the goal's title (else its statement), or the run's workflow and
//! number. Nothing here asks which kind of run made the item.

use crate::route_docs::RouteDoc;
use crate::{bad_request, not_found, ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::routing::get;
use axum::{Json, Router};
use bisa_core::event::JournalPayload;
use bisa_core::{Home, WorkItemId};
use bisa_store::Workspace;
use serde_json::json;
use std::str::FromStr;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/work-items", get(list))
        .route("/work-items/{item}", get(detail).delete(remove))
        .route("/work-items/{item}/result", get(result))
}

fn parse_item(raw: &str) -> Result<WorkItemId, ApiError> {
    WorkItemId::from_str(raw)
        .map_err(|_| bad_request(bisa_core::text!("error-node-goals-not-work-item-id")))
}

/// The words a row names a home by: the goal's title, else the start of
/// its statement; a run of the workspace's workflow and its number among
/// that workflow's runs. `None` when the home can no longer be read.
pub(crate) fn home_label(ws: &Workspace, home: &Home) -> Option<String> {
    match home {
        Home::Goal { goal } => ws.get_goal(*goal).ok().map(|g| {
            g.title
                .unwrap_or_else(|| g.statement.chars().take(80).collect())
        }),
        Home::Run { run } => {
            let run = ws.get_run(*run).ok()?;
            let number = ws
                .list_workflow_runs(run.workflow.id)
                .ok()?
                .iter()
                .position(|r| r.id == run.id)
                .map(|i| i + 1)?;
            Some(format!("{} #{number}", run.workflow.name))
        }
    }
}

/// Every work item in the workspace — every goal's, newest goal first, then
/// every run of the workspace's, newest run first — each with its home and
/// the label that home reads as. An item's instructions on their own do not
/// say which piece of work they belong to.
async fn list(State(state): State<Shared>) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    let mut homes: Vec<Home> = ws
        .list_goals(None)?
        .into_iter()
        .map(|g| Home::Goal { goal: g.id })
        .collect();
    let mut runs = ws.workspace_runs(None, false)?;
    runs.reverse();
    homes.extend(runs.iter().map(bisa_core::WorkflowRun::home));
    let mut rows = Vec::new();
    for home in homes {
        let label = home_label(ws, &home);
        for item in ws.list_work_items(&home)? {
            rows.push(json!({"home": home, "label": label, "item": item}));
        }
    }
    Ok(Json(json!({ "work_items": rows })))
}

/// One work item from its id alone: its home and label, the item, its
/// latest result from the home's journal, and whether a captured patch
/// exists (`has_result`).
async fn detail(
    State(state): State<Shared>,
    AxPath(item): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let item_id = parse_item(&item)?;
    let ws = state.engine.workspace();
    let home = ws.home_of_work_item(item_id)?;
    let spec = ws.get_work_item(&home, item_id)?;
    let result = ws
        .journal(&home)?
        .into_iter()
        .rev()
        .find_map(|e| match e.payload {
            JournalPayload::Result {
                work_item, output, ..
            } if work_item == item_id => Some(output),
            _ => None,
        });
    let has_result = ws.paths().home(&home).result(item_id).exists();
    Ok(Json(json!({
        "home": home,
        "label": home_label(ws, &home),
        "item": spec,
        "result": result,
        "has_result": has_result,
    })))
}

/// The `.patch` an item leaves behind, as `text/x-patch` — only a **copy
/// workstream** writes one; 404 when there is none.
async fn result(
    State(state): State<Shared>,
    AxPath(item): AxPath<String>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    let item_id = parse_item(&item)?;
    let ws = state.engine.workspace();
    let home = ws.home_of_work_item(item_id)?;
    let path = ws.paths().home(&home).result(item_id);
    let body = std::fs::read_to_string(&path)
        .map_err(|_| not_found(bisa_core::text!("error-node-goals-no-captured-result-item")))?;
    Ok(([(axum::http::header::CONTENT_TYPE, "text/x-patch")], body))
}

/// Forget a work item that never ran or is settled; 409 while it runs.
async fn remove(
    State(state): State<Shared>,
    AxPath(item): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let item_id = parse_item(&item)?;
    let home = state.engine.workspace().home_of_work_item(item_id)?;
    state.engine.delete_work_item(&home, item_id)?;
    Ok(Json(
        json!({"ok": true, "home": home, "work_item": item_id.to_string()}),
    ))
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc { method: "GET", path: "/work-items", summary: "Every work item in the workspace — every goal's, newest goal first, then every run of the workspace's, newest run first — as `{home, label, item}`: `home` is `{home: goal, goal}` or `{home: run, run}`, `label` the goal's title (else its statement) or the run's workflow and number (`Nightly report #3`)." },
    RouteDoc { method: "GET", path: "/work-items/{item}", summary: "One work item from its id alone: `{home, label, item, result, has_result}` — its home and label, the item, its latest result from the home's journal, and whether a captured patch exists." },
    RouteDoc { method: "GET", path: "/work-items/{item}/result", summary: "The captured patch as `text/x-patch` — only a copy workstream leaves one; 404 when none." },
    RouteDoc { method: "DELETE", path: "/work-items/{item}", summary: "Forget a work item that never ran or is settled → `{ok, home, work_item}`; 409 while it runs." },
];
