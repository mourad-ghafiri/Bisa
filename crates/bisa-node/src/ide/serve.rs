//! Serving a folder (ide/18): the node binds a loopback port and serves the
//! files under one folder as pages — `index.html` for a directory, the right
//! content type for the rest. Two owners: a **folder of a checkout**, so a
//! page of the project renders with its own scripts and assets in the
//! embedded browser and an element pointed at there resolves back to a file
//! of the project ([`Servers::resolve`]); and an **artifact's named copy**,
//! so a page an agent posted opens in the browser with an origin of its own
//! — a port of its own, never the node's origin, which serves no agent page
//! (12-artifacts §Security).
//!
//! Loopback only, never a dotfile (`.git`, `.env`) however the URL spells
//! it — the path is percent-decoded before it is judged — and never a
//! hidden folder served whole, which would hand out by their plain names
//! the files a URL is refused ([`Servers::folder_root`]: where the folder
//! is once its links are followed); nothing outside
//! the folder (`resolve_within` for the folder, [`stays_within`] for every
//! file asked of it, so a link that leaves is not followed), and nothing
//! durable: a server lives while the node does, or until it is stopped.
//! At most [`MAX_SERVERS`] at once: each is a port and a task of this
//! node's, and a start past the bound is refused with what to do.

use crate::route_docs::RouteDoc;
use crate::Query;
use crate::{bad_request, conflict, not_found, ApiError, Shared};
use axum::body::Body;
use axum::extract::{Path as AxPath, State};
use axum::http::{Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use bisa_core::sync::Locked;
use bisa_core::WorkstreamId;
use bisa_engine::{EngineEvent, EnginePayload};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::watch;
use tower_http::services::ServeDir;

/// The most folders served at once, of checkouts and artifacts together. A
/// person serves a handful; a session that serves a folder for every page it
/// writes is told to stop one, rather than the node binding ports without
/// end.
pub const MAX_SERVERS: usize = 32;

/// Whose folder a server serves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServedOwner {
    /// A folder of a checkout — `folder` relative to it, empty for the
    /// checkout itself.
    Workstream { workstream: String, folder: String },
    /// An artifact's named copy: the page is `<url><name>`.
    Artifact { sha256: String, name: String },
}

/// One served folder, as the desktop reads it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[schemars(rename = "ServedFolder")]
pub struct ServedView {
    pub id: String,
    pub owner: ServedOwner,
    pub port: u16,
    /// `http://127.0.0.1:<port>/`.
    pub url: String,
    /// Where to land: the root for a folder, the file for an artifact.
    pub page: String,
    pub started_at: u64,
}

impl ServedView {
    /// The checkout this server belongs to, when it is a folder of one.
    pub fn workstream(&self) -> Option<&str> {
        match &self.owner {
            ServedOwner::Workstream { workstream, .. } => Some(workstream),
            ServedOwner::Artifact { .. } => None,
        }
    }
}

struct Entry {
    view: ServedView,
    root: PathBuf,
    stop: watch::Sender<bool>,
}

/// The servers up right now, by id.
#[derive(Default)]
pub struct Servers {
    entries: Mutex<BTreeMap<String, Entry>>,
    /// Held across a start: *already served?* and the new entry are one act,
    /// so two asks for one folder never bind two ports.
    starting: tokio::sync::Mutex<()>,
}

impl Servers {
    /// The servers of one checkout, oldest first.
    pub fn list(&self, workstream: WorkstreamId) -> Vec<ServedView> {
        let wid = workstream.to_string();
        self.entries
            .locked()
            .values()
            .filter(|e| e.view.workstream() == Some(wid.as_str()))
            .map(|e| e.view.clone())
            .collect()
    }

    /// Every server up, oldest first — folders and artifacts alike.
    pub fn list_all(&self) -> Vec<ServedView> {
        self.entries
            .locked()
            .values()
            .map(|e| e.view.clone())
            .collect()
    }

    /// One server's row.
    pub fn get(&self, id: &str) -> Option<ServedView> {
        self.entries.locked().get(id).map(|e| e.view.clone())
    }

