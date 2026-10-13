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
use bisa_harness::{
    InputAnswer, LifecycleEvent, McpMount, McpServerConfig, Outcome, SessionEvent, SessionSpec,
};
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

// ---------------------------------------------------------------------------
// A scripted peer: what `tasks/get` answers is the test's say, by what the
// peer has seen so far — how many polls, how many messages, a flag the test
// flips — so every state a task can stop in is reached on purpose.

/// What the peer has seen when a poll arrives.
struct Facts {
    polls: u32,
    sends: usize,
    flag: bool,
}

enum Reply {
    /// A task in this state, with these extra fields.
    Task(Value),
    /// A JSON-RPC error object.
    Error(Value),
    /// A body that is no JSON at all.
    NotJson,
}

enum Card {
    Fine,
    Status(u16),
    /// A card naming an endpoint nobody listens on.
    Dead,
}

struct Scripted {
    script: Box<dyn Fn(&Facts) -> Reply + Send + Sync>,
    polls: AtomicU32,
    cancels: AtomicU32,
    sends: std::sync::Mutex<Vec<Value>>,
    flag: std::sync::atomic::AtomicBool,
    base: std::sync::Mutex<String>,
    card: Card,
}

fn task(state: &str, extra: Value) -> Reply {
    let mut v =
        json!({"id": "task-1", "contextId": "ctx-1", "kind": "task", "status": {"state": state}});
    if let (Some(into), Some(from)) = (v.as_object_mut(), extra.as_object()) {
        for (k, val) in from {
            if k == "message" {
                into["status"]["message"] = val.clone();
            } else {
                into.insert(k.clone(), val.clone());
            }
        }
    }
    Reply::Task(v)
}

async fn scripted_card(State(stub): State<Arc<Scripted>>) -> axum::response::Response {
    let base = stub.base.lock().unwrap().clone();
    match stub.card {
        Card::Fine => Json(json!({"protocolVersion": "0.3.0", "name": "scripted", "url": format!("{base}/a2a"), "skills": []})).into_response(),
        Card::Status(code) => (axum::http::StatusCode::from_u16(code).unwrap(), "no card").into_response(),
        Card::Dead => Json(json!({"protocolVersion": "0.3.0", "name": "scripted", "url": "http://127.0.0.1:1/a2a", "skills": []})).into_response(),
    }
}

async fn scripted_rpc(State(stub): State<Arc<Scripted>>, body: String) -> axum::response::Response {
    let req: Value = serde_json::from_str(&body).expect("the adapter sends JSON");
    let id = req["id"].clone();
    let result = match req["method"].as_str().unwrap() {
        "message/send" => {
            stub.sends
                .lock()
                .unwrap()
                .push(req["params"]["message"].clone());
            json!({"id": "task-1", "contextId": "ctx-1", "status": {"state": "submitted"}, "kind": "task"})
        }
        "tasks/cancel" => {
            stub.cancels.fetch_add(1, Ordering::SeqCst);
            json!({"id": "task-1", "status": {"state": "canceled"}, "kind": "task"})
        }
        "tasks/get" => {
            let facts = Facts {
                polls: stub.polls.fetch_add(1, Ordering::SeqCst),
                sends: stub.sends.lock().unwrap().len(),
                flag: stub.flag.load(Ordering::SeqCst),
            };
            match (stub.script)(&facts) {
                Reply::Task(v) => v,
                Reply::Error(e) => {
                    return Json(json!({"jsonrpc": "2.0", "id": id, "error": e})).into_response()
                }
                Reply::NotJson => return "not json at all".into_response(),
            }
        }
        other => panic!("the scripted peer got {other}"),
    };
    Json(json!({"jsonrpc": "2.0", "id": id, "result": result})).into_response()
}

async fn start_scripted(
    card: Card,
    script: impl Fn(&Facts) -> Reply + Send + Sync + 'static,
) -> (String, Arc<Scripted>) {
    let stub = Arc::new(Scripted {
        script: Box::new(script),
        polls: AtomicU32::new(0),
        cancels: AtomicU32::new(0),
        sends: std::sync::Mutex::new(Vec::new()),
        flag: std::sync::atomic::AtomicBool::new(false),
        base: std::sync::Mutex::new(String::new()),
        card,
    });
    let app = Router::new()
        .route("/.well-known/agent-card.json", get(scripted_card))
        .route("/a2a", post(scripted_rpc))
        .with_state(Arc::clone(&stub));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    *stub.base.lock().unwrap() = base.clone();
    tokio::spawn(axum::serve(listener, app).into_future());
    (base, stub)
}

