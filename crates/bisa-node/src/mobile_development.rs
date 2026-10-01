//! Mobile development on this machine (ide/19): the toolchain as Settings
//! reads it, the devices as the IDE lists them, a device booted, shut down,
//! shown or made, its screen as a frame for the mirror or as a capture
//! stored beside the store, and — for the Tauri shell alone — the
//! `flutter run` line a terminal runs. The engine (`bisa_engine::mobile_development`)
//! asks the machine; nothing here spawns a program or writes the store.
//!
//! These routes are the person's: the switch refuses them with 409 while
//! mobile development is off, and a platform that is off hides its devices;
//! who among the agents may ask is checked at the op, not here.

use crate::route_docs::RouteDoc;
use crate::Query;
use crate::{bad_request, ApiError, Shared};
use axum::body::Body;
use axum::extract::{Path as AxPath, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use bisa_engine::mobile_development::{self, ImageFormat};
use serde::Deserialize;
use serde_json::json;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/mobile-development/status", get(status))
        .route("/mobile-development/status/check", post(check))
        .route("/mobile-development/devices", get(devices))
        .route("/mobile-development/devices/{id}/boot", post(boot))
        .route("/mobile-development/devices/{id}/shutdown", post(shutdown))
        .route("/mobile-development/devices/{id}/show", post(show))
        .route("/mobile-development/devices/{id}/frame", get(frame))
        .route(
            "/mobile-development/devices/{id}/screenshot",
            post(screenshot),
        )
        .route("/mobile-development/simulators", post(create_simulator))
        .route(
            "/workstreams/{wid}/mobile-development",
            get(workstream_mobile),
        )
        .route(
            "/workstreams/{wid}/mobile-development/run-command",
            get(workstream_run_command),
        )
}

pub const ROUTES: &[RouteDoc] = &[
    RouteDoc { method: "GET", path: "/mobile-development/status", summary: "What this machine has for mobile development (ide/19): `{enabled, platforms, toolchain: {flutter, xcode, ios_runtimes, ios_devicetypes, cocoapods, android, java, doctor}, hints, checked_at}` — the last examination, or one run now when none stands or the last is older than ten minutes. Read whether or not mobile development is on, so the person sees what to install first. 503 when this node was started without the mobile tools." },
    RouteDoc { method: "POST", path: "/mobile-development/status/check", summary: "Examine the machine again — *Check again* after a tool was installed — and answer the status. A `mobile_development_changed` frame with `what: toolchain` follows." },
    RouteDoc { method: "GET", path: "/mobile-development/devices", summary: "The simulators, emulators and phones this machine can reach, of the platforms it develops for: `{devices: [{id, name, platform, kind, state, os?}]}`, the ones that are up first. 409 while mobile development is off." },
    RouteDoc { method: "POST", path: "/mobile-development/devices/{id}/boot", summary: "Boot a simulator, or start an emulator's image; answers the device once it is up. A phone is not booted from here (400). 404 for an id nobody answers to, 409 for a device of a platform that is off. A `mobile_development_changed` frame with `what: devices` follows." },
    RouteDoc { method: "POST", path: "/mobile-development/devices/{id}/shutdown", summary: "Shut a simulator or an emulator down; answers the device as it stands." },
    RouteDoc { method: "POST", path: "/mobile-development/devices/{id}/show", summary: "Bring the device's window forward — the Simulator app for an iOS simulator; an emulator has a window of its own (400)." },
    RouteDoc { method: "GET", path: "/mobile-development/devices/{id}/frame", summary: "The device's screen as it is now, as image bytes for the IDE's mirror — never stored: `?format=jpeg` (the default) or `png` for a simulator, PNG always for Android. `Cache-Control: no-store`. 409 while the device's platform is off; 400 for a device that is not up." },
    RouteDoc { method: "POST", path: "/mobile-development/devices/{id}/screenshot", summary: "Capture the device's screen as a PNG, stored as an attachment: `{attachment: AttachmentRef, path, width?, height?}` — the path is a named copy beside the store, what a capture the person marks carries and what an agent reads." },
    RouteDoc { method: "POST", path: "/mobile-development/simulators", summary: "Make an iOS simulator: `{name, devicetype, runtime}` — a device type and a runtime the status lists. Answers the device (201). 409 while iOS is off." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/mobile-development", summary: "What this checkout holds for mobile: `{flutter, ios, android}` — a `pubspec.yaml` naming Flutter, and the platform folders beside it. The IDE shows its Devices button on a Flutter app alone." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/mobile-development/run-command", summary: "The line a terminal runs to put this checkout's app on a device: `?device=<id>` → `{command, cwd, device}` — the resolved `flutter`, `run -d <id>`, in the checkout. Read by the desktop's shell alone, which names no program of its own (ide/01); the desktop's page never calls it. 409 while mobile development is off; 503 when Flutter is not installed." },
];

#[derive(Deserialize)]
struct FrameQuery {
    #[serde(default)]
    format: Option<String>,
}

#[derive(Deserialize)]
struct RunCommandQuery {
    #[serde(default)]
    device: String,
}

async fn status(State(state): State<Shared>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!(
        mobile_development::status(state.engine.inner(), false).await?
    )))
}

async fn check(State(state): State<Shared>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!(
        mobile_development::status(state.engine.inner(), true).await?
    )))
}

async fn devices(State(state): State<Shared>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        json!({"devices": mobile_development::devices(state.engine.inner(), None).await?}),
    ))
}

async fn boot(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!(
        mobile_development::boot(state.engine.inner(), None, &id).await?
    )))
}

async fn shutdown(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!(
        mobile_development::shutdown(state.engine.inner(), None, &id).await?
    )))
}

async fn show(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!(
        mobile_development::show(state.engine.inner(), None, &id).await?
    )))
}

async fn frame(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    Query(q): Query<FrameQuery>,
) -> Result<Response, ApiError> {
    let format = match q.format.as_deref() {
        None | Some("jpeg") | Some("jpg") => ImageFormat::Jpeg,
        Some("png") => ImageFormat::Png,
        Some(other) => {
            return Err(bad_request(bisa_core::text!(
                "error-node-mobile-development-format-png-jpeg",
                other = format!("{other:?}")
            )))
        }
    };
    let (bytes, format) =
        mobile_development::frame(state.engine.inner(), None, &id, format).await?;
    // The type the bytes say, never the one asked for: Android answers PNG.
    let mime = crate::attachments::image_type(&bytes).unwrap_or_else(|| format.mime());
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "no-store"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        Body::from(bytes),
    )
        .into_response())
}

async fn screenshot(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!(
        mobile_development::screenshot(state.engine.inner(), None, &id).await?
    )))
}

async fn create_simulator(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<crate::dto::CreateSimulatorBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let device = mobile_development::create_simulator(
        state.engine.inner(),
        &body.name,
        &body.devicetype,
        &body.runtime,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(json!(device))))
}

async fn workstream_mobile(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = crate::projects::parse_workstream_id(&wid)?;
    Ok(Json(json!(mobile_development::project_facts(
        state.engine.inner(),
        wid
    )?)))
}

async fn workstream_run_command(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    Query(q): Query<RunCommandQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = crate::projects::parse_workstream_id(&wid)?;
    Ok(Json(json!(mobile_development::run_command(
        state.engine.inner(),
        wid,
        &q.device
    )?)))
}