    /// Serve `folder` of `checkout` on a fresh loopback port. Refused when
    /// the folder is not a directory of the checkout, or already served —
    /// a person asked twice from the Browser menu is told where it is.
    pub(crate) async fn start(
        &self,
        workstream: WorkstreamId,
        checkout: &Path,
        folder: &str,
    ) -> Result<ServedView, ApiError> {
        let (root, folder) = Self::folder_root(checkout, folder)?;
        let wid = workstream.to_string();
        let _one_at_a_time = self.starting.lock().await;
        if let Some(existing) = self.serving(&root) {
            if existing.workstream() == Some(wid.as_str()) {
                return Err(conflict(bisa_core::text!(
                    "error-node-ide-serve-folder-already-served",
                    a0 = (existing.url).to_string()
                )));
            }
        }
        self.start_workstream_root(wid, folder, root).await
    }

    /// The folder on disk a served folder of a checkout names: the checkout
    /// itself when `folder` is empty, else a directory within it — and never
    /// a hidden one. A file a URL is refused by its name (`.git/config`)
    /// would be handed out by its plain one (`/config`) were its folder the
    /// root, so a folder is judged where it *is*, once its links are
    /// followed: no part of its place under the checkout starts with a dot.
    fn folder_root(checkout: &Path, folder: &str) -> Result<(PathBuf, String), ApiError> {
        let folder = folder.trim().trim_matches('/').to_string();
        let base = checkout.canonicalize().map_err(|e| {
            bad_request(bisa_core::text!(
                "error-node-ide-serve-checkout-not-disk",
                e = e.to_string()
            ))
        })?;
        let root = if folder.is_empty() {
            base.clone()
        } else {
            bisa_store::resolve_within(checkout, &folder).map_err(|e| {
                bad_request(bisa_core::text!(
                    "error-node-ide-serve-not-folder-checkout",
                    folder = format!("{folder:?}"),
                    e = e.to_string()
                ))
            })?
        };
        let hidden = root.strip_prefix(&base).map_or(true, |place| {
            place
                .components()
                .any(|part| part.as_os_str().to_string_lossy().starts_with('.'))
        });
        if hidden {
            return Err(bad_request(bisa_core::text!(
                "error-node-ide-serve-hidden-folder",
                folder = format!("{folder:?}")
            )));
        }
        if !root.is_dir() {
            return Err(bad_request(if folder.is_empty() {
                bisa_core::text!("error-node-ide-serve-checkout-not-folder")
            } else {
                bisa_core::text!("error-node-ide-serve-not-folder", a0 = folder.clone())
            }));
        }
        Ok((root, folder))
    }

    async fn start_workstream_root(
        &self,
        wid: String,
        folder: String,
        root: PathBuf,
    ) -> Result<ServedView, ApiError> {
        self.start_root(
            ServedOwner::Workstream {
                workstream: wid,
                folder,
            },
            root,
            "",
        )
        .await
    }

    /// Serve the folder holding an artifact's named copy, so the page opens
    /// with an origin of its own. Idempotent: a second ask answers the
    /// server already up.
    pub(crate) async fn start_artifact(
        &self,
        sha256: &str,
        name: &str,
        named_copy: &Path,
    ) -> Result<ServedView, ApiError> {
        let root = named_copy
            .parent()
            .ok_or_else(|| {
                bad_request(bisa_core::text!(
                    "error-node-ide-serve-named-copy-has-no-folder"
                ))
            })?
            .to_path_buf();
        // Roots are kept resolved (`start_root`), so they are compared so.
        let root = root.canonicalize().unwrap_or(root);
        let _one_at_a_time = self.starting.lock().await;
        if let Some(existing) = self.serving(&root) {
            return Ok(existing);
        }
        self.start_root(
            ServedOwner::Artifact {
                sha256: sha256.to_string(),
                name: name.to_string(),
            },
            root,
            name,
        )
        .await
    }

    /// The server on a root, if one is up.
    fn serving(&self, root: &Path) -> Option<ServedView> {
        self.entries
            .locked()
            .values()
            .find(|e| e.root == root)
            .map(|e| e.view.clone())
    }

