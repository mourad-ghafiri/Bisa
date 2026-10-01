//! Addons over HTTP: what is installed, importing a folder, the switches, the
//! network broker — and the one route on the node that answers **without
//! the token**: an installed bundle's files, for the frame that runs them
//! ([18 — Addons](../../../docs/architecture/18-addons.md)).
//!
//! # Why the files route is open
//!
//! An addon runs in `<iframe sandbox="allow-scripts" src=…>` with an opaque
//! origin: it holds no token, and a token in its URL would be a token it
//! could read. So its files are served to anyone who names the addon and the
//! path — which is the bundle a person installed and nothing else. What
//! keeps that surface small is spelled here and nowhere else: `GET` alone;
//! the id parsed as an [`AddonId`] and the addon **active** (enabled, files
//! here) or a 404 that says nothing more; the path judged by
//! `is_bundle_path` and resolved inside the bundle by `resolve_within`; the
//! type decided from the extension allowlist, else `application/octet-stream`
//! as a download; `nosniff`; and a Content-Security-Policy header on every
//! answer that lets the page load its own files and reach nothing — the
//! frame's sandbox is the first wall, this header the second, and neither
//! depends on the other.
//!
//! The library (`bisa-addon.js`) is served at every addon's root from the
//! binary, so a bundle cannot ship a tampered one — the store refuses a
//! file of that name at install.

