//! What the platform needs before it can work (16 — The setup gate): the
//! five checks the desktop's gate and `bisa doctor` read, with the official
//! way to fix each and the fixes the node vouches for.

use crate::route_docs::RouteDoc;
use crate::Shared;
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};

pub(crate) fn routes() -> Router<Shared> {
    Router::new().route("/readiness", get(readiness))
}

/// The five checks, read now: git, a harness and the three core agents — the
/// Decision-Making Agent, the General Agent, the Workflow Agent — each with
/// its state, its words, the official install hint and the one-click fixes.
async fn readiness(State(state): State<Shared>) -> Json<bisa_engine::readiness::Readiness> {
    Json(state.engine.readiness().await)
}

pub const ROUTES: &[RouteDoc] = &[
    RouteDoc { method: "GET", path: "/readiness", summary: "What the platform needs before it can work: `{ready, checks: [{id: git|harness|decision_making_agent|general_agent|workflow_agent, state: ready|missing|unready, title, detail, hint?: {url, commands: [{platform, command}], verify?, sign_in?}, door: {door: settings, tab} | {door: agents} | {door: none}, fixes: [{kind: settings, label, set} | {kind: agent_harness, label, agent, harness, models}]}], checked_at}`. Probes nothing beyond the cached harness listing; installs nothing." },
];
