//! One supervised language server over stdio.
//!
//! A reader task decodes frames: responses go to whoever is waiting on that
//! id, notifications to a channel the owner drains, and requests *from* the
//! server (progress tokens, capability registration) are answered `null` so a
//! server that expects a client never stalls on one. When the reader ends —
//! the process exited, or a frame did not parse — every waiter gets
//! [`LspError::Closed`] and the status turns `Failed` with the reason.

use crate::catalog::{login_shell, shell_word, ServerDescriptor};
use crate::codec::{encode, Decoder};
use crate::{LspError, LspResult};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};

pub const INIT_TIMEOUT: Duration = Duration::from_secs(30);
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum ServerState {
    Starting,
    Running,
    Failed { reason: String },
    Stopped,
}

/// A message from the server that is not an answer: diagnostics, progress,
/// log messages.
#[derive(Clone, Debug)]
pub struct Notification {
    pub method: String,
    pub params: Value,
}

struct Shared {
    pending: Mutex<HashMap<i64, oneshot::Sender<Result<Value, LspError>>>>,
    state: Mutex<ServerState>,
}

pub struct Server {
    pub descriptor: ServerDescriptor,
    pub root: PathBuf,
    child: Mutex<Option<tokio::process::Child>>,
    stdin: tokio::sync::Mutex<Option<tokio::process::ChildStdin>>,
    shared: Arc<Shared>,
    next_id: AtomicI64,
    notifications: Mutex<Option<mpsc::Receiver<Notification>>>,
    capabilities: Mutex<Value>,
}

