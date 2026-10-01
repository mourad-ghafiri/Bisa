//! Language servers, supervised by the engine (ide/10).
//!
//! One server per `(root, language)`, started when the first document of that
//! language opens in a workbench and stopped `lsp.idle_ttl_secs` after the
//! last one closes. Crash-loop containment: three starts in a rolling minute,
//! then `Failed` with the reason until a person asks for a restart. Nothing
//! runs because an index is open; nothing is installed by the platform.
//!
//! The editor speaks root-relative paths; the server speaks `file://` URIs.
//! Both directions are rewritten here, so the webview never learns an
//! absolute path from a server.

use crate::ide::files::writable_root;
use crate::{EngineError, EngineEvent, EnginePayload, Inner};
use bisa_lsp::catalog::{self, ServerDescriptor};
use bisa_lsp::server::{Server, ServerState};
use bisa_lsp::{uri, LspError};
use bisa_store::FileScope;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Starts allowed per root+language in a rolling minute before giving up.
pub const MAX_STARTS_PER_MINUTE: usize = 3;
/// Requests the editor may proxy. A method not here is a 400, not a guess.
pub const ALLOWED_METHODS: &[&str] = &[
    "textDocument/hover",
    "textDocument/definition",
    "textDocument/references",
    "textDocument/documentSymbol",
    "textDocument/formatting",
    "workspace/symbol",
];

/// A server is one per root and language.
type ServerKey = (PathBuf, String);
/// What a start of one waits behind: held by the caller that is starting it.
type StartGate = Arc<tokio::sync::Mutex<()>>;

struct Managed {
    server: Arc<Server>,
    /// Open documents by relative path, with their sync version.
    docs: Mutex<HashMap<String, i64>>,
    /// When the last document closed; the TTL sweeper reads it.
    idle_since: Mutex<Option<Instant>>,
}

#[derive(Default)]
pub struct LspRegistry {
    servers: Mutex<HashMap<(PathBuf, String), Arc<Managed>>>,
    starts: Mutex<HashMap<(PathBuf, String), Vec<Instant>>>,
    /// Given up after a crash loop, with the reason, until `restart`.
    failed: Mutex<HashMap<(PathBuf, String), String>>,
    /// One start at a time per root and language. A workbench that restores
    /// its tabs opens every document at once: without this each would find
    /// no server and start its own — the last kept, the others ended with
    /// the documents opened on them, and the fourth counted as a crash loop.
    /// Whoever comes second waits for the first's server and is given it.
    starting: Mutex<HashMap<ServerKey, StartGate>>,
}

/// One row of the status answer.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ServerStatusRow {
    pub language: String,
    pub command: String,
    pub available: bool,
    pub install_hint: Option<String>,
    pub state: ServerState,
    pub documents: usize,
}

fn err_lsp(e: LspError) -> EngineError {
    EngineError::Invalid(bisa_core::text!(
        "error-engine-lsp-refused",
        detail = e.to_string()
    ))
}

fn enabled(inner: &Inner) -> bool {
    inner
        .ws
        .setting("lsp.enabled", None)
        .map(|r| r.value.as_bool().unwrap_or(true))
        .unwrap_or(true)
}

fn descriptors(inner: &Inner) -> Vec<ServerDescriptor> {
    inner
        .ws
        .setting("lsp.servers", None)
        .map(|r| catalog::user_descriptors(&r.value))
        .unwrap_or_default()
}

fn idle_ttl(inner: &Inner) -> Duration {
    let secs = inner
        .ws
        .setting("lsp.idle_ttl_secs", None)
        .ok()
        .and_then(|r| r.value.as_u64())
        .unwrap_or(300);
    Duration::from_secs(secs.clamp(30, 3600))
}

fn key(root: &Path, language: &str) -> (PathBuf, String) {
    (root.to_path_buf(), language.to_string())
}

/// Whether a server has idled its whole time to live: no document is open,
/// and none was opened since the last one closed `ttl` ago. A document
/// opened in between clears `idle_since`, and one closed again sets it
/// afresh — so an earlier close's sweep finds the server not yet due.
fn gone_idle(
    idle_since: Option<Instant>,
    open_documents: usize,
    ttl: Duration,
    now: Instant,
) -> bool {
    open_documents == 0
        && idle_since.is_some_and(|since| now.saturating_duration_since(since) >= ttl)
}

