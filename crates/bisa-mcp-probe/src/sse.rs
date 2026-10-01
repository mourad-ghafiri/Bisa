//! The 2024-11-05 HTTP+SSE transport, spoken just far enough to probe a
//! server (modelcontextprotocol.io/specification/2024-11-05/basic/transports):
//! `GET` the URL with `Accept: text/event-stream`; the first event is
//! `endpoint` and names where to `POST`; every answer comes back on the
//! stream as a `message` event. `rmcp` ships no client for this transport
//! on purpose, so this is ours — the handshake era only, since the
//! discover era never used it.

use crate::{
    bounded, sanitize, McpCapabilities, McpEra, McpProbeReport, McpProbeStage, McpServerInfo,
    McpToolSummary, MAX_INSTRUCTIONS_CHARS, MAX_TOOLS,
};
use bisa_core::sync::Locked;
use bisa_core::McpServerConfig;
use bisa_http::SseFrames;
use futures::StreamExt;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// The revision this transport belongs to — what the probe offers.
pub const SSE_PROTOCOL_VERSION: &str = "2024-11-05";

/// The endpoint the first event names, made absolute against the stream's URL.
pub fn resolve_endpoint(stream_url: &str, endpoint: &str) -> String {
    if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        return endpoint.to_string();
    }
    let base_end = stream_url.find("://").map(|i| i + 3).unwrap_or(0);
    let host_end = stream_url[base_end..]
        .find('/')
        .map(|i| base_end + i)
        .unwrap_or(stream_url.len());
    if endpoint.starts_with('/') {
        format!("{}{endpoint}", &stream_url[..host_end])
    } else {
        let dir_end = stream_url
            .rfind('/')
            .filter(|&i| i >= host_end)
            .unwrap_or(stream_url.len());
        format!("{}/{endpoint}", &stream_url[..dir_end])
    }
}

fn apply_headers(
    mut req: reqwest::RequestBuilder,
    headers: &BTreeMap<String, String>,
) -> reqwest::RequestBuilder {
    for (k, v) in headers {
        req = req.header(k.as_str(), v.as_str());
    }
    req
}

/// The way to send: the client, the caller's headers and the endpoint the
/// first event named. Its own type, apart from the stream, so that sending
/// borrows only what is `Sync` — the byte stream is `Send` and nothing more,
/// and a future holding `&Session` across an await could not be sent
/// between threads.
struct Sender<'a> {
    http: &'a reqwest::Client,
    headers: &'a BTreeMap<String, String>,
    endpoint: String,
}

impl Sender<'_> {
    async fn send(&self, message: Value) -> Result<(), String> {
        let resp = apply_headers(self.http.post(&self.endpoint), self.headers)
            .header("content-type", "application/json")
            .json(&message)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("the endpoint answered {}", resp.status()));
        }
        Ok(())
    }
}

/// The stream, its reader (`bisa_http::SseFrames` — the platform's one SSE
/// reader, decoded once per complete frame and capped), and the way to send.
struct Session<'a> {
    sender: Sender<'a>,
    stream: futures::stream::BoxStream<'static, Result<bytes::Bytes, reqwest::Error>>,
    parser: SseFrames,
}

impl Session<'_> {
    /// The next `message` event that carries a JSON-RPC answer to `id`.
    async fn answer(&mut self, id: u64) -> Result<Value, String> {
        loop {
            if let Some(ev) = self.parser.next_event() {
                if ev.event != "message" {
                    continue;
                }
                let v: Value = serde_json::from_str(&ev.data)
                    .map_err(|e| format!("a message that is not JSON: {e}"))?;
                if v.get("id").and_then(Value::as_u64) == Some(id) {
                    if let Some(err) = v.get("error") {
                        return Err(format!(
                            "the server answered an error: {}",
                            err.get("message").and_then(Value::as_str).unwrap_or("?")
                        ));
                    }
                    return Ok(v.get("result").cloned().unwrap_or(Value::Null));
                }
                continue;
            }
            match self.stream.next().await {
                Some(Ok(bytes)) => self
                    .parser
                    .push(&bytes)
                    .map_err(|e| format!("the stream is not one the probe reads: {e}"))?,
                Some(Err(e)) => return Err(format!("the stream failed: {e}")),
                None => return Err("the stream ended before the answer".to_string()),
            }
        }
    }
}

