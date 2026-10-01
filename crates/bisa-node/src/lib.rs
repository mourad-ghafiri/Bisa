//! The Bisa daemon: an axum control plane over a running [`Engine`].
//!
//! Serves HTTP/JSON on a unix socket (0600) and optionally on a loopback TCP
//! address. Access control is the socket's file permissions, the loopback-only
//! bind, and the workspace's bearer token on every route but the few
//! [`auth`] names (a public hook's own secret, an addon's token-less files);
//! anything wider arrives with the Nostr transport.
//!
//! The full route table lives in `docs/reference/http-api.md`. Modules: [`goals`] (capture, runs, steps + threads),
//! [`workflows`] (the library a designer edits and a goal picks from),
//! [`conversation`] (channels/DMs), [`inbox`] (conversation
//! rows), [`pulse`] (workspace timeline), [`agents_api`] (agent definitions,
//! teams, sessions, recall), [`skills`] (the shared procedure library),
//! [`catalog`] (the staff a workspace may install, and the install itself),
//! [`mcp`] (the MCP server registry), [`tags`] (the `?tag=` filter every list
//! route wears, and the facet counts behind a filter bar), [`listening`]
//! (who hears a workflow's start events: On and Off, the listeners, a hook's
//! local door, the durable signal queue), [`usage`] (what points at an
//! object, which is what a delete refuses over), [`files`] (the folders a
//! workspace owns, read back: a bounded listing, a bounded read, and where a
//! scope's files live), [`hooks`] (`POST /hooks/{host}/{step}` — the one
//! route an external system calls, authenticated by its listener's own
//! secret), [`admin`] (harnesses, governance, search, members, transcripts,
//! pause).
//!
//! SSE: `GET /events` emits a tagged envelope, one JSON object per event:
//! `{stream: "engine", payload: EngineEvent}` |
//! `{stream: "conversation", payload: {scope, kind, event_id, author, snippet, unread_count, latest}}` |
//! `{stream: "inbox",  payload: {key, kind, unread_count, needs_action_count,
//! latest_at, read, handled, notice_count?, unread_notices?}}`.
//!
//! The `inbox` frame rides alongside the frame that caused it — a message
//! arriving, a gate opening or being decided, a goal moving, a notice on a
//! workstream, a project or a workflow, a listener that failed, a read
//! marker set — and carries one row's whole inbox state; the two notice
//! counts ride only when the cause could have moved them. It exists so a
//! list can change a row *in place* rather than refetch and replace the
//! array underneath a selection somebody is using.

#[cfg(feature = "a2a")]
pub mod a2a;
pub mod attachments;
pub mod auth;
pub mod browser;
pub mod mobile_development;
#[cfg(feature = "a2a")]
pub use a2a::A2aExposeConfig;
pub mod addons;
pub mod admin;
pub mod agents_api;
pub mod catalog;
pub mod changes;
pub mod codehost;
pub mod collab;
pub mod connectors;
pub mod conversation;
pub mod conversations;
pub mod decisions;
pub mod drawings;
pub mod dto;
pub mod files;
pub mod git_config;
pub mod goals;
pub mod hooks;
pub mod i18n;
pub mod ide;
pub mod inbox;
pub mod listening;
pub mod logs;
pub mod mcp;
pub mod network;
pub mod notes;
pub mod owner;
pub mod pets;
pub mod projects;
pub mod pulse;
pub mod readiness;
pub mod repo_git;
pub mod review;
pub mod route_docs;
pub mod runs;
pub mod security;
pub mod settings;
pub mod skills;
pub mod ssh;
pub mod tags;
pub mod usage;
pub mod work_items;
pub mod workflows;

use axum::extract::rejection::JsonRejection;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use bisa_core::{GoalId, Localize, Text};
use bisa_engine::Engine;
use futures::stream::Stream;
use serde_json::json;
use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use tokio::net::UnixListener;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

/// Ask a connected peer for an attachment's bytes.
///
/// The same shape and the same reason as [`collab::CollabDoors`]: the wire
/// belongs to the process that started it, and this crate does not depend on
/// `bisa-net`. `None` means no pump is running, which is what
/// `POST /attachments/{sha}/fetch` reports as a **409** — an honest "there is
/// nobody to ask" rather than a 404 that would read as "the file does not
/// exist".
///
/// Answers `true` when the bytes are now in the store, `false` when no peer
/// had them, and `Err(reason)` when the ask could not be made at all.
pub type AttachmentFetcher =
    Arc<dyn Fn(String) -> futures::future::BoxFuture<'static, Result<bool, String>> + Send + Sync>;

pub struct NodeConfig {
    /// Preferred unix-socket path (usually `<data-dir>/run/node.sock`).
    pub socket: PathBuf,
    /// Optional additional loopback TCP bind.
    pub http: Option<SocketAddr>,
    /// The workspace data dir.
    pub data_dir: PathBuf,
    /// The wire and the hosted memberships, when the embedding process runs
    /// the collaboration pump (14-collaboration).
    pub collab: Option<collab::Collab>,
    /// Fetch an attachment's bytes from a peer, when a pump is running.
    pub fetch_attachment: Option<AttachmentFetcher>,
    /// Expose this node as an A2A agent (feature `a2a`). `None` = not exposed.
    #[cfg(feature = "a2a")]
    pub a2a: Option<a2a::A2aExposeConfig>,
    /// The control-plane token to require. `None` reads or mints
    /// `run/token`; `Some` is recorded there so the CLI finds it too.
    pub token: Option<String>,
}

pub(crate) struct AppState {
    pub(crate) engine: Engine,
    /// Unix seconds when this node started serving — `GET /node`'s.
    started_at: u64,
    /// The unix socket actually bound (a long data dir lands on the
    /// fallback path), and the TCP address actually bound, when one was
    /// asked for — never the one asked for, whose port may be "any".
    socket: PathBuf,
    listen: Option<SocketAddr>,
    /// What every request must present — see [`auth`].
    pub(crate) token: String,
    /// The public hook door's rate-limit buckets and the decoy secret a
    /// listener with none is compared against — process-lifetime,
    /// deliberately not persisted.
    pub(crate) hooks: hooks::Hooks,
    /// The loopback listener an OAuth redirect lands on, up only while a
    /// connection is pending — see [`connectors`].
    pub(crate) oauth: connectors::CallbackListener,
    pub(crate) collab: Option<collab::Collab>,
    fetch_attachment: Option<AttachmentFetcher>,
    #[cfg(feature = "a2a")]
    pub(crate) a2a: Option<a2a::A2aExposeConfig>,
    /// The checkout folders served on loopback ports (ide/18).
    pub(crate) servers: Arc<ide::serve::Servers>,
    /// Whether the node was asked to stop. A stream is open for as long as
    /// the node is: every one that would otherwise never end — `/events`, a
    /// search still walking — ends on this ([`until_stopped`]), so a stop
    /// never waits on a listener that would not leave.
    pub(crate) stop: tokio::sync::watch::Receiver<bool>,
}

