//! HTTP-over-unix-socket client for a running `bisa node` daemon.
//!
//! One hyper connection per request keeps the client trivial; the daemon is
//! on the same machine. Discovery: `<data-dir>/run/node.sock`, or the pointer
//! file `node.sock.path` a long-path fallback bind leaves behind.

use anyhow::{anyhow, bail, Context as _, Result};
use http_body_util::{BodyExt as _, Empty, Full};
use hyper::body::Bytes;
use hyper::Request;
use hyper_util::rt::TokioIo;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::net::UnixStream;
use tokio::sync::mpsc;

pub struct NodeClient {
    socket: PathBuf,
    /// The control-plane token, from `BISA_API_TOKEN` or `run/token`.
    /// `None` only when neither exists — the request then answers 401 and
    /// says so, which beats guessing.
    token: Option<String>,
    /// The language every request asks the node for, and a refusal's `text`
    /// is rendered in here (17 — Internationalisation).
    locale: bisa_i18n::Locale,
}

/// The token a daemon on this workspace expects. The environment wins so a
/// shell that was handed one can use it against a node it did not start.
fn token_for(data_dir: &Path) -> Option<String> {
    if let Ok(t) = std::env::var("BISA_API_TOKEN") {
        let t = t.trim().to_string();
        if !t.is_empty() {
            return Some(t);
        }
    }
    std::fs::read_to_string(bisa_store::Paths::new(data_dir).token_file())
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

/// What an answer's body holds: JSON — or nothing, which is what a route
/// that did what it was asked and has nothing to add answers (`204`).
/// Nothing is `null`, never a body that could not be read.
fn said_in(body: &[u8]) -> serde_json::Result<Value> {
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(Value::Null);
    }
    serde_json::from_slice(body)
}

impl NodeClient {
    /// Find and health-check a running daemon for this data dir. `None` means
    /// "no daemon" — callers fall back to embedded mode silently.
    pub async fn discover(data_dir: &Path, locale: bisa_i18n::Locale) -> Option<NodeClient> {
        let preferred = bisa_store::Paths::new(data_dir).node_socket();
        let mut candidates = vec![preferred.clone()];
        let pointer = bisa_node::pointer_path(&preferred);
        if let Ok(actual) = std::fs::read_to_string(&pointer) {
            candidates.push(PathBuf::from(actual.trim()));
        }
        for socket in candidates {
            if !socket.exists() {
                continue;
            }
            let client = NodeClient {
                socket,
                token: token_for(data_dir),
                locale: locale.clone(),
            };
            let health = tokio::time::timeout(Duration::from_millis(500), client.get("/health"));
            if matches!(health.await, Ok(Ok(v)) if v["ok"] == Value::Bool(true)) {
                return Some(client);
            }
        }
        None
    }

    pub async fn get(&self, path: &str) -> Result<Value> {
        self.request("GET", path, None).await
    }

    pub async fn post(&self, path: &str, body: Value) -> Result<Value> {
        self.request("POST", path, Some(body)).await
    }

    pub async fn put(&self, path: &str, body: Value) -> Result<Value> {
        self.request("PUT", path, Some(body)).await
    }

    pub async fn patch(&self, path: &str, body: Value) -> Result<Value> {
        self.request("PATCH", path, Some(body)).await
    }

    pub async fn delete(&self, path: &str) -> Result<Value> {
        self.request("DELETE", path, None).await
    }

    /// A delete that says what of the thing goes: which goal a project is
    /// detached from.
    pub async fn delete_with(&self, path: &str, body: Value) -> Result<Value> {
        self.request("DELETE", path, Some(body)).await
    }

