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

// ---------------------------------------------------------------------------
// The edges: what a server says besides its tools, what it writes before it
// dies, and the ways a stream can fail to be one.

/// A server that advertises nothing has no tools to count and is said by its
/// name alone; one that leaves saying nothing is a failure with no words of
/// its own; one that writes more than is kept is quoted by its last words;
/// and a working directory named is the one the server is started in.
#[tokio::test(flavor = "multi_thread")]
async fn what_a_server_says_besides_its_tools_and_what_it_writes_before_it_dies() {
    let bare = RmcpProbe
        .probe(&scripted("bare", &[]), DEFAULT_BUDGET)
        .await;
    assert!(bare.ok, "{bare:?}");
    assert_eq!(bare.tool_count, 0);
    assert!(!bare.capabilities.tools);
    assert_eq!(bare.words(), "bare-mcp · 2025-06-18 · 0 tools", "{bare:?}");

    let quiet = RmcpProbe
        .probe(&scripted("quiet", &[]), Duration::from_secs(5))
        .await;
    assert!(!quiet.ok);
    assert!(
        !quiet
            .error
            .as_deref()
            .unwrap_or("")
            .contains("the server wrote"),
        "nothing was written: {quiet:?}"
    );

    let noisy = RmcpProbe
        .probe(&scripted("noisy", &[]), Duration::from_secs(5))
        .await;
    assert!(!noisy.ok);
    let error = noisy.error.clone().unwrap_or_default();
    assert!(error.contains("the last word"), "{error}");
    assert!(
        !error.contains("line 0000"),
        "the first lines were let go: {error}"
    );
    assert!(error.len() < 8 * 1024, "the tail is bounded");

    let placed = McpServerConfig::Stdio {
        name: "scripted".into(),
        command: env!("CARGO_BIN_EXE_scripted-mcp").into(),
        args: vec!["handshake:2025-06-18".into()],
        env: BTreeMap::new(),
        cwd: Some(std::env::temp_dir().display().to_string()),
    };
    let report = RmcpProbe.probe(&placed, DEFAULT_BUDGET).await;
    assert!(report.ok, "{report:?}");
    assert_eq!(report.tool_count, 2);
}

/// A stage's words for a person, and a report's: the failure's stage with
/// or without its error, and a success by the server's name — alone when
/// it says no version — the revision and the count.
#[test]
fn a_report_reads_as_one_line() {
    assert_eq!(
        McpProbeStage::Initialize.failure_words(),
        "reached the server, but the MCP handshake did not complete"
    );
    assert_eq!(
        McpProbeStage::Ping.failure_words(),
        "answered the handshake, but not a ping"
    );
    assert_eq!(
        McpProbeStage::Tools.failure_words(),
        "answered the handshake, but not the tool list"
    );
    assert_eq!(McpProbeStage::Done.failure_words(), "answered");
    let config = scripted("hang", &[]);
    let mut failed = bisa_mcp_probe::McpProbeReport::failed(
        &config,
        McpProbeStage::Ping,
        "no answer within 3 s",
        Duration::from_millis(10),
    );
    assert_eq!(
        failed.words(),
        "answered the handshake, but not a ping: no answer within 3 s"
    );
    failed.error = None;
    assert_eq!(failed.words(), "answered the handshake, but not a ping");
    let mut ok = failed.clone();
    ok.ok = true;
    ok.stage = McpProbeStage::Done;
    ok.tool_count = 1;
    assert_eq!(ok.words(), "1 tool");
    ok.server = Some(bisa_mcp_probe::McpServerInfo {
        name: "docs".into(),
        version: String::new(),
        title: None,
    });
    ok.protocol_version = Some("2025-06-18".into());
    assert_eq!(ok.words(), "docs · 2025-06-18 · 1 tool");
}