    /// Bind a fresh loopback port and serve `root` on it; `landing` is the
    /// path under the root the page is at.
    async fn start_root(
        &self,
        owner: ServedOwner,
        root: PathBuf,
        landing: &str,
    ) -> Result<ServedView, ApiError> {
        // Counted under the start's own lock (`starting`), which every
        // caller holds: two starts never both take the last place.
        let up = self.entries.locked().len();
        if up >= MAX_SERVERS {
            return Err(conflict(bisa_core::text!(
                "error-node-ide-serve-too-many",
                count = up.to_string()
            )));
        }
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|e| {
                ApiError::text(
                    StatusCode::SERVICE_UNAVAILABLE,
                    bisa_core::text!("error-node-ide-serve-no-loopback-port", e = e.to_string()),
                )
            })?;
        let port = listener
            .local_addr()
            .map_err(|e| {
                ApiError::text(
                    StatusCode::SERVICE_UNAVAILABLE,
                    bisa_core::text!("error-node-ide-serve-refused", detail = e.to_string()),
                )
            })?
            .port();
        let id = ulid::Ulid::from_datetime(SystemTime::now()).to_string();
        let url = format!("http://127.0.0.1:{port}/");
        let view = ServedView {
            id: id.clone(),
            owner,
            port,
            page: format!("{url}{}", url_escape(landing)),
            url,
            started_at: now_secs(),
        };
        let served = id.clone();
        let (stop, mut stopped) = watch::channel(false);
        // The guard compares resolved paths, so the root is resolved too.
        let root = root.canonicalize().unwrap_or(root);
        let guarded = root.clone();
        let app: Router<()> = Router::new()
            .fallback_service(ServeDir::new(root.clone()).append_index_html_on_directories(true))
            .layer(middleware::from_fn(
                move |req: Request<Body>, next: Next| {
                    only_the_folders_own_files(guarded.clone(), req, next)
                },
            ));
        tokio::spawn(async move {
            let serve = axum::serve(listener, app).with_graceful_shutdown(async move {
                while !*stopped.borrow() {
                    if stopped.changed().await.is_err() {
                        break;
                    }
                }
            });
            if let Err(e) = serve.await {
                tracing::warn!(target: "bisa_node::serve", server = %served, port, "a served folder ended on its own: {e}");
            }
        });
        self.entries.locked().insert(
            id,
            Entry {
                view: view.clone(),
                root,
                stop,
            },
        );
        Ok(view)
    }

    /// Stop one server; `None` when there is none under that id.
    pub fn stop(&self, id: &str) -> Option<ServedView> {
        let entry = self.entries.locked().remove(id)?;
        // A server already gone hears no stop, and needs none.
        let _server_gone = entry.stop.send(true);
        Some(entry.view)
    }

    /// Stop every server — the node is going down.
    pub fn stop_all(&self) {
        let mut entries = self.entries.locked();
        for (_, entry) in std::mem::take(&mut *entries) {
            let _server_gone = entry.stop.send(true);
        }
    }

    /// The file of the checkout a URL path of this server lands on —
    /// `index.html` for a directory — relative to the checkout, or `None`
    /// when nothing is there. What makes an annotation on a served page a
    /// file chip.
    pub(crate) fn resolve(&self, id: &str, url_path: &str) -> Result<Option<String>, ApiError> {
        let (root, folder) = {
            let entries = self.entries.locked();
            let entry = entries.get(id).ok_or_else(|| {
                not_found(bisa_core::text!("error-node-ide-serve-no-such-server"))
            })?;
            let ServedOwner::Workstream { folder, .. } = &entry.view.owner else {
                // An artifact's page is nobody's file to edit.
                return Ok(None);
            };
            (entry.root.clone(), folder.clone())
        };
        let Some(rel) = url_file(url_path) else {
            return Ok(None);
        };
        let mut path = root.join(&rel);
        let mut rel = rel;
        if !stays_within(&root, &path) {
            return Ok(None);
        }
        if path.is_dir() {
            path = path.join("index.html");
            rel = if rel.is_empty() {
                "index.html".to_string()
            } else {
                format!("{rel}/index.html")
            };
        }
        if !path.is_file() {
            return Ok(None);
        }
        Ok(Some(if folder.is_empty() {
            rel
        } else {
            format!("{folder}/{rel}")
        }))
    }
}

