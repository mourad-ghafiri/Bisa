//! How the platform reaches the internet, for Settings › Capabilities ›
//! Network (ide/13): the `network.*` settings as the node holds them with
//! what is in force now — a proxy's password masked — and one request the
//! person asks for, to see whether the way out works. The settings themselves
//! are written through `/settings/machine`; the engine swaps its clients on
//! every write of one (`bisa_engine::network`).

use crate::dto::NetworkCheckBody;
use crate::route_docs::RouteDoc;
use crate::{ApiError, Shared};
use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use bisa_engine::network::{NetworkCheck, NetworkStatus};

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/network", get(status))
        .route("/network/check", post(check))
}

async fn status(State(state): State<Shared>) -> Json<NetworkStatus> {
    Json(state.engine.network_status())
}

/// 400 for a URL that is not `http(s)` or is this machine; otherwise what
/// one `GET` answered, or why it could not.
async fn check(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<NetworkCheckBody>,
) -> Result<Json<NetworkCheck>, ApiError> {
    Ok(Json(state.engine.network_check(&body.url).await?))
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc {
        method: "GET",
        path: "/network",
        summary: "The `network.*` settings as the node holds them — the mode, the manual URLs with their password masked, the bypass, HTTP/1.1 only — with what is in force now (`in_force`: `direct`, or `proxy` with its URLs), what the node's own environment names (`environment`), and what keeps the settings from being in force (`problems`).",
    },
    RouteDoc {
        method: "POST",
        path: "/network/check",
        summary: "One `GET` to `{url}` through the node's outbound client, the body dropped: `ok` when any answer came back, its `status`, `elapsed_ms`, the `error` otherwise, and whether it went `via_proxy`. 400 for a URL that is not `http(s)` or names this machine.",
    },
];