/// A peer on a loopback stub's clock.
fn quick(base: &str) -> A2aAdapter {
    A2aAdapter::new(base.to_string()).with_poll(bisa_adapters::a2a::PollPlan {
        first: std::time::Duration::from_millis(20),
        factor: 1.5,
        max: std::time::Duration::from_millis(60),
    })
}

/// The session's terminal end, within a bound.
async fn terminal_end(events: &mut bisa_harness::BoxEventStream) -> Outcome {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            match events.next().await {
                Some(SessionEvent::Lifecycle(LifecycleEvent::Ended {
                    outcome,
                    is_terminal: true,
                })) => break outcome,
                Some(_) => continue,
                None => panic!("the stream closed without a terminal end"),
            }
        }
    })
    .await
    .expect("a terminal end within 30 s")
}

/// The next question or sign-in the session raises, within a bound.
async fn asked(events: &mut bisa_harness::BoxEventStream) -> bisa_harness::InputRequest {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            match events.next().await {
                Some(SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request })) => {
                    break request
                }
                Some(_) => continue,
                None => panic!("the stream closed without a request"),
            }
        }
    })
    .await
    .expect("a request within 30 s")
}

/// An intake that answers one connection with these words.
async fn start_intake_saying(dir: &std::path::Path, reply: &'static str) -> std::path::PathBuf {
    let socket = dir.join("intake.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut write) = stream.into_split();
        let mut line = String::new();
        BufReader::new(read).read_line(&mut line).await.unwrap();
        write
            .write_all(format!("{reply}\n").as_bytes())
            .await
            .unwrap();
    });
    socket
}