impl AppState {
    /// Ask a peer for bytes we do not have, or say why we cannot.
    pub(crate) async fn request_attachment(&self, sha256: &str) -> Result<bool, Text> {
        let Some(fetch) = self.fetch_attachment.clone() else {
            return Err(bisa_core::text!("error-node-attachments-no-sync-loop"));
        };
        fetch(sha256.to_string())
            .await
            .map_err(|e| bisa_core::text!("error-node-attachments-peer-said", detail = e))
    }
}

pub(crate) type Shared = Arc<AppState>;

/// Bind the node socket, falling back to a short temp path when the preferred
/// path exceeds the platform `sun_path` limit (~104 bytes on macOS). The
/// actual path is recorded next to the preferred one (`<preferred>.path`) so
/// clients can always discover it.
fn bind_socket(preferred: &std::path::Path) -> std::io::Result<(UnixListener, PathBuf)> {
    if let Some(dir) = preferred.parent() {
        std::fs::create_dir_all(dir)?;
    }
    clear_stale(preferred);
    let pointer = pointer_path(preferred);
    clear_stale(&pointer);
    let (listener, actual) = match listen_at(preferred) {
        Ok(l) => (l, preferred.to_path_buf()),
        Err(_) if preferred.as_os_str().len() > 90 => {
            let short = std::env::temp_dir().join(format!(
                "itfn-{}.sock",
                ulid::Ulid::from_datetime(std::time::SystemTime::now())
            ));
            clear_stale(&short);
            let l = listen_at(&short)?;
            std::fs::write(&pointer, short.to_string_lossy().as_bytes())?;
            (l, short)
        }
        Err(e) => return Err(e),
    };
    Ok((listener, actual))
}

/// A socket that listens at `path`, **there only once it does**. A socket's
/// file exists from the moment it is bound, a moment before it listens, and a
/// client that takes the file for the node being there is refused in
/// between. So the socket is bound under a name nobody looks for, made the
/// owner's alone — it is the admin surface, and one the machine's other
/// users could open is not one to serve on — and given its own name when it
/// answers: what a client finds at `path` is a socket that takes its call.
fn listen_at(path: &std::path::Path) -> std::io::Result<UnixListener> {
    let mut binding = path.as_os_str().to_owned();
    binding.push("~");
    let binding = PathBuf::from(binding);
    clear_stale(&binding);
    let listener = UnixListener::bind(&binding)?;
    let placed = std::fs::set_permissions(
        &binding,
        std::os::unix::fs::PermissionsExt::from_mode(0o600),
    )
    .and_then(|()| std::fs::rename(&binding, path));
    if let Err(e) = placed {
        clear_stale(&binding);
        return Err(e);
    }
    Ok(listener)
}

/// Remove what an earlier node left at `path` — a socket or its pointer.
/// Nothing there is the common case and no news; anything else that stops
/// the removal is said, because the bind that follows will fail on it.
fn clear_stale(path: &std::path::Path) {
    match std::fs::remove_file(path) {
        Ok(()) => {
            tracing::debug!(target: "bisa_node", path = %path.display(), "stale file removed")
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            tracing::warn!(target: "bisa_node", path = %path.display(), "stale file not removed: {e}")
        }
    }
}

/// `<preferred>.path` — where a fallback bind records the actual socket path.
pub fn pointer_path(preferred: &std::path::Path) -> PathBuf {
    let mut os = preferred.as_os_str().to_owned();
    os.push(".path");
    PathBuf::from(os)
}

/// Serve the control plane until `shutdown` completes, then shut the engine
/// down. Returns after everything has stopped.
pub async fn serve(
    engine: Engine,
    cfg: NodeConfig,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    // **The token before any door.** The socket is how a client knows the
    // node is there — the command line, the desktop, a test — and the token
    // is the first thing it reads then: written after the socket was bound,
    // a client that came at once found a node and no token to ask it with.
    let token = auth::ensure_token(&bisa_store::Paths::new(&cfg.data_dir), cfg.token.as_deref())?;
    let (listener, actual) = bind_socket(&cfg.socket)?;
    // Bound before anything is said about it: the address this node answers
    // at is the one the socket got — asked for port 0, that is whichever was
    // free — and the one `GET /node` reports and a terminal's hooks call.
    let tcp_listener = match cfg.http {
        Some(addr) => Some(tokio::net::TcpListener::bind(addr).await?),
        None => None,
    };
    let listen = match &tcp_listener {
        Some(bound) => Some(bound.local_addr()?),
        None => None,
    };
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    // The served folders are the node's; the engine is lent them so a
    // session's `browser_serve` has a server to ask (ide/18).
    let servers = Arc::new(ide::serve::Servers::default());
    engine.set_folder_server(Arc::clone(&servers) as Arc<dyn bisa_engine::browser::FolderServer>);
    let state: Shared = Arc::new(AppState {
        engine,
        started_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        socket: actual.clone(),
        listen,
        token,
        hooks: hooks::Hooks::default(),
        oauth: connectors::CallbackListener::default(),
        collab: cfg.collab.clone(),
        fetch_attachment: cfg.fetch_attachment.clone(),
        #[cfg(feature = "a2a")]
        a2a: cfg.a2a.clone(),
        servers,
        stop: stop_rx.clone(),
    });
    let app = router(Arc::clone(&state));

    tokio::spawn(async move {
        shutdown.await;
        if stop_tx.send(true).is_err() {
            tracing::debug!(target: "bisa_node", "nobody listens for the stop");
        }
    });

    let unix = axum::serve(listener, app.clone()).with_graceful_shutdown(stopped(stop_rx.clone()));

    let tcp = match (tcp_listener, listen) {
        (Some(bound), Some(addr)) => {
            // The CLI refuses this outright; a library caller only gets a
            // warning, but it should be one that names the actual consequence.
            if !addr.ip().is_loopback() {
                tracing::warn!(
                    "node bound to {addr}, which is NOT loopback — the bearer token is the only \
                     thing between this port and the workspace, and the routes that need none \
                     (public hooks, addon files) answer anyone who can reach it"
                );
            }
            tracing::info!("node listening on http://{addr}");
            Some(axum::serve(bound, app).with_graceful_shutdown(stopped(stop_rx.clone())))
        }
        _ => None,
    };
    tracing::info!(
        target: "bisa_node",
        version = env!("CARGO_PKG_VERSION"),
        data_dir = %cfg.data_dir.display(),
        socket = %actual.display(),
        listen = ?listen,
        "node started"
    );
    tracing::info!("node listening on {}", actual.display());

    match tcp {
        Some(tcp) => {
            let (a, b) = tokio::join!(unix, tcp);
            a?;
            b?;
        }
        None => unix.await?,
    }
    stopped(stop_rx).await;
    tracing::info!(target: "bisa_node", "node stopping");
    state.oauth.stop().await;

    // The socket and its pointer go with the node; one left behind is a
    // debug line, not a failure — the next start clears it.
    for left in [actual.clone(), pointer_path(&cfg.socket)] {
        if let Err(e) = std::fs::remove_file(&left) {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::debug!(target: "bisa_node", path = %left.display(), "not removed at stop: {e}");
            }
        }
    }
    // The engine stops whoever still holds the node's state: a task that
    // outlived its request may keep the state alive, never the engine
    // running. What holds the workspace goes with the last holder.
    state.engine.stop().await;
    let holders = Arc::strong_count(&state);
    if holders > 1 {
        tracing::warn!(
            target: "bisa_node",
            holders = holders - 1,
            "the node stopped while something still held its state; the engine is stopped, the workspace is released when it lets go"
        );
    }
    Ok(())
}

