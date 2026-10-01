//! The one place a [`HumanConsent`] is minted (ide/04).
//!
//! A consented git operation needs proof that a person asked. The proof is
//! the request itself: it carried the workspace's bearer token, which only a
//! process able to read `run/token` on this machine can present — the CLI at
//! a terminal, the desktop shell — and never an agent session, whose only
//! path into the engine is the MCP intake, which has no field for one.
//!
//! The middleware already checked the token to let the request in; this
//! reads it again on purpose, so minting is tied to evidence in hand and not
//! to "this handler is behind the middleware", which is a property of a
//! router configuration a future edit could change.
//!
//! Only the `Authorization` header counts. A token in the query string is
//! what an `EventSource` sends, because it cannot set a header — the one
//! place the middleware accepts it — and a URL is copied, logged and sent as
//! a referrer in ways a header is not; a person's click on a destructive
//! verb is not proved by it.

use crate::auth;
use crate::{ApiError, Shared};
use axum::http::{HeaderMap, StatusCode};
use bisa_vcs::HumanConsent;

/// Mint consent from an authenticated request, or refuse with 401.
pub(crate) fn from_request(headers: &HeaderMap, state: &Shared) -> Result<HumanConsent, ApiError> {
    match auth::presented_in_header(headers) {
        Some(token) if auth::same(&token, &state.token) => Ok(HumanConsent::mint()),
        _ => Err(ApiError::text(StatusCode::UNAUTHORIZED, bisa_core::text!("error-node-ide-consent-consented-git-operation-needs-workspace-token-authorization"))),
    }
}