impl LspRegistry {
    fn get(&self, k: &(PathBuf, String)) -> Option<Arc<Managed>> {
        self.servers.lock().ok()?.get(k).cloned()
    }

    fn record_start(&self, k: &(PathBuf, String)) -> bool {
        let Ok(mut starts) = self.starts.lock() else {
            return false;
        };
        let now = Instant::now();
        // Every ledger forgets what is older than the minute, and a ledger
        // with nothing left goes — a root a person closed long ago keeps no
        // row here.
        starts.retain(|key, v| {
            v.retain(|t| now.duration_since(*t) < Duration::from_secs(60));
            key == k || !v.is_empty()
        });
        let v = starts.entry(k.clone()).or_default();
        if v.len() >= MAX_STARTS_PER_MINUTE {
            return false;
        }
        v.push(now);
        true
    }

    fn remove(&self, k: &(PathBuf, String)) -> Option<Arc<Managed>> {
        self.servers.lock().ok()?.remove(k)
    }

    /// The gate a start of this root and language goes through.
    fn start_gate(&self, k: &ServerKey) -> StartGate {
        let mut gates = self.starting.lock().unwrap_or_else(|e| e.into_inner());
        Arc::clone(gates.entry(k.clone()).or_default())
    }
}

/// The running server for a root+language, started if needed. `Ok(None)`
/// when no server is configured for the language or servers are off.
async fn server_for(
    inner: &Arc<Inner>,
    scope: FileScope,
    id: &str,
    root: &Path,
    language: &str,
) -> Result<Option<Arc<Managed>>, EngineError> {
    if !enabled(inner) {
        return Ok(None);
    }
    let k = key(root, language);
    // Found running, or started — by one caller at a time.
    let gate = inner.lsp.start_gate(&k);
    let _one_start = gate.lock().await;
    if let Some(m) = inner.lsp.get(&k) {
        if matches!(
            m.server.state(),
            ServerState::Running | ServerState::Starting
        ) {
            return Ok(Some(m));
        }
        // It died: forget it and fall through to a restart, bounded below.
        if let ServerState::Failed { reason } = m.server.state() {
            tracing::warn!(language, "language server failed: {reason}");
        }
        inner.lsp.remove(&k);
    }
    if inner
        .lsp
        .failed
        .lock()
        .ok()
        .is_some_and(|f| f.contains_key(&k))
    {
        return Ok(None);
    }
    let Some(desc) = catalog::resolve(language, &descriptors(inner)) else {
        return Ok(None);
    };
    if !inner.lsp.record_start(&k) {
        let reason = format!(
            "{} restarted {MAX_STARTS_PER_MINUTE} times in a minute; not started again until you ask",
            desc.command
        );
        if let Ok(mut f) = inner.lsp.failed.lock() {
            f.insert(k.clone(), reason.clone());
        }
        emit(
            inner,
            scope,
            id,
            language,
            "bisa/serverFailed",
            json!({"reason": reason}),
        );
        return Ok(None);
    }
    let server = match Server::start(root, desc.clone()).await {
        Ok(s) => s,
        Err(e) => {
            emit(
                inner,
                scope,
                id,
                language,
                "bisa/serverFailed",
                json!({"reason": e.to_string()}),
            );
            return Err(err_lsp(e));
        }
    };
    // It started: an earlier verdict — a crash loop the person asked to
    // forget, a start that failed — is no longer the state of things.
    if let Ok(mut f) = inner.lsp.failed.lock() {
        f.remove(&k);
    }
    let managed = Arc::new(Managed {
        server: Arc::clone(&server),
        docs: Mutex::new(HashMap::new()),
        idle_since: Mutex::new(None),
    });
    if let Ok(mut m) = inner.lsp.servers.lock() {
        m.insert(k.clone(), Arc::clone(&managed));
    }
    // The pump: notifications → the engine stream, URIs rewritten.
    if let Some(mut rx) = server.take_notifications() {
        let inner = Arc::clone(inner);
        let (root, language, id) = (root.to_path_buf(), language.to_string(), id.to_string());
        tokio::spawn(async move {
            while let Some(n) = rx.recv().await {
                if matches!(
                    n.method.as_str(),
                    "window/logMessage" | "$/progress" | "telemetry/event"
                ) {
                    continue;
                }
                let params = uri::outbound(&root, n.params);
                emit(&inner, scope, &id, &language, &n.method, params);
            }
            emit(
                &inner,
                scope,
                &id,
                &language,
                "bisa/serverStopped",
                Value::Null,
            );
        });
    }
    emit(
        inner,
        scope,
        id,
        language,
        "bisa/serverStarted",
        json!({"command": desc.command}),
    );
    Ok(Some(managed))
}