/// A Streamable HTTP server that advertises resources is asked for them and
/// they are counted; one that advertises nothing has nothing counted.
async fn streamable_with(
    capabilities: Value,
) -> impl Fn(
    Json<Value>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = axum::response::Response> + Send>>
       + Clone {
    move |Json(body): Json<Value>| {
        let capabilities = capabilities.clone();
        Box::pin(async move {
            let Some(id) = body.get("id").cloned() else {
                return axum::http::StatusCode::ACCEPTED.into_response();
            };
            let result = match body["method"].as_str().unwrap_or("") {
                "initialize" => json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": capabilities,
                    "serverInfo": {"name": "loopback-http", "version": "2.0.0"}
                }),
                "ping" => json!({}),
                "tools/list" => json!({"tools": []}),
                "resources/list" => json!({"resources": [{"uri": "file:///a", "name": "a"}, {"uri": "file:///b", "name": "b"}]}),
                _ => return Json(json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "Method not found"}})).into_response(),
            };
            Json(json!({"jsonrpc": "2.0", "id": id, "result": result})).into_response()
        })
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_streamable_server_is_asked_for_what_it_advertises_and_nothing_else() {
    let app = Router::new().route(
        "/mcp",
        post(streamable_with(json!({"tools": {}, "resources": {}})).await).get(no_stream),
    );
    let base = serve(app).await;
    let config = McpServerConfig::Http {
        name: "docs".into(),
        url: format!("{base}/mcp"),
        headers: BTreeMap::new(),
    };
    let report = RmcpProbe.probe(&config, DEFAULT_BUDGET).await;
    assert!(report.ok, "{report:?}");
    assert_eq!(report.resource_count, Some(2));
    assert_eq!(report.prompt_count, None);

    let app = Router::new().route(
        "/mcp",
        post(streamable_with(json!({})).await).get(no_stream),
    );
    let base = serve(app).await;
    let config = McpServerConfig::Http {
        name: "docs".into(),
        url: format!("{base}/mcp"),
        headers: BTreeMap::new(),
    };
    let report = RmcpProbe.probe(&config, DEFAULT_BUDGET).await;
    assert!(report.ok, "{report:?}");
    assert_eq!(report.tool_count, 0);
    assert_eq!(report.resource_count, None);
}

