//! A2A consume-side test (feature `a2a`): the adapter driven against a
//! scripted in-process A2A server, with a fake intake unix listener asserting
//! the completed task's data artifact arrives as a `result_submit`.
#![cfg(feature = "a2a")]

use axum::extract::State;
use axum::response::IntoResponse as _;
use axum::routing::{get, post};
use axum::{Json, Router};
use bisa_adapters::a2a::A2aAdapter;
use bisa_core::ToolTier;
use bisa_harness::{LifecycleEvent, McpMount, McpServerConfig, Outcome, SessionEvent, SessionSpec};
use futures::StreamExt as _;
use serde_json::{json, Value};
use std::future::IntoFuture as _;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};

struct Stub {
    polls: AtomicU32,
    base: std::sync::Mutex<String>,
}

async fn card(State(stub): State<Arc<Stub>>) -> Json<Value> {
    let base = stub.base.lock().unwrap().clone();
    Json(json!({
        "protocolVersion": "0.3.0",
        "name": "stub",
        "url": format!("{base}/a2a"),
        "skills": [],
    }))
}

/// Scripted lifecycle: message/send → submitted; the first two tasks/get →
/// working; then completed with one data artifact.
async fn rpc(State(stub): State<Arc<Stub>>, body: String) -> Json<Value> {
    let req: Value = serde_json::from_str(&body).expect("stub got valid JSON");
    let id = req["id"].clone();
    let method = req["method"].as_str().unwrap();
    let result = match method {
        "message/send" => {
            assert_eq!(
                req["params"]["message"]["parts"][0]["kind"],
                json!("text"),
                "adapter sends kind-tagged text parts"
            );
            json!({"id": "task-77", "contextId": "ctx-1",
                   "status": {"state": "submitted"}, "kind": "task"})
        }
        "tasks/get" => {
            assert_eq!(req["params"]["id"], json!("task-77"));
            let n = stub.polls.fetch_add(1, Ordering::SeqCst);
            if n < 2 {
                json!({"id": "task-77", "contextId": "ctx-1",
                       "status": {"state": "working"}, "kind": "task"})
            } else {
                json!({"id": "task-77", "contextId": "ctx-1",
                "status": {"state": "completed"}, "kind": "task",
                "artifacts": [{"artifactId": "a1", "parts": [
                    {"kind": "text", "text": "done"},
                    {"kind": "data", "data": {"answer": 42}},
                ]}]})
            }
        }
        other => panic!("stub got unexpected method {other}"),
    };
    Json(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

async fn start_stub() -> (String, Arc<Stub>) {
    let stub = Arc::new(Stub {
        polls: AtomicU32::new(0),
        base: std::sync::Mutex::new(String::new()),
    });
    let app = Router::new()
        .route("/.well-known/agent-card.json", get(card))
        .route("/a2a", post(rpc))
        .with_state(Arc::clone(&stub));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    *stub.base.lock().unwrap() = base.clone();
    tokio::spawn(axum::serve(listener, app).into_future());
    (base, stub)
}

/// One-shot fake intake: accepts a connection, reads a line, replies ok, and
/// hands the parsed request to the test.
async fn start_intake(
    dir: &std::path::Path,
) -> (std::path::PathBuf, tokio::sync::oneshot::Receiver<Value>) {
    let socket = dir.join("intake.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut write) = stream.into_split();
        let mut line = String::new();
        BufReader::new(read).read_line(&mut line).await.unwrap();
        let v: Value = serde_json::from_str(line.trim()).unwrap();
        write
            .write_all(b"{\"ok\":true,\"result_event\":\"deadbeef\"}\n")
            .await
            .unwrap();
        // The test may have stopped listening by the time the peer answers.
        let _captured = tx.send(v);
    });
    (socket, rx)
}

fn spec(intake_socket: &std::path::Path) -> SessionSpec {
    SessionSpec {
        work_item: None,
        cwd: std::env::temp_dir(),
        prompt: String::new(), // M1 contract: caller drives via prompt()
        model: None,
        effort: None,
        mcp_servers: vec![McpMount::platform(McpServerConfig::Stdio {
            name: "bisa".into(),
            command: "bisa".into(),
            args: vec![
                "mcp".into(),
                "--socket".into(),
                intake_socket.display().to_string(),
                "--work-item".into(),
                "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
            ],
            env: Default::default(),
            cwd: None,
        })],
        env: Default::default(),
        env_remove: Vec::new(),
        tier_ceiling: ToolTier::Write,
        output_schema: Some(json!({"type": "object"})),
        skills: vec![],
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a2a_adapter_full_run() {
    let (base, stub) = start_stub().await;
    let dir = tempfile::tempdir().unwrap();
    let (intake_socket, intake_rx) = start_intake(dir.path()).await;

    // Resolver namespace.
    assert!(A2aAdapter::resolve(&format!("a2a:{base}")).is_some());
    assert!(A2aAdapter::resolve("a2a:ftp://nope").is_none());
    assert!(A2aAdapter::resolve("acp:goose").is_none());

    // The same peer on a loopback stub's clock: the default plan waits in
    // seconds, which a remote agent earns and a stub does not.
    use bisa_harness::HarnessAdapter as _;
    let adapter = A2aAdapter::new(base.clone()).with_poll(bisa_adapters::a2a::PollPlan {
        first: std::time::Duration::from_millis(20),
        factor: 1.5,
        max: std::time::Duration::from_millis(100),
    });
    let probe = adapter.probe().await;
    assert!(probe.available, "{:?}", probe.reason);
    assert_eq!(probe.version.as_deref(), Some("0.3.0"));

    let session = adapter.launch(spec(&intake_socket)).await.expect("launch");
    let mut events = session.subscribe();
    session.prompt("do the thing".into()).await.expect("prompt");

    // Drain until the terminal end (completed on the third poll).
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            match events.next().await {
                Some(SessionEvent::Lifecycle(LifecycleEvent::Ended {
                    outcome,
                    is_terminal: true,
                })) => break outcome,
                Some(_) => continue,
                None => panic!("event stream closed without terminal end"),
            }
        }
    })
    .await
    .expect("terminal end within 30s");
    assert!(matches!(outcome, Outcome::Completed), "{outcome:?}");
    assert!(stub.polls.load(Ordering::SeqCst) >= 3);

    // The data artifact landed on the intake socket as a result_submit.
    let submitted = intake_rx.await.expect("intake was called");
    assert_eq!(submitted["op"], json!("result_submit"));
    assert_eq!(submitted["work_item"], json!("01ARZ3NDEKTSV4RRFFQ69G5FAV"));
    assert_eq!(submitted["output"], json!({"answer": 42}));

    // Resume token carries the remote task id.
    let token = session.resume_token().expect("token");
    assert_eq!(token.native_id, "task-77");
    session.dispose().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a2a_probe_unavailable_when_unreachable() {
    let adapter = A2aAdapter::resolve("a2a:http://127.0.0.1:9").unwrap();
    let probe = adapter.probe().await;
    assert!(!probe.available);
}

// ---------------------------------------------------------------------------
// The one launch that really is synchronous
// ---------------------------------------------------------------------------

/// A2A is the only adapter here whose `launch()` performs the round trip
/// itself, so it is the only one that can return
/// `HarnessError::ModelUnavailable` rather than reporting the same fact
/// through the event stream. HTTP 429 is a typed signal — no prose is read.
#[tokio::test(flavor = "multi_thread")]
async fn a2a_launch_maps_429_to_model_unavailable() {
    let base = start_quota_stub(QuotaMode::Http429).await;
    let adapter = A2aAdapter::resolve(&format!("a2a:{base}")).expect("resolves");
    let dir = tempfile::tempdir().unwrap();
    let (intake_socket, _rx) = start_intake(dir.path()).await;
    let err = adapter
        .launch(SessionSpec {
            prompt: "go".into(),
            model: Some("remote/opus".into()),
            ..spec(&intake_socket)
        })
        .await
        .err()
        .expect("launch must fail");
    assert!(err.is_model_unavailable(), "got {err:?}");
    assert!(!err.is_unavailable(), "must not walk the harness chain");
    match err {
        bisa_harness::HarnessError::ModelUnavailable {
            model, retry_after, ..
        } => {
            assert_eq!(model, "remote/opus");
            assert_eq!(retry_after, Some(90), "the Retry-After header is honoured");
        }
        other => panic!("expected ModelUnavailable, got {other:?}"),
    }
}

/// The same launch, but the remote answers 200 with a JSON-RPC error whose
/// message carries the wall. Prose, through the shared classifier.
#[tokio::test(flavor = "multi_thread")]
async fn a2a_launch_maps_quota_prose_to_model_unavailable() {
    let base = start_quota_stub(QuotaMode::RpcError).await;
    let adapter = A2aAdapter::resolve(&format!("a2a:{base}")).expect("resolves");
    let dir = tempfile::tempdir().unwrap();
    let (intake_socket, _rx) = start_intake(dir.path()).await;
    let err = adapter
        .launch(SessionSpec {
            prompt: "go".into(),
            model: Some("remote/opus".into()),
            ..spec(&intake_socket)
        })
        .await
        .err()
        .expect("launch must fail");
    assert!(err.is_model_unavailable(), "got {err:?}");
}

/// An ordinary JSON-RPC error stays a protocol error — the classifier must not
/// turn every remote failure into a model failure.
#[tokio::test(flavor = "multi_thread")]
async fn a2a_launch_keeps_ordinary_errors_ordinary() {
    let base = start_quota_stub(QuotaMode::PlainError).await;
    let adapter = A2aAdapter::resolve(&format!("a2a:{base}")).expect("resolves");
    let dir = tempfile::tempdir().unwrap();
    let (intake_socket, _rx) = start_intake(dir.path()).await;
    let err = adapter
        .launch(SessionSpec {
            prompt: "go".into(),
            model: Some("remote/opus".into()),
            ..spec(&intake_socket)
        })
        .await
        .err()
        .expect("launch must fail");
    assert!(!err.is_model_unavailable(), "got {err:?}");
}

#[derive(Clone, Copy)]
enum QuotaMode {
    /// captured shape — an HTTP 429 with `Retry-After`, what every provider
    /// and gateway in this space returns when the model is throttled.
    Http429,
    /// captured wording — Claude Code 2.1.246's own prefix list.
    RpcError,
    PlainError,
}

async fn start_quota_stub(mode: QuotaMode) -> String {
    let base = Arc::new(std::sync::Mutex::new(String::new()));
    let card_base = Arc::clone(&base);
    let app = Router::new()
        .route(
            "/.well-known/agent-card.json",
            get(move || {
                let base = card_base.lock().unwrap().clone();
                async move {
                    Json(json!({
                        "protocolVersion": "0.3.0",
                        "name": "quota-stub",
                        "url": format!("{base}/a2a"),
                        "skills": [],
                    }))
                }
            }),
        )
        .route(
            "/a2a",
            post(move || async move {
                match mode {
                    QuotaMode::Http429 => (
                        axum::http::StatusCode::TOO_MANY_REQUESTS,
                        [("retry-after", "90")],
                        "rate limited",
                    )
                        .into_response(),
                    QuotaMode::RpcError => Json(json!({
                        "jsonrpc": "2.0", "id": "1",
                        "error": {"code": -32000, "message": "You've reached your Fable 5 limit"},
                    }))
                    .into_response(),
                    QuotaMode::PlainError => Json(json!({
                        "jsonrpc": "2.0", "id": "1",
                        "error": {"code": -32602, "message": "invalid params: message.parts is empty"},
                    }))
                    .into_response(),
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = format!("http://{}", listener.local_addr().unwrap());
    *base.lock().unwrap() = addr.clone();
    tokio::spawn(axum::serve(listener, app).into_future());
    addr
}