fn router(state: Shared) -> Router {
    #[allow(unused_mut)]
    let mut extra = Router::new();
    #[cfg(feature = "a2a")]
    if state.a2a.is_some() {
        extra = extra.merge(a2a::routes());
    }
    extra
        .route("/health", get(health))
        .route("/events", get(events))
        .route("/node", get(node_info))
        .route("/workspace", get(workspace_info))
        .merge(collab::routes())
        .merge(goals::routes())
        .merge(workflows::routes())
        .merge(runs::routes())
        .merge(work_items::routes())
        .merge(projects::routes())
        .merge(conversation::routes())
        .merge(conversations::routes())
        .merge(changes::routes())
        .merge(notes::routes())
        .merge(drawings::routes())
        .merge(pets::routes())
        .merge(addons::routes())
        .merge(attachments::routes())
        .merge(inbox::routes())
        .merge(pulse::routes())
        .merge(agents_api::routes())
        .merge(catalog::routes())
        .merge(skills::routes())
        .merge(mcp::routes())
        .merge(tags::routes())
        .merge(listening::routes())
        .merge(usage::routes())
        .merge(files::routes())
        .merge(hooks::routes())
        .merge(admin::routes())
        .merge(ssh::routes())
        .merge(settings::routes())
        .merge(logs::routes())
        .merge(network::routes())
        .merge(git_config::routes())
        .merge(ide::routes())
        .merge(ide::interactive::routes())
        .merge(ide::lsp::routes())
        .merge(ide::serve::routes())
        .merge(browser::routes())
        .merge(mobile_development::routes())
        .merge(review::routes())
        .merge(codehost::routes())
        .merge(connectors::routes())
        .merge(security::routes())
        .merge(decisions::routes())
        .merge(readiness::routes())
        // The token, on every route but the named exceptions. Inside the
        // CORS layer, so a preflight is answered without one.
        .layer(axum::middleware::from_fn_with_state(
            Arc::clone(&state),
            auth::require_token,
        ))
        // The request's language: an error body's sentence re-said in it on
        // the way out (17 — Internationalisation). Outside the token check,
        // so a 401 is said in it too; inside CORS, so a preflight is not.
        .layer(axum::middleware::from_fn(i18n::localize))
        // Permissive CORS lets the Vite dev server (a different loopback
        // origin) call the API during `tauri dev`; the token is what
        // authenticates, not the origin.
        .layer(tower_http::cors::CorsLayer::permissive())
        // One `debug` span per request, the method and the path alone: the
        // default span records the whole URI, and `?token=` is the bearer
        // an `EventSource` sends. A failure is the 5xx line `ApiError`
        // writes, so the layer's own failure line stays at debug.
        .layer(
            tower_http::trace::TraceLayer::new_for_http()
                .make_span_with(|req: &axum::http::Request<axum::body::Body>| {
                    tracing::debug_span!(
                        "request",
                        method = %req.method(),
                        path = %req.uri().path()
                    )
                })
                .on_failure(
                    tower_http::trace::DefaultOnFailure::new().level(tracing::Level::DEBUG),
                ),
        )
        // Outermost, so a panic anywhere under it — a handler, the token
        // check — is one 500 and never the process.
        .layer(catch_panics())
        .with_state(state)
}

/// A handler that panicked answers the same JSON body every other 500 has,
/// and the node lives. The panic hook (`bisa-log`) has already written
/// the panic with its location; `ApiError::into_response` writes the 500.
#[derive(Clone, Copy)]
struct PanicToJson;

impl tower_http::catch_panic::ResponseForPanic for PanicToJson {
    type ResponseBody = axum::body::Body;

    fn response_for_panic(
        &mut self,
        err: Box<dyn std::any::Any + Send + 'static>,
    ) -> axum::http::Response<Self::ResponseBody> {
        // for the log: `internal` says the generic refusal to the person and logs this.
        ApiError::internal(&format!(
            "the node's handler panicked: {}",
            bisa_log::panic_message(&*err)
        ))
        .into_response()
    }
}

fn catch_panics() -> tower_http::catch_panic::CatchPanicLayer<PanicToJson> {
    tower_http::catch_panic::CatchPanicLayer::custom(PanicToJson)
}

// ---------------------------------------------------------------------------
// Error plumbing
// ---------------------------------------------------------------------------

/// A failed request: the status, the sentence a person reads — as a [`Text`],
/// the message and its arguments, rendered in the request's language by the
/// `i18n::localize` middleware and in English here for the log — and, when
/// the refusal is a definition's problems, the problems themselves, by step
/// and kind, so a designer selects the step rather than parsing prose. The
/// body is [`dto::ErrorBody`] ([17](../../../docs/architecture/17-internationalisation.md)).
pub(crate) struct ApiError {
    pub status: StatusCode,
    pub text: Text,
    /// Which refusal, for the ones a client renders differently (`dto::ErrorCode`).
    pub code: Option<dto::ErrorCode>,
    /// The problems and the typed facts, boxed: most refusals carry neither,
    /// and every handler's `Result` is the size of its `Err` — an `ApiError`
    /// stays small so a route's answer is not paid for by its refusal.
    facts: Option<Box<ErrorFacts>>,
}

/// What a refusal carries beyond its sentence: a definition's problems, and
/// the typed facts behind a coded refusal.
#[derive(Default)]
struct ErrorFacts {
    problems: Vec<bisa_core::Problem>,
    detail: Option<serde_json::Value>,
}

impl ApiError {
    /// A refusal: its status and its sentence as data.
    pub(crate) fn text(status: StatusCode, text: Text) -> Self {
        ApiError {
            status,
            text,
            code: None,
            facts: None,
        }
    }

    /// A malfunction: the fault goes to the log, in full and in English;
    /// the person gets the one sentence every 500 has — the log has the rest.
    pub(crate) fn internal(fault: &dyn std::fmt::Display) -> Self {
        tracing::error!(target: "bisa_node", "{fault}");
        ApiError::text(
            StatusCode::INTERNAL_SERVER_ERROR,
            bisa_core::text!("error-node-internal"),
        )
    }

    fn facts_mut(&mut self) -> &mut ErrorFacts {
        self.facts.get_or_insert_with(Default::default)
    }

    /// The typed facts behind a coded refusal — what a client acts on
    /// without parsing the sentence.
    pub(crate) fn with_detail(mut self, detail: serde_json::Value) -> Self {
        self.facts_mut().detail = Some(detail);
        self
    }