/// Dial, handshake, ping, list — and report.
pub async fn probe(
    http: &reqwest::Client,
    url: &str,
    headers: &BTreeMap<String, String>,
    config: &McpServerConfig,
    stage: Arc<Mutex<McpProbeStage>>,
    started: Instant,
) -> Result<McpProbeReport, (McpProbeStage, String)> {
    let set = |s: McpProbeStage| *stage.locked() = s;
    let resp = apply_headers(http.get(url), headers)
        .header("accept", "text/event-stream")
        .send()
        .await
        .map_err(|e| (McpProbeStage::Spawn, e.to_string()))?;
    if !resp.status().is_success() {
        return Err((
            McpProbeStage::Spawn,
            format!("the URL answered {}", resp.status()),
        ));
    }
    let mut session = Session {
        sender: Sender {
            http,
            headers,
            endpoint: String::new(),
        },
        stream: resp.bytes_stream().boxed(),
        parser: SseFrames::default(),
    };
    // The first event names the endpoint.
    let endpoint = loop {
        if let Some(ev) = session.parser.next_event() {
            if ev.event == "endpoint" {
                break resolve_endpoint(url, ev.data.trim());
            }
            continue;
        }
        match session.stream.next().await {
            Some(Ok(bytes)) => session.parser.push(&bytes).map_err(|e| {
                (
                    McpProbeStage::Spawn,
                    format!("the stream is not one the probe reads: {e}"),
                )
            })?,
            Some(Err(e)) => return Err((McpProbeStage::Spawn, format!("the stream failed: {e}"))),
            None => {
                return Err((
                    McpProbeStage::Spawn,
                    "the stream ended before naming its endpoint".to_string(),
                ))
            }
        }
    };
    session.sender.endpoint = endpoint;

    set(McpProbeStage::Initialize);
    session
        .sender
        .send(
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
                "protocolVersion": SSE_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "bisa", "version": env!("CARGO_PKG_VERSION")}
            }}),
        )
        .await
        .map_err(|e| (McpProbeStage::Initialize, e))?;
    let init = session
        .answer(1)
        .await
        .map_err(|e| (McpProbeStage::Initialize, e))?;
    session
        .sender
        .send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
        .await
        .map_err(|e| (McpProbeStage::Initialize, e))?;
    let protocol_version = init["protocolVersion"]
        .as_str()
        .unwrap_or(SSE_PROTOCOL_VERSION)
        .to_string();
    let caps = &init["capabilities"];
    let capabilities = McpCapabilities {
        tools: caps.get("tools").is_some(),
        resources: caps.get("resources").is_some(),
        prompts: caps.get("prompts").is_some(),
        logging: caps.get("logging").is_some(),
        completions: caps.get("completions").is_some(),
    };
    let server = init.get("serverInfo").map(|s| McpServerInfo {
        name: s["name"].as_str().unwrap_or("").to_string(),
        version: s["version"].as_str().unwrap_or("").to_string(),
        title: s["title"].as_str().map(str::to_string),
    });
    let instructions = init["instructions"]
        .as_str()
        .map(|i| bounded(i, MAX_INSTRUCTIONS_CHARS));

    set(McpProbeStage::Ping);
    session
        .sender
        .send(json!({"jsonrpc": "2.0", "id": 2, "method": "ping"}))
        .await
        .map_err(|e| (McpProbeStage::Ping, e))?;
    session
        .answer(2)
        .await
        .map_err(|e| (McpProbeStage::Ping, e))?;

    set(McpProbeStage::Tools);
    let (tools, tool_count) = if capabilities.tools {
        session
            .sender
            .send(json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list"}))
            .await
            .map_err(|e| (McpProbeStage::Tools, e))?;
        let listed = session
            .answer(3)
            .await
            .map_err(|e| (McpProbeStage::Tools, e))?;
        let all = listed["tools"].as_array().cloned().unwrap_or_default();
        let named = all
            .iter()
            .take(MAX_TOOLS)
            .map(|t| McpToolSummary {
                name: t["name"].as_str().unwrap_or("").to_string(),
                description: t["description"].as_str().map(|d| bounded(d, 200)),
            })
            .collect();
        (named, all.len())
    } else {
        (Vec::new(), 0)
    };
    set(McpProbeStage::Done);
    Ok(McpProbeReport {
        ok: true,
        transport: config.kind().to_string(),
        era: Some(McpEra::Handshake),
        protocol_version: Some(protocol_version),
        server,
        capabilities,
        instructions: instructions.map(|i| sanitize::sanitize(&i, config)),
        tools,
        tool_count,
        resource_count: None,
        prompt_count: None,
        elapsed_ms: started.elapsed().as_millis() as u64,
        stage: McpProbeStage::Done,
        error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_endpoint_is_made_absolute_against_the_stream() {
        assert_eq!(
            resolve_endpoint("https://x.test/sse", "/messages"),
            "https://x.test/messages"
        );
        assert_eq!(
            resolve_endpoint("https://x.test/a/sse", "messages?s=1"),
            "https://x.test/a/messages?s=1"
        );
        assert_eq!(
            resolve_endpoint("https://x.test/sse", "https://y.test/m"),
            "https://y.test/m"
        );
        assert_eq!(
            resolve_endpoint("http://127.0.0.1:9/sse", "/m"),
            "http://127.0.0.1:9/m"
        );
    }
}