/// A URL path as a relative file path: no query, each segment
/// percent-decoded, and then no `..`, no dotfile, no separator smuggled in
/// by an escape; `None` when it cannot be one.
fn url_file(url_path: &str) -> Option<String> {
    let path = url_path.split(['?', '#']).next().unwrap_or("");
    let mut out = Vec::new();
    for segment in path.split('/') {
        if segment.is_empty() {
            continue;
        }
        let segment = percent_decoded(segment)?;
        if segment.starts_with('.') || segment.contains(['/', '\\', '\0']) {
            return None;
        }
        out.push(segment);
    }
    Some(out.join("/"))
}

/// A URL segment with its `%XX` escapes read; `None` for an escape that is
/// not one, or bytes that are not UTF-8.
fn percent_decoded(segment: &str) -> Option<String> {
    let bytes = segment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let hex = segment.get(at + 1..at + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Whether `path` — a file that may not stand — is the folder's own once
/// links are followed: under `root`, by no hidden name. A path that does not
/// stand leaves nothing to hand out, so it passes to the 404 it is owed.
fn stays_within(root: &Path, path: &Path) -> bool {
    match path.canonicalize() {
        Ok(real) => real.strip_prefix(root).is_ok_and(|rel| {
            rel.components()
                .all(|c| !c.as_os_str().to_string_lossy().starts_with('.'))
        }),
        Err(_) => true,
    }
}

/// A file name as a URL path segment: what a browser would not take as is,
/// percent-encoded.
fn url_escape(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for b in name.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Only the folder's own files: never a dotfile or a dot directory however
/// the URL spells it, never a file a link reaches outside the folder.
async fn only_the_folders_own_files(root: PathBuf, req: Request<Body>, next: Next) -> Response {
    let Some(rel) = url_file(req.uri().path()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let path = root.join(rel);
    let own = tokio::task::spawn_blocking(move || stays_within(&root, &path))
        .await
        .unwrap_or(false);
    if !own {
        return StatusCode::NOT_FOUND.into_response();
    }
    next.run(req).await
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/servers", get(list_all))
        .route("/servers/{id}", axum::routing::delete(stop_any))
        .route("/workstreams/{wid}/servers", get(list).post(start))
        .route(
            "/workstreams/{wid}/servers/{id}",
            axum::routing::delete(stop),
        )
        .route("/workstreams/{wid}/servers/{id}/resolve", get(resolve))
        .route(
            "/artifacts/{sha256}/serve",
            axum::routing::post(serve_artifact),
        )
}

pub const ROUTES: &[RouteDoc] = &[
    RouteDoc { method: "GET", path: "/servers", summary: "Every folder the node serves right now, folders of checkouts and artifacts alike: `{servers: [{id, owner: {kind: workstream, workstream, folder} | {kind: artifact, sha256, name}, port, url, page, started_at}]}` — `page` is where to land." },
    RouteDoc { method: "DELETE", path: "/servers/{id}", summary: "Stop one server of either kind; 404 when there is none under that id." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/servers", summary: "The folders of this checkout the node serves right now: `{servers: [{id, owner, port, url, page, started_at}]}`." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/servers", summary: "Serve a folder of the checkout on a fresh loopback port: `{folder?}` relative to the checkout, absent for the checkout itself — `index.html` for a directory, never a dotfile, nothing outside the folder. 409 when that folder is already served, or when the node already serves as many folders as it does at once (32); 400 when it is not a folder of the checkout, or is a hidden one — a folder whose place under the checkout has a name starting with a dot, once its links are followed, is never served whole." },
    RouteDoc { method: "DELETE", path: "/workstreams/{wid}/servers/{id}", summary: "Stop one served folder; 404 when there is none under that id." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/servers/{id}/resolve", summary: "The file of the checkout `?path=` (a URL path of that server) lands on — `index.html` for a directory — as `{path}` relative to the checkout, `null` when nothing is there: what makes an annotation on a served page a file chip. An artifact's server resolves to nothing." },
    RouteDoc { method: "POST", path: "/artifacts/{sha256}/serve", summary: "Serve an artifact's named copy `{name}` on a fresh loopback port of its own — the page opens in the embedded browser with an origin that is not the node's. Answers the server, its `page` the file's URL; a second ask answers the server already up. 404 when this machine does not hold the bytes; 400 when the name is not a file name." },
];

fn wid_of(s: &str) -> Result<WorkstreamId, ApiError> {
    crate::projects::parse_workstream_id(s)
}

fn changed(state: &Shared, workstream: Option<WorkstreamId>) {
    state
        .engine
        .inner()
        .emit(EngineEvent::global(EnginePayload::ServerChanged {
            workstream,
        }));
}

async fn list_all(State(state): State<Shared>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::json!({"servers": state.servers.list_all()}),
    ))
}

async fn stop_any(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let stopped = state
        .servers
        .stop(&id)
        .ok_or_else(|| not_found(bisa_core::text!("error-node-ide-serve-no-such-server")))?;
    changed(&state, stopped.workstream().and_then(|w| w.parse().ok()));
    Ok(Json(serde_json::json!({"stopped": stopped})))
}

async fn serve_artifact(
    State(state): State<Shared>,
    AxPath(sha256): AxPath<String>,
    crate::Body(body): crate::Body<crate::dto::NamedFileBody>,
) -> Result<Json<ServedView>, ApiError> {
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
    // The name must already be a file name: one that sanitising would change
    // — a path such as `../x.html`, a name with a stray space — is refused,
    // never quietly served under another name than the one asked for.
    if bisa_store::sanitise_file_name(&body.name).as_deref() != Some(body.name.as_str()) {
        return Err(bad_request(bisa_core::text!(
            "error-node-attachments-not-file-name"
        )));
    }
    let named = bisa_engine::admin::attachment_named(state.engine.inner(), &sha256, &body.name)?;
    let before = state.servers.list_all().len();
    let view = state
        .servers
        .start_artifact(&sha256, &body.name, &named)
        .await?;
    // A second ask answers the server already up: nothing changed.
    if state.servers.list_all().len() != before {
        changed(&state, None);
    }
    Ok(Json(view))
}

async fn list(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = wid_of(&wid)?;
    state.engine.workspace().get_workstream(wid)?;
    Ok(Json(serde_json::json!({
        "workstream": wid.to_string(),
        "servers": state.servers.list(wid),
    })))
}

async fn start(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    crate::Body(body): crate::Body<crate::dto::ServeBody>,
) -> Result<Json<ServedView>, ApiError> {
    let wid = wid_of(&wid)?;
    let (_, checkout) = bisa_engine::projects::checkout_tree_pub(state.engine.inner(), wid)?;
    let view = state
        .servers
        .start(wid, &checkout, body.folder.as_deref().unwrap_or(""))
        .await?;
    changed(&state, Some(wid));
    Ok(Json(view))
}

async fn stop(
    State(state): State<Shared>,
    AxPath((wid, id)): AxPath<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = wid_of(&wid)?;
    // Only a server of this checkout is stopped through its door.
    let owned = state
        .servers
        .get(&id)
        .is_some_and(|view| view.workstream() == Some(wid.to_string().as_str()));
    let stopped = owned
        .then(|| state.servers.stop(&id))
        .flatten()
        .ok_or_else(|| not_found(bisa_core::text!("error-node-ide-serve-no-such-server")))?;
    changed(&state, Some(wid));
    Ok(Json(serde_json::json!({"stopped": stopped})))
}

#[derive(Deserialize)]
struct ResolveQuery {
    path: String,
}

async fn resolve(
    State(state): State<Shared>,
    AxPath((wid, id)): AxPath<(String, String)>,
    Query(q): Query<ResolveQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    wid_of(&wid)?;
    let path = state.servers.resolve(&id, &q.path)?;
    Ok(Json(serde_json::json!({"path": path})))
}

/// The engine's port (ide/18 §Serving a folder): a session's `browser_serve`
/// lands on the same list the Browser menu draws, and reads the same
/// refusals in a sentence. Idempotent — an agent that asks twice reads the
/// server already up, never a refusal to retry.
#[async_trait::async_trait]
impl bisa_engine::browser::FolderServer for Servers {
    async fn serve(
        &self,
        workstream: WorkstreamId,
        checkout: &Path,
        folder: &str,
    ) -> Result<bisa_engine::browser::ServedPage, String> {
        let (root, folder) = Self::folder_root(checkout, folder).map_err(|e| e.text.to_string())?;
        let wid = workstream.to_string();
        let _one_at_a_time = self.starting.lock().await;
        let existing = self
            .serving(&root)
            .filter(|s| s.workstream() == Some(wid.as_str()));
        let view = match existing {
            Some(view) => view,
            None => self
                .start_workstream_root(wid, folder, root)
                .await
                .map_err(|e| e.text.to_string())?,
        };
        let folder = match &view.owner {
            ServedOwner::Workstream { folder, .. } => folder.clone(),
            ServedOwner::Artifact { .. } => String::new(),
        };
        Ok(bisa_engine::browser::ServedPage {
            id: view.id,
            url: view.url,
            page: view.page,
            folder,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_url_path_is_a_file_path_or_nothing() {
        assert_eq!(url_file("/"), Some(String::new()));
        assert_eq!(
            url_file("/docs/index.html?x=1#top"),
            Some("docs/index.html".into())
        );
        assert_eq!(url_file("//a//b"), Some("a/b".into()));
        assert_eq!(url_file("/../etc/passwd"), None, "never above the folder");
        assert_eq!(url_file("/.git/config"), None, "never a dotfile");
        assert_eq!(url_file("/a/.env"), None);
    }

    #[test]
    fn a_url_path_is_judged_as_the_name_it_decodes_to() {
        assert_eq!(url_file("/my%20page.html"), Some("my page.html".into()));
        assert_eq!(url_file("/caf%C3%A9.html"), Some("café.html".into()));
        for hidden in [
            "/%2Egit/config",
            "/%2egit/config",
            "/a/%2Eenv",
            "/%2E%2E/etc/hosts",
            "/..%2Fetc%2Fhosts",
            "/a%2F..%2F..%2Fb",
            "/a%5C..%5Cb",
            "/a%00.html",
        ] {
            assert_eq!(url_file(hidden), None, "{hidden}");
        }
        // An escape that is not one, and bytes that are no name.
        for broken in ["/%", "/%2", "/%zz.html", "/%FF.html", "/é%2"] {
            assert_eq!(url_file(broken), None, "{broken}");
        }
        assert_eq!(url_file("/100%25.html"), Some("100%.html".into()));
    }

    #[test]
    fn a_file_is_the_folders_own_only_where_its_links_end() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("site").canonicalize().unwrap_or_else(|_| {
            std::fs::create_dir_all(dir.path().join("site/.cache")).unwrap();
            dir.path().join("site").canonicalize().unwrap()
        });
        std::fs::write(root.join("index.html"), "hi").unwrap();
        std::fs::write(root.join(".cache/inner.txt"), "hidden").unwrap();
        std::fs::write(dir.path().join("outside.txt"), "theirs").unwrap();
        assert!(stays_within(&root, &root));
        assert!(stays_within(&root, &root.join("index.html")));
        assert!(
            stays_within(&root, &root.join("missing.html")),
            "nothing stands there: the 404 is the server's to give"
        );
        assert!(!stays_within(&root, &root.join(".cache/inner.txt")));
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(dir.path().join("outside.txt"), root.join("out.txt")).unwrap();
            symlink(root.join(".cache"), root.join("cache")).unwrap();
            symlink(root.join("index.html"), root.join("home.html")).unwrap();
            assert!(
                !stays_within(&root, &root.join("out.txt")),
                "a link that leaves"
            );
            assert!(
                !stays_within(&root, &root.join("cache/inner.txt")),
                "a link to what is hidden"
            );
            assert!(stays_within(&root, &root.join("home.html")));
        }
    }

    #[test]
    fn a_hidden_folder_is_never_served_whole_wherever_a_link_says_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let checkout = dir.path().join("tree");
        for folder in ["site/pages", "site/.cache", ".git/objects", ".config"] {
            std::fs::create_dir_all(checkout.join(folder)).unwrap();
        }
        let refused = |folder: &str| {
            let refusal = Servers::folder_root(&checkout, folder)
                .err()
                .unwrap_or_else(|| panic!("{folder} was given a root"));
            assert_eq!(refusal.status, StatusCode::BAD_REQUEST, "{folder}");
            refusal.text.id.to_string()
        };
        for hidden in [
            ".git",
            ".git/objects",
            ".config",
            "site/.cache",
            "/.git/",
            " .git ",
        ] {
            assert_eq!(
                refused(hidden),
                "error-node-ide-serve-hidden-folder",
                "{hidden}"
            );
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(checkout.join(".git"), checkout.join("history")).unwrap();
            symlink(checkout.join("site/.cache"), checkout.join("site/cache")).unwrap();
            symlink(checkout.join("site/pages"), checkout.join("pages")).unwrap();
            for link in ["history", "history/objects", "site/cache"] {
                assert_eq!(
                    refused(link),
                    "error-node-ide-serve-hidden-folder",
                    "{link}: where a link ends is what counts"
                );
            }
            let (root, folder) = Servers::folder_root(&checkout, "pages")
                .ok()
                .expect("a link that ends in the open");
            assert!(root.ends_with("site/pages"), "{}", root.display());
            assert_eq!(folder, "pages");
        }
        for open in ["", "site", "site/pages", "/site/"] {
            let (root, _) = Servers::folder_root(&checkout, open)
                .ok()
                .unwrap_or_else(|| panic!("{open} was refused"));
            assert!(root.is_dir(), "{open}");
        }
        // Outside the checkout is refused as before, in its own words.
        assert_eq!(refused(".."), "error-node-ide-serve-not-folder-checkout");
    }

    /// A folder of the test's own, served as a checkout's.
    async fn served(servers: &Servers, dir: &Path, n: usize) -> Result<ServedView, ApiError> {
        let root = dir.join(format!("site-{n}"));
        std::fs::create_dir_all(&root).unwrap();
        let _one_at_a_time = servers.starting.lock().await;
        servers
            .start_workstream_root("w1".into(), format!("site-{n}"), root)
            .await
    }

    #[tokio::test]
    async fn the_folders_served_at_once_have_a_bound_and_a_stop_makes_room() {
        let dir = tempfile::tempdir().unwrap();
        let servers = Servers::default();
        let mut up = Vec::new();
        for n in 0..MAX_SERVERS {
            up.push(
                served(&servers, dir.path(), n)
                    .await
                    .ok()
                    .expect("within the bound"),
            );
        }
        let refused = served(&servers, dir.path(), MAX_SERVERS)
            .await
            .expect_err("one past the bound");
        assert_eq!(refused.status, StatusCode::CONFLICT);
        assert_eq!(refused.text.id, "error-node-ide-serve-too-many");
        assert_eq!(
            servers.list_all().len(),
            MAX_SERVERS,
            "and nothing was bound for it"
        );

        assert!(servers.stop(&up[0].id).is_some());
        assert!(
            served(&servers, dir.path(), MAX_SERVERS).await.is_ok(),
            "a stop made room"
        );
        servers.stop_all();
        assert!(servers.list_all().is_empty());
    }

    #[test]
    fn a_landing_file_name_is_escaped_for_a_url() {
        assert_eq!(url_escape(""), "");
        assert_eq!(url_escape("report.html"), "report.html");
        assert_eq!(
            url_escape("q1 report (v2).html"),
            "q1%20report%20%28v2%29.html"
        );
        assert_eq!(url_escape("é.svg"), "%C3%A9.svg");
    }

    #[test]
    fn a_view_knows_its_checkout_only_when_it_is_a_folder_of_one() {
        let folder = ServedView {
            id: "1".into(),
            owner: ServedOwner::Workstream {
                workstream: "w1".into(),
                folder: "site".into(),
            },
            port: 1,
            url: "http://127.0.0.1:1/".into(),
            page: "http://127.0.0.1:1/".into(),
            started_at: 0,
        };
        assert_eq!(folder.workstream(), Some("w1"));
        let artifact = ServedView {
            owner: ServedOwner::Artifact {
                sha256: "a".repeat(64),
                name: "report.html".into(),
            },
            page: "http://127.0.0.1:1/report.html".into(),
            ..folder
        };
        assert_eq!(artifact.workstream(), None);
        let json = serde_json::to_value(&artifact).unwrap();
        assert_eq!(json["owner"]["kind"], "artifact");
        assert_eq!(json["page"], "http://127.0.0.1:1/report.html");
    }
}