    /// A refusal a client tells apart from its neighbours on the same status.
    pub(crate) fn coded(status: StatusCode, code: dto::ErrorCode, text: Text) -> Self {
        ApiError {
            code: Some(code),
            ..ApiError::text(status, text)
        }
    }

    pub(crate) fn with_problems(
        status: StatusCode,
        text: Text,
        problems: Vec<bisa_core::Problem>,
    ) -> Self {
        let mut e = ApiError::text(status, text);
        e.facts_mut().problems = problems;
        e
    }
}

#[cfg(test)]
mod the_socket {
    /// The socket is at its name only as a socket that answers, the
    /// owner's alone, and the name it was bound under is gone.
    #[tokio::test]
    async fn a_socket_is_at_its_name_once_it_listens_and_is_the_owners_alone() {
        use std::os::unix::fs::{FileTypeExt as _, PermissionsExt as _};
        let dir = tempfile::tempdir().expect("a folder");
        let path = dir.path().join("run").join("node.sock");
        // A file an earlier node left at the name is replaced, not tripped on.
        std::fs::create_dir_all(path.parent().expect("its folder")).expect("the folder");
        std::fs::write(&path, b"left behind").expect("a stale file");

        let (listener, actual) = crate::bind_socket(&path).expect("the socket");
        assert_eq!(actual, path);
        let meta = std::fs::metadata(&path).expect("the socket's file");
        assert!(meta.file_type().is_socket());
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        let names: Vec<String> = std::fs::read_dir(path.parent().expect("its folder"))
            .expect("the folder")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["node.sock".to_string()], "nothing else is left");

        // It takes a call the moment it is there.
        let called = tokio::net::UnixStream::connect(&path).await;
        assert!(called.is_ok(), "{called:?}");
        let accepted = listener.accept().await;
        assert!(accepted.is_ok(), "{accepted:?}");
    }
}

#[cfg(test)]
mod api_error_size {
    /// Every handler returns `Result<_, ApiError>`, so the refusal's size is
    /// every answer's: the boxed facts keep it under a cache line.
    #[test]
    fn a_refusal_is_small() {
        assert!(std::mem::size_of::<super::ApiError>() <= 64);
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        // A 5xx is a malfunction and the log's business; a 4xx is a refusal
        // the caller asked for, said at debug. The line is the sentence in
        // English — never a body, never a value. The body carries the
        // English too, and the `Text` behind it; the `i18n::localize`
        // middleware re-says `error` in the request's language on the way out.
        if self.status.is_server_error() {
            tracing::error!(
                target: "bisa_node",
                status = self.status.as_u16(),
                code = ?self.code,
                "{}",
                self.text
            );
        } else {
            tracing::debug!(
                target: "bisa_node",
                status = self.status.as_u16(),
                code = ?self.code,
                "{}",
                self.text
            );
        }
        let facts = self.facts.map(|f| *f).unwrap_or_default();
        let body = dto::ErrorBody {
            error: self.text.to_string(),
            text: self.text,
            problems: facts.problems,
            code: self.code,
            detail: facts.detail,
        };
        let pending = i18n::Pending {
            status: self.status,
            body: body.clone(),
        };
        let mut response = (self.status, Json(body)).into_response();
        response.extensions_mut().insert(pending);
        response
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        ApiError::internal(&e)
    }
}

/// A JSON request body, refused as a **400 with the same error body every
/// other refusal has** when it does not fit — a misspelled key, a missing
/// field, a wrong type. Axum's own `Json` rejection is a 422 in prose; a
/// client reads one shape here.
/// Resolves when the stop is said — or when its sender is gone, which is a
/// stop too: a listener with nobody left to tell it to stop has no reason to
/// go on. The one shape every graceful shutdown here waits on.
pub(crate) async fn stopped(mut rx: tokio::sync::watch::Receiver<bool>) {
    if rx.wait_for(|s| *s).await.is_err() {
        tracing::debug!(target: "bisa_node", "the stop sender is gone; that is a stop");
    }
}

/// `stream`, for as long as the node is not asked to stop: the one way a
/// stream with no end of its own is answered, so the listener reads an end
/// when the node goes and the stop waits on nobody.
pub(crate) fn until_stopped<S: Stream>(state: &AppState, stream: S) -> impl Stream<Item = S::Item> {
    use futures::StreamExt as _;
    stream.take_until(stopped(state.stop.clone()))
}

pub(crate) struct Body<T>(pub T);

impl<S, T> axum::extract::FromRequest<S> for Body<T>
where
    S: Send + Sync,
    T: serde::de::DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(req: axum::extract::Request, state: &S) -> Result<Self, ApiError> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(value)) => Ok(Body(value)),
            // The documented contract (`docs/reference/http-api.md` § Status
            // codes): JSON that is not this shape — an unknown key, a missing
            // field, a wrong type — and JSON that is not JSON are a 400 with
            // the extractor's own sentence, never axum's 422.
            Err(r @ (JsonRejection::JsonDataError(_) | JsonRejection::JsonSyntaxError(_))) => {
                Err(ApiError::text(
                    StatusCode::BAD_REQUEST,
                    bisa_core::text!("error-node-body-unreadable", detail = r.body_text()),
                ))
            }
            Err(JsonRejection::MissingJsonContentType(_)) => Err(ApiError::text(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                bisa_core::text!("error-node-body-needs-json-content-type"),
            )),
            // The body could not be buffered: over the route's limit is the
            // 413 the client can act on; anything else is the read failing.
            Err(JsonRejection::BytesRejection(r)) => {
                let status = r.status();
                let said = if status == StatusCode::PAYLOAD_TOO_LARGE {
                    bisa_core::text!("error-node-body-too-large")
                } else {
                    bisa_core::text!("error-node-body-unreadable", detail = r.body_text())
                };
                Err(ApiError::text(status, said))
            }
            Err(other) => Err(ApiError::text(
                other.status(),
                bisa_core::text!("error-node-body-unreadable", detail = other.body_text()),
            )),
        }
    }
}

/// A query string, refused as a **400 with the same error body every other
/// refusal has** when it does not fit — a number that is no number, a field
/// the route needs and the address lacks. The framework's own `Query` says
/// it in prose; a client reads one shape here.
pub(crate) struct Query<T>(pub T);

impl<S, T> axum::extract::FromRequestParts<S> for Query<T>
where
    S: Send + Sync,
    T: serde::de::DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, ApiError> {
        match axum::extract::Query::<T>::from_request_parts(parts, state).await {
            Ok(axum::extract::Query(value)) => Ok(Query(value)),
            Err(refused) => Err(ApiError::text(
                StatusCode::BAD_REQUEST,
                bisa_core::text!("error-node-query-unreadable", detail = refused.body_text()),
            )),
        }
    }
}

