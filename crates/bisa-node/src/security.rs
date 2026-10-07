//! Security over HTTP: the policy as this node runs it, and two previews for
//! Settings › Security to try a rule on. Nothing here writes a rule — rules
//! are settings (`security.*`) and go through the settings routes — and
//! nothing here returns a secret: a preview redacts on a scratch vault, the
//! status carries counts and redacted subjects only.

use crate::dto::{GuardPreviewBody, RedactPreviewBody};
use crate::route_docs::RouteDoc;
use crate::{bad_request, ApiError, Shared};
use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use bisa_engine::security as eng;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/security/status", get(status))
        .route("/security/redact-preview", post(redact_preview))
        .route("/security/guard-preview", post(guard_preview))
}

/// The effective policy: every rule with its origin, the rules that could not
/// be read, the classifier's settings and readiness, which harnesses the guard
/// can veto, and the last decisions (redacted).
async fn status(State(state): State<Shared>) -> Json<eng::SecurityStatus> {
    Json(state.engine.security_status())
}

/// What the redaction rules would do to a text — on a scratch vault.
async fn redact_preview(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<RedactPreviewBody>,
) -> Result<Json<eng::RedactPreview>, ApiError> {
    if body.text.len() > 64 * 1024 {
        return Err(bad_request(bisa_core::text!(
            "error-node-security-preview-reads-most-64-kib"
        )));
    }
    Ok(Json(state.engine.redact_preview(&body.text)))
}

/// What the guard rules alone say about a tool call — no classifier, no
/// restore, nothing recorded.
async fn guard_preview(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<GuardPreviewBody>,
) -> Result<Json<eng::GuardPreview>, ApiError> {
    if body.tool.trim().is_empty() {
        return Err(bad_request(bisa_core::text!(
            "error-node-security-tool-name-required"
        )));
    }
    Ok(Json(
        state.engine.guard_preview(body.tool.trim(), &body.input),
    ))
}

pub const ROUTES: &[RouteDoc] = &[
    RouteDoc { method: "GET", path: "/security/status", summary: "The security policy as this node runs it: the redaction and guard rules in order (built-ins with `origin: builtin`, switched-off ones with `enabled: false`, where each guard rule applies as `applies_to` — everywhere when absent, `platform` for the sessions the platform drives, `terminal` for a person's harness in a terminal), the rules that could not be read (`problems`), the classifier's settings and whether its agent can be launched, which harnesses the guard can veto (`tool_guard`) and hand a restored input to (`input_rewrite`), the last fifty decisions with their redacted subjects, and the vault's size. Never a secret or an environment value." },
    RouteDoc { method: "POST", path: "/security/redact-preview", summary: "Try the redaction rules on a text: `{text}` → `{text, count, kinds}`. Runs on a scratch vault, so the real one learns nothing; the text never leaves the node." },
    RouteDoc { method: "POST", path: "/security/guard-preview", summary: "Try the guard rules on a tool call: `{tool, input}` → `{verdict: allow | deny | ask | classify | fallthrough, rule?, label?, reason?, paths}`. The rules alone, as a session the platform drives would meet them — no classifier call, no restore, nothing recorded." },
];
