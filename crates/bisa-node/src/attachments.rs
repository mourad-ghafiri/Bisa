//! The bytes behind a message's files: in, out, and fetched from a peer.
//!
//! # Why upload and post are two calls
//!
//! A message names its files by content hash; the bytes arrive first, on their
//! own. That is not ceremony. A post that carried the bytes would re-send them
//! every time it was retried, would put a 25 MB body on a route whose other
//! callers send a sentence, and would give the composer nothing to show while a
//! photo uploads. Two calls also mean attaching the same file twice costs one
//! transfer, because the store is content-addressed and the second upload finds
//! it already there.
//!
//! # Serving bytes into a webview that has no CSP
//!
//! `tauri.conf.json` sets `"csp": null`, so anything this route says about a
//! file's type, the webview believes. A route that echoed a caller-declared
//! `Content-Type` would therefore be a scripting vector: upload an HTML file
//! declared as `text/html`, open its URL, and it runs with the app's origin.
//!
//! So the default is `application/octet-stream` with `nosniff` and
//! `Content-Disposition: attachment` — a download, never a page. Rendering an
//! image inline is the one exception, and it is granted on the evidence of the
//! **bytes** rather than on anyone's claim about them: `?as=image` reads the
//! file's magic number and serves the type that matches, or refuses. The same
//! rule `bisa_store::tree` follows when it decides a file is binary
//! *"from the bytes, never from the extension"*.

use crate::route_docs::RouteDoc;
use crate::Query;
use crate::{bad_request, not_found, ApiError, Shared};
use axum::body::Body;
use axum::extract::{Path as AxPath, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
/// The image sniff every photo passes — the core's (ide/14 §Photos).
pub(crate) use bisa_core::image_type;
use bisa_core::Localize as _;
use bisa_core::{AttachmentRef, PhotoProfile, MAX_ATTACHMENT_BYTES};
use futures::StreamExt as _;
use serde::Deserialize;
use serde_json::json;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/attachments",
            // The one route in this node that may take more than a sentence.
            // Nothing else raises the framework's default, so the cap below is
            // the only ceiling — which is why it is enforced twice.
            post(upload).layer(axum::extract::DefaultBodyLimit::disable()),
        )
        .route("/attachments/{sha256}", get(download))
        .route("/attachments/{sha256}/fetch", post(fetch_from_peer))
        .route("/attachments/{sha256}/file", post(named_file))
        .route("/artifacts/{scope}", get(list_artifacts))
}

#[derive(Deserialize)]
struct ListQuery {
    limit: Option<usize>,
}

/// `POST /attachments/{sha256}/file {name}` — the blob under a real name, made
/// on demand, so a file manager can reveal it and the default application
/// open it. The path comes back for the desktop shell, which alone may hand
/// it to the platform; the node writes it inside its own store and nowhere
/// else.
async fn named_file(
    State(state): State<Shared>,
    AxPath(sha256): AxPath<String>,
    crate::Body(body): crate::Body<crate::dto::NamedFileBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !bisa_core::AttachmentRef::is_valid_hash(&sha256) {
        return Err(bad_request(bisa_core::text!(
            "error-node-attachments-not-sha-256-digest"
        )));
    }
    if state.engine.workspace().attachment_path(&sha256).is_none() {
        return Err(not_found(bisa_core::text!(
            "error-node-attachments-no-attachment-with-hash-machine"
        )));
    }
    if bisa_store::sanitise_file_name(&body.name).is_none() {
        return Err(bad_request(bisa_core::text!(
            "error-node-attachments-not-file-name"
        )));
    }
    let path = bisa_engine::admin::attachment_named(state.engine.inner(), &sha256, &body.name)?;
    Ok(Json(json!({"path": path})))
}

/// `GET /artifacts/{scope}` — a conversation's artifacts, newest first: its
/// gallery, and the versions of one title.
async fn list_artifacts(
    State(state): State<Shared>,
    AxPath(scope): AxPath<String>,
    Query(q): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let rows = state.engine.workspace().list_artifacts(&scope, limit)?;
    Ok(Json(json!({"scope": scope, "artifacts": rows})))
}