/// A body a route may go without — `Option<Body<T>>`: a request that names
/// no content type carries none, and one that does is read as any other
/// body is, refused in the same words.
impl<S, T> axum::extract::OptionalFromRequest<S> for Body<T>
where
    S: Send + Sync,
    T: serde::de::DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(
        req: axum::extract::Request,
        state: &S,
    ) -> Result<Option<Self>, ApiError> {
        if !req.headers().contains_key(axum::http::header::CONTENT_TYPE) {
            return Ok(None);
        }
        <Self as axum::extract::FromRequest<S>>::from_request(req, state)
            .await
            .map(Some)
    }
}

impl From<bisa_store::StoreError> for ApiError {
    fn from(e: bisa_store::StoreError) -> Self {
        use bisa_store::StoreError as SE;
        match &e {
            SE::GoalNotFound(_)
            | SE::WorkItemNotFound(_)
            | SE::SessionNotFound(_)
            | SE::WorkstreamNotFound(_)
            | SE::ProjectNotFound(_)
            | SE::ConversationNotFound(_)
            | SE::WorkflowNotFound(_)
            | SE::RunNotFound(_)
            | SE::DefinitionNotFound { .. } => ApiError::text(StatusCode::NOT_FOUND, e.text()),
            // A record another shape of the code wrote: the lists leave it
            // out and a single read finds nothing readable — a 404 that says
            // why, and a malfunction the log keeps (the sentence names the
            // way out).
            SE::Unreadable { path, what, .. } => {
                tracing::warn!(target: "bisa_node", path = %path, what, "an unreadable record was asked for");
                ApiError::text(StatusCode::NOT_FOUND, e.text())
            }
            // Deleting `general`, disabling the core agent: the record is
            // fine and the request is not — a conflict with what the
            // workspace is, not a malformed body.
            SE::Channel(bisa_core::ChannelError::Permanent { .. })
            | SE::Agent(bisa_core::AgentError::CoreCannotBeRemoved)
            | SE::Agent(bisa_core::AgentError::CoreCannotBeDisabled) => {
                ApiError::text(StatusCode::CONFLICT, e.text())
            }
            // The run said no: the step is not where the caller thinks it
            // is, or a run is still unfinished.
            SE::Run(_) | SE::RunNotFinished { .. } | SE::RunNotQueued { .. } | SE::WorkItem(_) => {
                ApiError::text(StatusCode::CONFLICT, e.text())
            }
            // The workstream's table said no — named, so a client never reads
            // it as the project's publishing policy.
            SE::Workstream(_) => ApiError::coded(
                StatusCode::CONFLICT,
                dto::ErrorCode::WorkstreamState,
                e.text(),
            ),
            // A definition that cannot start: the message spells the problems
            // out, and the body carries them typed.
            SE::WorkflowInvalid(problems) => {
                ApiError::with_problems(StatusCode::BAD_REQUEST, e.text(), problems.clone())
            }
            // Two editors, a promotion of what is already promoted, a delete
            // of what is still in use: the request is fine and what it is
            // about has moved, or is held.
            SE::RevisionConflict { .. } | SE::AlreadyLibrary(_) | SE::StillUsed(_) => {
                ApiError::text(StatusCode::CONFLICT, e.text())
            }
            // The inputs do not fit the definition: the caller's to fix.
            SE::Inputs(_) => ApiError::text(StatusCode::BAD_REQUEST, e.text()),
            _ if e.is_refusal() => ApiError::text(StatusCode::BAD_REQUEST, e.text()),
            _ => ApiError::internal(&e),
        }
    }
}

