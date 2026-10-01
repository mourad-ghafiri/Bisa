//! What points at an object, over HTTP — the question a `DELETE` answers the
//! hard way.
//!
//! The four `DELETE` routes refuse while anything still references what they
//! were asked to remove, and the refusal names the first few holders. That is
//! the right answer to a delete and the wrong shape for a UI: a client that
//! wants to grey out a button, or to show "used by 4 things" beside a row,
//! should not have to attempt the destructive call and parse prose out of a
//! 400. So the same store query is readable on its own here.
//!
//! One route, shaped like `/catalog/{kind}/{slug}` because that is the closest
//! existing thing: a kind and an id in the path, the store's own answer in the
//! body. An unknown kind is a 400 naming all four, built from `UsageKind::ALL`
//! so a fifth kind cannot leave the message behind.

use crate::dto::*;
use crate::route_docs::RouteDoc;
use crate::{bad_request, ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::routing::get;
use axum::{Json, Router};
use bisa_store::UsageKind;
use serde_json::json;
use std::str::FromStr;

pub(crate) fn routes() -> Router<Shared> {
    Router::new().route("/usage/{kind}/{id}", get(usage))
}

/// One of `agent|team|skill|mcp`, or a 400 that spells out all four.
///
/// The store's `FromStr` refuses an unknown kind too, but its message repeats
/// what was typed without saying what would have worked, and this is the
/// surface where the typo is made.
fn parse_kind(raw: &str) -> Result<UsageKind, ApiError> {
    UsageKind::from_str(raw).map_err(|_| {
        let valid: Vec<&str> = UsageKind::ALL.iter().map(|k| k.as_str()).collect();
        bad_request(bisa_core::text!(
            "error-node-usage-unknown-usage-kind-use-one",
            raw = format!("{raw:?}"),
            a0 = (valid.join(", ")).to_string()
        ))
    })
}

/// Everything referencing one object. An empty list means the matching
/// `DELETE` would succeed, and a non-empty one is exactly what it would
/// refuse with.
///
/// An id nothing knows about is the store's error rather than an empty list:
/// answering "nothing references it" for a typo would read as permission to
/// delete.
async fn usage(
    State(state): State<Shared>,
    AxPath((kind, id)): AxPath<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let usage = state.engine.workspace().usage_of(parse_kind(&kind)?, &id)?;
    let refs: Vec<ReferenceDto> = usage.into_iter().map(ReferenceDto::from).collect();
    Ok(Json(json!({"usage": refs})))
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[RouteDoc {
    method: "GET",
    path: "/usage/{kind}/{id}",
    summary: "Everything referencing one object — what a delete would have to answer for.",
}];