fn emit(inner: &Inner, scope: FileScope, id: &str, language: &str, method: &str, params: Value) {
    inner.emit(EngineEvent::global(EnginePayload::Lsp {
        scope: scope.as_str().to_string(),
        id: id.to_string(),
        language: language.to_string(),
        method: method.to_string(),
        params,
    }));
}

/// A document path the editor names is contained by the root like every file
/// route's: one that leaves it — `../`, an absolute path, a symlink out — is
/// refused before it becomes a URI a server would read.
fn contained(root: &std::path::Path, path: &str) -> Result<(), EngineError> {
    bisa_store::resolve_within(root, path)?;
    Ok(())
}

/// A document opened in the editor. Starts the server when it is the first
/// of its language in this root. `Ok(None)` when no server applies.
pub async fn open(
    inner: &Arc<Inner>,
    scope: FileScope,
    id: &str,
    path: &str,
    text: &str,
) -> Result<DocumentOpened, EngineError> {
    let Some(language) = catalog::language_for_path(path) else {
        return Ok(DocumentOpened::no_language());
    };
    let root = writable_root(inner, scope, id)?;
    contained(&root, path)?;
    let Some(m) = server_for(inner, scope, id, &root, language).await? else {
        return Ok(DocumentOpened::unfollowed(language));
    };
    let version = {
        let mut docs = m
            .docs
            .lock()
            .map_err(|_| EngineError::Invalid(bisa_core::text!("error-engine-invalid-lsp-lock")))?;
        let v = docs.entry(path.to_string()).or_insert(0);
        *v += 1;
        *v
    };
    if let Ok(mut idle) = m.idle_since.lock() {
        *idle = None;
    }
    m.server
        .did_open(&uri::to_uri(&root, path), language, version, text)
        .await
        .map_err(err_lsp)?;
    Ok(DocumentOpened::followed(language))
}

/// What an open came to: the language the path is, when the catalog knows
/// one, and whether a server of it now follows the document. A language
/// whose server did not take it — servers off at this machine, no server
/// configured for it, a crash loop given up on — is still named, so the
/// editor knows which server's start to open the document on again; `None`
/// is a path no language applies to, which nothing will ever follow.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DocumentOpened {
    pub language: Option<String>,
    pub following: bool,
}

impl DocumentOpened {
    fn no_language() -> Self {
        Self {
            language: None,
            following: false,
        }
    }

    fn unfollowed(language: &str) -> Self {
        Self {
            language: Some(language.to_string()),
            following: false,
        }
    }

    fn followed(language: &str) -> Self {
        Self {
            language: Some(language.to_string()),
            following: true,
        }
    }
}

/// The editor's buffer changed — full text, so diagnostics reflect what is
/// typed, not what is saved.
pub async fn change(
    inner: &Arc<Inner>,
    scope: FileScope,
    id: &str,
    path: &str,
    text: &str,
) -> Result<(), EngineError> {
    let Some(language) = catalog::language_for_path(path) else {
        return Ok(());
    };
    let root = writable_root(inner, scope, id)?;
    contained(&root, path)?;
    let Some(m) = inner.lsp.get(&key(&root, language)) else {
        // Not open here: treat as an open, which starts the server if needed.
        return open(inner, scope, id, path, text).await.map(|_| ());
    };
    let version = {
        let mut docs = m
            .docs
            .lock()
            .map_err(|_| EngineError::Invalid(bisa_core::text!("error-engine-invalid-lsp-lock")))?;
        let v = docs.entry(path.to_string()).or_insert(0);
        *v += 1;
        *v
    };
    m.server
        .did_change(&uri::to_uri(&root, path), version, text)
        .await
        .map_err(err_lsp)
}

