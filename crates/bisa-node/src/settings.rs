//! Settings over HTTP.
//!
//! Three scopes, one registry. `GET /settings/registry` is what lets the
//! desktop generate its panels from the same list the node resolves with, so
//! an origin badge cannot drift from the resolution; `GET /settings/resolved`
//! is every key with its value and where it came from; `GET` and `PUT` on a
//! scope are the raw layer. A write to a scope the key does not allow is a
//! `400` naming the allowed scopes — refused, never stored and ignored.
//!
//! Writes go through the engine, which announces `settings.changed` on the
//! bus (rule 2 of the layering).

use crate::dto::{SettingDefDto, SettingsWriteBody};
use crate::i18n::Lang;
use crate::route_docs::RouteDoc;
use crate::Query;
use crate::{bad_request, ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::routing::get;
use axum::{Json, Router};
use bisa_core::Localize as _;
use bisa_core::{ProjectId, SettingScope};
use serde::Deserialize;
use serde_json::json;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/settings/registry", get(registry))
        .route("/settings/resolved", get(resolved))
        .route("/settings/{scope}", get(layer).put(write_layer))
        .route("/settings/{scope}/{key}", axum::routing::delete(unset))
}

#[derive(Deserialize)]
pub(crate) struct ProjectQuery {
    /// The project whose scope is meant; required for `project`, ignored
    /// otherwise. Also what `resolved` resolves for.
    project: Option<String>,
}

fn parse_scope(raw: &str) -> Result<SettingScope, ApiError> {
    match raw {
        "machine" => Ok(SettingScope::Machine),
        "workspace" => Ok(SettingScope::Workspace),
        "project" => Ok(SettingScope::Project),
        other => Err(bad_request(bisa_core::text!(
            "error-node-settings-unknown-settings-scope-machine-workspace-project",
            other = format!("{other:?}")
        ))),
    }
}

fn parse_project(q: &ProjectQuery) -> Result<Option<ProjectId>, ApiError> {
    match &q.project {
        None => Ok(None),
        Some(raw) => raw.parse::<ProjectId>().map(Some).map_err(|_| {
            bad_request(bisa_core::text!(
                "error-node-settings-not-project-id",
                raw = format!("{raw:?}")
            ))
        }),
    }
}

/// The registry with every word — a label, a help, a choice's word — in the
/// request's language (`Accept-Language`); the registry itself carries none.
async fn registry(Lang(locale): Lang) -> Json<serde_json::Value> {
    let defs: Vec<SettingDefDto> = bisa_core::SETTINGS
        .iter()
        .map(|d| SettingDefDto::localized(d, &locale))
        .collect();
    Json(json!({"settings": defs}))
}

async fn resolved(
    State(state): State<Shared>,
    Query(q): Query<ProjectQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let project = parse_project(&q)?;
    let rows = state.engine.workspace().settings(project)?;
    Ok(Json(
        json!({"project": project.map(|p| p.to_string()), "settings": rows}),
    ))
}

async fn layer(
    State(state): State<Shared>,
    AxPath(scope): AxPath<String>,
    Query(q): Query<ProjectQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let scope = parse_scope(&scope)?;
    let project = parse_project(&q)?;
    let values = state.engine.workspace().settings_layer(scope, project)?;
    Ok(Json(json!({"scope": scope.as_str(), "values": values})))
}

async fn write_layer(
    State(state): State<Shared>,
    AxPath(scope): AxPath<String>,
    Query(q): Query<ProjectQuery>,
    crate::Body(body): crate::Body<SettingsWriteBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let scope = parse_scope(&scope)?;
    let project = parse_project(&q)?;
    // Check the whole body before writing any of it: a batch that is half
    // applied is worse than one refused.
    for (key, value) in &body.values {
        // The core's own sentence, for the reader's language — never its
        // `Display`, which is the developer's.
        bisa_core::check_setting_write(key, scope, value).map_err(|e| bad_request(e.text()))?;
    }
    let mut written = Vec::new();
    for (key, value) in body.values {
        written.push(state.engine.set_setting(scope, project, &key, value)?);
    }
    Ok(Json(json!({"scope": scope.as_str(), "settings": written})))
}

async fn unset(
    State(state): State<Shared>,
    AxPath((scope, key)): AxPath<(String, String)>,
    Query(q): Query<ProjectQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let scope = parse_scope(&scope)?;
    let project = parse_project(&q)?;
    let resolved = state.engine.unset_setting(scope, project, &key)?;
    Ok(Json(json!({"scope": scope.as_str(), "setting": resolved})))
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc {
        method: "GET",
        path: "/settings/registry",
        summary: "Every setting the registry declares: key, kind, default, allowed scopes, label, help.",
    },
    RouteDoc {
        method: "GET",
        path: "/settings/resolved",
        summary: "Every key with its value and the scope it came from (`?project=` resolves for a project).",
    },
    RouteDoc {
        method: "GET",
        path: "/settings/{scope}",
        summary: "The raw values held at one scope: `machine`, `workspace` or `project` (`?project=`).",
    },
    RouteDoc {
        method: "PUT",
        path: "/settings/{scope}",
        summary: "Write `{values: {key: value}}` at one scope; 400 names a key's allowed scopes or a value out of range.",
    },
    RouteDoc {
        method: "DELETE",
        path: "/settings/{scope}/{key}",
        summary: "Remove one key from one scope; the value falls back to the next layer.",
    },
];
