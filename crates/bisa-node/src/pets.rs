//! Pets: list the built-ins and what is installed, install a package, serve a
//! sprite sheet. The nine the platform ships come from the binary
//! (`bisa_store::CATALOG.pets`): listed first with `origin: catalog`, their
//! sheets served from the bundle, a delete of one refused with **409**.
//!
//! # Why the sheet comes over HTTP
//!
//! The webview has no filesystem access — no `plugin-fs`, no `asset://`, and
//! `capabilities/default.json` grants a folder picker and Reveal in Finder and
//! nothing else, on purpose. So bytes reach the UI the way every other byte
//! does: over this API. `GET /pets/{id}/sprite` is to a pet what
//! `GET /attachments/{hash}` is to a photo, and the desktop points a CSS
//! `background-image` at it exactly as `Chat.tsx` points an `<img>` at the
//! other one.
//!
//! # Serving an image into a webview with no CSP
//!
//! `tauri.conf.json` sets `"csp": null`, so whatever this route says about a
//! file's type, the webview believes. A route that echoed a stored manifest's
//! claim would therefore be a scripting vector: install a package whose
//! "sprite sheet" is HTML and open its URL.
//!
//! So the type is decided by the **bytes**: `image/webp` only when the file
//! begins `RIFF….WEBP`, and `application/octet-stream` with
//! `Content-Disposition: attachment` otherwise. `nosniff` either way. The
//! store already refuses a non-WebP at install, which makes this the second
//! check on the same fact — deliberately, because the first one guards what
//! arrives and this one guards what is served, and a file on disk can change
//! between the two.

use crate::route_docs::RouteDoc;
use crate::{bad_request, not_found, ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get};
use axum::{Json, Router};
use bisa_core::PetOrigin;
use bisa_store::PetSprite;
use serde_json::json;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/pets", get(list).post(install))
        .route("/pets/{id}", delete(remove))
        .route("/pets/{id}/sprite", get(sprite))
}

async fn list(
    State(state): State<Shared>,
    crate::i18n::Lang(locale): crate::i18n::Lang,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pets: Vec<bisa_core::Pet> = state
        .engine
        .workspace()
        .list_pets()?
        .into_iter()
        .map(|p| localized(p, &locale))
        .collect();
    Ok(Json(json!({ "pets": pets })))
}

/// A pet's display fields in the request's language — its display name,
/// description and tagline through the content seam (`catalog-pet-<id>`,
/// 17 — Internationalisation), the package's own words when no translation ships.
fn localized(mut pet: bisa_core::Pet, locale: &bisa_i18n::Locale) -> bisa_core::Pet {
    let id = bisa_i18n::content_id("pet", &pet.id);
    pet.display_name = bisa_i18n::content(locale, &id, None, &pet.display_name);
    pet.description = bisa_i18n::content(locale, &id, Some("description"), &pet.description);
    if let Some(flavour) = pet.flavour.as_mut() {
        if let Some(tagline) = flavour.tagline.as_deref() {
            flavour.tagline = Some(bisa_i18n::content(locale, &id, Some("tagline"), tagline));
        }
    }
    pet
}

/// `POST /pets {path}` — copy a package in from a folder on this machine.
///
/// The path is validated the way an adopted project's is: absolute,
/// canonicalized, must exist, and **refused if it is inside the workspace**.
/// That last one matters here for a reason it does not for a project — a
/// package copied from inside `pets/` would be a pet installing itself.
async fn install(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<crate::dto::NewPetBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let root = state.engine.workspace().root().to_path_buf();
    let from = crate::projects::resolve_source_path(&root, &body.path, "install-pet")
        .map_err(bad_request)?;
    let pet = bisa_engine::admin::install_pet(state.engine.inner(), std::path::Path::new(&from))?;
    Ok(Json(json!({ "pet": pet })))
}

/// `DELETE /pets/{id}` — a person's package goes; a built-in is a state the
/// delete meets, not a malformed ask: **409**, put it away instead.
async fn remove(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if let Ok(pet) = state.engine.workspace().get_pet(&id) {
        if pet.origin == PetOrigin::Catalog {
            return Err(ApiError::text(
                StatusCode::CONFLICT,
                bisa_core::text!(
                    "error-node-pets-ships-with-platform-not-removed-put-away",
                    a0 = (pet.display_name).to_string()
                ),
            ));
        }
    }
    bisa_engine::admin::remove_pet(state.engine.inner(), &id)?;
    Ok(Json(json!({ "ok": true })))
}

/// `GET /pets/{id}/sprite` — the sheet, typed from its own bytes.
async fn sprite(State(state): State<Shared>, AxPath(id): AxPath<String>) -> Response {
    let ws = state.engine.workspace();
    let bytes = match ws.pet_sprite(&id) {
        Ok(PetSprite::Bundled(sheet)) => sheet.to_vec(),
        Ok(PetSprite::File(path)) => match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => {
                return not_found(bisa_core::text!(
                    "error-node-pets-pet-has-no-readable-sprite-sheet"
                ))
                .into_response()
            }
        },
        Err(e) => return ApiError::from(e).into_response(),
    };

    // The one claim this route makes about the file, and it is made from the
    // file rather than from the manifest that named it.
    let webp = bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP";
    let (kind, disposition) = if webp {
        ("image/webp", "inline")
    } else {
        ("application/octet-stream", "attachment")
    };

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, kind),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        bytes,
    )
        .into_response()
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc {
        method: "GET",
        path: "/pets",
        summary: "Every pet: the built-ins the platform ships (`origin: catalog`, Moonrice among them) first, then the packages installed here; each with its animations and who it is.",
    },
    RouteDoc {
        method: "POST",
        path: "/pets",
        summary: "Install a pet package from a folder on this machine: `{path}`.",
    },
    RouteDoc {
        method: "DELETE",
        path: "/pets/{id}",
        summary: "Remove an installed pet; a built-in is refused with 409 — put it away instead.",
    },
    RouteDoc {
        method: "GET",
        path: "/pets/{id}/sprite",
        summary: "The sprite sheet — the bundle's for a built-in, the package's for an installed pet — typed from its own bytes.",
    },
];