impl From<bisa_engine::EngineError> for ApiError {
    /// An engine refusal is usually the caller's business, not a server
    /// fault: a manual-publishing project, a declined gate and a clean tree
    /// are all things a client should render, so they get 4xx codes and their
    /// own message rather than a flat 500.
    fn from(e: bisa_engine::EngineError) -> Self {
        use bisa_engine::EngineError as EE;
        // A store error keeps the mapping it already has — one place decides
        // what a missing goal means over HTTP.
        if let EE::Store(store) = e {
            return ApiError::from(store);
        }
        match &e {
            EE::Vcs(bisa_vcs::VcsError::RepositoryBusy { .. }) => ApiError::coded(
                StatusCode::CONFLICT,
                dto::ErrorCode::RepositoryBusy,
                e.text(),
            ),
            // No git on this machine: the service is not there, whatever was asked.
            EE::Vcs(bisa_vcs::VcsError::NotAvailable(_)) => {
                ApiError::text(StatusCode::SERVICE_UNAVAILABLE, e.text())
            }
            // git did not answer in its budget: a wedged repository, not a bad request.
            EE::Vcs(bisa_vcs::VcsError::Timeout { .. }) => {
                ApiError::text(StatusCode::GATEWAY_TIMEOUT, e.text())
            }
            // An answer the caller got wrong is the caller's fault, not this
            // node's. Without this arm every refusal — an unknown option id,
            // a selection on a decision gate, an empty answer — fell through
            // to a 500, so a client could not tell "you sent me nonsense"
            // from "I broke". The message was right the whole time; only the
            // status lied.
            EE::Invalid(_)
            | EE::Vcs(_)
            | EE::IdentityUnset { .. }
            | EE::RepoIdentityUnset { .. }
            | EE::ProjectAmbiguous { .. }
            | EE::Answer(_)
            | EE::Core(_)
            | EE::Template(_) => ApiError::text(StatusCode::BAD_REQUEST, e.text()),
            // A file saved under the caller: what is there now comes back.
            EE::FileConflict { .. } => ApiError::text(StatusCode::CONFLICT, e.text()),
            // The drawings repository has no pull to offer: the verb is not
            // there, not the request wrong.
            EE::PullNotOffered { .. } => ApiError::text(StatusCode::METHOD_NOT_ALLOWED, e.text()),
            // Refused for now — the workspace's state, not the request:
            // try again later, not a bad request.
            EE::Conflict(_) => ApiError::text(StatusCode::CONFLICT, e.text()),
            // The repository is not in a state a workstream can be cut from —
            // an unborn HEAD, a worktree that could not be made: its state,
            // not the caller's request and not this node's fault.
            EE::Iso(_) => ApiError::text(StatusCode::CONFLICT, e.text()),
            // A backend that is there and failed — the disk, the permissions:
            // this node's failure, said as one.
            EE::IsoFailed(_) => ApiError::internal(&e),
            // Another engine holds the workspace: try again, not a bug here.
            EE::Locked { .. } => ApiError::text(StatusCode::SERVICE_UNAVAILABLE, e.text()),
            // The decision provider: what it is set up as, or what it did, is
            // named by the kind — a request or an answer that breaks the
            // contract is a 400; a provider that refused, or answered what
            // cannot be read, is a 502; one that is not set up, or busy, or
            // out of reach, a 503; one that did not answer in time a 504.
            EE::Provider(p) => {
                use bisa_engine::ProviderError as PE;
                let status = match p {
                    PE::Contract(_) => StatusCode::BAD_REQUEST,
                    PE::Refused { .. } | PE::Unreadable(_) => StatusCode::BAD_GATEWAY,
                    PE::Misconfigured(_) | PE::Busy { .. } | PE::Unreachable(_) => {
                        StatusCode::SERVICE_UNAVAILABLE
                    }
                    PE::TimedOut => StatusCode::GATEWAY_TIMEOUT,
                };
                ApiError::text(status, e.text())
            }
            // The run refused the event: a step that is not live, an
            // amendment touching a started step.
            EE::Run(_) => ApiError::text(StatusCode::CONFLICT, e.text()),
            // Refusals a client renders differently from one another, all on
            // one status: each carries its code.
            EE::NothingToCommit(_) => ApiError::coded(
                StatusCode::CONFLICT,
                dto::ErrorCode::NothingToCommit,
                e.text(),
            ),
            EE::NothingToPublish { .. } => ApiError::coded(
                StatusCode::CONFLICT,
                dto::ErrorCode::NothingToPublish,
                e.text(),
            ),
            EE::PublishManual { .. } => ApiError::coded(
                StatusCode::CONFLICT,
                dto::ErrorCode::PublishManual,
                e.text(),
            ),
            EE::PublishNoGoal { .. } => ApiError::coded(
                StatusCode::CONFLICT,
                dto::ErrorCode::PublishNoGoal,
                e.text(),
            ),
            EE::PublishDeclined { .. } => ApiError::coded(
                StatusCode::CONFLICT,
                dto::ErrorCode::PublishDeclined,
                e.text(),
            ),
            // Only an open pull request has a branch to take up.
            EE::PullRequestNotOpen { number, state } => ApiError::coded(
                StatusCode::CONFLICT,
                dto::ErrorCode::PullRequestState,
                e.text(),
            )
            .with_detail(serde_json::json!({ "number": number, "state": state })),
            // A workstream script refused: the dialog shows the phase and the
            // tail of what it printed, in place.
            EE::WorkstreamScript { phase, output, .. } => {
                ApiError::coded(StatusCode::CONFLICT, dto::ErrorCode::ScriptFailed, e.text())
                    .with_detail(serde_json::json!({ "phase": phase, "output": output }))
            }
            EE::UnknownGate(_) => ApiError::text(StatusCode::NOT_FOUND, e.text()),
            // The author's own pull request: a comment is fine, a verdict is a
            // reviewer's. Coded, so the screen offers the right verdicts.
            EE::OwnPullRequest { author } => ApiError::coded(
                StatusCode::CONFLICT,
                dto::ErrorCode::OwnPullRequest,
                e.text(),
            )
            .with_detail(serde_json::json!({ "author": author })),
            // A connector's refusals are the person's to act on — connect the
            // account, fix the definition or the step, wait out the limit —
            // and the platform's failures are the platform's.
            EE::Connector(c) => {
                use bisa_engine::connectors::ConnectorError as CE;
                let status = match c {
                    CE::NotAuthenticated(_) => StatusCode::UNAUTHORIZED,
                    CE::NotFound(_) => StatusCode::NOT_FOUND,
                    CE::BadDefinition(_)
                    | CE::BadParam { .. }
                    | CE::Unresolved(_)
                    | CE::SelectMissing { .. }
                    | CE::OAuth(_) => StatusCode::BAD_REQUEST,
                    CE::HostRefused { .. } | CE::Refused { .. } => StatusCode::CONFLICT,
                    CE::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
                    CE::Upstream { .. } | CE::Transport(_) | CE::Unreachable(_) => {
                        StatusCode::BAD_GATEWAY
                    }
                    CE::Timeout(_) => StatusCode::GATEWAY_TIMEOUT,
                    CE::Open { .. } => StatusCode::SERVICE_UNAVAILABLE,
                    CE::Store(_) | CE::TooLarge(_) => StatusCode::INTERNAL_SERVER_ERROR,
                };
                ApiError::text(status, e.text())
            }
            // A code host's refusals are the caller's to act on: sign in, name a
            // real pull request, resolve what stopped the merge. Only its
            // transport failure is ours.
            EE::CodeHost(bisa_engine::codehost::CodeHostError::NotAuthenticated(_)) => {
                ApiError::text(StatusCode::UNAUTHORIZED, e.text())
            }
            EE::CodeHost(bisa_engine::codehost::CodeHostError::NotFound(_)) => {
                ApiError::text(StatusCode::NOT_FOUND, e.text())
            }
            EE::CodeHost(bisa_engine::codehost::CodeHostError::Refused(_))
            | EE::CodeHost(bisa_engine::codehost::CodeHostError::Unsupported(_)) => {
                ApiError::text(StatusCode::CONFLICT, e.text())
            }
            EE::CodeHost(bisa_engine::codehost::CodeHostError::Transport(_)) => {
                ApiError::text(StatusCode::BAD_GATEWAY, e.text())
            }
            EE::GateAlreadyDecided(_) | EE::DesignRefused(_) => {
                ApiError::text(StatusCode::CONFLICT, e.text())
            }
            // SSH for git hosts: a refusal is the person's to act on (a name
            // taken, a key that wants a passphrase); a node without SSH, a
            // missing program and a timeout are the machine's.
            EE::Ssh(bisa_engine::ssh::SshError::Refused(_)) => {
                ApiError::text(StatusCode::BAD_REQUEST, e.text())
            }
            EE::Ssh(bisa_engine::ssh::SshError::Unavailable(_)) | EE::SshUnavailable(_) => {
                ApiError::text(StatusCode::SERVICE_UNAVAILABLE, e.text())
            }
            EE::Ssh(bisa_engine::ssh::SshError::Timeout { .. }) => {
                ApiError::text(StatusCode::GATEWAY_TIMEOUT, e.text())
            }
            EE::Ssh(bisa_engine::ssh::SshError::Failed { .. }) => {
                ApiError::text(StatusCode::BAD_GATEWAY, e.text())
            }
            // The mobile tools (ide/19): a node without them and a program
            // that is missing are the machine's; a device that is not there
            // is a 404; a call that makes no sense for it is the caller's.
            EE::MobileDevelopmentUnavailable(_)
            | EE::MobileDevelopment(
                bisa_engine::mobile_development::MobileDevelopmentError::NotInstalled(_),
            ) => ApiError::text(StatusCode::SERVICE_UNAVAILABLE, e.text()),
            EE::MobileDevelopment(
                bisa_engine::mobile_development::MobileDevelopmentError::Timeout { .. },
            ) => ApiError::text(StatusCode::GATEWAY_TIMEOUT, e.text()),
            EE::MobileDevelopment(
                bisa_engine::mobile_development::MobileDevelopmentError::Failed { .. },
            ) => ApiError::text(StatusCode::BAD_GATEWAY, e.text()),
            EE::MobileDevelopment(
                bisa_engine::mobile_development::MobileDevelopmentError::NoSuchDevice(_),
            ) => ApiError::text(StatusCode::NOT_FOUND, e.text()),
            EE::MobileDevelopment(
                bisa_engine::mobile_development::MobileDevelopmentError::Unsupported(_),
            ) => ApiError::text(StatusCode::BAD_REQUEST, e.text()),
            // Every other considered refusal is the caller's to read; only a
            // malfunction is a 500.
            _ if e.is_refusal() => ApiError::text(StatusCode::BAD_REQUEST, e.text()),
            _ => ApiError::internal(&e),
        }
    }
}