/// A legacy stream as a test writes it: the events in order, the answers
/// to the messages posted, and how the messages endpoint answers.
#[derive(Clone)]
struct LegacyScript {
    /// What the stream says, in order, before any answer: `(event, data)`.
    prelude: Vec<(&'static str, String)>,
    /// How `POST /messages` answers: `None` is 202, `Some(code)` that status.
    post_status: Option<u16>,
    /// What rides the stream after a message is posted — `None` closes it.
    answer: Option<Answer>,
    /// A server that advertises no tools.
    no_tools: bool,
}

#[derive(Clone, Copy)]
enum Answer {
    /// The result, after an unrelated message and a non-message event.
    AfterNoise,
    /// A JSON-RPC error.
    Error,
}

#[derive(Clone)]
struct LegacyState {
    script: LegacyScript,
    answers: Arc<Mutex<Option<mpsc::Sender<Event>>>>,
}

async fn legacy_stream(State(state): State<LegacyState>) -> impl IntoResponse {
    let (tx, rx) = mpsc::channel::<Event>(16);
    let (out_tx, out_rx) = mpsc::channel::<Result<Event, std::convert::Infallible>>(16);
    *state.answers.lock().unwrap() = Some(tx);
    let prelude = state.script.prelude.clone();
    let closes = state.script.answer.is_none();
    tokio::spawn(async move {
        let mut rx = rx;
        for (event, data) in prelude {
            let ev = if event == "comment" {
                Event::default().comment(data)
            } else {
                Event::default().event(event).data(data)
            };
            if out_tx.send(Ok(ev)).await.is_err() {
                return;
            }
        }
        if closes {
            return;
        }
        while let Some(ev) = rx.recv().await {
            if out_tx.send(Ok(ev)).await.is_err() {
                return;
            }
        }
    });
    Sse::new(ReceiverStream::new(out_rx)).keep_alive(KeepAlive::default())
}

async fn legacy_messages(
    State(state): State<LegacyState>,
    Json(body): Json<Value>,
) -> axum::http::StatusCode {
    if let Some(code) = state.script.post_status {
        return axum::http::StatusCode::from_u16(code).unwrap();
    }
    let Some(id) = body.get("id").cloned() else {
        return axum::http::StatusCode::ACCEPTED;
    };
    let result = match body["method"].as_str().unwrap_or("") {
        "initialize" if state.script.no_tools => {
            json!({"protocolVersion": "2024-11-05", "capabilities": {}, "serverInfo": {"name": "loopback-sse", "version": "0.9.0"}})
        }
        "initialize" => {
            json!({"protocolVersion": "2024-11-05", "capabilities": {"tools": {}}, "serverInfo": {"name": "loopback-sse", "version": "0.9.0"}})
        }
        "ping" => json!({}),
        "tools/list" => {
            json!({"tools": [{"name": "legacy_tool", "inputSchema": {"type": "object"}}]})
        }
        _ => json!({}),
    };
    let tx = state.answers.lock().unwrap().clone();
    if let Some(tx) = tx {
        match state.script.answer {
            Some(Answer::AfterNoise) => {
                // A probe that has already left drops the channel: nothing to tell.
                tx.try_send(Event::default().event("ping").data("{}")).ok();
                tx.try_send(
                    Event::default()
                        .event("message")
                        .data(json!({"jsonrpc": "2.0", "id": 99, "result": {}}).to_string()),
                )
                .ok();
                tx.try_send(
                    Event::default()
                        .event("message")
                        .data(json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()),
                )
                .ok();
            }
            Some(Answer::Error) => {
                tx.try_send(Event::default().event("message").data(
                    json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32000, "message": "not today"}}).to_string(),
                ))
                .ok();
            }
            None => {}
        }
    }
    axum::http::StatusCode::ACCEPTED
}

async fn legacy(script: LegacyScript) -> McpServerConfig {
    let state = LegacyState {
        script,
        answers: Arc::new(Mutex::new(None)),
    };
    let app = Router::new()
        .route("/sse", get(legacy_stream))
        .route("/messages", post(legacy_messages))
        .with_state(state);
    let base = serve(app).await;
    McpServerConfig::Sse {
        name: "old".into(),
        url: format!("{base}/sse"),
        headers: BTreeMap::new(),
    }
}

fn endpoint_first() -> Vec<(&'static str, String)> {
    vec![("endpoint", "/messages".to_string())]
}