    async fn request(&self, method: &str, path: &str, body: Option<Value>) -> Result<Value> {
        let stream = UnixStream::connect(&self.socket).await.with_context(|| {
            bisa_core::text!(
                "cli-client-connecting-node",
                a0 = (self.socket.display()).to_string()
            )
        })?;
        let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
            .await
            .context(bisa_core::text!("cli-client-node-handshake"))?;
        tokio::spawn(conn);

        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header(hyper::header::HOST, "localhost")
            .header(hyper::header::ACCEPT_LANGUAGE, self.locale.tag());
        if let Some(token) = &self.token {
            builder = builder.header(hyper::header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let req = match body {
            Some(v) => builder
                .header(hyper::header::CONTENT_TYPE, "application/json")
                .body(full(v.to_string()))?,
            None => builder.body(full(String::new()))?,
        };
        let resp = sender
            .send_request(req)
            .await
            .context(bisa_core::text!("cli-client-node-request"))?;
        let status = resp.status();
        let bytes = resp.into_body().collect().await?.to_bytes();
        let value: Value = said_in(&bytes).with_context(|| {
            bisa_core::text!(
                "cli-client-node-returned-non-json",
                method = method.to_string(),
                path = path.to_string()
            )
        })?;
        if !status.is_success() {
            bail!(bisa_core::text!(
                "cli-client-node-error",
                status = status.to_string(),
                said = self.said(&value)
            ));
        }
        Ok(value)
    }

    /// A refusal as data, rendered here in the CLI's language; the node's
    /// own rendering stands in when a body carries no `text`.
    fn said(&self, refused: &Value) -> String {
        refused
            .get("text")
            .cloned()
            .and_then(|t| serde_json::from_value::<bisa_core::Text>(t).ok())
            .map(|t| bisa_i18n::render(&self.locale, &t))
            .or_else(|| refused["error"].as_str().map(str::to_string))
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// Tail the daemon's SSE event stream. Events arrive on the returned
    /// channel until the connection drops (channel closes).
    pub async fn events(&self) -> Result<mpsc::Receiver<Value>> {
        self.stream("/events").await
    }

    /// A route that answers as a stream of frames — the events, a search —
    /// each frame on the returned channel until the stream ends. A refusal
    /// before the stream opens is the node's, in its words.
    pub async fn stream(&self, path: &str) -> Result<mpsc::Receiver<Value>> {
        let stream = UnixStream::connect(&self.socket).await?;
        let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
            .await
            .context(bisa_core::text!("cli-client-node-handshake"))?;
        tokio::spawn(conn);
        let mut builder = Request::builder()
            .method("GET")
            .uri(path)
            .header(hyper::header::HOST, "localhost")
            .header(hyper::header::ACCEPT_LANGUAGE, self.locale.tag());
        // The stream is behind the same token as every other route.
        if let Some(token) = &self.token {
            builder = builder.header(hyper::header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let req = builder.body(Empty::<Bytes>::new())?;
        let resp = sender.send_request(req).await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let bytes = resp.into_body().collect().await?.to_bytes();
            return Err(match serde_json::from_slice::<Value>(&bytes) {
                Ok(refused) => anyhow!(bisa_core::text!(
                    "cli-client-node-error",
                    status = status.to_string(),
                    said = self.said(&refused)
                )),
                Err(_) => anyhow!(bisa_core::text!(
                    "cli-client-node-events-returned",
                    a0 = status.to_string()
                )),
            });
        }
        let (tx, rx) = mpsc::channel(256);
        tokio::spawn(async move {
            let mut body = resp.into_body();
            // The one SSE reader: a frame is decoded once its blank line has
            // arrived, so a character split across two chunks is one
            // character; a frame that never ends is a stream the CLI leaves.
            let mut frames = bisa_http::SseFrames::default();
            while let Some(frame) = body.frame().await {
                let Ok(frame) = frame else { break };
                let Some(data) = frame.data_ref() else {
                    continue;
                };
                if let Err(e) = frames.push(data) {
                    tracing::warn!(target: "bisa_cli", "the node's event stream broke: {e}");
                    return;
                }
                while let Some(event) = frames.next_event() {
                    if let Ok(v) = serde_json::from_str::<Value>(&event.data) {
                        if tx.send(v).await.is_err() {
                            return;
                        }
                    }
                }
            }
        });
        Ok(rx)
    }
}

fn full(s: String) -> Full<Bytes> {
    Full::new(Bytes::from(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_with_no_body_is_nothing_and_never_a_failure() {
        assert_eq!(said_in(b"").unwrap(), Value::Null);
        assert_eq!(said_in(b"\r\n").unwrap(), Value::Null);
        assert_eq!(
            said_in(br#"{"ok":true}"#).unwrap(),
            serde_json::json!({"ok": true})
        );
        assert!(said_in(b"<html>").is_err(), "what is no JSON is still said");
    }
}