pub(crate) fn parse_id(s: &str) -> Result<GoalId, ApiError> {
    GoalId::from_str(s).map_err(|_| {
        bad_request(bisa_core::text!(
            "error-node-not-a-goal-id",
            value = format!("{s:?}")
        ))
    })
}

/// The three statuses a handler refuses with, each taking the sentence as
/// data; the words are `locales/en/errors.ftl`'s (`error-node-…`).
pub(crate) fn bad_request(text: Text) -> ApiError {
    ApiError::text(StatusCode::BAD_REQUEST, text)
}

pub(crate) fn not_found(text: Text) -> ApiError {
    ApiError::text(StatusCode::NOT_FOUND, text)
}

pub(crate) fn conflict(text: Text) -> ApiError {
    ApiError::text(StatusCode::CONFLICT, text)
}

/// Typed-id parsers for the slug ids on the wire. A bad id is the caller's
/// fault, and the message names the id.
pub(crate) fn agent_id(s: &str) -> Result<bisa_core::AgentId, ApiError> {
    bisa_core::AgentId::new(s).map_err(|e| bad_request(e.text()))
}

pub(crate) fn team_id(s: &str) -> Result<bisa_core::TeamId, ApiError> {
    bisa_core::TeamId::new(s).map_err(|e| bad_request(e.text()))
}

pub(crate) fn skill_id(s: &str) -> Result<bisa_core::SkillId, ApiError> {
    bisa_core::SkillId::new(s).map_err(|e| bad_request(e.text()))
}

pub(crate) fn mcp_id(s: &str) -> Result<bisa_core::McpId, ApiError> {
    bisa_core::McpId::new(s).map_err(|e| bad_request(e.text()))
}

pub(crate) fn channel_id(s: &str) -> Result<bisa_core::ChannelId, ApiError> {
    bisa_core::ChannelId::new(s).map_err(|e| bad_request(e.text()))
}

pub(crate) fn addon_id(s: &str) -> Result<bisa_core::AddonId, ApiError> {
    bisa_core::AddonId::new(s).map_err(|e| bad_request(e.text()))
}

// ---------------------------------------------------------------------------
// Root handlers
// ---------------------------------------------------------------------------

async fn health() -> Json<serde_json::Value> {
    Json(json!({"ok": true, "version": env!("CARGO_PKG_VERSION")}))
}

/// The frame a subscriber gets in place of what it missed: the bus dropped
/// `dropped` events because this client read too slowly. A `system` frame is
/// for the client's bus, not its subscribers — the desktop re-reads what it
/// shows, as it does after a reconnect.
fn lagged_frame(dropped: u64) -> Vec<serde_json::Value> {
    tracing::warn!(
        dropped,
        "an event stream client lagged; it is told to re-read"
    );
    vec![json!({"stream": "system", "payload": {"kind": "lagged", "dropped": dropped}})]
}

/// The Inbox row an engine payload moves, and so warrants an `inbox` frame
/// beside the engine one — `notices::target_of`: the goal a gate, a question,
/// a decision or a run's state names; the workstream, project or workflow a
/// notice concerns; the host of a listener that failed.
///
/// Deliberately not every payload: an agent streaming tokens moves nothing on
/// an inbox row, and a frame per token is what made the desktop refetch the
/// whole list while somebody was reading it.
fn inbox_key_of(ev: &bisa_engine::EngineEvent) -> Option<String> {
    bisa_engine::notices::target_of(ev).map(|t| t.id)
}

/// One SSE stream, tagged envelope per event. Engine events pass through;
/// store conversation events are bridged with a cheap inbox delta so list views
/// can update without refetching everything.
async fn events(
    State(state): State<Shared>,
) -> Sse<impl Stream<Item = Result<SseEvent, std::convert::Infallible>>> {
    use futures::StreamExt as _;
    let engine_rx = state.engine.events();
    let store_rx = state.engine.workspace().subscribe_store_events();
    let engine_st = Arc::clone(&state);
    let st = Arc::clone(&state);

    let engine_stream = tokio_stream::wrappers::BroadcastStream::new(engine_rx)
        .filter_map(move |e| {
            let st = Arc::clone(&engine_st);
            async move {
                let ev = match e {
                    Ok(ev) => ev,
                    Err(BroadcastStreamRecvError::Lagged(n)) => return Some(lagged_frame(n)),
                };
                let inbox = inbox_key_of(&ev).and_then(|key| inbox::delta(&st, &key, true));
                let mut frames = vec![json!({"stream": "engine", "payload": ev})];
                frames.extend(inbox.map(|p| json!({"stream": "inbox", "payload": p})));
                Some(frames)
            }
        })
        .flat_map(futures::stream::iter);

    let conversation_stream = tokio_stream::wrappers::BroadcastStream::new(store_rx)
        .filter_map(move |e| {
            let st = Arc::clone(&st);
            async move {
                use bisa_store::StoreEvent;
                let e = match e {
                    Ok(e) => e,
                    Err(BroadcastStreamRecvError::Lagged(n)) => return Some(lagged_frame(n)),
                };
                match e {
                    StoreEvent::ConversationAppended { scope, event, .. } => {
                        let snippet: String = event.content.chars().take(120).collect();
                        let unread = st
                            .engine
                            .workspace()
                            .unread_counts()
                            .ok()
                            .and_then(|c| c.into_iter().find(|(s, _)| *s == scope).map(|(_, n)| n))
                            .unwrap_or(0);
                        // What now stands as the room's last words, by the
                        // rule the lists answer by — a post takes the place,
                        // a retraction gives it back, a reaction leaves it —
                        // so a list that follows the bus mirrors nothing.
                        let latest = st
                            .engine
                            .workspace()
                            .latest_message(&scope)
                            .ok()
                            .flatten()
                            .map(|row| inbox::preview_of(&row));
                        let mut frames = vec![json!({
                            "stream": "conversation",
                            "payload": {
                                "scope": scope,
                                "kind": event.kind.as_u16(),
                                "event_id": event.id.to_hex(),
                                "author": event.pubkey.to_hex(),
                                "snippet": snippet,
                                "unread_count": unread,
                                "latest": latest,
                            }
                        })];
                        frames.extend(
                            inbox::delta(&st, &scope, false)
                                .map(|p| json!({"stream": "inbox", "payload": p})),
                        );
                        Some(frames)
                    }
                    // No `conversation` frame: nothing was posted, and a frame
                    // claiming otherwise would put a phantom line in every
                    // activity timeline listening for one. Only the row's own state moved.
                    StoreEvent::ReadMarkerSet { scope } => Some(
                        inbox::delta(&st, &scope, true)
                            .map(|p| vec![json!({"stream": "inbox", "payload": p})])
                            .unwrap_or_default(),
                    ),
                    StoreEvent::ConversationSnapshot { kind, d, .. } => Some(vec![json!({
                        "stream": "conversation",
                        "payload": {"scope": d, "kind": kind, "snapshot": true}
                    })]),
                    _ => None,
                }
            }
        })
        .flat_map(futures::stream::iter);

    let stream = futures::stream::select(engine_stream, conversation_stream)
        .filter_map(|v| async move { Some(Ok(SseEvent::default().json_data(&v).ok()?)) });
    Sse::new(until_stopped(&state, stream))
        .keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15)))
}