/// The legacy stream at its edges: a comment and a stray event before the
/// endpoint are passed over, as a ping event and another message's answer
/// are while an answer is awaited; an error answered is the stage's failure;
/// a messages endpoint that refuses, a stream that ends before naming its
/// endpoint or before answering, and a server with no tools each say so.
#[tokio::test(flavor = "multi_thread")]
async fn a_legacy_stream_at_its_edges() {
    let noisy = legacy(LegacyScript {
        prelude: vec![
            ("comment", "keep-alive".into()),
            ("ping", "{}".into()),
            ("endpoint", "/messages".into()),
        ],
        post_status: None,
        answer: Some(Answer::AfterNoise),
        no_tools: true,
    })
    .await;
    let report = RmcpProbe.probe(&noisy, DEFAULT_BUDGET).await;
    assert!(report.ok, "{report:?}");
    assert_eq!(report.tool_count, 0, "no tools advertised, none asked for");

    let refusing = legacy(LegacyScript {
        prelude: endpoint_first(),
        post_status: Some(500),
        answer: None,
        no_tools: false,
    })
    .await;
    let report = RmcpProbe.probe(&refusing, Duration::from_secs(5)).await;
    assert!(!report.ok);
    assert_eq!(report.stage, McpProbeStage::Initialize);
    assert!(
        report
            .error
            .as_deref()
            .unwrap_or("")
            .contains("the endpoint answered 500"),
        "{report:?}"
    );

    let erring = legacy(LegacyScript {
        prelude: endpoint_first(),
        post_status: None,
        answer: Some(Answer::Error),
        no_tools: false,
    })
    .await;
    let report = RmcpProbe.probe(&erring, Duration::from_secs(5)).await;
    assert!(!report.ok);
    assert!(
        report
            .error
            .as_deref()
            .unwrap_or("")
            .contains("the server answered an error: not today"),
        "{report:?}"
    );

    let silent = legacy(LegacyScript {
        prelude: endpoint_first(),
        post_status: None,
        answer: None,
        no_tools: false,
    })
    .await;
    let report = RmcpProbe.probe(&silent, Duration::from_secs(5)).await;
    assert!(!report.ok);
    assert!(
        report
            .error
            .as_deref()
            .unwrap_or("")
            .contains("ended before the answer"),
        "{report:?}"
    );

    let nameless = legacy(LegacyScript {
        prelude: vec![("ping", "{}".into())],
        post_status: None,
        answer: None,
        no_tools: false,
    })
    .await;
    let report = RmcpProbe.probe(&nameless, Duration::from_secs(5)).await;
    assert!(!report.ok);
    assert_eq!(report.stage, McpProbeStage::Spawn);
    assert!(
        report
            .error
            .as_deref()
            .unwrap_or("")
            .contains("ended before naming its endpoint"),
        "{report:?}"
    );
}

/// How a raw host misbehaves on the wire.
#[derive(Clone, Copy)]
enum Cut {
    /// The connection closes with a chunk unfinished, before the endpoint.
    BeforeEndpoint,
    /// The same, once the endpoint was named.
    AfterEndpoint,
    /// A frame that runs past what the probe reads, never ending.
    Overlong,
}

/// A stream cut mid-frame by its host — the connection closed with a chunk
/// unfinished — fails as a stream, before the endpoint and after it; a frame
/// that never ends is a stream the probe does not read.
#[tokio::test(flavor = "multi_thread")]
async fn a_stream_cut_mid_frame_fails_as_a_stream() {
    use std::io::{Read as _, Write as _};
    fn cutting_server(cut: Cut) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut head = [0u8; 4096];
                let n = stream.read(&mut head).unwrap_or(0);
                let request = String::from_utf8_lossy(&head[..n]).to_string();
                if request.starts_with("POST") {
                    // The probe may hang up first: a failed write is the same cut.
                    stream
                        .write_all(b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\n\r\n")
                        .ok();
                    continue;
                }
                let body = match cut {
                    Cut::BeforeEndpoint => "event: endpo".to_string(),
                    Cut::AfterEndpoint => "event: endpoint\ndata: /messages\n\n".to_string(),
                    Cut::Overlong => {
                        format!("data: {}", "x".repeat(bisa_http::MAX_SSE_FRAME_BYTES + 16))
                    }
                };
                let chunk = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{body}\r\n",
                    body.len()
                );
                stream.write_all(chunk.as_bytes()).ok();
                stream.flush().ok();
                std::thread::sleep(Duration::from_millis(300));
                // Closed with the chunked body unfinished: no terminating chunk.
            }
        });
        format!("http://{addr}/sse")
    }
    for (cut, said) in [
        (Cut::BeforeEndpoint, "the stream failed"),
        (Cut::AfterEndpoint, "the stream failed"),
        (Cut::Overlong, "not one the probe reads"),
    ] {
        let config = McpServerConfig::Sse {
            name: "cut".into(),
            url: cutting_server(cut),
            headers: BTreeMap::new(),
        };
        let report = RmcpProbe.probe(&config, Duration::from_secs(5)).await;
        assert!(!report.ok, "{report:?}");
        assert!(
            report.error.as_deref().unwrap_or("").contains(said),
            "{said}: {report:?}"
        );
    }
}