use crate::dto::*;
use crate::i18n::Lang;
use crate::route_docs::RouteDoc;
use crate::{addon_id, bad_request, not_found, ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::http::{header, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use bisa_core::addon::{served_content_type, SDK_FILE_NAME};
use bisa_core::AddonId;
use bisa_engine::addons as engine_addons;
use bisa_engine::admin;
use serde_json::json;

/// The policy every bundle file is served under: the page may load its own
/// files and inline what it carries, draw images and fonts it holds or
/// encodes, and reach **nothing** — no fetch, no frame, no form, no base.
/// `sandbox allow-scripts` repeats the frame's own sandbox, for a document
/// somebody opens outside the desktop.
pub const ADDON_FILES_CSP: &str = "default-src 'none'; script-src 'self' 'unsafe-inline'; \
style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; \
media-src 'self' data: blob:; connect-src 'none'; frame-src 'none'; form-action 'none'; \
base-uri 'none'; sandbox allow-scripts";

/// The library, compiled in — one copy, the platform's.
pub const SDK_SOURCE: &str = include_str!("../../../addons/sdk/bisa-addon.js");

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/addons", get(list).post(import))
        .route("/addons/offer", get(offer))
        .route("/addons/validate", post(validate))
        .route("/addons/{id}", get(one).patch(patch).delete(remove))
        .route("/addons/{id}/fetch", post(fetch))
        .route("/addons/{id}/files/{*path}", get(file))
}

/// An installed addon in the request's language: its name and description
/// through the content seam (`catalog-addon-<id>`, 17 — Internationalisation),
/// the manifest's own words when no translation ships.
fn localized(entry: bisa_store::AddonEntry, locale: &bisa_i18n::Locale) -> AddonDto {
    let id = bisa_i18n::content_id("addon", entry.id().as_str());
    let mut dto = AddonDto::from(entry);
    dto.manifest.name = bisa_i18n::content(locale, &id, None, &dto.manifest.name);
    dto.manifest.description =
        bisa_i18n::content(locale, &id, Some("description"), &dto.manifest.description);
    dto
}

async fn list(
    State(state): State<Shared>,
    Lang(locale): Lang,
) -> Result<Json<AddonsDto>, ApiError> {
    let addons = state
        .engine
        .workspace()
        .list_addons()?
        .into_iter()
        .map(|a| localized(a, &locale))
        .collect();
    Ok(Json(AddonsDto {
        addons,
        addons_enabled: engine_addons::addons_enabled(state.engine.inner()),
    }))
}

/// `POST /addons {path, granted?, enabled?}` — copy a folder in, with the
/// grants and the switch the person chose in the review. The path is
/// validated the way an adopted project's is: absolute, canonicalized,
/// existing, and never inside the workspace.
async fn import(
    State(state): State<Shared>,
    Lang(locale): Lang,
    crate::Body(body): crate::Body<NewAddonBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let root = state.engine.workspace().root().to_path_buf();
    let from = crate::projects::resolve_source_path(&root, &body.path, "import-addon")
        .map_err(bad_request)?;
    let addon = admin::install_addon(
        state.engine.inner(),
        std::path::Path::new(&from),
        body.granted,
        body.enabled,
    )?;
    Ok(Json(json!({ "addon": localized(addon, &locale) })))
}

/// `GET /addons/offer` — the built-ins the catalog offers, each with its
/// manifest and whether it is here: what the review reads before an install.
async fn offer(
    State(state): State<Shared>,
    Lang(locale): Lang,
) -> Result<Json<AddonOffersDto>, ApiError> {
    let offers = state
        .engine
        .workspace()
        .list_addon_offers()?
        .into_iter()
        .map(|o| {
            let id = bisa_i18n::content_id("addon", &o.slug);
            let mut dto = AddonOfferDto::from(o);
            dto.manifest.name = bisa_i18n::content(&locale, &id, None, &dto.manifest.name);
            dto.manifest.description =
                bisa_i18n::content(&locale, &id, Some("description"), &dto.manifest.description);
            dto
        })
        .collect();
    Ok(Json(AddonOffersDto { offers }))
}

/// `POST /addons/validate {path}` — the folder's manifest and every problem
/// it has as an addon, writing nothing: what the import dialog shows before
/// the person decides.
async fn validate(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<ValidateAddonBody>,
) -> Result<Json<AddonProblemsDto>, ApiError> {
    let root = state.engine.workspace().root().to_path_buf();
    let from = crate::projects::resolve_source_path(&root, &body.path, "validate-addon")
        .map_err(bad_request)?;
    let (manifest, problems) = state
        .engine
        .workspace()
        .validate_addon_dir(std::path::Path::new(&from))?;
    Ok(Json(AddonProblemsDto { manifest, problems }))
}

async fn one(
    State(state): State<Shared>,
    Lang(locale): Lang,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let addon = state.engine.workspace().get_addon(&addon_id(&id)?)?;
    Ok(Json(json!({ "addon": localized(addon, &locale) })))
}

/// `PATCH /addons/{id} {enabled?, granted?}` — the switch first, then the
/// grants; each through its own engine door so each is said on the bus.
async fn patch(
    State(state): State<Shared>,
    Lang(locale): Lang,
    AxPath(id): AxPath<String>,
    crate::Body(body): crate::Body<AddonPatchBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = addon_id(&id)?;
    if body.enabled.is_none() && body.granted.is_none() {
        return Err(bad_request(bisa_core::text!(
            "error-node-addons-nothing-to-patch"
        )));
    }
    let inner = state.engine.inner();
    let mut addon = None;
    if let Some(enabled) = body.enabled {
        addon = Some(admin::set_addon_enabled(inner, &id, enabled)?);
    }
    if let Some(granted) = body.granted {
        addon = Some(admin::set_addon_grants(inner, &id, granted)?);
    }
    let addon = match addon {
        Some(a) => a,
        None => state.engine.workspace().get_addon(&id)?,
    };
    Ok(Json(json!({ "addon": localized(addon, &locale) })))
}

async fn remove(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    admin::remove_addon(state.engine.inner(), &addon_id(&id)?)?;
    Ok(Json(json!({ "ok": true })))
}

/// `POST /addons/{id}/fetch {url, accept?}` — the network broker: the one
/// way an addon reaches the internet, judged in the engine.
async fn fetch(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::Body(body): crate::Body<AddonFetchBody>,
) -> Result<Json<engine_addons::AddonFetchResult>, ApiError> {
    let result = engine_addons::addon_fetch(
        state.engine.inner(),
        &addon_id(&id)?,
        &body.url,
        body.accept.as_deref(),
    )
    .await?;
    Ok(Json(result))
}

/// The one answer a token-less caller gets for anything that is not a file
/// of an active addon: a 404 that names no reason.
fn no_file() -> ApiError {
    not_found(bisa_core::text!("error-node-addons-file-not-found"))
}

/// `GET /addons/{id}/files/{*path}` — a bundle file, served under the walls
/// the module doc spells. Answers without the token.
async fn file(
    State(state): State<Shared>,
    AxPath((id, path)): AxPath<(String, String)>,
) -> Response {
    match serve_file(&state, &id, &path).await {
        Ok(response) => response,
        Err(e) => e.into_response(),
    }
}

async fn serve_file(state: &Shared, id: &str, path: &str) -> Result<Response, ApiError> {
    let id = AddonId::new(id).map_err(|_| no_file())?;
    let ws = state.engine.workspace();
    let addon = ws.get_addon(&id).map_err(|_| no_file())?;
    if !addon.is_active() || !engine_addons::addons_enabled(state.engine.inner()) {
        return Err(no_file());
    }
    let path = path.trim_start_matches('/');
    let (bytes, name): (Vec<u8>, String) = if path == SDK_FILE_NAME {
        (SDK_SOURCE.as_bytes().to_vec(), SDK_FILE_NAME.to_string())
    } else {
        let resolved = ws.get_addon_file(&id, path).map_err(|_| no_file())?;
        let meta = std::fs::symlink_metadata(&resolved).map_err(|_| no_file())?;
        if !meta.is_file() {
            return Err(no_file());
        }
        let bytes = std::fs::read(&resolved).map_err(|_| no_file())?;
        let name = resolved
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        (bytes, name)
    };
    let (content_type, disposition) = match served_content_type(&name) {
        Some(ty) => (ty, "inline"),
        None => ("application/octet-stream", "attachment"),
    };
    let mut response = (StatusCode::OK, bytes).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static(disposition),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(ADDON_FILES_CSP),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        HeaderName::from_static("cross-origin-resource-policy"),
        HeaderValue::from_static("cross-origin"),
    );
    Ok(response)
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc {
        method: "GET",
        path: "/addons",
        summary: "Every addon installed here: `{addons: [{id, manifest, origin, enabled, granted, installed_at, files_present, active}], addons_enabled}` — the manifest as installed, where it came from, whether it runs, what the person granted (a subset of what the manifest declares), whether its files are on this machine, and the machine's `addons.enabled` switch.",
    },
    RouteDoc {
        method: "POST",
        path: "/addons",
        summary: "Import an addon from a folder on this machine: `{path, granted?: [permission], enabled?}` — the grants and the switch the person chose in the review; nothing is granted by default. The folder is validated whole before a byte is copied; a built-in's id is refused.",
    },
    RouteDoc {
        method: "GET",
        path: "/addons/offer",
        summary: "The built-in addons the catalog offers: `{offers: [{slug, manifest, installed}]}` — each manifest whole, so the review shows what an addon asks for before it is installed (`POST /catalog/install {kind: \"addon\", slug}`).",
    },
    RouteDoc {
        method: "POST",
        path: "/addons/validate",
        summary: "What is wrong with a folder as an addon, writing nothing: `{path}` → `{manifest, problems: [{field?, text}]}` — the manifest as read, and the manifest's rules and the bundle's.",
    },
    RouteDoc {
        method: "GET",
        path: "/addons/{id}",
        summary: "One installed addon, as a row of `GET /addons`.",
    },
    RouteDoc {
        method: "PATCH",
        path: "/addons/{id}",
        summary: "`{enabled?, granted?}` — turn the addon on or off (on needs its files here), or replace what it is granted: a subset of what it declares, exactly as declared, else 400 naming the word. Answers the addon.",
    },
    RouteDoc {
        method: "DELETE",
        path: "/addons/{id}",
        summary: "Remove an addon: its record, its bundle, its snapshot. A built-in installed from the catalog is removed like any other and can be installed again.",
    },
    RouteDoc {
        method: "POST",
        path: "/addons/{id}/fetch",
        summary: "The network broker: `{url, accept?}` → `{status, content_type?, body_text, truncated}`. Refused by name unless the machine's switch is on, the addon is active and granted `network`, the URL is `https://` to a host the grant names and the person's `security.net.*` lists allow, and never this machine; GET only, no redirect followed, the body cut at 1 MiB.",
    },
    RouteDoc {
        method: "GET",
        path: "/addons/{id}/files/{*path}",
        summary: "A file of an installed bundle, for the frame that runs it — **answers without the token**: GET only, an active addon or a bare 404, the path resolved inside the bundle, the type from an extension allowlist (else a download), `nosniff`, `Cache-Control: no-store`, and a Content-Security-Policy that lets the page load its own files and reach nothing. `bisa-addon.js` at the root is the platform's library, served from the binary.",
    },
];