/// The document closed. The last one of its language arms the idle TTL.
pub async fn close(
    inner: &Arc<Inner>,
    scope: FileScope,
    id: &str,
    path: &str,
) -> Result<(), EngineError> {
    let Some(language) = catalog::language_for_path(path) else {
        return Ok(());
    };
    let root = writable_root(inner, scope, id)?;
    contained(&root, path)?;
    let k = key(&root, language);
    let Some(m) = inner.lsp.get(&k) else {
        return Ok(());
    };
    let remaining = {
        let mut docs = m
            .docs
            .lock()
            .map_err(|_| EngineError::Invalid(bisa_core::text!("error-engine-invalid-lsp-lock")))?;
        docs.remove(path);
        docs.len()
    };
    if let Err(e) = m.server.did_close(&uri::to_uri(&root, path)).await {
        tracing::debug!(language, "didClose not delivered: {e}");
    }
    if remaining == 0 {
        if let Ok(mut idle) = m.idle_since.lock() {
            *idle = Some(Instant::now());
        }
        let ttl = idle_ttl(inner);
        let inner = Arc::clone(inner);
        tokio::spawn(async move {
            tokio::time::sleep(ttl).await;
            let Some(m) = inner.lsp.get(&k) else { return };
            let still_idle = gone_idle(
                m.idle_since.lock().ok().and_then(|i| *i),
                m.docs.lock().map(|d| d.len()).unwrap_or(0),
                ttl,
                Instant::now(),
            );
            if still_idle {
                inner.lsp.remove(&k);
                m.server.shutdown().await;
                tracing::info!(language = %k.1, "language server stopped after idling");
            }
        });
    }
    Ok(())
}

/// Proxy one request. The method must be on the allow list; `uri` fields in
/// the params are root-relative and rewritten both ways.
pub async fn request(
    inner: &Arc<Inner>,
    scope: FileScope,
    id: &str,
    path: &str,
    method: &str,
    params: Value,
) -> Result<Value, EngineError> {
    if !ALLOWED_METHODS.contains(&method) {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-not-request-editor-may-proxy",
            method = method.to_string()
        )));
    }
    let Some(language) = catalog::language_for_path(path) else {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-no-language-server-applies",
            path = path.to_string()
        )));
    };
    let root = writable_root(inner, scope, id)?;
    contained(&root, path)?;
    let Some(m) = server_for(inner, scope, id, &root, language).await? else {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-no-language-server-configured",
            language = language.to_string()
        )));
    };
    let params = uri::inbound(&root, params);
    let answer = m.server.request(method, params).await.map_err(err_lsp)?;
    Ok(uri::outbound(&root, answer))
}

/// Every language a server is configured for — the presets and this
/// machine's own descriptors, one a language — with its server's state in
/// this root, its open documents and whether its command is installed: what
/// the editor's status chip and Settings read.
pub async fn status(
    inner: &Arc<Inner>,
    scope: FileScope,
    id: &str,
) -> Result<Vec<ServerStatusRow>, EngineError> {
    let root = writable_root(inner, scope, id)?;
    let user = descriptors(inner);
    let mut rows = Vec::new();
    let mut all: Vec<ServerDescriptor> = catalog::presets();
    for d in &user {
        all.retain(|p| p.language != d.language);
        all.push(d.clone());
    }
    for d in all {
        let k = key(&root, &d.language);
        let running = inner.lsp.get(&k);
        let given_up = inner
            .lsp
            .failed
            .lock()
            .ok()
            .and_then(|f| f.get(&k).cloned());
        let state = match (&running, given_up) {
            (Some(m), _) => m.server.state(),
            (None, Some(reason)) => ServerState::Failed { reason },
            (None, None) => ServerState::Stopped,
        };
        let available = tokio::task::spawn_blocking({
            let cmd = d.command.clone();
            move || catalog::available(&cmd)
        })
        .await
        .unwrap_or(false);
        rows.push(ServerStatusRow {
            documents: running
                .as_ref()
                .and_then(|m| m.docs.lock().ok().map(|d| d.len()))
                .unwrap_or(0),
            language: d.language,
            command: d.command,
            available,
            install_hint: d.install_hint,
            state,
        });
    }
    rows.sort_by(|a, b| a.language.cmp(&b.language));
    Ok(rows)
}

