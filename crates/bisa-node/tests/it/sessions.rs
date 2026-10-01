//! The terminal session doors over HTTP: `POST /sessions/terminal`
//! under the control-plane token, then `exit` and `close` under the session's
//! own secret — the wire a desktop terminal host speaks for a harness a
//! person opened in it.

use bisa_engine::{Engine, EngineConfig};
use bisa_harness::mock::MockAdapter;
use bisa_harness::{HarnessCatalog, InteractiveLaunch, ReportingPlan};
use bisa_node::{serve, NodeConfig};
use bisa_store::{FileKeyStore, Workspace};
use http_body_util::{BodyExt as _, Full};
use hyper::body::Bytes;
use hyper::Request;
use hyper_util::rt::TokioIo;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::UnixStream;

const TOKEN: &str = "test-token-0123456789abcdef";

/// A node over an engine whose one harness has an interactive form that
/// reports through an argument — enough for the desk to mint a session.
struct Node {
    socket: PathBuf,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    server: Option<tokio::task::JoinHandle<()>>,
    _dir: tempfile::TempDir,
}

impl Node {
    async fn start() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let data = dir.path().to_path_buf();
        let ws = Workspace::open_with_keystore(
            &data,
            Box::new(FileKeyStore::new(
                bisa_store::Paths::new(&data).identity_dir(),
            )),
        )
        .expect("workspace");
        let mut catalog = HarnessCatalog::new();
        catalog.register(Arc::new(MockAdapter {
            id: "mock".into(),
            interactive: Some(InteractiveLaunch::new("mock")),
            reporting: ReportingPlan {
                args: vec!["--report".into()],
                ..Default::default()
            },
            ..Default::default()
        }));
        let engine = Engine::start(
            ws,
            catalog,
            EngineConfig {
                design_enabled: false,
                events_enabled: false,
                ..Default::default()
            },
        )
        .expect("engine");
        let socket = bisa_store::Paths::new(&data).node_socket();
        let (stop, stop_rx) = tokio::sync::oneshot::channel::<()>();
        let cfg = NodeConfig {
            socket: socket.clone(),
            http: None,
            data_dir: data.clone(),
            collab: None,
            fetch_attachment: None,
            token: Some(TOKEN.to_string()),
            #[cfg(feature = "a2a")]
            a2a: None,
        };
        let server = tokio::spawn(async move {
            serve(engine, cfg, async {
                let _stopped_or_dropped = stop_rx.await;
            })
            .await
            .expect("serve");
        });
        let mut actual = socket.clone();
        for _ in 0..50 {
            if actual.exists() {
                break;
            }
            if let Ok(p) = std::fs::read_to_string(bisa_node::pointer_path(&socket)) {
                actual = PathBuf::from(p.trim());
                if actual.exists() {
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        Self {
            socket: actual,
            stop: Some(stop),
            server: Some(server),
            _dir: dir,
        }
    }

    /// One request, with whatever bearer the caller presents.
    async fn req(
        &self,
        method: &str,
        path: &str,
        bearer: &str,
        body: Option<Value>,
    ) -> (u16, Value) {
        let stream = UnixStream::connect(&self.socket).await.expect("connect");
        let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
            .await
            .expect("handshake");
        tokio::spawn(conn);
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header(hyper::header::HOST, "localhost")
            .header(hyper::header::AUTHORIZATION, format!("Bearer {bearer}"))
            .header(hyper::header::CONTENT_TYPE, "application/json")
            .body(Full::new(Bytes::from(
                body.map(|b| b.to_string()).unwrap_or_default(),
            )))
            .unwrap();
        let resp = sender.send_request(request).await.expect("request");
        let status = resp.status().as_u16();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    /// A goal of the workspace's, for a session to stand in.
    async fn goal(&self) -> String {
        let (status, v) = self
            .req(
                "POST",
                "/goals",
                TOKEN,
                Some(json!({"statement": "sit in a terminal", "title": "T", "mode": "manual"})),
            )
            .await;
        assert_eq!(status, 200, "{v}");
        v["goal"]["id"].as_str().expect("the goal's id").to_string()
    }

    async fn open(&self) -> (String, String) {
        let goal = self.goal().await;
        let (status, v) = self
            .req(
                "POST",
                "/sessions/terminal",
                TOKEN,
                Some(json!({"scope": "goal", "id": goal, "harness": "mock"})),
            )
            .await;
        assert_eq!(status, 200, "{v}");
        let session = v["session"].as_str().expect("a session").to_string();
        let secret = v["env"]["BISA_SESSION_SECRET"]
            .as_str()
            .expect("the secret, in the environment")
            .to_string();
        assert!(v.get("secret").is_none(), "the secret travels once");
        assert_eq!(v["args"], json!(["--report"]));
        (session, secret)
    }

    async fn stop(mut self) {
        if let Some(stop) = self.stop.take() {
            let _server_gone = stop.send(());
        }
        if let Some(server) = self.server.take() {
            let _ended = server.await;
        }
    }
}

fn raw_git(dir: &std::path::Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?} in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Closing a workstream over HTTP stops the harness a person opened in it —
/// the row reads *aborted*, which is the frame the desktop closes the tab on —
/// and answers how many it stopped; the primary's harness is left alone.
#[tokio::test(flavor = "multi_thread")]
async fn closing_a_workstream_stops_the_harness_standing_in_it() {
    let node = Node::start().await;
    let (status, v) = node
        .req(
            "POST",
            "/projects",
            TOKEN,
            Some(json!({"kind": "new", "slug": "web"})),
        )
        .await;
    assert_eq!(status, 200, "{v}");
    let pid = v["project"]["id"].as_str().unwrap().to_string();
    let root = PathBuf::from(v["path"].as_str().unwrap());
    raw_git(&root, &["config", "user.name", "Bisa Test"]);
    raw_git(&root, &["config", "user.email", "test@example.invalid"]);
    std::fs::write(root.join("README.md"), "baseline\n").unwrap();
    raw_git(&root, &["add", "-A"]);
    raw_git(&root, &["commit", "-m", "baseline", "--quiet"]);
    let (status, v) = node
        .req(
            "POST",
            &format!("/projects/{pid}/workstreams"),
            TOKEN,
            Some(json!({"label": "checkout"})),
        )
        .await;
    assert_eq!(status, 200, "{v}");
    let wid = v["workstream"]["id"].as_str().unwrap().to_string();

    let open = |scope_id: String| {
        let node = &node;
        async move {
            let (status, v) = node
                .req(
                    "POST",
                    "/sessions/terminal",
                    TOKEN,
                    Some(json!({"scope": "workstream", "id": scope_id, "harness": "mock"})),
                )
                .await;
            assert_eq!(status, 200, "{v}");
            v["session"].as_str().expect("a session").to_string()
        }
    };
    let in_checkout = open(wid.clone()).await;
    let on_primary = open(pid.clone()).await;

    let (status, v) = node
        .req("DELETE", &format!("/workstreams/{wid}"), TOKEN, None)
        .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["stopped_sessions"], json!(1), "{v}");
    assert_eq!(v["workstream"]["state"]["state"], json!("closed"));
    let (_, v) = node
        .req("GET", &format!("/sessions/{in_checkout}"), TOKEN, None)
        .await;
    assert_eq!(
        v["state"]["state"],
        json!("aborted"),
        "the harness in the checkout is ended on the roster: {v}"
    );
    let (_, v) = node
        .req("GET", &format!("/sessions/{on_primary}"), TOKEN, None)
        .await;
    assert_eq!(
        v["state"]["state"],
        json!("starting"),
        "the primary's harness is left alone: {v}"
    );
    node.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_terminal_harness_exits_held_and_leaves_when_its_tab_closes() {
    let node = Node::start().await;
    let (session, secret) = node.open().await;

    let (status, v) = node
        .req("GET", &format!("/sessions/{session}"), TOKEN, None)
        .await;
    assert_eq!(status, 200);
    assert_eq!(v["state"]["state"], json!("starting"));

    // The process ended by itself: the row reads done and stays.
    let (status, _) = node
        .req(
            "POST",
            &format!("/sessions/{session}/exit"),
            &secret,
            Some(json!({"code": 0})),
        )
        .await;
    assert_eq!(status, 200);
    let (_, v) = node
        .req("GET", &format!("/sessions/{session}"), TOKEN, None)
        .await;
    assert_eq!(v["state"]["state"], json!("done"));

    // The tab closed: the row is gone at once.
    let (status, _) = node
        .req("POST", &format!("/sessions/{session}/close"), &secret, None)
        .await;
    assert_eq!(status, 200);
    let (status, _) = node
        .req("GET", &format!("/sessions/{session}"), TOKEN, None)
        .await;
    assert_eq!(status, 404);
    let (status, _) = node
        .req("POST", &format!("/sessions/{session}/close"), &secret, None)
        .await;
    assert_eq!(
        status, 404,
        "closed twice is a session the desk no longer knows"
    );
    node.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_exit_by_signal_reads_as_a_failure_named_after_the_signal() {
    let node = Node::start().await;
    let (session, secret) = node.open().await;
    let (status, _) = node
        .req(
            "POST",
            &format!("/sessions/{session}/exit"),
            &secret,
            Some(json!({"code": 1, "signal": "Hangup: 1"})),
        )
        .await;
    assert_eq!(status, 200);
    let (_, v) = node
        .req("GET", &format!("/sessions/{session}"), TOKEN, None)
        .await;
    assert_eq!(v["state"]["state"], json!("failed"));
    assert_eq!(v["state"]["reason"], json!("ended by Hangup: 1"));
    node.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_sessions_doors_refuse_the_control_plane_token_and_a_wrong_secret() {
    let node = Node::start().await;
    let (session, secret) = node.open().await;

    // The token opens the control plane, not a session's own doors.
    let (status, _) = node
        .req(
            "POST",
            &format!("/sessions/{session}/exit"),
            TOKEN,
            Some(json!({"code": 0})),
        )
        .await;
    assert_eq!(status, 401);
    let (status, _) = node
        .req(
            "POST",
            &format!("/sessions/{session}/close"),
            "not-the-secret",
            None,
        )
        .await;
    assert_eq!(status, 401);
    let (_, v) = node
        .req("GET", &format!("/sessions/{session}"), TOKEN, None)
        .await;
    assert_eq!(v["state"]["state"], json!("starting"), "nothing moved");

    // And a secret opens only the roster's doors, never the control plane.
    let (status, _) = node.req("GET", "/sessions", &secret, None).await;
    assert_eq!(status, 401);
    node.stop().await;
}

/// A session stands where a terminal can open. A scope nobody knows, an id
/// that is no id and a key the body does not declare are each a 400 in words;
/// an id of nothing the workspace has is a 404 — never a 500, and never a
/// row about something that is not there.
#[tokio::test(flavor = "multi_thread")]
async fn a_session_is_opened_only_where_a_terminal_can_open() {
    let node = Node::start().await;
    let goal = node.goal().await;
    let absent = "00000000010000000000000001";
    for (body, expected) in [
        (
            json!({"scope": "machine", "id": goal, "harness": "mock"}),
            400,
        ),
        (
            json!({"scope": "goal", "id": "not an id", "harness": "mock"}),
            400,
        ),
        (
            json!({"scope": "workstream", "id": "not an id", "harness": "mock"}),
            400,
        ),
        (
            json!({"scope": "goal", "id": absent, "harness": "mock"}),
            404,
        ),
        (
            json!({"scope": "workstream", "id": absent, "harness": "mock"}),
            404,
        ),
        (
            json!({"scope": "run", "id": absent, "harness": "mock"}),
            404,
        ),
        (
            json!({"scope": "goal", "id": goal, "harness": "nobody"}),
            400,
        ),
        (
            json!({"scope": "goal", "id": goal, "harness": "mock", "program": "sh"}),
            400,
        ),
    ] {
        let (status, v) = node
            .req("POST", "/sessions/terminal", TOKEN, Some(body.clone()))
            .await;
        assert_eq!(status, expected, "{body}: {v}");
        assert!(v["error"].is_string(), "{body}: {v}");
        assert!(v.get("session").is_none(), "{body}: {v}");
    }
    let (_, v) = node.req("GET", "/sessions", TOKEN, None).await;
    assert_eq!(v["sessions"], json!([]), "no row for any of them: {v}");
    node.stop().await;
}

/// The guard's door opens to the session's secret, as the report's does: the
/// hook inside a terminal holds no token, and a door shut on it is a guard
/// that never judges — the harness's own prompt standing where a refusal
/// was due. A refusal comes back as one, a call the guard has no opinion on
/// as nothing to say, and neither the token nor a wrong secret opens it.
#[tokio::test(flavor = "multi_thread")]
async fn a_terminal_harness_asks_the_guard_under_its_sessions_secret() {
    let node = Node::start().await;
    let (session, secret) = node.open().await;
    let guard = format!("/sessions/{session}/guard");
    let asking = |command: &str| {
        json!({"payload": {
            "hook_event_name": "PreToolUse",
            "tool_name": "Bash",
            "tool_input": {"command": command},
        }})
    };

    let (status, v) = node
        .req("POST", &guard, &secret, Some(asking("sudo ls")))
        .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["decision"], json!("deny"), "{v}");
    assert!(
        v["reason"].as_str().unwrap_or_default().contains("sudo"),
        "{v}"
    );

    let (status, v) = node
        .req("POST", &guard, &secret, Some(asking("ls -la")))
        .await;
    assert_eq!(status, 200, "{v}");
    assert!(
        v.get("decision").is_none_or(Value::is_null),
        "no opinion: the harness's own prompt stands: {v}"
    );

    for bearer in [TOKEN, "not-the-secret"] {
        let (status, v) = node
            .req("POST", &guard, bearer, Some(asking("ls -la")))
            .await;
        assert_eq!(status, 401, "{bearer}: {v}");
        assert!(v.get("decision").is_none(), "{bearer}: {v}");
    }
    let (status, v) = node
        .req(
            "POST",
            "/sessions/01J8ZQ0000000000000000SESS/guard",
            &secret,
            Some(asking("ls -la")),
        )
        .await;
    assert_eq!(status, 404, "a session the desk does not know: {v}");
    node.stop().await;
}

/// What a harness's hooks send is not the platform's to trust: a body that
/// is too large, no JSON, or events nobody knows is refused or passed over —
/// never a crash, never a row that moved — and a report that lands after the
/// session ended leaves the outcome as it was.
#[tokio::test(flavor = "multi_thread")]
async fn a_sessions_report_door_takes_no_harm_from_what_a_hook_sends() {
    let node = Node::start().await;
    let (session, secret) = node.open().await;
    let report = format!("/sessions/{session}/report");
    let state = |v: &serde_json::Value| v["state"]["state"].clone();

    // Far past any event a hook sends: refused before it is read.
    let huge = json!({"events": [{"tier": "progress", "event": {"type": "text_delta", "text": "x".repeat(4 * 1024 * 1024)}}]});
    let (status, v) = node.req("POST", &report, &secret, Some(huge)).await;
    assert_eq!(status, 413, "an oversized report: {v}");
    // Shapes nobody knows: each refused in words, as a body that does not
    // fit — never passed over, never a 5xx.
    for body in [
        json!({"events": "many"}),
        json!({"events": [{"tier": "invented", "event": {"type": "nothing"}}]}),
        json!({"events": [{"tier": "progress"}]}),
        json!({"events": [{"tier": "progress", "event": {"type": "turn_started"}}], "secret": "x"}),
        json!({}),
        json!([1, 2, 3]),
    ] {
        let (status, v) = node.req("POST", &report, &secret, Some(body.clone())).await;
        assert_eq!(status, 400, "{body}: {v}");
        assert!(
            v["error"].is_string(),
            "{body}: in the error body's shape: {v}"
        );
    }
    // A harness-native frame nobody parses is an event like another.
    let (status, v) = node
        .req(
            "POST",
            &report,
            &secret,
            Some(json!({"events": [{"tier": "raw", "event": {"anything": ["at", "all"]}}]})),
        )
        .await;
    assert_eq!(status, 200, "{v}");
    let (_, v) = node
        .req("GET", &format!("/sessions/{session}"), TOKEN, None)
        .await;
    assert_eq!(
        state(&v),
        json!("starting"),
        "none of it moved the row: {v}"
    );

    // The process ends; a hook that was still in flight lands after it.
    let (status, _) = node
        .req(
            "POST",
            &format!("/sessions/{session}/exit"),
            &secret,
            Some(json!({"code": 0})),
        )
        .await;
    assert_eq!(status, 200);
    let late = json!({"events": [
        {"tier": "progress", "event": {"type": "turn_started"}},
        {"tier": "progress", "event": {"type": "tool_started", "name": "Bash", "args_summary": "ls", "tier": "exec"}},
    ]});
    let (status, v) = node.req("POST", &report, &secret, Some(late)).await;
    assert_eq!(
        status, 200,
        "a late report is taken, and moves nothing: {v}"
    );
    let (_, v) = node
        .req("GET", &format!("/sessions/{session}"), TOKEN, None)
        .await;
    assert_eq!(
        state(&v),
        json!("done"),
        "a finished session never reads as running again: {v}"
    );
    node.stop().await;
}

/// A harness at its own prompt in a terminal is a `session` row in the
/// Inbox while it waits — owed, titled by the harness and where it stands,
/// its words the roster's — and gone once the prompt is answered or the
/// tab closes. The row is answered in the terminal, never here.
#[tokio::test(flavor = "multi_thread")]
async fn a_terminal_harness_waiting_at_its_prompt_is_an_inbox_row_until_answered() {
    let node = Node::start().await;
    let (status, v) = node
        .req(
            "POST",
            "/projects",
            TOKEN,
            Some(json!({"kind": "new", "slug": "web"})),
        )
        .await;
    assert_eq!(status, 200, "{v}");
    let pid = v["project"]["id"].as_str().unwrap().to_string();
    let root = PathBuf::from(v["path"].as_str().unwrap());
    raw_git(&root, &["config", "user.name", "Bisa Test"]);
    raw_git(&root, &["config", "user.email", "test@example.invalid"]);
    std::fs::write(root.join("README.md"), "baseline\n").unwrap();
    raw_git(&root, &["add", "-A"]);
    raw_git(&root, &["commit", "-m", "baseline", "--quiet"]);
    let (status, v) = node
        .req(
            "POST",
            &format!("/projects/{pid}/workstreams"),
            TOKEN,
            Some(json!({"label": "checkout"})),
        )
        .await;
    assert_eq!(status, 200, "{v}");
    let wid = v["workstream"]["id"].as_str().unwrap().to_string();
    let (status, v) = node
        .req(
            "POST",
            "/sessions/terminal",
            TOKEN,
            Some(json!({"scope": "workstream", "id": wid, "harness": "mock"})),
        )
        .await;
    assert_eq!(status, 200, "{v}");
    let session = v["session"].as_str().unwrap().to_string();
    let secret = v["env"]["BISA_SESSION_SECRET"]
        .as_str()
        .unwrap()
        .to_string();

    // Nothing waits yet: no row.
    let (_, v) = node
        .req("GET", "/inbox?filter=needs_you", TOKEN, None)
        .await;
    assert!(
        v["rows"].as_array().unwrap().is_empty(),
        "a harness that is not waiting is no row: {v}"
    );

    // The harness's own hook reports a permission prompt.
    let (status, v) = node
        .req(
            "POST",
            &format!("/sessions/{session}/report"),
            &secret,
            Some(json!({"events": [{"tier": "lifecycle", "event": {
                "type": "input_requested",
                "request": {"id": "p1", "kind": "permission", "tool_name": "Bash", "tier": "exec", "args_summary": "cargo test", "input": {"command": "cargo test"}}
            }}]})),
        )
        .await;
    assert_eq!(status, 200, "{v}");

    let (_, v) = node
        .req("GET", "/inbox?filter=needs_you", TOKEN, None)
        .await;
    let rows = v["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "one row, the waiting harness: {v}");
    let row = &rows[0];
    assert_eq!(row["kind"], json!("session"));
    assert_eq!(row["key"], json!(session));
    // Where it stands is the checkout's label: a workstream opened with a
    // label and no name is called by its branch, `work/<label>-<tail>`.
    let title = row["title"].as_str().unwrap();
    assert!(
        title.starts_with("mock · work/checkout-"),
        "the harness and where it stands: {title}"
    );
    assert_eq!(row["waiting"]["on"]["on"], json!("permission"));
    assert_eq!(row["waiting"]["on"]["tool"], json!("Bash"));
    assert_eq!(row["waiting"]["words"], json!("permission: Bash"));
    assert_eq!(row["waiting"]["workstream"], json!(wid));
    assert_eq!(row["waiting"]["harness"], json!("mock"));
    assert!(
        row["needs_action"].as_array().unwrap().is_empty(),
        "no gate: it is answered in the terminal"
    );
    let (_, v) = node.req("GET", "/inbox?source=projects", TOKEN, None).await;
    assert!(
        v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["kind"] == json!("session")),
        "a harness in a checkout comes under projects: {v}"
    );

    // Answered at the prompt: the row is gone.
    let (status, v) = node
        .req(
            "POST",
            &format!("/sessions/{session}/report"),
            &secret,
            Some(json!({"events": [{"tier": "lifecycle", "event": {"type": "input_resolved", "id": "p1"}}]})),
        )
        .await;
    assert_eq!(status, 200, "{v}");
    let (_, v) = node.req("GET", "/inbox", TOKEN, None).await;
    assert!(
        !v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["kind"] == json!("session")),
        "the wait is over, the row with it: {v}"
    );

    // Waiting again, then the tab closes: the row goes with the session.
    let (status, _) = node
        .req(
            "POST",
            &format!("/sessions/{session}/report"),
            &secret,
            Some(json!({"events": [{"tier": "lifecycle", "event": {
                "type": "input_requested",
                "request": {"id": "q1", "kind": "question", "text": "Which branch?", "options": []}
            }}]})),
        )
        .await;
    assert_eq!(status, 200);
    let (_, v) = node
        .req("GET", "/inbox?filter=needs_you", TOKEN, None)
        .await;
    assert_eq!(
        v["rows"][0]["waiting"]["words"],
        json!("a question: Which branch?"),
        "{v}"
    );
    let (status, _) = node
        .req("POST", &format!("/sessions/{session}/close"), &secret, None)
        .await;
    assert_eq!(status, 200);
    let (_, v) = node.req("GET", "/inbox", TOKEN, None).await;
    assert!(
        !v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["kind"] == json!("session")),
        "a closed tab is no row: {v}"
    );
    node.stop().await;
}
