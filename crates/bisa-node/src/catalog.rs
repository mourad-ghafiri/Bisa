//! The catalog over HTTP: the staff a workspace *may* have, and the one call
//! that gives it one.
//!
//! A fresh workspace has exactly one agent — the platform's own, created
//! inside `Workspace::open` — and nothing else. Everything the previous
//! milestone seeded is here instead: browsable, tagged, and installed only
//! because somebody chose it. So these three routes are the whole difference
//! between a workspace that starts as an organisation nobody hired and one
//! that starts as an offer.
//!
//! An install is the store's, including its transitivity (a team brings its
//! agents, an agent its skills), its idempotence, and its refusal to overwrite
//! an id it did not create. This module's job is to name a kind and a slug and
//! to let those decisions reach the caller intact: a collision is a 400
//! carrying the store's message, which names the id and what holds it, because
//! "install failed" leaves the owner to guess whether their own work is at
//! risk.

use crate::dto::*;
use crate::i18n::Lang;
use crate::route_docs::RouteDoc;
use crate::tags::TagFilter;
use crate::Query;
use crate::{bad_request, ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use bisa_store::{CatalogEntry, CatalogKind};
use serde::Deserialize;
use serde_json::json;
use std::str::FromStr;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/catalog", get(list))
        .route("/catalog/install", post(install))
        .route("/catalog/{kind}/{slug}", get(detail))
}

#[derive(Deserialize)]
struct KindQuery {
    #[serde(default)]
    kind: Option<String>,
}

/// One of `CatalogKind::ALL`, or a 400 that spells every kind out.
///
/// The store's `FromStr` refuses an unknown kind too, but its message repeats
/// what was typed without saying what would have worked. This is the surface
/// where the typo is made — a hand-written `?kind=agents` — so the answer is
/// the vocabulary, built from `CatalogKind::ALL` so a new kind cannot leave
/// the message behind.
fn parse_kind(raw: &str) -> Result<CatalogKind, ApiError> {
    CatalogKind::from_str(raw).map_err(|_| {
        let valid: Vec<&str> = CatalogKind::ALL.iter().map(|k| k.as_str()).collect();
        bad_request(bisa_core::text!(
            "error-node-catalog-unknown-catalog-kind-use-one",
            raw = format!("{raw:?}"),
            a0 = (valid.join(", ")).to_string()
        ))
    })
}

/// The catalog, whole or narrowed by `?kind=` and `?tag=`.
///
/// Every row carries `installed`, so one request draws the picker and its
/// checkmarks; a client that had to diff the catalog against `/agents`,
/// `/skills`, `/teams` and `/channels` would get four races instead.
async fn list(
    State(state): State<Shared>,
    Lang(locale): Lang,
    Query(q): Query<KindQuery>,
    filter: TagFilter,
) -> Result<Json<serde_json::Value>, ApiError> {
    let kind = q
        .kind
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(parse_kind)
        .transpose()?;
    let entries: Vec<CatalogEntryDto> = state
        .engine
        .workspace()
        .catalog_entries(kind)?
        .into_iter()
        .filter(|e| filter.matches(&e.tags))
        .map(|e| localized(e, &locale))
        .collect();
    Ok(Json(json!({"entries": entries})))
}

/// One entry, for the panel a picker opens before committing to an install.
///
/// A slug this build does not ship is the store's error rather than an empty
/// answer: asking for a name that is not in the catalog is a bug in the
/// caller, not an absence in the workspace.
async fn detail(
    State(state): State<Shared>,
    Lang(locale): Lang,
    AxPath((kind, slug)): AxPath<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let entry = state
        .engine
        .workspace()
        .catalog_entry(parse_kind(&kind)?, &slug)?;
    Ok(Json(json!({"entry": localized(entry, &locale)})))
}

/// The row in the request's language: its name and description through the
/// content seam (`catalog-<kind>-<slug>`, 17 — Internationalisation), the
/// file's own words when no translation ships.
fn localized(entry: CatalogEntry, locale: &bisa_i18n::Locale) -> CatalogEntryDto {
    let id = bisa_i18n::content_id(entry.kind.as_str(), &entry.slug);
    let mut dto = CatalogEntryDto::from(entry);
    dto.name = bisa_i18n::content(locale, &id, None, &dto.name);
    dto.description = bisa_i18n::content(locale, &id, Some("description"), &dto.description);
    dto
}

/// Install one entry and everything it needs.
///
/// **200 with what was created**, never 201 or 204: an install is transitive,
/// so the interesting answer is the list of six things a one-line request
/// produced, and an owner who cannot see them cannot undo them. Installing
/// something already present is a 200 with every list empty — idempotence is
/// the contract, not an error condition.
async fn install(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<CatalogInstallBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let kind = parse_kind(&body.kind)?;
    let installed =
        bisa_engine::admin::install_catalog_entry(state.engine.inner(), kind, &body.slug)?;
    Ok(Json(json!({"installed": InstalledDto::from(installed)})))
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc {
        method: "GET",
        path: "/catalog",
        summary:
            "The catalog, whole or narrowed by `?kind=` and `?tag=`, marking what is installed; a workflow template's row carries its whole definition (`workflow`: inputs, steps, flows), so a gallery draws its graph before it is installed.",
    },
    RouteDoc {
        method: "POST",
        path: "/catalog/install",
        summary: "Install one entry and everything it needs: `{kind, slug}`.",
    },
    RouteDoc {
        method: "GET",
        path: "/catalog/{kind}/{slug}",
        summary: "One catalog entry, for the panel a picker opens before installing — a workflow template's with its definition.",
    },
];