impl Server {
    /// Spawn through the login shell, run `initialize`, send `initialized`.
    pub async fn start(root: &Path, descriptor: ServerDescriptor) -> LspResult<Arc<Server>> {
        let mut command = format!("exec {}", shell_word(&descriptor.command));
        for a in &descriptor.args {
            command.push(' ');
            command.push_str(&shell_word(a));
        }
        let mut child = tokio::process::Command::new(login_shell())
            .args(["-l", "-c", &command])
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| LspError::Spawn(format!("{}: {e}", descriptor.command)))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| LspError::Spawn("no stdin".into()))?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| LspError::Spawn("no stdout".into()))?;
        // The last of what the server wrote to stderr, bounded — the words a
        // person reads when it exits, instead of *the server exited*.
        let tail: Arc<std::sync::Mutex<String>> = Arc::new(std::sync::Mutex::new(String::new()));
        if let Some(mut stderr) = child.stderr.take() {
            let tail = Arc::clone(&tail);
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                loop {
                    let n = match stderr.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    if let Ok(mut t) = tail.lock() {
                        t.push_str(&String::from_utf8_lossy(&buf[..n]));
                        if t.len() > STDERR_TAIL {
                            let cut = t.len() - STDERR_TAIL;
                            let at = t
                                .char_indices()
                                .map(|(i, _)| i)
                                .find(|&i| i >= cut)
                                .unwrap_or(cut);
                            t.drain(..at);
                        }
                    }
                }
            });
        }
        let shared = Arc::new(Shared {
            pending: Mutex::new(HashMap::new()),
            state: Mutex::new(ServerState::Starting),
        });
        let (tx, rx) = mpsc::channel::<Notification>(256);
        let server = Arc::new(Server {
            descriptor,
            root: root.to_path_buf(),
            child: Mutex::new(Some(child)),
            stdin: tokio::sync::Mutex::new(Some(stdin)),
            shared: Arc::clone(&shared),
            next_id: AtomicI64::new(1),
            notifications: Mutex::new(Some(rx)),
            capabilities: Mutex::new(Value::Null),
        });

        // The reader.
        let reader_shared = Arc::clone(&shared);
        let reader_server = Arc::downgrade(&server);
        tokio::spawn(async move {
            let mut decoder = Decoder::default();
            let mut buf = vec![0u8; 64 * 1024];
            let reason: String = loop {
                let n = match stdout.read(&mut buf).await {
                    Ok(0) => {
                        let said = tail
                            .lock()
                            .map(|t| t.trim().to_string())
                            .unwrap_or_default();
                        break if said.is_empty() {
                            "the server exited".to_string()
                        } else {
                            format!("the server exited: {said}")
                        };
                    }
                    Ok(n) => n,
                    Err(e) => break format!("reading the server: {e}"),
                };
                decoder.push(&buf[..n]);
                loop {
                    match decoder.next_message() {
                        Ok(Some(msg)) => {
                            if let Some(s) = reader_server.upgrade() {
                                s.dispatch(msg, &tx).await;
                            }
                        }
                        Ok(None) => break,
                        Err(e) => return fail(&reader_shared, format!("protocol: {e}")),
                    }
                }
            };
            fail(&reader_shared, reason);
        });

        // initialize → initialized.
        let init = json!({
            "processId": std::process::id(),
            "clientInfo": {"name": "bisa", "version": env!("CARGO_PKG_VERSION")},
            "rootUri": crate::uri::root_uri(root),
            "workspaceFolders": [{"uri": crate::uri::root_uri(root), "name": root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()}],
            "initializationOptions": server.descriptor.initialization_options.clone().unwrap_or(Value::Null),
            "capabilities": {
                "textDocument": {
                    "synchronization": {"dynamicRegistration": false, "didSave": true},
                    "hover": {"contentFormat": ["markdown", "plaintext"]},
                    "definition": {"linkSupport": false},
                    "references": {},
                    "documentSymbol": {"hierarchicalDocumentSymbolSupport": true},
                    "publishDiagnostics": {"relatedInformation": false},
                    "formatting": {},
                    "rename": {"prepareSupport": false},
                    "completion": {"completionItem": {"snippetSupport": false}}
                },
                "workspace": {"workspaceFolders": true, "symbol": {}},
                "window": {"workDoneProgress": true}
            }
        });
        let answer = tokio::time::timeout(INIT_TIMEOUT, server.request("initialize", init))
            .await
            .map_err(|_| LspError::Timeout("initialize".into()))??;
        if let Ok(mut c) = server.capabilities.lock() {
            *c = answer["capabilities"].clone();
        }
        server.notify("initialized", json!({})).await?;
        if let Ok(mut s) = shared.state.lock() {
            if *s == ServerState::Starting {
                *s = ServerState::Running;
            }
        }
        Ok(server)
    }

    async fn dispatch(&self, msg: Value, notifications: &mpsc::Sender<Notification>) {
        let id = msg.get("id").cloned();
        let method = msg
            .get("method")
            .and_then(|m| m.as_str())
            .map(str::to_string);
        match (id, method) {
            // A response to one of ours.
            (Some(id), None) => {
                let Some(id) = id.as_i64() else { return };
                let waiter = self
                    .shared
                    .pending
                    .lock()
                    .ok()
                    .and_then(|mut p| p.remove(&id));
                if let Some(tx) = waiter {
                    let result = if let Some(err) = msg.get("error") {
                        Err(LspError::ServerError {
                            code: err["code"].as_i64().unwrap_or(0),
                            message: err["message"].as_str().unwrap_or("").to_string(),
                        })
                    } else {
                        Ok(msg.get("result").cloned().unwrap_or(Value::Null))
                    };
                    if tx.send(result).is_err() {
                        tracing::debug!("an answer arrived after its asker gave up");
                    }
                }
            }
            // A request from the server: answered empty so it never stalls.
            (Some(id), Some(method)) => {
                let result = match method.as_str() {
                    "workspace/configuration" => {
                        let n = msg["params"]["items"]
                            .as_array()
                            .map(|a| a.len())
                            .unwrap_or(0);
                        Value::Array(vec![Value::Null; n])
                    }
                    "workspace/workspaceFolders" => {
                        json!([{"uri": crate::uri::root_uri(&self.root), "name": "root"}])
                    }
                    _ => Value::Null,
                };
                // The server asked; an answer that cannot be written means the
                // pipe is gone, and the reader ends on it next.
                if let Err(e) = self
                    .send(json!({"jsonrpc": "2.0", "id": id, "result": result}))
                    .await
                {
                    tracing::debug!("a server request went unanswered: {e}");
                }
            }
            // A notification.
            (None, Some(method)) => {
                // Nobody listening is the engine letting the server go.
                if notifications
                    .send(Notification {
                        method,
                        params: msg.get("params").cloned().unwrap_or(Value::Null),
                    })
                    .await
                    .is_err()
                {
                    tracing::debug!("a notification found no listener; the server is being let go");
                }
            }
            (None, None) => {}
        }
    }

    async fn send(&self, msg: Value) -> LspResult<()> {
        let mut guard = self.stdin.lock().await;
        let Some(stdin) = guard.as_mut() else {
            return Err(LspError::Closed);
        };
        stdin
            .write_all(&encode(&msg))
            .await
            .map_err(|e| LspError::Protocol(format!("writing to the server: {e}")))?;
        stdin
            .flush()
            .await
            .map_err(|e| LspError::Protocol(format!("flushing: {e}")))
    }

    /// A request with a deadline.
    pub async fn request(&self, method: &str, params: Value) -> LspResult<Value> {
        if matches!(
            self.state(),
            ServerState::Failed { .. } | ServerState::Stopped
        ) {
            return Err(LspError::Closed);
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        if let Ok(mut p) = self.shared.pending.lock() {
            p.insert(id, tx);
        }
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .await?;
        match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(LspError::Closed),
            Err(_) => {
                if let Ok(mut p) = self.shared.pending.lock() {
                    p.remove(&id);
                }
                Err(LspError::Timeout(method.to_string()))
            }
        }
    }

    pub async fn notify(&self, method: &str, params: Value) -> LspResult<()> {
        self.send(json!({"jsonrpc": "2.0", "method": method, "params": params}))
            .await
    }

    pub async fn did_open(
        &self,
        uri: &str,
        language: &str,
        version: i64,
        text: &str,
    ) -> LspResult<()> {
        self.notify(
            "textDocument/didOpen",
            json!({"textDocument": {"uri": uri, "languageId": language, "version": version, "text": text}}),
        )
        .await
    }

    /// Full-document sync: one change with the whole text. Simple, and what
    /// every server accepts.
    pub async fn did_change(&self, uri: &str, version: i64, text: &str) -> LspResult<()> {
        self.notify(
            "textDocument/didChange",
            json!({"textDocument": {"uri": uri, "version": version}, "contentChanges": [{"text": text}]}),
        )
        .await
    }

    pub async fn did_close(&self, uri: &str) -> LspResult<()> {
        self.notify(
            "textDocument/didClose",
            json!({"textDocument": {"uri": uri}}),
        )
        .await
    }

    /// Take the notification stream. Once.
    pub fn take_notifications(&self) -> Option<mpsc::Receiver<Notification>> {
        self.notifications.lock().ok().and_then(|mut n| n.take())
    }

    pub fn state(&self) -> ServerState {
        self.shared
            .state
            .lock()
            .map(|s| s.clone())
            .unwrap_or(ServerState::Stopped)
    }

    pub fn capabilities(&self) -> Value {
        self.capabilities
            .lock()
            .map(|c| c.clone())
            .unwrap_or(Value::Null)
    }

    /// `shutdown` then `exit`, then the process is gone whatever it said.
    pub async fn shutdown(&self) {
        match tokio::time::timeout(
            Duration::from_secs(3),
            self.request("shutdown", Value::Null),
        )
        .await
        {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => tracing::debug!("shutdown was not accepted: {e}"),
            Err(_) => tracing::debug!("shutdown was not answered in 3 s"),
        }
        if let Err(e) = self.notify("exit", Value::Null).await {
            tracing::debug!("exit was not delivered: {e}");
        }
        if let Ok(mut g) = self.shared.state.lock() {
            *g = ServerState::Stopped;
        }
        // A request still waiting hears the close now, rather than never:
        // its sender would otherwise sit in the map for the server's life.
        if let Ok(mut p) = self.shared.pending.lock() {
            for (_, tx) in p.drain() {
                let _asker_gone = tx.send(Err(LspError::Closed));
            }
        }
        let child = self.child.lock().ok().and_then(|mut c| c.take());
        if let Some(mut child) = child {
            // Two seconds to exit on its own; then it is ended. An end that
            // fails is a child already gone.
            if tokio::time::timeout(Duration::from_secs(2), child.wait())
                .await
                .is_err()
            {
                if let Err(e) = child.start_kill() {
                    tracing::debug!("ending a language server that did not exit: {e}");
                }
            }
        }
        if let Ok(mut g) = self.stdin.try_lock() {
            g.take();
        }
    }
}

/// How much of the server's stderr is kept for its exit reason.
const STDERR_TAIL: usize = 2048;

fn fail(shared: &Arc<Shared>, reason: String) {
    if let Ok(mut s) = shared.state.lock() {
        if !matches!(*s, ServerState::Stopped) {
            *s = ServerState::Failed { reason };
        }
    }
    if let Ok(mut p) = shared.pending.lock() {
        for (_, tx) in p.drain() {
            let _asker_gone = tx.send(Err(LspError::Closed));
        }
    }
}
