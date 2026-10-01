//! The Decision-Making Agent over HTTP: what it is and who answers for it on
//! this node, the judgements it made, a question to try it with, and the API
//! key of a remote provider.
//!
//! Who answers is settings (`decisions.*`) and goes through the settings
//! routes. The key is the one thing that is not a setting: it is written here,
//! kept in this machine's keystore, and **never read back** — the status says
//! whether one is stored and nothing more.

use crate::dto::{DecisionKeyBody, RecentJudgementsQuery};
use crate::route_docs::RouteDoc;
use crate::Query;
use crate::{bad_request, ApiError, Shared};
use axum::extract::{Path, State};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use bisa_core::{DecisionProviderKind, DecisionRequest, DecisionResponse};
use bisa_engine::decider as eng;

/// The most judgements one read returns.
const MAX_RECENT: usize = 200;
pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/decisions/status", get(status))
        .route("/decisions", get(recent))
        .route("/decisions/try", post(try_it))
        .route("/decisions/key/{provider}", put(set_key).delete(clear_key))
}

/// The Decision-Making Agent as it stands here: the agent itself, the provider
/// and the model that answers, whether it can be asked, whether a key is
/// stored, and which decision points the workspace's switch reaches.
async fn status(State(state): State<Shared>) -> Result<Json<eng::DeciderStatus>, ApiError> {
    Ok(Json(state.engine.decider_status()?))
}

/// The newest judgements, newest first.
async fn recent(
    State(state): State<Shared>,
    Query(query): Query<RecentJudgementsQuery>,
) -> Result<Json<Vec<eng::JudgementRecord>>, ApiError> {
    let limit = query.limit.unwrap_or(50).clamp(1, MAX_RECENT);
    Ok(Json(state.engine.recent_judgements(limit)?))
}

/// Put one request to the provider as it is set up. Nothing is decided and
/// nothing is recorded; the request is held to the contract first and the
/// answer after.
async fn try_it(
    State(state): State<Shared>,
    crate::Body(request): crate::Body<DecisionRequest>,
) -> Result<Json<DecisionResponse>, ApiError> {
    // The size bound is the contract's (`DecisionRequest::validate`).
    Ok(Json(state.engine.try_decision(&request).await?))
}

fn remote(provider: &str) -> Result<DecisionProviderKind, ApiError> {
    let kind: DecisionProviderKind = provider.parse().map_err(|_| {
        bad_request(bisa_core::text!(
            "error-node-decisions-not-decision-provider",
            provider = provider.to_string()
        ))
    })?;
    if !kind.is_remote() {
        return Err(bad_request(bisa_core::text!(
            "error-node-decisions-provider-takes-no-api-key",
            provider = provider.to_string()
        )));
    }
    Ok(kind)
}

/// Keep a remote provider's API key in this machine's keystore. The answer
/// says a key is stored, and never which.
async fn set_key(
    State(state): State<Shared>,
    Path(provider): Path<String>,
    crate::Body(body): crate::Body<DecisionKeyBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let kind = remote(&provider)?;
    state.engine.set_decision_key(kind, Some(&body.key))?;
    Ok(Json(serde_json::json!({ "key_stored": true })))
}

/// Forget it. Idempotent.
async fn clear_key(
    State(state): State<Shared>,
    Path(provider): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let kind = remote(&provider)?;
    state.engine.set_decision_key(kind, None)?;
    Ok(Json(serde_json::json!({ "key_stored": false })))
}

pub const ROUTES: &[RouteDoc] = &[
    RouteDoc { method: "GET", path: "/decisions/status", summary: "The Decision-Making Agent as it stands on this node: the agent itself (`agent` — its reserved id `decision-making-agent`, its name, what it is, what answers out of the box), whether `decisions.enabled` is on, the provider and the model that answers (`answers_as`) and, when a harness answers, the effort it works at (`effort`), whether its probabilities are a calibrated model's, whether it can be asked (`ready`, `problem`), whether an API key is stored for a remote provider (`key_stored` — never the key), every decision point with whether the switch reaches it, the deadline and the two confidence thresholds. Asks nobody." },
    RouteDoc { method: "GET", path: "/decisions", summary: "The newest judgements this node asked for, newest first (`?limit=`, 50 by default, 200 at most): when, the goal, agent, run and step it stood at, and the judgement — the decision point, the provider and model, the questions as they were asked (redacted), the answers, `applied | unsure | failed` and the reason. Read from the activity feed, which keeps every judgement, goal or no goal." },
    RouteDoc { method: "POST", path: "/decisions/try", summary: "Put one request to the provider as it is set up: `{state, questions}` → `{model, answers, usage}`, the decision contract both ways. Nothing is decided and nothing is recorded; a request or an answer that breaks the contract — or a request over 64 KiB — is a 400 saying how, a provider that refused or answered what cannot be read a 502, one not set up or busy or out of reach a 503, one that did not answer in time a 504." },
    RouteDoc { method: "PUT", path: "/decisions/key/{provider}", summary: "Keep the API key of a remote provider (`jev`, `rlcd`) in this machine's keystore: `{key}` → `{key_stored: true}`. The key is never a setting and is never read back." },
    RouteDoc { method: "DELETE", path: "/decisions/key/{provider}", summary: "Forget that key: `{key_stored: false}`. Idempotent." },
];