#[derive(Deserialize)]
struct UploadQuery {
    name: String,
    #[serde(default)]
    mime: Option<String>,
}

/// Read a body, refusing rather than buffering past `cap`.
///
/// Frame by frame, so a lying `Content-Length` costs the cap and not the whole
/// stream. Lifted in shape from `hooks.rs`, which does the same for a webhook
/// payload for the same reason.
async fn read_capped(body: Body, cap: usize) -> Result<Vec<u8>, ()> {
    let mut stream = body.into_data_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| ())?;
        if buf.len() + chunk.len() > cap {
            return Err(());
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

fn too_large(locale: &bisa_i18n::Locale) -> Response {
    let text = bisa_core::text!(
        "error-node-attachments-too-large",
        max = MAX_ATTACHMENT_BYTES as i64
    );
    (
        StatusCode::PAYLOAD_TOO_LARGE,
        Json(json!({
            "error": bisa_i18n::render(locale, &text),
            "text": text,
            "limit": MAX_ATTACHMENT_BYTES,
        })),
    )
        .into_response()
}

/// `POST /attachments?name=&mime=` — bytes in, a descriptor out.
async fn upload(
    State(state): State<Shared>,
    crate::i18n::Lang(locale): crate::i18n::Lang,
    Query(q): Query<UploadQuery>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let cap = MAX_ATTACHMENT_BYTES as usize;
    // Declared length first, so an oversized upload is refused before it is
    // transferred rather than after.
    if let Some(len) = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok())
    {
        if len > cap {
            return too_large(&locale);
        }
    }
    let Ok(bytes) = read_capped(body, cap).await else {
        return too_large(&locale);
    };

    let mime = q.mime.as_deref().unwrap_or("application/octet-stream");
    match bisa_engine::admin::put_attachment(state.engine.inner(), &bytes, &q.name, mime) {
        Ok(file) => (StatusCode::CREATED, Json(json!(file))).into_response(),
        Err(e) => ApiError::from(e).into_response(),
    }
}

#[derive(Deserialize)]
struct DownloadQuery {
    /// `image` asks for inline rendering, which is granted only if the bytes
    /// really are one of the four formats below.
    #[serde(default)]
    r#as: Option<String>,
}

/// A year, and never revalidated: the hash is the content.
const IMMUTABLE: &str = "public, max-age=31536000, immutable";

/// Whether the request already holds this very content.
fn matches_etag(headers: &HeaderMap, etag: &str) -> bool {
    headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(',')
                .any(|tag| tag.trim() == etag || tag.trim() == "*")
        })
}

/// Whether `r` may be attached as a photo of `profile` — a content hash,
/// held on this machine, a picture by its bytes, within the profile's cap —
/// each refusal a 400 by name (`bisa_core::check_photo`). The bytes are read
/// once here and never again on a draw (ide/14 §Photos).
pub(crate) fn photo_check(
    ws: &bisa_store::Workspace,
    r: &AttachmentRef,
    profile: PhotoProfile,
) -> Result<(), ApiError> {
    let bytes = if AttachmentRef::is_valid_hash(&r.sha256) {
        ws.attachment_bytes(&r.sha256)?
    } else {
        None
    };
    bisa_core::check_photo(bytes.as_deref(), r, profile).map_err(|e| bad_request(e.text()))
}

