//! What the notes routes and the drawings routes share: the scope a caller
//! sends as two strings (`?scope=goal&id=…`, or in a body), the filter a
//! listing takes, and the 409 that hands back what is actually there.

use crate::{bad_request, ApiError};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use bisa_core::OwnerScope;
use bisa_engine::EngineError;
use bisa_store::{OwnerFilter, StoreError};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
pub(crate) struct ScopeQuery {
    /// One of `OwnerScope::KINDS`. Absent means every record.
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
}

/// Build the store's scope from the two strings a caller can send.
///
/// The pairing is checked rather than assumed: a standalone kind with an id,
/// and a record kind without one, are both refusals here rather than
/// something the store has to guess at. Whether the id names anything is
/// the store's question, and it answers it when the record is written.
pub(crate) fn scope_of(kind: &str, id: Option<&str>) -> Result<OwnerScope, ApiError> {
    let id = id.filter(|s| !s.trim().is_empty());
    if !OwnerScope::KINDS.contains(&kind) {
        return Err(bad_request(bisa_core::text!(
            "error-node-notes-unknown-scope-one",
            kind = format!("{kind:?}"),
            a0 = (OwnerScope::KINDS.join(", ")).to_string()
        )));
    }
    match (OwnerScope::kind_takes_id(kind), id) {
        (false, Some(_)) => Err(bad_request(bisa_core::text!(
            "error-node-notes-note-names-nothing-drop-id",
            kind = kind.to_string()
        ))),
        (true, None) => Err(bad_request(bisa_core::text!(
            "error-node-notes-scope-needs-id",
            kind = kind.to_string()
        ))),
        (_, id) => OwnerScope::from_parts(kind, id).ok_or_else(|| {
            bad_request(bisa_core::text!(
                "error-node-notes-id",
                kind = kind.to_string(),
                a0 = (id.unwrap_or("")).to_string()
            ))
        }),
    }
}

/// What a listing lists: nothing asked is every record; a kind alone is
/// every record of that kind (a tab); a kind with an id is one scope.
pub(crate) fn filter_of(kind: Option<&str>, id: Option<&str>) -> Result<OwnerFilter, ApiError> {
    let id = id.filter(|s| !s.trim().is_empty());
    match (kind, id) {
        (None, None) => Ok(OwnerFilter::All),
        (None, Some(_)) => Err(bad_request(bisa_core::text!(
            "error-node-notes-id-needs-scope"
        ))),
        (Some(kind), None) if OwnerScope::kind_takes_id(kind) => OwnerScope::KINDS
            .iter()
            .find(|k| **k == kind)
            .map(|k| OwnerFilter::Kind(k))
            .ok_or_else(|| {
                bad_request(bisa_core::text!(
                    "error-node-notes-unknown-scope-one",
                    kind = format!("{kind:?}"),
                    a0 = (OwnerScope::KINDS.join(", ")).to_string()
                ))
            }),
        (Some(kind), id) => Ok(OwnerFilter::Scope(scope_of(kind, id)?)),
    }
}

/// A conflict is a 409 that hands back what is actually there.
///
/// Built as a `Response` rather than an [`ApiError`], and that is the whole
/// reason this function exists: `ApiError` renders `{"error": "..."}`, and a
/// sentence is not something a client can merge from — it needs the body it
/// lost the race to: `current` is a note's body as a string, a drawing's
/// scene as an object, with `current_hash` beside it. The sentence still
/// rides along, as every refusal's does: `text` for the reader to say in its
/// language, `error` in the request's (17). Every other failure keeps the
/// mapping it already has.
pub(crate) fn conflict_or(e: EngineError, locale: &bisa_i18n::Locale) -> Response {
    match e {
        EngineError::Store(StoreError::EditConflict {
            current,
            current_hash,
            ..
        }) => {
            let text = bisa_core::text!("error-node-notes-changed-since-read");
            (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": bisa_i18n::render(locale, &text),
                    "text": text,
                    "current": current,
                    "current_hash": current_hash,
                })),
            )
                .into_response()
        }
        other => ApiError::from(other).into_response(),
    }
}