/// A task the remote failed is the session's failure in the remote's words
/// — a model wall among them walks the plan; one the remote cancelled ends
/// aborted; and a stop of ours cancels the task and ends the session.
#[tokio::test(flavor = "multi_thread")]
async fn a2a_a_task_that_stops_on_the_remote_ends_the_session_in_the_remotes_words() {
    use bisa_harness::HarnessAdapter as _;
    let (base, _stub) = start_scripted(Card::Fine, |f| match f.polls {
        0 => task("working", json!({})),
        _ => task(
            "failed",
            json!({"message": {"parts": [{"kind": "text", "text": "usage limit reached"}]}}),
        ),
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let (intake, _rx) = start_intake(dir.path()).await;
    let adapter = quick(&base);
    assert_eq!(adapter.id(), format!("a2a:{base}"));
    assert_eq!(adapter.display_name(), "Remote A2A agent");
    assert!(adapter
        .caps()
        .contains(bisa_core::HarnessCaps::INPUT_REQUESTS));
    let session = adapter
        .launch(SessionSpec {
            prompt: "go".into(),
            ..spec(&intake)
        })
        .await
        .unwrap();
    let mut events = session.subscribe();
    assert!(matches!(
        terminal_end(&mut events).await,
        Outcome::ModelUnavailable { .. }
    ));
    assert_eq!(session.phase(), bisa_harness::Phase::Ended);
    assert_eq!(session.snapshot().phase, bisa_harness::Phase::Ended);

    let (base, _stub) = start_scripted(Card::Fine, |_| {
        task("rejected", json!({"message": "not for me"}))
    })
    .await;
    let session = quick(&base)
        .launch(SessionSpec {
            prompt: "go".into(),
            ..spec(&intake)
        })
        .await
        .unwrap();
    let mut events = session.subscribe();
    match terminal_end(&mut events).await {
        Outcome::Failed { error } => assert_eq!(error, "\"not for me\""),
        other => panic!("{other:?}"),
    }
    let (base, _stub) = start_scripted(Card::Fine, |_| task("failed", json!({}))).await;
    let session = quick(&base)
        .launch(SessionSpec {
            prompt: "go".into(),
            ..spec(&intake)
        })
        .await
        .unwrap();
    let mut events = session.subscribe();
    assert!(
        matches!(terminal_end(&mut events).await, Outcome::Failed { error } if error == "a2a task failed")
    );

    let (base, _stub) = start_scripted(Card::Fine, |_| task("canceled", json!({}))).await;
    let session = quick(&base)
        .launch(SessionSpec {
            prompt: "go".into(),
            ..spec(&intake)
        })
        .await
        .unwrap();
    let mut events = session.subscribe();
    assert!(matches!(terminal_end(&mut events).await, Outcome::Aborted));

    let (base, stub) = start_scripted(Card::Fine, |_| task("working", json!({}))).await;
    let session = quick(&base)
        .launch(SessionSpec {
            prompt: "go".into(),
            ..spec(&intake)
        })
        .await
        .unwrap();
    let mut events = session.subscribe();
    assert!(matches!(
        session.prompt("again".into()).await,
        Err(bisa_harness::HarnessError::Busy)
    ));
    assert!(matches!(
        session.steer("x".into()).await,
        Err(bisa_harness::HarnessError::NotSupported("steer"))
    ));
    assert!(matches!(
        session.follow_up("x".into()).await,
        Err(bisa_harness::HarnessError::NotSupported("follow_up"))
    ));
    session.abort().await.unwrap();
    assert!(matches!(terminal_end(&mut events).await, Outcome::Aborted));
    assert_eq!(
        stub.cancels.load(Ordering::SeqCst),
        1,
        "the task was cancelled on the remote"
    );
    assert!(matches!(
        session.prompt("again".into()).await,
        Err(bisa_harness::HarnessError::Terminated)
    ));
}

/// A task that stops for a person is a question, answered on the task:
/// the words go to the remote as the next message and the task goes on;
/// a sign-in done elsewhere resumes the polling on an allow and ends the
/// session on a deny; an answer nobody asked for is refused; and a second
/// stop while one waits asks nothing more.
#[tokio::test(flavor = "multi_thread")]
async fn a2a_a_task_that_stops_for_a_person_is_answered_on_the_task() {
    use bisa_harness::HarnessAdapter as _;
    let (base, stub) = start_scripted(Card::Fine, |f| {
        if f.sends < 2 {
            task("input-required", json!({"message": {"parts": [{"kind": "text", "text": "which database?"}]}}))
        } else {
            task("completed", json!({"artifacts": [{"artifactId": "a1", "parts": [{"kind": "text", "text": "done"}, {"kind": "text", "text": "and dusted"}]}]}))
        }
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let (intake, intake_rx) = start_intake(dir.path()).await;
    let session = quick(&base)
        .launch(SessionSpec {
            prompt: "go".into(),
            ..spec(&intake)
        })
        .await
        .unwrap();
    let mut events = session.subscribe();
    let request = asked(&mut events).await;
    assert!(
        matches!(&request.kind, bisa_harness::InputKind::Question { text, .. } if text == "which database?")
    );
    assert_eq!(session.phase(), bisa_harness::Phase::AwaitingInput);
    match session
        .answer("nobody", InputAnswer::Text { text: "x".into() })
        .await
    {
        Err(bisa_harness::HarnessError::Protocol(why)) => assert!(why.contains("nobody"), "{why}"),
        other => panic!("{other:?}"),
    }
    // The remote says the same stop again while the question waits: it is
    // asked once. Then the answer taken, the question is asked no more.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    session
        .answer(
            &request.id,
            InputAnswer::Text {
                text: "postgres".into(),
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        terminal_end(&mut events).await,
        Outcome::Completed
    ));
    let sends = stub.sends.lock().unwrap().clone();
    assert_eq!(sends.len(), 2);
    assert_eq!(
        sends[1]["taskId"],
        json!("task-1"),
        "the answer rides the task"
    );
    assert_eq!(sends[1]["parts"][0]["text"], json!("postgres"));
    let submitted = intake_rx.await.expect("the intake was called");
    assert_eq!(
        submitted["output"],
        json!({"text": "done\nand dusted"}),
        "the text parts, with no data part"
    );

    // A sign-in: allowed once done, the polling resumes; denied, the task ends.
    let (base, stub) = start_scripted(Card::Fine, |f| {
        if f.flag {
            task("completed", json!({"artifacts": [{"artifactId": "a1", "parts": [{"kind": "data", "data": {"ok": true}}]}]}))
        } else {
            task("auth-required", json!({}))
        }
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let (intake, _rx) = start_intake(dir.path()).await;
    let session = quick(&base)
        .launch(SessionSpec {
            prompt: "go".into(),
            ..spec(&intake)
        })
        .await
        .unwrap();
    let mut events = session.subscribe();
    let request = asked(&mut events).await;
    assert!(
        matches!(&request.kind, bisa_harness::InputKind::Auth { .. }),
        "{request:?}"
    );
    stub.flag.store(true, Ordering::SeqCst);
    session
        .answer(&request.id, InputAnswer::Allow { input: None })
        .await
        .unwrap();
    assert!(matches!(
        terminal_end(&mut events).await,
        Outcome::Completed
    ));

    let (base, _stub) = start_scripted(Card::Fine, |_| task("auth-required", json!({}))).await;
    let session = quick(&base)
        .launch(SessionSpec {
            prompt: "go".into(),
            ..spec(&intake)
        })
        .await
        .unwrap();
    let mut events = session.subscribe();
    let request = asked(&mut events).await;
    session
        .answer(
            &request.id,
            InputAnswer::Deny {
                reason: "no".into(),
            },
        )
        .await
        .unwrap();
    assert!(matches!(terminal_end(&mut events).await, Outcome::Aborted));
}

/// A poll the remote answers with an error ends the session at once; one
/// the remote answers with no JSON, or does not answer at all, is given
/// five tries and then ends it; a revived session is polled the same way.
#[tokio::test(flavor = "multi_thread")]
async fn a2a_a_poll_the_remote_cannot_answer_ends_the_session_after_its_tries() {
    use bisa_harness::HarnessAdapter as _;
    let dir = tempfile::tempdir().unwrap();
    let (intake, _rx) = start_intake(dir.path()).await;
    let (base, _stub) = start_scripted(Card::Fine, |_| {
        Reply::Error(json!({"code": -32001, "message": "no such task"}))
    })
    .await;
    let session = quick(&base)
        .launch(SessionSpec {
            prompt: "go".into(),
            ..spec(&intake)
        })
        .await
        .unwrap();
    let mut events = session.subscribe();
    assert!(
        matches!(terminal_end(&mut events).await, Outcome::Failed { error } if error.contains("tasks/get error"))
    );

    let (base, stub) = start_scripted(Card::Fine, |_| Reply::NotJson).await;
    let session = quick(&base)
        .launch(SessionSpec {
            prompt: "go".into(),
            ..spec(&intake)
        })
        .await
        .unwrap();
    let mut events = session.subscribe();
    assert!(
        matches!(terminal_end(&mut events).await, Outcome::Failed { error } if error.starts_with("tasks/get:"))
    );
    assert_eq!(stub.polls.load(Ordering::SeqCst), 5, "five tries");

    // Revived from its token against a peer whose endpoint nobody serves.
    let (base, _stub) = start_scripted(Card::Dead, |_| task("working", json!({}))).await;
    let adapter = quick(&base);
    let token = bisa_harness::ResumeToken {
        adapter_id: format!("a2a:{base}"),
        native_id: "task-1".into(),
        cwd: std::env::temp_dir(),
        transcript_path: None,
        model: None,
        effort: None,
    };
    let revived = adapter.attach(&token).await.unwrap();
    assert_eq!(revived.phase(), bisa_harness::Phase::Turn);
    assert_eq!(revived.resume_token().unwrap().native_id, "task-1");
    let mut events = revived.subscribe();
    assert!(
        matches!(terminal_end(&mut events).await, Outcome::Failed { error } if error.contains("remote unreachable"))
    );
}

/// A completed task whose result the intake refuses is a failure, said;
/// a session with no intake to hand its result to completes all the same —
/// a remote mount, or the platform's mount naming no work item, is no
/// intake; and a peer whose card is no card is unavailable by its status.
#[tokio::test(flavor = "multi_thread")]
async fn a2a_a_result_the_intake_refuses_is_a_failure_and_no_intake_is_no_failure() {
    use bisa_harness::HarnessAdapter as _;
    let (base, _stub) = start_scripted(Card::Fine, |_| {
        task("completed", json!({"artifacts": [{"artifactId": "a1", "parts": [{"kind": "data", "data": {"answer": 1}}]}]}))
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let intake = start_intake_saying(dir.path(), r#"{"ok":false,"error":"not yours"}"#).await;
    let session = quick(&base)
        .launch(SessionSpec {
            prompt: "go".into(),
            ..spec(&intake)
        })
        .await
        .unwrap();
    let mut events = session.subscribe();
    assert!(
        matches!(terminal_end(&mut events).await, Outcome::Failed { error } if error.contains("result intake failed"))
    );

    let mut no_intake = spec(&intake);
    no_intake.prompt = "go".into();
    no_intake.mcp_servers = vec![
        McpMount::platform(McpServerConfig::Http {
            name: "docs".into(),
            url: "https://mcp.example.com/".into(),
            headers: Default::default(),
        }),
        McpMount::platform(McpServerConfig::Stdio {
            name: "bisa".into(),
            command: "bisa".into(),
            args: vec!["mcp".into(), "--socket".into(), "/tmp/x.sock".into()],
            env: Default::default(),
            cwd: None,
        }),
    ];
    let session = quick(&base).launch(no_intake).await.unwrap();
    let mut events = session.subscribe();
    assert!(matches!(
        terminal_end(&mut events).await,
        Outcome::Completed
    ));

    let (base, _stub) = start_scripted(Card::Status(503), |_| task("working", json!({}))).await;
    let probe = quick(&base).probe().await;
    assert!(!probe.available);
    assert!(
        probe
            .reason
            .as_deref()
            .unwrap_or_default()
            .contains("agent card HTTP 503"),
        "{:?}",
        probe.reason
    );
}