/// `GET /attachments/{sha256}` — the bytes, when this machine has them.
///
/// **404 is an ordinary answer.** A peer's attachment syncs as a descriptor and
/// the bytes are fetched on demand, so a conversation routinely names files
/// this node has never held; the client turns that into a *Request* control
/// rather than an error.
async fn download(
    State(state): State<Shared>,
    AxPath(sha256): AxPath<String>,
    Query(q): Query<DownloadQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    // The address is the content: a client that holds it holds it for good.
    let etag = format!("\"{sha256}\"");
    if matches_etag(&headers, &etag) && state.engine.workspace().attachment_path(&sha256).is_some()
    {
        return Ok((
            StatusCode::NOT_MODIFIED,
            [
                (header::ETAG, etag.as_str()),
                (header::CACHE_CONTROL, IMMUTABLE),
            ],
        )
            .into_response());
    }
    let bytes = state
        .engine
        .workspace()
        .attachment_bytes(&sha256)?
        .ok_or_else(|| {
            not_found(bisa_core::text!(
                "error-node-attachments-no-attachment-with-hash-machine"
            ))
        })?;

    let inline = q.r#as.as_deref() == Some("image");
    let content_type = inline
        .then(|| image_type(&bytes))
        .flatten()
        .unwrap_or("application/octet-stream");
    let disposition = if content_type == "application/octet-stream" {
        "attachment"
    } else {
        "inline"
    };

    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            // The webview trusts what this route says, so it is told not to
            // second-guess it either way.
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::CONTENT_DISPOSITION, disposition),
            // Content-addressed, so immutable: the webview's own cache keeps
            // it, and a photo drawn on every reload is fetched once.
            (header::CACHE_CONTROL, IMMUTABLE),
            (header::ETAG, etag.as_str()),
        ],
        bytes,
    )
        .into_response())
}

/// `POST /attachments/{sha256}/fetch` — ask a peer for bytes we do not have.
async fn fetch_from_peer(
    State(state): State<Shared>,
    AxPath(sha256): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !bisa_core::AttachmentRef::is_valid_hash(&sha256) {
        return Err(bad_request(bisa_core::text!(
            "error-node-attachments-not-sha-256-digest"
        )));
    }
    if state.engine.workspace().attachment_path(&sha256).is_some() {
        return Ok(Json(json!({"status": "present"})));
    }
    match state.request_attachment(&sha256).await {
        Ok(true) => Ok(Json(json!({"status": "fetched"}))),
        Ok(false) => Err(not_found(bisa_core::text!(
            "error-node-attachments-no-connected-peer-has"
        ))),
        Err(reason) => Err(ApiError::text(
            StatusCode::CONFLICT,
            bisa_core::text!(
                "error-node-attachments-peer-refused",
                detail = reason.to_string() // `Display` is the plain English; the peer's word is a `Text` of ours
            ),
        )),
    }
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc {
        method: "POST",
        path: "/attachments",
        summary: "Upload bytes (`?name=&mime=`) → an attachment descriptor keyed by sha256.",
    },
    RouteDoc {
        method: "GET",
        path: "/attachments/{sha256}",
        summary: "The bytes, when this machine has them; 404 otherwise. Content-addressed, so served immutable: `Cache-Control: public, max-age=31536000, immutable` and an `ETag` of the hash; a request with a matching `If-None-Match` answers 304 with no body read.",
    },
    RouteDoc {
        method: "POST",
        path: "/attachments/{sha256}/fetch",
        summary: "Ask a peer for bytes this machine does not have.",
    },
    RouteDoc {
        method: "POST",
        path: "/attachments/{sha256}/file",
        summary: "The blob under a real name, made on demand: `{name}` → `{path}` inside the store, for the file manager and the default app. 404 when the bytes are not here.",
    },
    RouteDoc {
        method: "GET",
        path: "/artifacts/{scope}",
        summary: "A conversation's artifacts, newest first (`?limit=`, 50 by default, 200 at most), each with the message it rode on and whether its bytes are here.",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The sniff is the core's (`bisa_core::image_type`); here it guards
    /// `?as=image` — the whole wall between a picture and a scripting vector in
    /// a webview with no CSP — and a photo's check reads it once.
    #[test]
    fn the_sniff_is_the_core_s_and_a_photo_is_checked_by_it() {
        assert_eq!(image_type(b"\x89PNG\r\n\x1a\n....."), Some("image/png"));
        assert_eq!(image_type(b"<html><script>alert(1)</script>"), None);
    }
}