/// What this node is — the process, where it listens, where the workspace
/// lives, whether the engine is paused and how many sessions are live: the
/// footer's node overlay and anything else that asks *which node is this*.
async fn node_info(State(state): State<Shared>) -> Json<dto::NodeInfo> {
    let ws = state.engine.workspace();
    let live_sessions = state
        .engine
        .inner()
        .presence
        .snapshot()
        .iter()
        .filter(|row| row.state.is_live())
        .count();
    Json(dto::NodeInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        pid: std::process::id(),
        started_at: state.started_at,
        socket: state.socket.display().to_string(),
        listen: state.listen.map(|a| format!("http://{a}")),
        data_dir: ws.root().display().to_string(),
        logs_dir: ws.paths().logs_dir().display().to_string(),
        paused: state.engine.is_paused(),
        live_sessions,
    })
}

async fn workspace_info(State(state): State<Shared>) -> Result<Json<serde_json::Value>, ApiError> {
    use nostr::nips::nip19::ToBech32;
    let ws = state.engine.workspace();
    let pubkey = ws.owner_principal().to_string();
    let npub = ws.owner_keys().public_key().to_bech32().unwrap_or_default();
    let members = ws.members()?;
    Ok(Json(json!({
        "pubkey": pubkey,
        "npub": npub,
        "data_dir": ws.root(),
        // Where this machine's diagnostic log is — the store's word, so the
        // desktop shell never spells a workspace folder itself.
        "logs_dir": ws.paths().logs_dir(),
        "members": members,
    })))
}

#[cfg(test)]
mod panics {
    use super::*;
    use axum::routing::get;
    use tower::ServiceExt;

    /// A route that panics answers a 500 with the node's own error body, and
    /// the next request on the same router is served — the shipped node
    /// unwinds and never aborts.
    #[tokio::test]
    async fn a_panicking_handler_answers_500_and_the_node_lives() {
        let router: Router = Router::new()
            .route(
                "/boom",
                get(|| async {
                    if std::env::var_os("BISA_NEVER_SET").is_none() {
                        panic!("kaboom");
                    }
                    "never"
                }),
            )
            .route("/ok", get(|| async { "fine" }))
            .layer(catch_panics());
        let res = router
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/boom")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        // The generic refusal, never the panic's words: those are the log's (`ApiError::internal`).
        assert_eq!(body["text"]["id"], "error-node-internal", "{body}");

        let res = router
            .oneshot(
                axum::http::Request::builder()
                    .uri("/ok")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }
}

#[cfg(test)]
mod error_mapping {
    //! The refusals a client tells apart carry a code; a status alone never
    //! decides what a person is told.
    use super::ApiError;
    use crate::dto::ErrorCode;
    use axum::http::StatusCode;
    use bisa_engine::EngineError;

    fn mapped(e: EngineError) -> (StatusCode, Option<ErrorCode>) {
        let api = ApiError::from(e);
        (api.status, api.code)
    }

    #[test]
    fn each_publishing_refusal_is_a_409_with_its_own_code() {
        assert_eq!(
            mapped(EngineError::PublishManual {
                project: "storefront".into(),
                what: "push work/x".into()
            }),
            (StatusCode::CONFLICT, Some(ErrorCode::PublishManual))
        );
        assert_eq!(
            mapped(EngineError::PublishNoGoal {
                project: "storefront".into(),
                workstream: "01H".into(),
                what: "push work/x".into()
            }),
            (StatusCode::CONFLICT, Some(ErrorCode::PublishNoGoal))
        );
        assert_eq!(
            mapped(EngineError::PublishDeclined {
                what: "push work/x".into()
            }),
            (StatusCode::CONFLICT, Some(ErrorCode::PublishDeclined))
        );
        assert_eq!(
            mapped(EngineError::NothingToPublish {
                workstream: "01H".into(),
                branch: "work/x".into(),
                base: "main".into()
            }),
            (StatusCode::CONFLICT, Some(ErrorCode::NothingToPublish))
        );
        assert_eq!(
            mapped(EngineError::NothingToCommit("01H".into())),
            (StatusCode::CONFLICT, Some(ErrorCode::NothingToCommit))
        );
    }

    /// No isolation backend on this machine is its state — a refusal (409);
    /// a backend that is here and failed is this node's fault (500), said
    /// without the backend's own words, which go to the log.
    #[test]
    fn a_missing_isolation_backend_is_a_refusal_and_a_failed_one_is_this_nodes_fault() {
        let missing = ApiError::from(EngineError::Iso("no isolation backend: none".into()));
        assert_eq!(missing.status, StatusCode::CONFLICT);
        assert!(missing.text.to_string().contains("no isolation backend"));
        let failed = ApiError::from(EngineError::IsoFailed("no space left on device".into()));
        assert_eq!(failed.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(
            !failed.text.to_string().contains("no space left"),
            "what failed inside is the log's, not the answer's"
        );
    }

    #[test]
    fn a_workstream_table_refusal_is_named_and_never_reads_as_a_policy() {
        let store = bisa_store::StoreError::Workstream(bisa_core::WorkstreamError::Illegal {
            from: "open",
            transition: "pr_opened",
        });
        let api = ApiError::from(store);
        assert_eq!(api.status, StatusCode::CONFLICT);
        assert_eq!(api.code, Some(ErrorCode::WorkstreamState));
        assert!(api
            .text
            .to_string()
            .contains("cannot pr_opened a workstream that is open"));
    }

    #[test]
    fn a_plain_refusal_carries_no_code_and_the_body_omits_the_field() {
        let api = ApiError::from(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-nope"
        )));
        assert_eq!((api.status, api.code), (StatusCode::BAD_REQUEST, None));
        let body = serde_json::to_value(crate::dto::ErrorBody {
            error: api.text.to_string(),
            text: api.text.clone(),
            problems: Vec::new(),
            code: api.code,
            detail: None,
        })
        .unwrap();
        assert_eq!(
            body,
            serde_json::json!({"error": "nope", "text": {"id": "error-engine-invalid-nope"}})
        );
        let coded = serde_json::to_value(crate::dto::ErrorBody {
            error: "x".into(),
            text: bisa_core::text!("error-engine-invalid-nope"),
            problems: Vec::new(),
            code: Some(ErrorCode::PublishManual),
            detail: None,
        })
        .unwrap();
        assert_eq!(coded["code"], serde_json::json!("publish_manual"));
    }
}
