//! The probe against the scripted server (`tests/scripted_mcp.rs`, the
//! package's `scripted-mcp` binary) over stdio in both eras, against a
//! loopback Streamable HTTP stub, and against a loopback HTTP+SSE stub —
//! and what a hang, an exit and a secret come out as.

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use bisa_core::McpServerConfig;
use bisa_mcp_probe::{McpEra, McpProbe, McpProbeStage, RmcpProbe, DEFAULT_BUDGET, MAX_TOOLS};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

fn scripted(mode: &str, env: &[(&str, &str)]) -> McpServerConfig {
    McpServerConfig::Stdio {
        name: "scripted".into(),
        command: env!("CARGO_BIN_EXE_scripted-mcp").into(),
        args: vec![mode.into()],
        env: env
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        cwd: None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_handshake_era_server_answers_initialize_ping_and_tools_list_over_stdio() {
    let report = RmcpProbe
        .probe(&scripted("handshake:2025-06-18", &[]), DEFAULT_BUDGET)
        .await;
    assert!(report.ok, "{report:?}");
    assert_eq!(report.stage, McpProbeStage::Done);
    assert_eq!(report.transport, "stdio");
    assert_eq!(report.era, Some(McpEra::Handshake));
    assert_eq!(
        report.protocol_version.as_deref(),
        Some("2025-06-18"),
        "the negotiated revision is the server's word"
    );
    let server = report.server.as_ref().expect("who answered");
    assert_eq!(
        (server.name.as_str(), server.version.as_str()),
        ("scripted-mcp", "0.1.0")
    );
    assert!(
        report.capabilities.tools && report.capabilities.logging && !report.capabilities.resources
    );
    assert_eq!(report.tool_count, 2);
    assert_eq!(
        report
            .tools
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>(),
        ["search_docs", "read_page"]
    );
    assert_eq!(
        report.instructions.as_deref(),
        Some("A scripted server for the probe.")
    );
    assert!(
        report
            .words()
            .starts_with("scripted-mcp 0.1.0 · 2025-06-18 · 2 tools"),
        "{}",
        report.words()
    );
    assert!(report.tools.len() <= MAX_TOOLS);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_older_handshake_revision_is_the_servers_to_choose() {
    let report = RmcpProbe
        .probe(&scripted("handshake:2024-11-05", &[]), DEFAULT_BUDGET)
        .await;
    assert!(report.ok, "{report:?}");
    assert_eq!(report.protocol_version.as_deref(), Some("2024-11-05"));
    assert_eq!(report.era, Some(McpEra::Handshake));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_discover_era_server_is_read_in_one_call() {
    let report = RmcpProbe
        .probe(&scripted("discover", &[]), DEFAULT_BUDGET)
        .await;
    assert!(report.ok, "{report:?}");
    assert_eq!(report.era, Some(McpEra::Discover));
    assert_eq!(report.protocol_version.as_deref(), Some("2026-07-28"));
    assert_eq!(
        report.server.as_ref().map(|s| s.name.as_str()),
        Some("scripted-mcp")
    );
    assert_eq!(report.tool_count, 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hanging_server_times_out_at_the_stage_it_reached() {
    let started = std::time::Instant::now();
    let report = RmcpProbe
        .probe(&scripted("hang", &[]), Duration::from_millis(1500))
        .await;
    assert!(!report.ok);
    assert_eq!(report.stage, McpProbeStage::Initialize, "{report:?}");
    assert!(
        report
            .error
            .as_deref()
            .unwrap_or("")
            .contains("no answer within"),
        "{report:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "the budget bounds the wait"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_that_exits_is_a_failure_that_quotes_its_stderr_and_never_a_secret() {
    let report = RmcpProbe
        .probe(
            &scripted("exit", &[("API_KEY", "sk-live-secret-777")]),
            Duration::from_secs(5),
        )
        .await;
    assert!(!report.ok);
    assert!(
        matches!(
            report.stage,
            McpProbeStage::Spawn | McpProbeStage::Initialize
        ),
        "{report:?}"
    );
    let error = report.error.clone().unwrap_or_default();
    assert!(
        error.contains("refusing to start")
            || error.contains("no API key")
            || error.contains("closed")
            || error.contains("exit"),
        "the child's own words ride the error: {error}"
    );
    assert!(
        !error.contains("sk-live-secret-777"),
        "an environment value never leaves: {error}"
    );
    let missing = RmcpProbe
        .probe(
            &McpServerConfig::Stdio {
                name: "nope".into(),
                command: "/definitely/not/a/binary".into(),
                args: vec![],
                env: BTreeMap::new(),
                cwd: None,
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(!missing.ok);
    assert_eq!(missing.stage, McpProbeStage::Spawn, "{missing:?}");
}

// ---------------------------------------------------------------------------
// A Streamable HTTP server on the loopback interface
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
struct Seen {
    headers: Arc<Mutex<Vec<(String, String)>>>,
    methods: Arc<Mutex<Vec<String>>>,
}

async fn streamable(
    State(seen): State<Seen>,
    headers: axum::http::HeaderMap,
    Json(body): Json<Value>,
) -> axum::response::Response {
    for (k, v) in headers.iter() {
        seen.headers
            .lock()
            .unwrap()
            .push((k.to_string(), v.to_str().unwrap_or("").to_string()));
    }
    let method = body["method"].as_str().unwrap_or("").to_string();
    seen.methods.lock().unwrap().push(method.clone());
    let id = body.get("id").cloned();
    let Some(id) = id else {
        return axum::http::StatusCode::ACCEPTED.into_response();
    };
    let result = match method.as_str() {
        "server/discover" => {
            return Json(json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "Method not found"}})).into_response();
        }
        "initialize" => json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {"tools": {}, "prompts": {}},
            "serverInfo": {"name": "loopback-http", "version": "2.0.0"}
        }),
        "ping" => json!({}),
        "tools/list" => json!({"tools": [{"name": "echo", "inputSchema": {"type": "object"}}]}),
        "prompts/list" => json!({"prompts": [{"name": "greet"}, {"name": "farewell"}]}),
        _ => return Json(json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "Method not found"}})).into_response(),
    };
    Json(json!({"jsonrpc": "2.0", "id": id, "result": result})).into_response()
}

async fn no_stream() -> axum::http::StatusCode {
    axum::http::StatusCode::METHOD_NOT_ALLOWED
}

async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _served = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_streamable_http_server_is_probed_with_its_headers_and_counts_what_it_advertises() {
    let seen = Seen::default();
    let app = Router::new()
        .route("/mcp", post(streamable).get(no_stream))
        .with_state(seen.clone());
    let base = serve(app).await;
    let config = McpServerConfig::Http {
        name: "docs".into(),
        url: format!("{base}/mcp"),
        headers: BTreeMap::from([
            ("Authorization".to_string(), "Bearer t-123".to_string()),
            ("X-Tenant".to_string(), "acme".to_string()),
        ]),
    };
    let report = RmcpProbe.probe(&config, DEFAULT_BUDGET).await;
    assert!(report.ok, "{report:?}");
    assert_eq!(report.transport, "http");
    assert_eq!(report.era, Some(McpEra::Handshake));
    assert_eq!(
        report.server.as_ref().map(|s| s.version.as_str()),
        Some("2.0.0")
    );
    assert_eq!(report.tool_count, 1);
    assert_eq!(report.prompt_count, Some(2));
    assert_eq!(
        report.resource_count, None,
        "a capability not advertised is not asked for"
    );
    let headers = seen.headers.lock().unwrap().clone();
    assert!(
        headers
            .iter()
            .any(|(k, v)| k == "authorization" && v == "Bearer t-123"),
        "the bearer rode every request: {headers:?}"
    );
    assert!(headers.iter().any(|(k, v)| k == "x-tenant" && v == "acme"));
    let methods = seen.methods.lock().unwrap().clone();
    assert!(
        methods.contains(&"initialize".to_string()) && methods.contains(&"tools/list".to_string()),
        "{methods:?}"
    );
    assert!(
        !methods.contains(&"tools/call".to_string()),
        "a probe never calls a tool"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unreachable_url_fails_before_the_handshake_and_a_bad_status_says_so() {
    let config = McpServerConfig::Http {
        name: "gone".into(),
        url: "http://127.0.0.1:1/mcp".into(),
        headers: BTreeMap::new(),
    };
    let report = RmcpProbe.probe(&config, Duration::from_secs(5)).await;
    assert!(!report.ok);
    assert!(
        matches!(
            report.stage,
            McpProbeStage::Spawn | McpProbeStage::Initialize
        ),
        "{report:?}"
    );
    assert!(report.error.is_some());
}

// ---------------------------------------------------------------------------
// The 2024-11-05 HTTP+SSE server on the loopback interface
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct SseState {
    /// Answers pushed onto the one open stream.
    answers: Arc<Mutex<Option<mpsc::Sender<Event>>>>,
    seen: Seen,
}

async fn sse_stream(State(state): State<SseState>) -> impl IntoResponse {
    let (tx, rx) = mpsc::channel::<Event>(16);
    let (out_tx, out_rx) = mpsc::channel::<Result<Event, std::convert::Infallible>>(16);
    *state.answers.lock().unwrap() = Some(tx);
    tokio::spawn(async move {
        let mut rx = rx;
        let _client_gone = out_tx
            .send(Ok(Event::default()
                .event("endpoint")
                .data("/messages?session=abc")))
            .await;
        while let Some(ev) = rx.recv().await {
            if out_tx.send(Ok(ev)).await.is_err() {
                return;
            }
        }
    });
    Sse::new(ReceiverStream::new(out_rx)).keep_alive(KeepAlive::default())
}

async fn sse_messages(
    State(state): State<SseState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<Value>,
) -> axum::http::StatusCode {
    for (k, v) in headers.iter() {
        state
            .seen
            .headers
            .lock()
            .unwrap()
            .push((k.to_string(), v.to_str().unwrap_or("").to_string()));
    }
    let method = body["method"].as_str().unwrap_or("").to_string();
    state.seen.methods.lock().unwrap().push(method.clone());
    let Some(id) = body.get("id").cloned() else {
        return axum::http::StatusCode::ACCEPTED;
    };
    let result = match method.as_str() {
        "initialize" => {
            json!({"protocolVersion": "2024-11-05", "capabilities": {"tools": {}}, "serverInfo": {"name": "loopback-sse", "version": "0.9.0"}})
        }
        "ping" => json!({}),
        "tools/list" => {
            json!({"tools": [{"name": "legacy_tool", "inputSchema": {"type": "object"}}]})
        }
        _ => json!({}),
    };
    let answer = json!({"jsonrpc": "2.0", "id": id, "result": result});
    if let Some(tx) = state.answers.lock().unwrap().clone() {
        let _client_gone = tx.try_send(Event::default().event("message").data(answer.to_string()));
    }
    axum::http::StatusCode::ACCEPTED
}

#[tokio::test(flavor = "multi_thread")]
async fn a_legacy_http_sse_server_is_probed_through_its_endpoint_event() {
    let state = SseState {
        answers: Arc::new(Mutex::new(None)),
        seen: Seen::default(),
    };
    let app = Router::new()
        .route("/sse", get(sse_stream))
        .route("/messages", post(sse_messages))
        .with_state(state.clone());
    let base = serve(app).await;
    let config = McpServerConfig::Sse {
        name: "old".into(),
        url: format!("{base}/sse"),
        headers: BTreeMap::from([("X-Api-Key".to_string(), "k-1".to_string())]),
    };
    let report = RmcpProbe.probe(&config, DEFAULT_BUDGET).await;
    assert!(report.ok, "{report:?}");
    assert_eq!(report.transport, "sse");
    assert_eq!(report.era, Some(McpEra::Handshake));
    assert_eq!(report.protocol_version.as_deref(), Some("2024-11-05"));
    assert_eq!(
        report.server.as_ref().map(|s| s.name.as_str()),
        Some("loopback-sse")
    );
    assert_eq!(report.tool_count, 1);
    let methods = state.seen.methods.lock().unwrap().clone();
    assert_eq!(
        methods,
        [
            "initialize",
            "notifications/initialized",
            "ping",
            "tools/list"
        ]
    );
    let headers = state.seen.headers.lock().unwrap().clone();
    assert!(
        headers.iter().any(|(k, v)| k == "x-api-key" && v == "k-1"),
        "{headers:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_legacy_stream_that_never_names_its_endpoint_fails_before_the_handshake() {
    let app = Router::new().route(
        "/sse",
        get(|| async { axum::http::StatusCode::UNAUTHORIZED }),
    );
    let base = serve(app).await;
    let config = McpServerConfig::Sse {
        name: "old".into(),
        url: format!("{base}/sse"),
        headers: BTreeMap::new(),
    };
    let report = RmcpProbe.probe(&config, Duration::from_secs(5)).await;
    assert!(!report.ok);
    assert_eq!(report.stage, McpProbeStage::Spawn);
    assert!(
        report.error.as_deref().unwrap_or("").contains("401"),
        "{report:?}"
    );
}