/// An `lsp.*` setting changed: every running server is stopped — the next
/// document opened starts one from the new descriptors, or none when servers
/// are off — the crash-loop verdicts and start ledgers are forgotten, as a
/// person's restart forgets them, and the `PATH` probes are forgotten so a
/// server just installed is seen. Called from `settings::refresh`, the
/// one list every setting-reading module is on.
pub fn refresh_for(inner: &Inner, key: &str) {
    if !key.starts_with("lsp.") {
        return;
    }
    if let Ok(mut f) = inner.lsp.failed.lock() {
        f.clear();
    }
    if let Ok(mut s) = inner.lsp.starts.lock() {
        s.clear();
    }
    // The gates of roots nobody may open again go with them; one a start
    // holds is its own to finish behind.
    if let Ok(mut gates) = inner.lsp.starting.lock() {
        gates.clear();
    }
    catalog::forget_probes();
    let running: Vec<Arc<Managed>> = inner
        .lsp
        .servers
        .lock()
        .map(|mut m| m.drain().map(|(_, v)| v).collect())
        .unwrap_or_default();
    if running.is_empty() {
        return;
    }
    tracing::info!(
        key,
        servers = running.len(),
        "language servers stopped: the setting changed"
    );
    // The shutdowns are asynchronous and this is a settings write on a
    // request thread: spawned, as the idle stop is. Outside a runtime the
    // handles drop and the processes end with them.
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            for m in running {
                m.server.shutdown().await;
            }
        });
    }
}

/// A person asked: forget the crash-loop verdict and stop the server, which
/// says `bisa/serverStopped` — a document still open starts one again when
/// it is next opened or changed.
pub async fn restart(
    inner: &Arc<Inner>,
    scope: FileScope,
    id: &str,
    language: &str,
) -> Result<(), EngineError> {
    let root = writable_root(inner, scope, id)?;
    let k = key(&root, language);
    if let Ok(mut f) = inner.lsp.failed.lock() {
        f.remove(&k);
    }
    if let Ok(mut s) = inner.lsp.starts.lock() {
        s.remove(&k);
    }
    if let Some(m) = inner.lsp.remove(&k) {
        let docs: Vec<String> = m
            .docs
            .lock()
            .map(|d| d.keys().cloned().collect())
            .unwrap_or_default();
        m.server.shutdown().await;
        tracing::info!(
            language,
            docs = docs.len(),
            "language server restarted on request"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The idle stop: a server goes once its last document has been closed
    /// for the whole time to live — and not a moment before, not while a
    /// document is open, and not when one was opened and closed again since.
    #[test]
    fn a_server_stops_only_after_its_whole_time_to_live_with_no_document_open() {
        let ttl = Duration::from_secs(300);
        let closed = Instant::now();
        let at = |secs: u64| closed + Duration::from_secs(secs);
        assert!(gone_idle(Some(closed), 0, ttl, at(300)));
        assert!(gone_idle(Some(closed), 0, ttl, at(900)));
        assert!(!gone_idle(Some(closed), 0, ttl, at(299)), "not yet due");
        assert!(
            !gone_idle(Some(closed), 1, ttl, at(900)),
            "a document is open"
        );
        assert!(
            !gone_idle(None, 0, ttl, at(900)),
            "a document opened since, and nothing closed after it"
        );
        // Closed again later: the earlier close's sweep finds it not due.
        assert!(!gone_idle(Some(at(200)), 0, ttl, at(300)));
        // A clock read before the close is no idleness at all.
        assert!(!gone_idle(Some(at(10)), 0, ttl, closed));
    }

    /// Crash-loop containment is per root and language: three starts in the
    /// minute, then none — and another language, or another root, has its own.
    #[test]
    fn starts_are_counted_per_root_and_language() {
        let registry = LspRegistry::default();
        let rust = key(Path::new("/work/lab"), "rust");
        for attempt in 1..=MAX_STARTS_PER_MINUTE {
            assert!(registry.record_start(&rust), "start {attempt}");
        }
        assert!(!registry.record_start(&rust), "the fourth is the verdict");
        assert!(registry.record_start(&key(Path::new("/work/lab"), "go")));
        assert!(registry.record_start(&key(Path::new("/work/shed"), "rust")));
    }

    /// One gate per root and language, the same one to every caller, so two
    /// documents opened at once start one server between them.
    #[test]
    fn a_start_goes_through_one_gate_per_root_and_language() {
        let registry = LspRegistry::default();
        let rust = key(Path::new("/work/lab"), "rust");
        assert!(Arc::ptr_eq(
            &registry.start_gate(&rust),
            &registry.start_gate(&rust)
        ));
        assert!(!Arc::ptr_eq(
            &registry.start_gate(&rust),
            &registry.start_gate(&key(Path::new("/work/shed"), "rust"))
        ));
    }
}
