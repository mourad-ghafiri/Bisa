//! Daemon integration tests: a real engine + workspace on a tempdir, driven
//! over HTTP on the unix socket — the same surface the desktop app uses.
//!
//! Guided mode is disabled in every test but one: waking it needs an
//! installed harness, and these tests must run anywhere. What guided mode
//! *produces* (questions, gates, answers) is exercised through the API by
//! opening gates directly.

use bisa_codehost::CodeHost as _;
use bisa_core::event::{JournalPayload, RunFact, StepFact};
use bisa_core::{
    AgentId, AskKind, Gate, Goal, GoalId, MessageBody, ProjectId, RunId, RunOutcome, SessionId,
    StepId, TeamId, WorkItemId, WorkflowId,
};
use bisa_engine::{Engine, EngineConfig};
use bisa_harness::HarnessCatalog;
use bisa_node::{serve, NodeConfig};
use bisa_store::{FileKeyStore, PostOrigin, Workspace};
use http_body_util::{BodyExt as _, Full};
use hyper::body::Bytes;
use hyper::Request;
use hyper_util::rt::TokioIo;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UnixStream;

/// The control-plane token every test node is started with, and every request
/// in this file presents. One per process: the value is arbitrary.
const TOKEN: &str = "test-token-0123456789abcdef";

/// The General Agent, as the wire spells it.
const CORE_AGENT_ID: &str = AgentId::GENERAL;

/// A one-`human`-step workflow: the smallest run that waits on a person.
pub(crate) fn human_workflow() -> Value {
    json!({
        "name": "Ask first",
        "description": "one question, then done",
        "steps": [{
            "id": "ask", "name": "Which database?", "kind": "human",
            "prompt": "Which database should we use?",
            "options": [
                {"id": "sqlite", "label": "SQLite"},
                {"id": "postgres", "label": "Postgres", "recommended": true}
            ]
        }]
    })
}

/// A one-`agent`-step workflow on a harness nobody has: the step becomes a
/// work item and settles failed on its own, which is all these tests need.
fn agent_workflow() -> Value {
    json!({
        "name": "Build it",
        "steps": [{
            "id": "build", "name": "Build", "kind": "agent",
            "instructions": "do the thing", "harness": ["no-such-harness"]
        }]
    })
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

pub(crate) struct Node {
    socket: PathBuf,
    /// The engine's own intake socket, the door a harness session speaks
    /// through. Held beside the HTTP socket so one test can drive both: what
    /// an agent does and what a screen then sees are two surfaces over one
    /// workspace, and nothing else in this file crosses between them.
    intake: PathBuf,
    /// Store handle kept across `serve` for state a route cannot create
    /// (agent-authored messages have no HTTP path — the API posts as owner).
    pub(crate) ws: Arc<Workspace>,
    /// The engine bus, subscribed before the engine is moved into `serve`.
    ///
    /// Held so a test can assert what a route *did not* say. Most of this file
    /// checks responses; the notes routes have an invariant about silence —
    /// a write must not announce itself to the surface that made it — and
    /// silence is only observable here.
    bus: tokio::sync::broadcast::Receiver<bisa_engine::events::EngineEvent>,
    /// The engine's inner state, kept so a test can do what no route can —
    /// withdraw a live gate, the "opened by another process" case.
    inner: Arc<bisa_engine::Inner>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    server: Option<tokio::task::JoinHandle<anyhow::Result<()>>>,
    _dir: tempfile::TempDir,
}

impl Node {
    pub(crate) async fn start() -> Self {
        Self::start_with(EngineConfig {
            design_enabled: false,
            ..Default::default()
        })
        .await
    }

    pub(crate) async fn start_with(config: EngineConfig) -> Self {
        Self::start_in(tempfile::tempdir().expect("tempdir"), config).await
    }

    /// A node over an existing workspace directory — the second half of a
    /// restart: the same files, a fresh process.
    pub(crate) async fn start_in(dir: tempfile::TempDir, config: EngineConfig) -> Self {
        Self::start_in_with(dir, config, None, None).await
    }

    /// A node that listens on TCP as well, at whatever port is free.
    pub(crate) async fn start_listening() -> Self {
        Self::start_in_with(
            tempfile::tempdir().expect("tempdir"),
            EngineConfig {
                design_enabled: false,
                ..Default::default()
            },
            None,
            Some(([127, 0, 0, 1], 0).into()),
        )
        .await
    }

    /// A node lent a collaboration pump's doors — a fake, in these tests.
    pub(crate) async fn start_with_collab(collab: bisa_node::collab::Collab) -> Self {
        Self::start_in_with(
            tempfile::tempdir().expect("tempdir"),
            EngineConfig {
                design_enabled: false,
                ..Default::default()
            },
            Some(collab),
            None,
        )
        .await
    }

    async fn start_in_with(
        dir: tempfile::TempDir,
        mut config: EngineConfig,
        collab: Option<bisa_node::collab::Collab>,
        http: Option<std::net::SocketAddr>,
    ) -> Self {
        let data = dir.path().to_path_buf();
        // The engine's git sees an empty global config and no system one, so
        // this machine's identity, signing and hooks never reach a fixture —
        // and a commit the executor makes cannot stall on them.
        if config.git.is_none() {
            let global = data.join("test.gitconfig");
            std::fs::write(&global, "").expect("an empty global git config");
            config.git = Some(
                bisa_vcs::Git::new()
                    .with_env("GIT_CONFIG_GLOBAL", global)
                    .with_env("GIT_CONFIG_NOSYSTEM", "1"),
            );
        }
        let ws = Workspace::open_with_keystore(
            &data,
            Box::new(FileKeyStore::new(
                bisa_store::Paths::new(&data).identity_dir(),
            )),
        )
        .expect("workspace");
        let engine = Engine::start(ws, HarnessCatalog::new(), config).expect("engine");
        let ws = Arc::clone(&engine.inner().ws);
        let inner = Arc::clone(engine.inner());
        let bus = engine.events();
        let intake = engine.socket_path().to_path_buf();

        let socket = bisa_store::Paths::new(&data).node_socket();
        let (stop, stop_rx) = tokio::sync::oneshot::channel::<()>();
        let cfg = NodeConfig {
            socket: socket.clone(),
            http,
            data_dir: data.clone(),
            collab,
            fetch_attachment: None,
            token: Some(TOKEN.to_string()),
            #[cfg(feature = "a2a")]
            a2a: None,
        };
        let server = tokio::spawn(serve(engine, cfg, async {
            let _stopped_or_dropped = stop_rx.await;
        }));

        // The socket may land at a short fallback path on long tempdirs.
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
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(actual.exists(), "node socket never appeared");

        Self {
            socket: actual,
            intake,
            ws,
            bus,
            inner,
            stop: Some(stop),
            server: Some(server),
            _dir: dir,
        }
    }

    /// One intake request/response on the engine's socket: the wire a harness
    /// session's MCP tools speak, one JSON object per line.
    pub(crate) async fn intake_op(&self, req: Value) -> Value {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        let stream = UnixStream::connect(&self.intake)
            .await
            .expect("connect intake");
        let (read, mut write) = stream.into_split();
        write
            .write_all(format!("{req}\n").as_bytes())
            .await
            .unwrap();
        write.flush().await.unwrap();
        let mut line = String::new();
        BufReader::new(read).read_line(&mut line).await.unwrap();
        serde_json::from_str(line.trim()).expect("intake reply is JSON")
    }

    /// The HTTP socket, for a test that reads bytes rather than JSON.
    pub(crate) fn socket(&self) -> &std::path::Path {
        &self.socket
    }

    /// One request with the content type and the bytes the caller says — for
    /// what the JSON helpers cannot send: a body that is not JSON, or one
    /// sized past a route's limit.
    pub(crate) async fn req_raw(
        &self,
        method: &str,
        path: &str,
        content_type: Option<&str>,
        body: Vec<u8>,
    ) -> (u16, Value) {
        request_raw(&self.socket, method, path, content_type, body).await
    }

    pub(crate) async fn req(&self, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
        request(&self.socket, method, path, body).await
    }

    pub(crate) async fn get(&self, path: &str) -> Value {
        let (code, v) = self.req("GET", path, None).await;
        assert_eq!(code, 200, "GET {path} failed: {v}");
        v
    }

    /// The workspace root, for a test that needs a path known to be inside it.
    pub(crate) fn data_dir(&self) -> &std::path::Path {
        self._dir.path()
    }

    /// `PATCH`, asserting 200. Named apart from `patch` so the notes tests read
    /// the same way the rest of this file does.
    pub(crate) async fn post_patch(&self, path: &str, body: Value) -> Value {
        let (status, v) = self.req("PATCH", path, Some(body)).await;
        assert_eq!(status, 200, "PATCH {path} -> {v}");
        v
    }

    pub(crate) async fn post(&self, path: &str, body: Value) -> Value {
        let (code, v) = self.req("POST", path, Some(body)).await;
        assert_eq!(code, 200, "POST {path} failed: {v}");
        v
    }

    /// A fresh reader of the engine's bus, for a test in another module
    /// that asserts what a route announced.
    pub(crate) fn events(
        &self,
    ) -> tokio::sync::broadcast::Receiver<bisa_engine::events::EngineEvent> {
        self.bus.resubscribe()
    }

    /// A captured goal, straight through the creation path: a draft with no
    /// workflow yet.
    pub(crate) async fn new_goal(&self, statement: &str) -> GoalId {
        let v = self
            .post("/goals", json!({"statement": statement, "title": "T"}))
            .await;
        let goal: Goal = serde_json::from_value(v["goal"].clone()).expect("goal");
        goal.id
    }

    /// Record a workflow; returns its id.
    pub(crate) async fn workflow(&self, definition: Value) -> WorkflowId {
        let v = self.post("/workflows", definition).await;
        serde_json::from_value(v["workflow"]["id"].clone()).expect("workflow id")
    }

    /// Point the goal at a workflow and start its run; returns the run id.
    pub(crate) async fn start_run(&self, id: GoalId, workflow: WorkflowId) -> RunId {
        self.req(
            "PUT",
            &format!("/goals/{id}/workflow"),
            Some(json!({"workflow": workflow.to_string()})),
        )
        .await;
        let v = self.post(&format!("/goals/{id}/run"), json!({})).await;
        serde_json::from_value(v["run"]["id"].clone()).expect("run id")
    }

    /// The goal's current run's id — what its steps are answered through.
    pub(crate) async fn current_run(&self, id: GoalId) -> RunId {
        let v = self.get(&format!("/goals/{id}")).await;
        serde_json::from_value(v["run"]["id"].clone())
            .unwrap_or_else(|_| panic!("the goal has no run: {v}"))
    }

    /// Start the one-question workflow on a goal and return the gate its
    /// `human` step opened — the thing an inbox row carries.
    pub(crate) async fn ask(&self, id: GoalId) -> String {
        let wf = self.workflow(human_workflow()).await;
        self.start_run(id, wf).await;
        let v = self.get(&format!("/goals/{id}")).await;
        v["pending_gates"][0]["id"]
            .as_str()
            .unwrap_or_else(|| panic!("the human step opened no gate: {v}"))
            .to_string()
    }

    /// Start the one-agent-step workflow on a goal and return the work item
    /// its step became.
    async fn agent_item(&self, id: GoalId) -> String {
        let wf = self.workflow(agent_workflow()).await;
        self.start_run(id, wf).await;
        for _ in 0..50 {
            let v = self.get(&format!("/goals/{id}")).await;
            if let Some(item) = v["work_items"][0]["id"].as_str() {
                return item.to_string();
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("the agent step never became a work item");
    }

    pub(crate) async fn shutdown(mut self) {
        self.stop_server().await;
    }

    /// Stop this process's node and start another over the same workspace:
    /// no live gate, no in-memory mark survives — only the files.
    pub(crate) async fn restart(mut self) -> Self {
        self.stop_server().await;
        let Node { _dir, .. } = self;
        Self::start_in(
            _dir,
            EngineConfig {
                design_enabled: false,
                ..Default::default()
            },
        )
        .await
    }

    async fn stop_server(&mut self) {
        let _server_gone = self.stop.take().unwrap().send(());
        let server = self.server.take().unwrap();
        tokio::time::timeout(Duration::from_secs(10), server)
            .await
            .expect("server did not stop")
            .expect("server task")
            .expect("serve error");
    }
}

async fn request(
    socket: &std::path::Path,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (u16, Value) {
    request_raw(
        socket,
        method,
        path,
        Some("application/json"),
        body.map(|b| b.to_string().into_bytes()).unwrap_or_default(),
    )
    .await
}

/// The one wire helper the JSON forms wrap: any content type (or none), any
/// bytes, the token on every request.
async fn request_raw(
    socket: &std::path::Path,
    method: &str,
    path: &str,
    content_type: Option<&str>,
    body: Vec<u8>,
) -> (u16, Value) {
    let stream = UnixStream::connect(socket).await.expect("connect");
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .expect("handshake");
    tokio::spawn(conn);
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(hyper::header::HOST, "localhost")
        .header(hyper::header::AUTHORIZATION, format!("Bearer {TOKEN}"));
    if let Some(ct) = content_type {
        builder = builder.header(hyper::header::CONTENT_TYPE, ct);
    }
    let request = builder.body(Full::new(Bytes::from(body))).unwrap();
    let resp = sender.send_request(request).await.expect("request");
    let status = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

/// One request with an extra header, answering the status and the response
/// headers — for a route whose answer is in its headers (a cached attachment).
async fn request_headers(
    socket: &std::path::Path,
    method: &str,
    path: &str,
    extra: Option<(&str, &str)>,
) -> (u16, hyper::HeaderMap, usize) {
    let stream = UnixStream::connect(socket).await.expect("connect");
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .expect("handshake");
    tokio::spawn(conn);
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(hyper::header::HOST, "localhost")
        .header(hyper::header::AUTHORIZATION, format!("Bearer {TOKEN}"));
    if let Some((k, v)) = extra {
        builder = builder.header(k, v);
    }
    let request = builder.body(Full::new(Bytes::new())).unwrap();
    let resp = sender.send_request(request).await.expect("request");
    let status = resp.status().as_u16();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, headers, bytes.len())
}

// ---------------------------------------------------------------------------
// Goals: the guided lifecycle over HTTP
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn goal_lifecycle_over_uds() {
    let node = Node::start().await;

    let v = node.get("/health").await;
    assert_eq!(v["ok"], json!(true));

    // Create: auto by default — one text box is the whole interaction. A
    // fresh goal is a draft with no workflow.
    let v = node
        .post(
            "/goals",
            json!({"statement": "ship the demo", "title": "Demo"}),
        )
        .await;
    let goal: Goal = serde_json::from_value(v["goal"].clone()).unwrap();
    let id = goal.id;
    assert_eq!(
        goal.mode,
        bisa_core::GoalMode::Auto,
        "goals are auto unless the capture or `goals.default_mode` says otherwise"
    );
    assert!(goal.workflow.is_none() && goal.run.is_none());

    // Blank statements are refused.
    let (code, _) = node
        .req("POST", "/goals", Some(json!({"statement": "  "})))
        .await;
    assert_eq!(code, 400);

    // Status carries the projection and the guidance: an auto draft with no
    // workflow is in the Workflow Agent's `design` phase (designing is off
    // here, so nobody is actually designing).
    let v = node.get(&format!("/goals/{id}")).await;
    assert_eq!(v["status"], json!("draft"));
    assert!(v["run"].is_null());
    assert_eq!(v["guidance"]["mode"], json!("auto"));
    assert_eq!(v["guidance"]["design_enabled"], json!(false));
    assert_eq!(v["guidance"]["phase"], json!("design"));
    assert_eq!(v["guidance"]["open_questions"].as_array().unwrap().len(), 0);

    // Nothing to run before a workflow is chosen: a refusal that says so.
    let (code, v) = node
        .req("POST", &format!("/goals/{id}/run"), Some(json!({})))
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(
        v["error"]
            .as_str()
            .is_some_and(|e| e.contains("has no workflow")),
        "{v}"
    );

    // Choose a workflow and start its run: the one human step waits on us.
    let wf = node.workflow(human_workflow()).await;
    let (code, v) = node
        .req(
            "PUT",
            &format!("/goals/{id}/workflow"),
            Some(json!({"workflow": wf.to_string()})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["goal"]["workflow"], json!(wf.to_string()));
    let v = node.post(&format!("/goals/{id}/run"), json!({})).await;
    let run: RunId = serde_json::from_value(v["run"]["id"].clone()).unwrap();
    assert_eq!(v["run"]["outcome"], Value::Null, "still going: {v}");
    assert_eq!(v["run"]["steps"]["ask"]["state"]["state"], json!("waiting"));

    let v = node.get(&format!("/goals/{id}")).await;
    assert_eq!(v["status"], json!("waiting"));
    assert_eq!(v["guidance"]["phase"], json!("waiting"));
    assert_eq!(v["runs"].as_array().unwrap().len(), 1);
    assert_eq!(v["pending_gates"].as_array().unwrap().len(), 1);
    assert_eq!(v["pending_gates"][0]["gate"], json!("escalation"));
    assert_eq!(v["pending_gates"][0]["step"], json!("ask"));
    assert_eq!(v["pending_gates"][0]["expects"]["kind"], json!("answer"));

    // A second run made while this one is live is queued behind it: the
    // view lists both, newest first, the queued one numbered.
    let (code, v) = node
        .req("POST", &format!("/goals/{id}/run"), Some(json!({})))
        .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["status"], json!("queued"));
    assert!(v["run"].get("started_at").is_none(), "{v}");
    let queued: RunId = serde_json::from_value(v["run"]["id"].clone()).unwrap();
    let v = node.get(&format!("/goals/{id}")).await;
    assert_eq!(v["status"], json!("waiting"), "the live run is the goal's");
    assert_eq!(v["run"]["id"], json!(run.to_string()));
    let runs = v["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0]["id"], json!(queued.to_string()), "newest first");
    assert_eq!(runs[0]["status"], json!("queued"));
    assert_eq!(runs[0]["position"], json!(1));
    assert_eq!(runs[1]["status"], json!("waiting"));
    assert!(runs[1].get("position").is_none());
    // Withdrawn: cancelled with its cause, and the queue is empty.
    let (code, v) = node
        .req("DELETE", &format!("/goals/{id}/runs/{queued}"), None)
        .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["run"]["cancelled"], json!({"cause": "withdrawn"}));
    let (code, _) = node
        .req("DELETE", &format!("/goals/{id}/runs/{run}"), None)
        .await;
    assert_eq!(code, 409, "the live run is not withdrawn");

    // The question is waiting on a human, in the inbox, bound to its step.
    let v = node.get("/inbox").await;
    let row = v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == json!(id.to_string()))
        .expect("goal row");
    let action = &row["needs_action"][0];
    assert_eq!(action["gate_kind"], json!("escalation"));
    assert_eq!(action["run"], json!(run.to_string()));
    assert_eq!(action["step"], json!("ask"));
    assert_eq!(row["status"], json!("waiting"));

    // Answer the step: an option it offered plus a sentence. An option it
    // never offered is refused, not dropped.
    let (code, _) = node
        .req(
            "POST",
            &format!("/runs/{run}/steps/ask/answer"),
            Some(json!({"answer": {"selected": ["mysql"]}})),
        )
        .await;
    assert_eq!(code, 400);
    let v = node
        .post(
            &format!("/runs/{run}/steps/ask/answer"),
            json!({"answer": {"selected": ["postgres"], "text": "for full-text search"}}),
        )
        .await;
    assert_eq!(v["run"]["outcome"], json!("done"), "{v}");
    assert_eq!(v["run"]["steps"]["ask"]["state"]["state"], json!("done"));
    assert_eq!(
        v["run"]["steps"]["ask"]["answer"]["selected"],
        json!(["postgres"])
    );
    let v = node.get(&format!("/goals/{id}")).await;
    assert_eq!(v["status"], json!("done"));
    assert!(v["pending_gates"].as_array().unwrap().is_empty());

    // The journal records the walk: the run starting, the step's facts, the
    // run finishing.
    let v = node.get(&format!("/goals/{id}/journal?limit=50")).await;
    let types: Vec<&str> = v["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["payload"]["type"].as_str())
        .collect();
    assert!(types.contains(&"run"), "{types:?}");
    assert!(types.contains(&"step"), "{types:?}");

    // A finished goal can run again — its new run is its own record, beside
    // the finished one and the withdrawn one: a withdrawal cancels a run, it
    // does not erase it.
    let v = node.post(&format!("/goals/{id}/run"), json!({})).await;
    assert_ne!(v["run"]["id"], json!(run.to_string()));
    let v = node.get(&format!("/goals/{id}")).await;
    assert_eq!(v["runs"].as_array().unwrap().len(), 3);
    assert_eq!(v["status"], json!("waiting"));

    // Closing cancels the unfinished run and is the only move left.
    let v = node
        .post(
            &format!("/goals/{id}/close"),
            json!({"rationale": "enough"}),
        )
        .await;
    assert_eq!(v["goal"]["closed"]["reason"], json!("abandoned"));
    let v = node.get(&format!("/goals/{id}")).await;
    assert_eq!(v["status"], json!("closed"));
    assert!(
        !v["run"]["cancelled"].is_null(),
        "closing cancels the run, and the run says why: {v}"
    );
    assert_eq!(
        v["run"]["outcome"],
        Value::Null,
        "cancelled is not an outcome"
    );

    // Bad id → 400. Delete → gone.
    let (code, _) = node.req("GET", "/goals/not-a-ulid", None).await;
    assert_eq!(code, 400);
    let (code, _) = node.req("DELETE", &format!("/goals/{id}"), None).await;
    assert_eq!(code, 200);
    let (code, _) = node.req("GET", &format!("/goals/{id}"), None).await;
    assert_eq!(code, 404);

    node.shutdown().await;
}

/// An `agent` step is a work item bound to its run and step, and the caller
/// never names the runner: the engine records who took it, and the route
/// that used to add a free-standing item is gone.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_step_becomes_a_work_item_bound_to_its_step_and_the_caller_never_names_the_runner()
{
    let node = Node::start().await;
    let id = node.new_goal("build the thing").await;
    let item = node.agent_item(id).await;

    let v = node.get(&format!("/goals/{id}")).await;
    let run = v["run"]["id"].as_str().unwrap().to_string();
    let spec = &v["work_items"][0];
    assert_eq!(spec["id"], json!(item));
    assert_eq!(spec["run"], json!(run));
    assert_eq!(spec["step"], json!("build"));
    assert!(
        spec["agent"].is_null(),
        "no item names a runner yet: {spec}"
    );
    assert_eq!(v["run"]["steps"]["build"]["work_item"], json!(item));

    // The unknown harness settles the item, and the step follows it: the
    // run fails because `on_fail` defaults to `fail`.
    let mut failed = false;
    for _ in 0..50 {
        let v = node.get(&format!("/goals/{id}")).await;
        if v["run"]["outcome"] == json!("failed") {
            failed = true;
            assert_eq!(
                v["run"]["steps"]["build"]["state"]["state"],
                json!("failed")
            );
            assert_eq!(v["status"], json!("failed"));
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        failed,
        "an unknown harness must fail the step and the run: {}",
        node.get(&format!("/goals/{id}")).await["run"]
    );

    // There is no route that adds a work item outside a run.
    let (code, _) = node
        .req(
            "POST",
            &format!("/goals/{id}/work-items"),
            Some(json!({"instructions": "orphan"})),
        )
        .await;
    assert_eq!(code, 404, "no such route");

    node.shutdown().await;
}

/// An `agent` step may only place work in a project its goal is **attached
/// to**. Existence is the wrong check: a project id is a ULID somebody could
/// have copied from anywhere. The refusal is the step's failure, and it says
/// why.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_step_naming_an_unattached_project_fails_the_step_and_says_why() {
    let node = Node::start().await;
    let owner = node.new_goal("owns the folder").await;
    let stranger = node.new_goal("has no business here").await;
    let v = node
        .post(
            &format!("/goals/{owner}/projects"),
            json!({"kind": "new", "slug": "storefront"}),
        )
        .await;
    let pid = v["project"]["id"].as_str().unwrap().to_string();

    let wf = node
        .workflow(json!({
            "name": "Work in it",
            "steps": [{
                "id": "work", "name": "Work", "kind": "agent",
                "instructions": "work in it", "project": pid,
                "harness": ["no-such-harness"]
            }]
        }))
        .await;
    node.start_run(stranger, wf).await;
    let mut error = String::new();
    for _ in 0..50 {
        let v = node.get(&format!("/goals/{stranger}")).await;
        if v["run"]["outcome"] == json!("failed") {
            error = v["run"]["steps"]["work"]["error"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        error.contains("is not attached to"),
        "{error:?}: {}",
        node.get(&format!("/goals/{stranger}")).await["run"]
    );
    assert!(
        node.get(&format!("/goals/{stranger}")).await["work_items"]
            .as_array()
            .unwrap()
            .is_empty(),
        "a refused placement writes no items"
    );

    // Attached, the same step runs (and fails only for want of a harness).
    node.post(
        &format!("/projects/{pid}/attach"),
        json!({"goal": stranger.to_string()}),
    )
    .await;
    node.post(&format!("/goals/{stranger}/run"), json!({}))
        .await;
    let mut placed = false;
    for _ in 0..50 {
        let v = node.get(&format!("/goals/{stranger}")).await;
        if let Some(item) = v["work_items"].as_array().and_then(|w| w.first()) {
            assert_eq!(item["project"], json!(pid));
            placed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(placed, "the attached project takes the work");

    node.shutdown().await;
}

/// What the library refuses over HTTP, and that a refusal leaves it as it
/// was: a body the node cannot read, a workflow nobody has, an archived one
/// picked for a goal or run, a template installed twice.
#[tokio::test(flavor = "multi_thread")]
async fn the_library_refuses_in_words_and_a_refusal_changes_nothing() {
    let node = Node::start().await;
    let count = |v: &Value| v["workflows"].as_array().map(Vec::len).unwrap_or(0);
    let before = count(&node.get("/workflows").await);

    for (body, why) in [
        (json!({"steps": []}), "no name"),
        (
            json!({"name": "x", "steps": [{"id": "a", "name": "A", "kind": "teleport"}]}),
            "a kind nobody has",
        ),
        (
            json!({"name": "x", "steps": [{"id": "A B", "name": "A", "kind": "end", "finish": "done"}]}),
            "a step id that is no id",
        ),
        (
            json!({"name": "x", "steps": "many"}),
            "steps that are no list",
        ),
        (
            json!({"name": "x", "steps": [], "surprise": true}),
            "a key nobody knows",
        ),
    ] {
        let (status, v) = node.req("POST", "/workflows", Some(body.clone())).await;
        assert!(
            (400..500).contains(&status),
            "create with {why}: {status} {v}"
        );
        assert!(
            v["error"].as_str().is_some_and(|e| !e.is_empty()),
            "{why}: {v}"
        );
        let (status, v) = node.req("POST", "/workflows/validate", Some(body)).await;
        assert!(
            (400..500).contains(&status),
            "validate with {why}: {status} {v}"
        );
    }
    assert_eq!(
        count(&node.get("/workflows").await),
        before,
        "a refusal writes nothing"
    );

    // Read but unsound is not refused: it is a draft, kept with its problems
    // named, and one that cannot start.
    let unsound = json!({"name": "  ", "steps": []});
    let v = node.post("/workflows/validate", unsound.clone()).await;
    let kinds = |v: &Value| -> Vec<String> {
        v["problems"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["kind"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(kinds(&v), vec!["empty_name", "no_start"], "{v}");
    assert_eq!(
        count(&node.get("/workflows").await),
        before,
        "validating writes nothing"
    );
    let v = node.post("/workflows", unsound).await;
    assert_eq!(kinds(&v), vec!["empty_name", "no_start"], "{v}");
    let draft = v["workflow"]["id"].as_str().unwrap().to_string();
    let (status, v) = node
        .req("POST", &format!("/workflows/{draft}/runs"), Some(json!({})))
        .await;
    assert!(
        (400..500).contains(&status),
        "a draft with problems starts nothing: {status} {v}"
    );

    // A workflow nobody has, by every verb that names one: an id that is
    // no id is a bad request, an id nothing has is not found.
    for (ghost, refused) in [
        ("01J0NOSUCHWORKFLOW00000000", 400),
        ("01J00000000000000000000000", 404),
    ] {
        for (method, path) in [
            ("GET", format!("/workflows/{ghost}")),
            ("DELETE", format!("/workflows/{ghost}")),
            ("POST", format!("/workflows/{ghost}/runs")),
            ("GET", format!("/workflows/{ghost}/runs")),
            ("POST", format!("/workflows/{ghost}/stop")),
            ("POST", format!("/workflows/{ghost}/promote")),
            ("GET", format!("/workflows/{ghost}/retirement")),
        ] {
            let (status, v) = node.req(method, &path, Some(json!({}))).await;
            assert_eq!(status, refused, "{method} {path}: {v}");
        }
        // A save that is well made in every way but the workflow it names.
        let mut save = human_workflow();
        save["revision"] = json!(1);
        let (status, v) = node
            .req("PUT", &format!("/workflows/{ghost}"), Some(save))
            .await;
        assert_eq!(status, refused, "a save of nothing at {ghost}: {v}");
        assert_ne!(
            v["text"]["id"],
            json!("error-node-body-unreadable"),
            "refused for the workflow, not for its body: {v}"
        );
    }

    // Archived: still read, refused for a goal and for a run, and back with one word.
    let wf = node.workflow(human_workflow()).await;
    let (status, v) = node
        .req(
            "POST",
            &format!("/workflows/{wf}/archive"),
            Some(json!({"archived": true})),
        )
        .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(
        node.get(&format!("/workflows/{wf}")).await["workflow"]["id"],
        json!(wf.to_string())
    );
    let goal = node.new_goal("wants an archived workflow").await;
    let (status, v) = node
        .req(
            "PUT",
            &format!("/goals/{goal}/workflow"),
            Some(json!({"workflow": wf.to_string()})),
        )
        .await;
    assert!(
        (400..500).contains(&status),
        "an archived workflow is refused for a goal: {status} {v}"
    );
    let (status, v) = node
        .req("POST", &format!("/workflows/{wf}/runs"), Some(json!({})))
        .await;
    assert!((400..500).contains(&status), "and for a run: {status} {v}");
    let (status, _) = node
        .req(
            "POST",
            &format!("/workflows/{wf}/archive"),
            Some(json!({"archived": true})),
        )
        .await;
    assert_eq!(status, 200, "archived twice is archived once");
    let (status, v) = node
        .req(
            "POST",
            &format!("/workflows/{wf}/archive"),
            Some(json!({"archived": false})),
        )
        .await;
    assert_eq!(status, 200, "{v}");
    let (status, v) = node
        .req(
            "PUT",
            &format!("/goals/{goal}/workflow"),
            Some(json!({"workflow": wf.to_string()})),
        )
        .await;
    assert_eq!(status, 200, "taken back out, it is picked: {v}");
    node.shutdown().await;
}

/// The workflow library over HTTP: validation is pure and names problems by
/// step and kind; a definition that cannot start is kept as a draft with its
/// problems; a save names its revision and a stale one is a conflict; a
/// workflow in use cannot go.
#[tokio::test(flavor = "multi_thread")]
async fn workflows_are_created_validated_edited_and_refused_while_in_use() {
    let node = Node::start().await;

    let broken = json!({
        "name": "Broken",
        "steps": [{
            "id": "a", "name": "A", "kind": "human", "prompt": "?", "then": ["nowhere"]
        }]
    });
    let v = node.post("/workflows/validate", broken.clone()).await;
    let problems = v["problems"].as_array().unwrap();
    assert!(
        problems
            .iter()
            .any(|p| p["kind"] == json!("unknown_step") && p["step"] == json!("a")),
        "{v}"
    );
    let v = node.post("/workflows", broken).await;
    assert!(
        !v["problems"].as_array().unwrap().is_empty(),
        "a draft keeps its problems: {v}"
    );
    let draft = v["workflow"]["id"].as_str().unwrap().to_string();
    let listed = node.get("/workflows").await;
    let mine = listed["workflows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["workflow"]["id"] == json!(draft))
        .expect("the draft is in the library");
    assert!(!mine["problems"].as_array().unwrap().is_empty());
    node.req("DELETE", &format!("/workflows/{draft}"), None)
        .await;

    let wf = node.workflow(human_workflow()).await;
    let v = node.get("/workflows").await;
    let rows = v["workflows"].as_array().unwrap();
    assert!(
        rows.iter()
            .any(|r| r["workflow"]["id"] == json!(wf.to_string())),
        "{v}"
    );
    let mine = rows
        .iter()
        .find(|r| r["workflow"]["id"] == json!(wf.to_string()))
        .unwrap();
    assert!(mine["problems"].as_array().unwrap().is_empty());
    assert!(mine["used_by"].as_array().unwrap().is_empty());

    // Save: the edited definition and the revision it was edited from.
    let stored = node.get(&format!("/workflows/{wf}")).await["workflow"].clone();
    let mut edited = human_workflow();
    edited["name"] = json!("Ask first, renamed");
    edited["revision"] = stored["revision"].clone();
    let (code, v) = node
        .req("PUT", &format!("/workflows/{wf}"), Some(edited.clone()))
        .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["workflow"]["revision"], json!(2));
    assert_eq!(v["workflow"]["name"], json!("Ask first, renamed"));
    assert!(v["problems"].as_array().unwrap().is_empty());
    // A key the definition does not have is refused by a save exactly as by
    // a create: a 400 that names it, and nothing written.
    let mut stray = human_workflow();
    stray["revision"] = json!(2);
    stray["colour"] = json!("red");
    let (code, v) = node
        .req("PUT", &format!("/workflows/{wf}"), Some(stray))
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(v["error"].as_str().unwrap().contains("colour"), "{v}");
    assert_eq!(
        node.get(&format!("/workflows/{wf}")).await["workflow"]["revision"],
        json!(2),
        "a refused save writes nothing"
    );
    // A choice the designer has not made yet — a connector step fresh from
    // the palette — is a problem the save reports, never a body it refuses.
    let mut unfilled = human_workflow();
    unfilled["name"] = json!("Ask first, then call");
    unfilled["revision"] = json!(2);
    unfilled["steps"].as_array_mut().unwrap().push(json!({
        "id": "call", "name": "Call", "kind": "connector", "params": {}
    }));
    unfilled["steps"][0]["then"] = json!(["call"]);
    let (code, v) = node
        .req("PUT", &format!("/workflows/{wf}"), Some(unfilled))
        .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["workflow"]["revision"], json!(3));
    assert!(
        v["problems"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["kind"] == json!("unfilled") && p["step"] == json!("call")),
        "{v}"
    );
    let stored = node.get(&format!("/workflows/{wf}")).await["workflow"].clone();
    assert!(
        stored["steps"][1].get("connector").is_none(),
        "an absent reference is an absent key: {stored}"
    );
    let mut edited = human_workflow();
    edited["name"] = json!("Ask first, renamed");
    edited["revision"] = json!(3);
    let (code, v) = node
        .req("PUT", &format!("/workflows/{wf}"), Some(edited.clone()))
        .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["workflow"]["revision"], json!(4));
    // The same body again names revision 3 — behind the stored 4.
    let (code, v) = node
        .req("PUT", &format!("/workflows/{wf}"), Some(edited))
        .await;
    assert_eq!(
        code, 409,
        "a stale revision is a conflict, not a silent overwrite: {v}"
    );
    let message = v["error"].as_str().unwrap();
    assert!(message.contains("revision 4"), "{message}");
    assert!(message.contains("revision 3"), "{message}");
    assert!(message.contains("reload and merge"), "{message}");
    assert!(
        v.get("problems").is_none(),
        "a conflict carries no problems: {v}"
    );
    assert_eq!(
        node.get(&format!("/workflows/{wf}")).await["workflow"]["revision"],
        json!(4),
        "nothing was written"
    );

    // In use: a goal points at it, so it cannot go.
    let goal = node.new_goal("uses it").await;
    node.req(
        "PUT",
        &format!("/goals/{goal}/workflow"),
        Some(json!({"workflow": wf.to_string()})),
    )
    .await;
    let v = node.get(&format!("/workflows/{wf}")).await;
    assert_eq!(v["used_by"].as_array().unwrap().len(), 1, "{v}");
    let (code, _) = node.req("DELETE", &format!("/workflows/{wf}"), None).await;
    assert_eq!(code, 409);
    node.req("DELETE", &format!("/goals/{goal}"), None).await;
    let (code, _) = node.req("DELETE", &format!("/workflows/{wf}"), None).await;
    assert_eq!(code, 200);
    let (code, _) = node.req("GET", &format!("/workflows/{wf}"), None).await;
    assert_eq!(code, 404);

    node.shutdown().await;
}

// ---------------------------------------------------------------------------
// Channels, messages, DMs
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn channels_messages_and_dms() {
    let node = Node::start().await;

    let v = node
        .post("/channels", json!({"name": "design", "topic": "shapes"}))
        .await;
    let channel = v["channel"]["id"].as_str().unwrap().to_string();
    assert_eq!(v["channel"]["kind"], json!("standing"));

    let (code, _) = node
        .req("POST", "/channels", Some(json!({"name": "  "})))
        .await;
    assert_eq!(code, 400, "a channel needs a name");

    // `general` is seeded on every open; this is the other room.
    let v = node.get("/channels").await;
    assert_eq!(v["channels"].as_array().unwrap().len(), 2);
    assert!(v["channels"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["channel"]["id"] == json!("general")));
    let v = node.get(&format!("/channels/{channel}")).await;
    assert_eq!(v["channel"]["name"], json!("design"));

    for text in ["first", "second", "third"] {
        node.post(
            &format!("/channels/{channel}/messages"),
            json!({"content": text}),
        )
        .await;
    }
    let v = node.get(&format!("/channels/{channel}/messages")).await;
    let msgs = v["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 3);
    assert_eq!(msgs[0]["content"], json!("first"), "oldest first");
    assert_eq!(msgs[2]["content"], json!("third"));

    // Pagination: `limit` keeps the newest window.
    let v = node
        .get(&format!("/channels/{channel}/messages?limit=2"))
        .await;
    let msgs = v["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[1]["content"], json!("third"));
    // `before` excludes everything at or after that second.
    let cutoff = msgs[0]["created_at"].as_u64().unwrap();
    let v = node
        .get(&format!("/channels/{channel}/messages?before={cutoff}"))
        .await;
    assert!(v["messages"]
        .as_array()
        .unwrap()
        .iter()
        .all(|m| m["created_at"].as_u64().unwrap() < cutoff));

    // React, then retract the reaction; then retract a message.
    let target = v["messages"]
        .as_array()
        .unwrap()
        .first()
        .map(|m| m["id"].as_str().unwrap().to_string())
        .unwrap_or_else(|| {
            // `before` may have excluded everything in a fast second.
            msgs[0]["id"].as_str().unwrap().to_string()
        });
    let v = node
        .post(&format!("/messages/{target}/react"), json!({"emoji": "🎯"}))
        .await;
    let reaction = v["id"].as_str().unwrap().to_string();
    // The platform's own agent acknowledges what it reads with 👀; the person's
    // reaction is the one this test made.
    let mine = |v: &serde_json::Value| -> Vec<serde_json::Value> {
        v["reactions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["emoji"] == json!("🎯"))
            .cloned()
            .collect()
    };
    let v = node.get(&format!("/channels/{channel}/messages")).await;
    assert_eq!(mine(&v).len(), 1, "{v}");

    let (code, _) = node
        .req(
            "POST",
            &format!("/messages/{target}/react"),
            Some(json!({"emoji": ""})),
        )
        .await;
    assert_eq!(code, 400, "an empty emoji is not a reaction");

    node.post(&format!("/reactions/{reaction}/retract"), json!({}))
        .await;
    let v = node.get(&format!("/channels/{channel}/messages")).await;
    assert!(
        mine(&v).iter().all(|r| r["retracted"] == json!(true)),
        "{v}"
    );

    node.post(&format!("/messages/{target}/retract"), json!({}))
        .await;
    let v = node.get(&format!("/channels/{channel}/messages")).await;
    assert!(v["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["id"] == json!(target) && m["retracted"] == json!(true)));

    // The list says each room's latest live post — who, the first words, when
    // — and a room nobody wrote in says nothing.
    let newest_live = |v: &serde_json::Value| -> serde_json::Value {
        v["messages"]
            .as_array()
            .unwrap()
            .iter()
            .rfind(|m| m["retracted"] != json!(true))
            .cloned()
            .expect("a live message")
    };
    let live = newest_live(&v);
    let list = node.get("/channels").await;
    let row_of = |list: &serde_json::Value, id: &str| -> serde_json::Value {
        list["channels"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["channel"]["id"] == json!(id))
            .cloned()
            .unwrap_or_else(|| panic!("the room {id}: {list}"))
    };
    let row = row_of(&list, &channel);
    assert_eq!(row["latest"]["snippet"], live["content"], "{list}");
    assert_eq!(row["latest"]["author"], live["author"]);
    assert_eq!(row["latest"]["at"], live["created_at"]);
    assert!(
        row_of(&list, "general")["latest"].is_null(),
        "nothing said in general yet: {list}"
    );
    // A retracted last post gives way to the one before it.
    let fourth = node
        .post(
            &format!("/channels/{channel}/messages"),
            json!({"content": "fourth"}),
        )
        .await;
    let fourth_id = fourth["id"].as_str().expect("the post's id").to_string();
    assert_eq!(
        row_of(&node.get("/channels").await, &channel)["latest"]["snippet"],
        json!("fourth")
    );
    node.post(&format!("/messages/{fourth_id}/retract"), json!({}))
        .await;
    let v = node.get(&format!("/channels/{channel}/messages")).await;
    let live = newest_live(&v);
    assert_ne!(live["content"], json!("fourth"));
    assert_eq!(
        row_of(&node.get("/channels").await, &channel)["latest"]["snippet"],
        live["content"],
        "retracted, the last post is nobody's last words"
    );

    // DMs are idempotent on the participant set: same people, same room. A
    // direct channel is between people of this workspace, so the other party
    // is admitted first — a stranger's key opens nothing.
    let other = nostr::key::Keys::generate().public_key().to_hex();
    node.post(
        "/workspace/people",
        json!({"pubkey": other, "label": "bob", "role": "member"}),
    )
    .await;
    let v = node.post("/dms", json!({"members": [other]})).await;
    let dm = v["channel"]["id"].as_str().unwrap().to_string();
    assert_eq!(v["channel"]["kind"], json!("direct"));
    assert_eq!(
        v["channel"]["audience"]["principals"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or_else(|| v["channel"]["audience"]
                .as_array()
                .map(|a| a.len())
                .unwrap_or(0)),
        2
    );
    let v = node.post("/dms", json!({"members": [other]})).await;
    assert_eq!(
        v["channel"]["id"],
        json!(dm),
        "same set = same conversation"
    );

    let v = node.get("/dms").await;
    assert_eq!(v["dms"].as_array().unwrap().len(), 1);
    assert!(
        v["dms"][0]["latest"].is_null(),
        "nothing said in a room just opened: {v}"
    );
    let (code, _) = node.req("POST", "/dms", Some(json!({"members": []}))).await;
    assert_eq!(code, 400);

    // A DM is not a channel. `GET /channels` filtered nothing once, so opening a
    // DM grew the sidebar, the channels index, a message event's conversation
    // picker and the command palette by one phantom room each.
    let v = node.get("/channels").await;
    let rows = v["channels"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "opening a DM must not add a channel");
    assert!(rows.iter().any(|r| r["channel"]["id"] == json!(channel)));

    // `general` is permanent: deleting it is a conflict, and it is still
    // there afterwards. The room this test made can go.
    let (code, v) = node.req("DELETE", "/channels/general", None).await;
    assert_eq!(code, 409, "{v}");
    assert_eq!(
        node.get("/channels/general").await["channel"]["id"],
        json!("general")
    );
    let (code, v) = node
        .req("DELETE", &format!("/channels/{channel}"), None)
        .await;
    assert_eq!(code, 200, "{v}");
    let (code, v) = node.req("GET", &format!("/channels/{channel}"), None).await;
    assert_eq!(code, 404, "the deleted channel is gone: {v}");

    node.shutdown().await;
}

// ---------------------------------------------------------------------------
// Inbox: conversations, not events
// ---------------------------------------------------------------------------

/// A Workflow Agent proposal must read the same in the Inbox as on the goal
/// page: both surfaces carry the identical `NeedsAction`
/// for the `adopt:` gate — the full `ProposalView` with every step — so the
/// desktop mounts the same card. The Inbox once built its actions without the
/// proposal attachment, so it showed a bare text ask; this locks the parity.
#[tokio::test(flavor = "multi_thread")]
async fn a_proposal_reads_the_same_in_the_inbox_and_on_the_goal() {
    let node = Node::start().await;
    // Guided: an auto goal adopts a proposal alone and opens no gate.
    let v = node
        .post(
            "/goals",
            json!({"statement": "sell more socks", "title": "T", "mode": "guided"}),
        )
        .await;
    let id = serde_json::from_value::<Goal>(v["goal"].clone())
        .expect("goal")
        .id;

    // The Workflow Agent proposes a design for the goal.
    let reply = node
        .intake_op(json!({
            "op": "propose_workflow",
            "agent": AgentId::WORKFLOW,
            "goal": id.to_string(),
            "workflow": agent_workflow(),
        }))
        .await;
    assert_eq!(reply["ok"], json!(true), "propose failed: {reply}");

    // The goal page's proposal.
    let goal = node.get(&format!("/goals/{id}")).await;
    let on_goal = &goal["guidance"]["open_questions"][0];
    assert!(
        on_goal["subject"]
            .as_str()
            .unwrap_or("")
            .starts_with("adopt:"),
        "the goal page has no adopt gate: {goal}"
    );
    let goal_steps = on_goal["proposal"]["steps"]
        .as_array()
        .expect("the goal page carries the steps");
    assert!(
        !goal_steps.is_empty(),
        "the goal page's proposal has no steps"
    );

    // The Inbox's proposal — the same goal row.
    let inbox = node.get("/inbox").await;
    let row = inbox["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == json!(id.to_string()))
        .unwrap_or_else(|| panic!("no inbox row for the goal: {inbox}"));
    let in_inbox = &row["needs_action"][0];
    assert!(
        in_inbox["subject"]
            .as_str()
            .unwrap_or("")
            .starts_with("adopt:"),
        "the inbox row has no adopt gate: {row}"
    );
    assert_eq!(
        in_inbox["proposal"], on_goal["proposal"],
        "the inbox and the goal page must carry the identical proposal — every step, the same card",
    );

    node.shutdown().await;
}

/// A gate over the wire is decided once and only as its own goal's: the
/// second word is a conflict, another goal's gate a bad request, a gate
/// nobody has not found — and the goal that was named wrongly is untouched.
#[tokio::test(flavor = "multi_thread")]
async fn a_gate_over_the_wire_is_decided_once_and_only_as_its_own_goals() {
    let node = Node::start().await;
    let mine = node.new_goal("mine").await;
    let theirs = node.new_goal("theirs").await;
    let my_gate = node.ask(mine).await;
    let their_gate = node.ask(theirs).await;
    let decide = |goal: GoalId, gate: String, option: &'static str| {
        let node = &node;
        async move {
            node.req(
                "POST",
                &format!("/goals/{goal}/decide"),
                Some(json!({"approve": true, "gate": gate, "answer": {"selected": [option]}})),
            )
            .await
        }
    };

    let (status, v) = decide(mine, their_gate.clone(), "sqlite").await;
    assert_eq!(status, 400, "another goal's gate: {v}");
    let open = |v: &Value| v["pending_gates"].as_array().map(Vec::len).unwrap_or(0);
    assert_eq!(
        open(&node.get(&format!("/goals/{mine}")).await),
        1,
        "mine still asks"
    );
    assert_eq!(
        open(&node.get(&format!("/goals/{theirs}")).await),
        1,
        "theirs still asks"
    );

    let (status, v) = decide(mine, my_gate.clone(), "no-such-option").await;
    assert!(
        (400..500).contains(&status),
        "an option nobody offered: {status} {v}"
    );
    assert_eq!(
        open(&node.get(&format!("/goals/{mine}")).await),
        1,
        "a refused answer leaves the gate open"
    );

    let (status, v) = decide(mine, my_gate.clone(), "sqlite").await;
    assert_eq!(status, 200, "{v}");
    let (status, v) = decide(mine, my_gate.clone(), "postgres").await;
    assert_eq!(status, 409, "decided already: {v}");
    assert_eq!(open(&node.get(&format!("/goals/{theirs}")).await), 1);
    node.shutdown().await;
}

/// A capture is refused before anything is written, in words; what is taken
/// is trimmed, and a title of spaces is no title.
#[tokio::test(flavor = "multi_thread")]
async fn a_capture_is_refused_in_words_and_what_is_taken_is_trimmed() {
    let node = Node::start().await;
    let goals = |v: &Value| v["goals"].as_array().map(Vec::len).unwrap_or(0);
    let before = goals(&node.get("/goals").await);

    for (body, why) in [
        (json!({"statement": ""}), "an empty statement"),
        (json!({"statement": "  \n\t "}), "a statement of spaces"),
        (json!({"title": "no statement at all"}), "no statement"),
        (
            json!({"statement": "fine", "mode": "autopilot"}),
            "a mode nobody has",
        ),
        (
            json!({"statement": "fine", "workflow": "no-such-workflow"}),
            "a workflow nobody has",
        ),
        (json!({"statement": 7}), "a statement that is not text"),
    ] {
        let (status, v) = node.req("POST", "/goals", Some(body)).await;
        assert!((400..500).contains(&status), "{why}: {status} {v}");
        assert!(
            v["error"].as_str().is_some_and(|e| !e.is_empty()),
            "{why} is refused in words: {v}"
        );
    }
    assert_eq!(
        goals(&node.get("/goals").await),
        before,
        "a refusal writes nothing"
    );

    let v = node
        .post(
            "/goals",
            json!({"statement": "  ship the release \n", "title": "   ", "mode": "manual"}),
        )
        .await;
    assert_eq!(v["goal"]["statement"], json!("ship the release"));
    assert!(
        v["goal"]["title"].is_null(),
        "a title of spaces is no title: {v}"
    );
    assert_eq!(v["goal"]["mode"], json!("manual"));

    // A long statement is a statement: kept whole.
    let long = "word ".repeat(4000);
    let v = node
        .post("/goals", json!({"statement": long, "mode": "manual"}))
        .await;
    assert_eq!(
        v["goal"]["statement"].as_str().unwrap().len(),
        long.trim().len()
    );
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn inbox_rows_are_conversations() {
    let node = Node::start().await;
    // Manual: no Workflow Agent speaks at capture, so the thread starts empty.
    let v = node
        .post(
            "/goals",
            json!({"statement": "build the thing", "title": "T", "mode": "manual"}),
        )
        .await;
    let id = serde_json::from_value::<Goal>(v["goal"].clone())
        .expect("goal")
        .id;

    // An agent gives us a second author — the owner's own messages are never
    // "unread", so a single-author workspace has no representative to pick.
    let v = node
        .post(
            "/agents",
            json!({"name": "Scout", "system_prompt": "research", "harness": "claude-code"}),
        )
        .await;
    let agent = v["agent"]["id"].as_str().unwrap().to_string();

    let agent_id = AgentId::new(&agent).unwrap();
    for text in ["oldest unread", "middle", "newest"] {
        node.ws
            .post_message(
                &id.to_string(),
                MessageBody::post(text),
                None,
                &[],
                &[],
                Some(&agent_id),
                PostOrigin::Asked,
            )
            .expect("agent message");
    }

    let v = node.get("/inbox").await;
    let row = v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == json!(id.to_string()))
        .expect("goal conversation row");
    assert_eq!(row["kind"], json!("goal"));
    assert_eq!(row["unread_count"], json!(3));
    // The row's face is the *oldest unread*, so opening it lands you at the
    // first thing you have not seen — not at the bottom.
    assert_eq!(
        row["representative"]["snippet"],
        json!("oldest unread"),
        "representative must be the oldest unread message"
    );
    assert_eq!(row["status"], json!("draft"));

    // A waiting step floats a conversation up and carries its question
    // inline — with which run and step are waiting, not just the fact that
    // something is.
    let gate = node.ask(id).await;
    // A second goal that owes nothing: the bucket has something to leave out.
    let quiet = node.new_goal("nothing is asked here").await;
    let all = node.get("/inbox").await;
    assert!(
        all["rows"].as_array().unwrap().len() >= 2,
        "both goals have a row: {all}"
    );
    let v = node.get("/inbox?filter=needs_you").await;
    let rows = v["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "only the waiting conversation: {v}");
    assert_ne!(rows[0]["key"], json!(format!("goal:{quiet}")), "{v}");
    // A word that is no bucket, or no source, is refused — never every row.
    for wrong in ["/inbox?filter=needs_action", "/inbox?source=goal"] {
        let (status, body) = node.req("GET", wrong, None).await;
        assert_eq!(status, 400, "{wrong}: {body}");
    }
    let from_goals = node.get("/inbox?filter=needs_you&source=goals").await;
    assert_eq!(from_goals["rows"].as_array().unwrap().len(), 1);
    let from_people = node.get("/inbox?filter=needs_you&source=people").await;
    assert!(
        from_people["rows"].as_array().unwrap().is_empty(),
        "{from_people}"
    );
    let action = &rows[0]["needs_action"][0];
    assert_eq!(action["gate_id"], json!(gate));
    assert_eq!(action["expects"]["kind"], json!("answer"));
    assert_eq!(action["expects"]["options"].as_array().unwrap().len(), 2);
    assert_eq!(action["step"], json!("ask"));
    assert!(action["run"].is_string(), "{v}");
    assert!(
        action["subject"].as_str().is_some_and(|s| !s.is_empty()),
        "the row must say what is being asked: {v}"
    );
    assert!(
        action["opened_at"].as_u64().is_some(),
        "a gate carries the moment it opened: {v}"
    );

    // The messages source is channels and direct channels only — a goal
    // thread comes from the goals; every source at once is the default.
    let v = node.get("/inbox?source=messages").await;
    assert!(v["rows"].as_array().unwrap().is_empty(), "{v}");
    let v = node.get("/inbox?source=goals").await;
    assert_eq!(v["rows"].as_array().unwrap().len(), 2, "both goals: {v}");
    assert_eq!(
        v["rows"][0]["key"],
        json!(id.to_string()),
        "what is owed is first: {v}"
    );
    assert_eq!(v["rows"][0]["kind"], json!("goal"));
    assert_eq!(
        v["rows"][0]["unread_notices"],
        json!(0),
        "nothing happened to it yet: {v}"
    );
    assert_eq!(v["rows"][0]["notices"], json!([]));

    // `mentioned` is a real `p`-tag index, not "anything unread": three
    // unread agent messages that never addressed us do not qualify.
    let v = node.get("/inbox").await;
    let row = v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == json!(id.to_string()))
        .expect("the goal's row");
    assert_eq!(
        row["mentioned"],
        json!(false),
        "unread alone is not a mention: {v}"
    );
    let owner = node.ws.owner_principal();
    node.ws
        .post_message(
            &id.to_string(),
            MessageBody::post("@owner your call"),
            None,
            &[owner],
            &[],
            Some(&agent_id),
            PostOrigin::Asked,
        )
        .expect("mentioning message");
    let v = node.get("/inbox").await;
    let row = v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == json!(id.to_string()))
        .expect("the goal's row");
    assert_eq!(
        row["mentioned"],
        json!(true),
        "the conversation that addressed us: {v}"
    );

    // Reading changes the row; it does not remove it.
    node.post("/read", json!({"scope": id.to_string()})).await;
    let v = node.get("/inbox").await;
    let row = v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == json!(id.to_string()))
        .expect("row survives being read");
    assert_eq!(row["unread_count"], json!(0));
    assert_eq!(row["read"], json!(true), "read is a state, not a deletion");

    // Said twice, it is said once: the same answer, the same row.
    let (status, again) = node
        .req("POST", "/read", Some(json!({"scope": id.to_string()})))
        .await;
    assert_eq!(status, 200, "{again}");
    let v = node.get("/inbox").await;
    let twice = v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == json!(id.to_string()))
        .expect("the row is still there");
    assert_eq!(twice["read"], json!(true));
    assert_eq!(twice["unread_count"], json!(0));
    assert!(
        node.get("/inbox?filter=unread").await["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["key"] != json!(id.to_string())),
        "a read row is not in the unread bucket"
    );

    // Forced unread brings it back on its own.
    node.post("/unread", json!({"scope": id.to_string()})).await;
    let v = node.get("/inbox").await;
    let row = v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == json!(id.to_string()))
        .unwrap();
    assert!(row["unread_count"].as_u64().unwrap() >= 1);
    assert_eq!(
        row["read"],
        json!(false),
        "putting a row back deliberately outranks the watermark"
    );

    node.shutdown().await;
}

/// The row set is derived from what has *ever* needed a human, so deciding
/// the gate leaves the conversation on the screen — changed, not gone. This
/// is the behaviour the old `if actions.is_empty() && n_unread == 0` deleted,
/// and the reason the selection used to jump to somebody else's row.
#[tokio::test(flavor = "multi_thread")]
async fn a_row_survives_its_gate_being_decided_and_says_what_was_decided() {
    let node = Node::start().await;
    let id = node.new_goal("ship the thing").await;
    node.ask(id).await;

    let row_for = |v: &Value| -> Option<Value> {
        v["rows"]
            .as_array()?
            .iter()
            .find(|r| r["key"] == json!(id.to_string()))
            .cloned()
    };

    let v = node.get("/inbox").await;
    let row = row_for(&v).expect("the gated conversation");
    assert_eq!(row["handled"], json!(false));
    assert_eq!(row["needs_action"].as_array().unwrap().len(), 1);

    node.post(
        &format!("/goals/{id}/decide"),
        json!({"approve": true, "answer": {"selected": ["postgres"]}}),
    )
    .await;

    let v = node.get("/inbox").await;
    let row = row_for(&v).expect("the row survives the decision");
    assert!(
        row["needs_action"].as_array().unwrap().is_empty(),
        "nothing is waiting any more: {v}"
    );
    assert_eq!(
        row["handled"],
        json!(true),
        "a decided gate is a workspace fact, not a personal one: {v}"
    );
    assert_eq!(row["decided"]["gate_kind"], json!("escalation"));
    assert_eq!(row["decided"]["approve"], json!(true));

    node.shutdown().await;
}

/// A gate's clock is the moment it opened. Rendering it as `now` on every
/// request made a gate waiting nine days read as brand new forever — hiding
/// exactly the staleness the inbox exists to surface.
#[tokio::test(flavor = "multi_thread")]
async fn a_gates_row_is_dated_when_the_gate_opened() {
    let node = Node::start().await;
    // Manual: no Workflow Agent note at capture to count as unread.
    let v = node
        .post(
            "/goals",
            json!({"statement": "date me honestly", "title": "T", "mode": "manual"}),
        )
        .await;
    let id = serde_json::from_value::<Goal>(v["goal"].clone())
        .expect("goal")
        .id;
    node.ask(id).await;

    let v = node.get("/inbox").await;
    let row = v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == json!(id.to_string()))
        .expect("the gated conversation")
        .clone();
    let opened_at = row["needs_action"][0]["opened_at"]
        .as_u64()
        .expect("gate opened_at");
    assert_eq!(
        row["latest_at"].as_u64().unwrap(),
        opened_at,
        "an unspoken-in conversation is as old as its gate: {row}"
    );

    // And a gate nobody has looked at is unread, though it has no messages at
    // all — which `unread_count` alone can never say.
    assert_eq!(row["unread_count"], json!(0));
    assert_eq!(row["read"], json!(false));

    node.shutdown().await;
}

/// The `inbox` SSE channel is what lets a row change in place instead of the client
/// refetching the whole list and replacing the array under a selection
/// somebody is using, so it is worth proving it actually arrives.
#[tokio::test(flavor = "multi_thread")]
async fn a_timeline_pages_without_losing_a_second_refuses_a_novel_and_forgets_a_deleted_channel() {
    let node = Node::start().await;
    let made = node
        .post(
            "/channels",
            json!({"name": "burst", "agents": [], "teams": [], "humans": []}),
        )
        .await;
    let id = made["channel"]["id"]
        .as_str()
        .expect("a channel: {made}")
        .to_string();
    let path = format!("/channels/{id}/messages");
    for n in 0..9 {
        let (code, v) = node
            .req("POST", &path, Some(json!({"content": format!("m{n}")})))
            .await;
        assert_eq!(code, 200, "{v}");
    }
    // Pages of four, each continuing from the oldest row shown, by id.
    let mut seen: Vec<String> = Vec::new();
    let mut cursor: Option<(u64, String)> = None;
    loop {
        let query = match &cursor {
            Some((at, id)) => format!("?limit=4&before={at}&before_id={id}"),
            None => "?limit=4".to_string(),
        };
        let (code, page) = node.req("GET", &format!("{path}{query}"), None).await;
        assert_eq!(code, 200, "{page}");
        let rows = page["messages"].as_array().unwrap();
        if rows.is_empty() {
            break;
        }
        for m in rows.iter().rev() {
            let text = m["content"].as_str().unwrap().to_string();
            assert!(!seen.contains(&text), "{text} read twice");
            seen.push(text);
        }
        let oldest = &rows[0];
        cursor = Some((
            oldest["created_at"].as_u64().unwrap(),
            oldest["id"].as_str().unwrap().to_string(),
        ));
    }
    let expected: Vec<String> = (0..9).rev().map(|n| format!("m{n}")).collect();
    assert_eq!(
        seen, expected,
        "every message exactly once, however the seconds fell"
    );
    // `limit` is clamped to [1, 200]: zero is one, a thousand is two hundred.
    let (_, one) = node.req("GET", &format!("{path}?limit=0"), None).await;
    assert_eq!(one["messages"].as_array().unwrap().len(), 1);
    let (code, _) = node.req("GET", &format!("{path}?limit=1000"), None).await;
    assert_eq!(code, 200);

    // A novel is refused in words, with the way to send it, never a bare 413.
    let novel = "x".repeat(bisa_core::MAX_TEXT_BYTES + 1);
    let (code, v) = node
        .req("POST", &path, Some(json!({"content": novel})))
        .await;
    assert_eq!(
        code,
        400,
        "{}",
        v.to_string().chars().take(200).collect::<String>()
    );
    assert!(v["error"].as_str().unwrap().contains("attachment"), "{v}");
    let (code, v) = node
        .req("POST", &path, Some(json!({"content": "   \n"})))
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(v["error"].as_str().unwrap().contains("empty"), "{v}");

    // Deleted, the channel's id answers nothing — not its history.
    let (code, v) = node.req("DELETE", &format!("/channels/{id}"), None).await;
    assert_eq!(code, 200, "{v}");
    let (code, v) = node.req("GET", &path, None).await;
    assert_eq!(code, 404, "{v}");
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_inbox_channel_reports_one_row_changing() {
    let node = Node::start().await;
    let id = node.new_goal("tell me when this changes").await;

    let stream = UnixStream::connect(&node.socket).await.expect("connect");
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .expect("handshake");
    tokio::spawn(conn);
    let resp = sender
        .send_request(
            Request::builder()
                .method("GET")
                .uri("/events")
                .header(hyper::header::HOST, "localhost")
                .header(hyper::header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(Full::new(Bytes::new()))
                .unwrap(),
        )
        .await
        .expect("events");
    assert_eq!(resp.status().as_u16(), 200);
    let mut body = resp.into_body();

    // A step waiting on you changes what this conversation owes you.
    node.ask(id).await;

    let payload = tokio::time::timeout(Duration::from_secs(10), async {
        let mut buf = String::new();
        loop {
            let frame = body
                .frame()
                .await
                .expect("event stream ended")
                .expect("frame");
            let Some(chunk) = frame.data_ref() else {
                continue;
            };
            buf.push_str(&String::from_utf8_lossy(chunk));
            for line in buf.lines() {
                let Some(rest) = line.strip_prefix("data: ") else {
                    continue;
                };
                let Ok(v) = serde_json::from_str::<Value>(rest) else {
                    continue;
                };
                if v["stream"] == json!("inbox") {
                    return v["payload"].clone();
                }
            }
        }
    })
    .await
    .expect("no inbox frame arrived");

    assert_eq!(payload["key"], json!(id.to_string()));
    assert_eq!(payload["needs_action_count"], json!(1));
    assert_eq!(payload["handled"], json!(false));
    assert_eq!(
        payload["read"],
        json!(false),
        "a gate you have not seen is unread even with no messages: {payload}"
    );

    // The stream is open-ended by design, so it has to be let go before the
    // server is asked to stop — otherwise the shutdown waits on a connection
    // that is politely waiting for more events.
    drop(body);
    drop(sender);
    node.shutdown().await;
}

// ---------------------------------------------------------------------------
// Pulse
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn pulse_merges_spine_and_conversation() {
    let node = Node::start().await;
    let id = node.new_goal("ship pulse").await;
    // Starting the run is the first spine fact; answering its step the next.
    node.ask(id).await;
    let run = node.current_run(id).await;
    node.post(
        &format!("/runs/{run}/steps/ask/answer"),
        json!({"answer": {"selected": ["sqlite"]}}),
    )
    .await;
    let v = node.post("/channels", json!({"name": "watercooler"})).await;
    let channel = v["channel"]["id"].as_str().unwrap().to_string();
    node.post(
        &format!("/channels/{channel}/messages"),
        json!({"content": "hello everyone"}),
    )
    .await;

    let v = node.get("/pulse?limit=50").await;
    let rows = v["rows"].as_array().unwrap();
    assert!(!rows.is_empty(), "pulse should show recent activity");

    // A row carries the event, not a sentence about it. That is what lets the
    // desktop render a stored row with the same function it renders a live
    // one — and what a `text` field would take away again.
    let spine = rows
        .iter()
        .find(|r| r["event"]["type"] == json!("run"))
        .unwrap_or_else(|| panic!("run facts are journal rows: {v}"));
    assert_eq!(spine["concept"], json!("goals"));
    assert_eq!(spine["kind"], json!("run"));
    assert_eq!(spine["source"]["kind"], json!("goal"));
    assert_eq!(spine["source"]["id"], json!(id.to_string()));
    assert_eq!(
        spine["title"],
        json!("T"),
        "the title is a field of its own"
    );
    assert!(
        spine["author"].is_string(),
        "the signer, for the client to name"
    );
    assert!(spine["seq"].is_i64(), "a row knows its place in the feed");
    assert!(
        spine.get("text").is_none() && spine.get("cls").is_none(),
        "no prose and no tone on the wire: {spine}"
    );

    let conversation = rows
        .iter()
        .find(|r| r["event"]["type"] == json!("message"))
        .unwrap_or_else(|| panic!("channel chatter appears as a message row: {v}"));
    assert_eq!(conversation["concept"], json!("channels"));
    assert_eq!(conversation["source"]["kind"], json!("channel"));
    assert_eq!(conversation["title"], json!("watercooler"));
    assert_eq!(conversation["event"]["snippet"], json!("hello everyone"));
    assert_eq!(conversation["event"]["body_kind"], json!("post"));

    // The engine's own facts are rows too: a goal made is the workspace's
    // shape changing, and it reads as the frame did.
    let made = rows
        .iter()
        .find(|r| r["event"]["type"] == json!("goal_created"))
        .unwrap_or_else(|| panic!("a goal made is a workspace row: {v}"));
    assert_eq!(made["concept"], json!("workspace"));
    assert_eq!(made["event"]["goal"], json!(id.to_string()));
    assert_eq!(made["title"], json!("T"));

    // Newest first, by (at, seq).
    let keys: Vec<(u64, i64)> = rows
        .iter()
        .map(|r| (r["at"].as_u64().unwrap(), r["seq"].as_i64().unwrap()))
        .collect();
    assert!(
        keys.windows(2).all(|w| w[0] > w[1]),
        "pulse is newest-first, no two rows alike: {keys:?}"
    );

    node.shutdown().await;
}

/// The feed pages by keyset and narrows by concept: a page edge inside one
/// second loses nothing, `next` says where to go on and is absent on the
/// last page, a concept answers only its rows, and a word outside the seven
/// is refused.
#[tokio::test(flavor = "multi_thread")]
async fn the_pulse_pages_by_keyset_and_filters_by_concept() {
    let node = Node::start().await;
    let id = node.new_goal("page me").await;
    let owner = node.ws.owner_keys().clone();
    // Twelve notes in a burst: several share a second.
    for i in 0..12 {
        node.ws
            .append_journal(
                &bisa_core::Home::from(id),
                JournalPayload::Note {
                    text: format!("n{i}"),
                },
                &owner,
                None,
            )
            .expect("append");
    }
    let mut seen: Vec<String> = Vec::new();
    let mut next: Option<Value> = None;
    let mut pages = 0;
    loop {
        let path = match &next {
            Some(c) => format!(
                "/pulse?concept=goals&limit=5&before={}&before_seq={}",
                c["at"], c["seq"]
            ),
            None => "/pulse?concept=goals&limit=5".to_string(),
        };
        let v = node.get(&path).await;
        let rows = v["rows"].as_array().unwrap();
        pages += 1;
        assert!(rows.iter().all(|r| r["concept"] == json!("goals")), "{v}");
        seen.extend(
            rows.iter()
                .filter(|r| r["event"]["type"] == json!("note"))
                .map(|r| r["event"]["text"].as_str().unwrap().to_string()),
        );
        match v.get("next") {
            Some(c) if !c.is_null() => {
                assert_eq!(rows.len(), 5, "a page with a next is full");
                next = Some(c.clone());
            }
            _ => break,
        }
        assert!(pages < 10, "the pages end");
    }
    seen.sort();
    // The capture wrote the goal's first note, and the feed carries it as it
    // carries the twelve.
    let mut expected: Vec<String> = (0..12).map(|i| format!("n{i}")).collect();
    expected.push("goal captured: page me".to_string());
    expected.sort();
    assert_eq!(seen, expected, "every note once, across the page edges");

    let (status, v) = node
        .req("GET", "/pulse?concept=projects&limit=50", None)
        .await;
    assert_eq!(status, 200, "{v}");
    assert!(
        v["rows"].as_array().unwrap().is_empty(),
        "no project was made: {v}"
    );
    let (status, v) = node.req("GET", "/pulse?concept=everything", None).await;
    assert_eq!(status, 400, "{v}");
    let (status, v) = node.req("GET", "/pulse?before_seq=3", None).await;
    assert_eq!(status, 400, "a seq without its second is not a cursor: {v}");

    node.shutdown().await;
}

/// The feed's edges: a limit outside its bounds, a cursor with no `seq`, the
/// page after the last, a row this build cannot type in the middle of a page,
/// and a row whose source is gone.
#[tokio::test(flavor = "multi_thread")]
async fn the_pulse_holds_at_its_edges() {
    use bisa_core::activity::{ActivityConcept, ActivityFact, ActivitySource, ActivitySourceKind};

    let node = Node::start().await;
    // What a listener of a library workflow did is filed under workflows;
    // nothing here makes a workflow, so the concept starts empty.
    let fact = |at: u64, kind: &str, source: ActivitySource, event: Value| ActivityFact {
        at,
        concept: ActivityConcept::Workflows,
        kind: kind.into(),
        source,
        author: None,
        event,
    };
    let gone = ActivitySource::new(ActivitySourceKind::Workflow, "01J0NOSUCHWORKFLOW00000000");

    // An empty concept: no rows, no cursor.
    let v = node.get("/pulse?concept=workflows").await;
    assert_eq!(v["rows"], json!([]), "{v}");
    assert!(v.get("next").is_none_or(Value::is_null), "{v}");

    // Three facts in one second, the middle one a `message` that is no message.
    for (kind, event) in [
        ("listener_fired", json!({"type": "listener_fired", "n": 1})),
        ("message", json!({"type": "message", "not": "a message"})),
        ("listener_fired", json!({"type": "listener_fired", "n": 3})),
    ] {
        node.ws
            .record_activity(&fact(1_700_000_000, kind, gone.clone(), event), true)
            .expect("record");
    }

    // The untyped row is left out, never an error — and the page is paged past it.
    let v = node.get("/pulse?concept=workflows&limit=50").await;
    let rows = v["rows"].as_array().unwrap();
    assert_eq!(
        rows.len(),
        2,
        "the row that cannot be typed is skipped: {v}"
    );
    assert!(
        rows.iter().all(|r| r["kind"] == json!("listener_fired")),
        "{v}"
    );
    // A source that no longer resolves is a row without a title, not a failure.
    assert!(rows.iter().all(|r| r["title"].is_null()), "{v}");
    assert_eq!(rows[0]["source"]["id"], json!(gone.id), "{v}");

    // A limit of nothing is one row; the cursor it hands back walks the rest,
    // the skipped row included, and ends.
    let v = node.get("/pulse?concept=workflows&limit=0").await;
    assert_eq!(v["rows"].as_array().unwrap().len(), 1, "{v}");
    let mut seen = vec![v["rows"][0]["seq"].as_i64().unwrap()];
    let mut next = v["next"].clone();
    let mut pages = 1;
    while !next.is_null() {
        let v = node
            .get(&format!(
                "/pulse?concept=workflows&limit=1&before={}&before_seq={}",
                next["at"], next["seq"]
            ))
            .await;
        seen.extend(
            v["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["seq"].as_i64().unwrap()),
        );
        next = v.get("next").cloned().unwrap_or(Value::Null);
        pages += 1;
        assert!(pages < 8, "the pages end");
    }
    assert_eq!(
        seen.len(),
        2,
        "each typed row once, within one second: {seen:?}"
    );
    assert!(seen[0] > seen[1], "newest first");

    // A second with no `seq` is everything before that second's end; before
    // the first fact there is nothing, and that is not an error.
    let v = node.get("/pulse?concept=workflows&before=1700000000").await;
    assert_eq!(v["rows"].as_array().unwrap().len(), 2, "{v}");
    let v = node.get("/pulse?concept=workflows&before=1699999999").await;
    assert_eq!(v["rows"], json!([]), "{v}");
    assert!(v.get("next").is_none_or(Value::is_null), "{v}");

    // A limit past the cap is the cap.
    for i in 0..205u64 {
        node.ws
            .record_activity(
                &fact(
                    1_700_000_100 + i,
                    "listener_fired",
                    gone.clone(),
                    json!({"type": "listener_fired", "n": i}),
                ),
                false,
            )
            .expect("record");
    }
    let v = node.get("/pulse?concept=workflows&limit=100000").await;
    assert_eq!(v["rows"].as_array().unwrap().len(), 200, "the cap");
    assert!(v["next"].is_object(), "a full page has a next");

    node.shutdown().await;
}

/// The wire tag a payload serializes to.
///
/// Exhaustive on purpose, and the reason this file is where it lives: adding
/// a variant to `JournalPayload` stops this match compiling, which is the
/// prompt to add it to `one_of_each()` below and so to the Pulse's coverage.
/// The alternative — a fixture list nobody has to update — is exactly how the
/// desktop's engine activity lost six payloads for a milestone.
fn payload_tag(p: &JournalPayload) -> &'static str {
    match p {
        JournalPayload::Note { .. } => "note",
        JournalPayload::Guidance { .. } => "guidance",
        JournalPayload::Guard { .. } => "guard",
        JournalPayload::Judgement { .. } => "judgement",
        JournalPayload::Step { .. } => "step",
        JournalPayload::Run { .. } => "run",
        JournalPayload::Decision { .. } => "decision",
        JournalPayload::Question { .. } => "question",
        JournalPayload::Withdrawn { .. } => "withdrawn",
        JournalPayload::Claim { .. } => "claim",
        JournalPayload::Progress { .. } => "progress",
        JournalPayload::Result { .. } => "result",
        JournalPayload::Attachment { .. } => "attachment",
        JournalPayload::Document { .. } => "document",
        JournalPayload::Signal { .. } => "signal",
        JournalPayload::TurnMetrics { .. } => "turn_metrics",
    }
}

/// One of every journal payload. Keep in step with [`payload_tag`].
fn one_of_each() -> Vec<JournalPayload> {
    let work_item = WorkItemId::from_ulid(ulid::Ulid::from_parts(1, 1));
    let session = SessionId::from_ulid(ulid::Ulid::from_parts(2, 2));
    let run = RunId::from_ulid(ulid::Ulid::from_parts(4, 4));
    vec![
        JournalPayload::Note {
            text: "the total is wrong".into(),
        },
        JournalPayload::Guard {
            tool: "Bash".into(),
            subject:
                "curl https://api.example.test -H 'Authorization: Bearer «secret:bearer:0a1b2c»'"
                    .into(),
            verdict: bisa_core::event::GuardVerdict::Asked,
            by: bisa_core::event::GuardJudge::Classifier,
            rule: Some("network_and_remote".into()),
            reason: Some("it sends a credential to a remote host".into()),
        },
        JournalPayload::Guidance {
            phase: bisa_core::GuidancePhase::Design,
            status: bisa_core::GuidanceStatus::Working,
            detail: Some("on claude-code".into()),
            session: Some(session),
        },
        JournalPayload::Step {
            run,
            step: StepId::new("build").unwrap(),
            event: StepFact::Done { branches: vec![] },
        },
        JournalPayload::Run {
            run,
            event: RunFact::Finished {
                outcome: RunOutcome::Done,
            },
        },
        JournalPayload::Decision {
            gate: Gate::Approval,
            approve: true,
            subject: "approval:01J0/ship".into(),
            rationale: Some("scope looks right".into()),
            answer: Some(bisa_core::Answer::text("yes, ship it")),
        },
        JournalPayload::Question {
            work_item: Some(work_item),
            gate: "gate-7".into(),
            text: "which currency?".into(),
            expects: AskKind::one_of(vec![bisa_core::AskOption::new("usd", "US dollars")]),
        },
        JournalPayload::Withdrawn {
            subject: "workstream:01J0".into(),
            reason: "interrupted by a restart".into(),
        },
        JournalPayload::Claim {
            work_item,
            harness: "claude-code".into(),
            session,
        },
        JournalPayload::Progress {
            work_item,
            verb: "edited".into(),
            object: "src/total.rs".into(),
            outcome: Some("tests pass".into()),
        },
        JournalPayload::Result {
            work_item,
            output: json!({"total": 42}),
            artifacts: vec!["branch:fix/total".into()],
        },
        JournalPayload::Attachment {
            project: ProjectId::from_ulid(ulid::Ulid::from_parts(5, 5)),
            attached: true,
        },
        JournalPayload::Document {
            file: bisa_core::AttachmentRef {
                sha256: "ab".repeat(32),
                name: "brief.pdf".into(),
                mime: "application/pdf".into(),
                size: 1234,
            },
        },
        JournalPayload::Signal {
            signal: "01J0SIG".into(),
            listener: Some(
                "workspace:01BX5ZZKBKACTAV9WEVGEMMVRZ/ticket"
                    .parse()
                    .expect("a listener key"),
            ),
            source: bisa_core::SignalSource::Hook,
            name: None,
            payload: json!({"ticket": "the total is wrong"}),
        },
        JournalPayload::Judgement {
            judgement: bisa_core::decision::Judgement {
                point: bisa_core::decision::DecisionPoint::AssignPick,
                provider: bisa_core::decision::DecisionProviderKind::Harness,
                model: "claude-sonnet-5".into(),
                calibrated: false,
                questions: Default::default(),
                answers: Default::default(),
                outcome: bisa_core::decision::JudgementOutcome::Failed,
                reason: Some("no answer".into()),
                latency_ms: 12,
                usage: Default::default(),
            },
            run: None,
            step: None,
        },
        JournalPayload::TurnMetrics {
            session,
            input_tokens: 1200,
            output_tokens: 340,
            usd_cents: 250,
        },
    ]
}

/// Nothing the journal can record is invisible in the Pulse.
///
/// `TurnMetrics` used to be excluded outright — the route returned `None` for
/// it — so tokens and cost were a fact the CLI printed and the desktop could
/// not show at all. There are **no** deliberate exclusions now: a row is
/// produced for every variant, and what to do with it is the client's
/// decision, where it can be a filter the reader controls rather than a
/// silence they cannot.
#[tokio::test(flavor = "multi_thread")]
async fn every_journal_payload_variant_produces_a_pulse_row() {
    let node = Node::start().await;
    let id = node.new_goal("record everything").await;
    let owner = node.ws.owner_keys().clone();

    let expected: Vec<&str> = one_of_each().iter().map(payload_tag).collect();
    for payload in one_of_each() {
        node.ws
            .append_journal(&bisa_core::Home::from(id), payload, &owner, None)
            .expect("append");
    }

    let v = node.get("/pulse?limit=200").await;
    let rows = v["rows"].as_array().unwrap();
    for tag in expected {
        assert!(
            rows.iter().any(|r| r["event"]["type"] == json!(tag)),
            "no pulse row for the {tag} payload: {v}"
        );
    }

    node.shutdown().await;
}

// ---------------------------------------------------------------------------
// Agents, teams, recall, models
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn agents_teams_and_recall() {
    let node = Node::start().await;

    // A workspace opens with the platform's General Agent and Workflow Agent and nobody else.
    // They are created inside `Workspace::open`, so no surface has to remember to ask.
    let v = node.get("/agents").await;
    let agents = v["agents"].as_array().unwrap();
    assert_eq!(
        agents.len(),
        2,
        "a fresh workspace has the General Agent and the Workflow Agent: {v}"
    );
    let core = agents
        .iter()
        .find(|a| a["id"] == json!(CORE_AGENT_ID))
        .expect("the General Agent");
    assert_eq!(core["id"], json!(CORE_AGENT_ID));
    assert_eq!(core["origin"], json!("core"));
    assert_eq!(
        core["pubkey"].as_str().unwrap().len(),
        64,
        "every agent gets its own keypair"
    );

    // An agent is a definition + harness + model plan + references.
    let v = node
        .post(
            "/agents",
            json!({
                "name": "Cartographer",
                "description": "maps the codebase",
                "system_prompt": "You map code.",
                "harness": "claude-code",
                "models": {
                    "strategy": "fallback",
                    "models": [
                        {"model": "some-model", "weight": 1, "enabled": true},
                        {"model": "spare-model", "weight": 1, "enabled": true}
                    ]
                },
                "tags": ["Code", "engineering"],
            }),
        )
        .await;
    let agent = v["agent"]["id"].as_str().unwrap().to_string();
    assert_eq!(v["agent"]["models"]["strategy"], json!("fallback"));
    assert_eq!(
        v["agent"]["models"]["models"][0]["model"],
        json!("some-model")
    );
    assert_eq!(
        v["agent"]["models"]["models"][1]["model"],
        json!("spare-model"),
        "the fallback survives the round trip — order is the plan"
    );
    assert_eq!(
        v["agent"]["tags"],
        json!(["code", "engineering"]),
        "tags are normalized and sorted on the way in"
    );
    assert!(v["agent"]["skills"].as_array().unwrap().is_empty());
    assert_eq!(v["agent"]["respond"], json!("owner_only"), "fail-closed");

    let (code, _) = node
        .req(
            "POST",
            "/agents",
            Some(json!({"name": "x", "system_prompt": " ", "harness": "y"})),
        )
        .await;
    assert_eq!(code, 400);

    // A reference to something that is not there is refused at the door: the
    // store would tolerate it until launch, where it costs a procedure
    // silently.
    let (code, v) = node
        .req(
            "POST",
            "/agents",
            Some(
                json!({"name": "Ghost", "system_prompt": "s", "harness": "h",
                        "skills": ["no-such-skill"]}),
            ),
        )
        .await;
    assert_eq!(code, 400);
    assert!(
        v["error"].as_str().unwrap().contains("no-such-skill"),
        "{v}"
    );

    let (code, v) = node
        .req(
            "PATCH",
            &format!("/agents/{agent}"),
            Some(json!({"models": {
                "strategy": "round_robin",
                "models": [{"model": "other-model"}]
            }})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["agent"]["models"]["strategy"], json!("round_robin"));
    assert_eq!(
        v["agent"]["models"]["models"].as_array().unwrap().len(),
        1,
        "a plan patch replaces the ordered list rather than merging into it"
    );
    assert_eq!(
        v["agent"]["models"]["models"][0]["model"],
        json!("other-model")
    );

    // A tag that cannot be a tag is refused by name, on the way in as well as
    // on the way out.
    let (code, v) = node
        .req(
            "PATCH",
            &format!("/agents/{agent}"),
            Some(json!({"tags": ["#ops"]})),
        )
        .await;
    assert_eq!(code, 400);
    assert!(v["error"].as_str().unwrap().contains("#ops"), "{v}");

    // Recall is owner-readable and starts empty.
    let v = node.get(&format!("/agents/{agent}/recall")).await;
    assert_eq!(v["agent"], json!(agent));
    assert!(v["records"].as_array().unwrap().is_empty());

    // Model discovery: shape only — an uninstalled harness knows nothing, and
    // "unknown" must not read as "unsupported".
    let v = node.get("/harnesses/claude-code/models").await;
    assert!(v["models"].is_array());

    // Teams are agents + humans, and carry categories like everything else.
    let human = nostr::key::Keys::generate().public_key().to_hex();
    let v = node
        .post(
            "/teams",
            json!({"name": "Launch crew", "purpose": "ship it",
                   "members": [{"human": human}, {"agent": agent}],
                   "tags": ["delivery"]}),
        )
        .await;
    let team = v["team"]["id"].as_str().unwrap().to_string();
    assert_eq!(v["team"]["members"].as_array().unwrap().len(), 2);
    assert_eq!(v["team"]["tags"], json!(["delivery"]));

    let (code, _) = node
        .req(
            "POST",
            "/teams",
            Some(json!({"name": "bad", "members": [{"nope": "x"}]})),
        )
        .await;
    assert_eq!(code, 400, "a member is a human or an agent");

    // A team inside a team is refused: one level needs no cycle detection.
    let (code, v) = node
        .req(
            "POST",
            "/teams",
            Some(json!({"name": "nested", "members": [{"team": team}]})),
        )
        .await;
    assert_eq!(code, 400, "{v}");

    let v = node.get("/teams").await;
    assert_eq!(
        v["teams"].as_array().unwrap().len(),
        1,
        "nothing is seeded: this is the only team"
    );
    let (code, v) = node
        .req(
            "PATCH",
            &format!("/teams/{team}"),
            Some(json!({"name": "Crew", "members": [{"agent": agent}]})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["team"]["name"], json!("Crew"));
    assert_eq!(
        v["team"]["tags"],
        json!(["delivery"]),
        "a patch that omits tags keeps them"
    );
    let (code, _) = node.req("DELETE", &format!("/teams/{team}"), None).await;
    assert_eq!(code, 200);
    assert!(node.get("/teams").await["teams"]
        .as_array()
        .unwrap()
        .is_empty());

    // Deleting an agent leaves the one that is always there alone.
    let (code, _) = node.req("DELETE", &format!("/agents/{agent}"), None).await;
    assert_eq!(code, 200);
    let v = node.get("/agents").await;
    assert_eq!(
        v["agents"].as_array().unwrap().len(),
        2,
        "back to the General Agent and the Workflow Agent alone: {v}"
    );

    // The runtime roster is `/sessions` now — "agents" means definitions.
    let v = node.get("/sessions").await;
    assert!(v["sessions"].as_array().unwrap().is_empty());
    let (code, _) = node
        .req("POST", "/sessions/not-a-ulid/abort", Some(json!({})))
        .await;
    assert_eq!(code, 400);
    // A well-formed id the roster does not have is not found, in the node's
    // own words — what stopped a session is the engine's one door, and what
    // it does to each kind of session is the engine's `sessions` suite.
    let nobody = "01J000000000000000000000SE";
    let (code, v) = node
        .req(
            "POST",
            &format!("/sessions/{nobody}/abort"),
            Some(json!({})),
        )
        .await;
    assert_eq!(code, 404, "{v}");
    assert_eq!(v["error"], json!(format!("no session {nobody}")));

    node.shutdown().await;
}
// ---------------------------------------------------------------------------
// The core agent and the catalog
// ---------------------------------------------------------------------------

/// The milestone in one assertion: a workspace opens with the platform's own
/// agent and nothing else.
///
/// The previous shape put every catalog agent, skill, team and channel into
/// every workspace on first run, and charged the owner for
/// an organisation they never chose. What replaces it is one agent — created
/// inside `Workspace::open`, so no surface has to remember to ask for it — and
/// a catalog that installs on demand.
/// A node asked for any free port says the port it bound — the address a
/// terminal's hooks are handed, and the one the footer shows — and answers
/// there. The address it was asked for ends in `:0`, where nothing listens.
#[tokio::test(flavor = "multi_thread")]
async fn a_node_on_any_free_port_says_the_port_it_bound() {
    let node = Node::start_listening().await;
    let v = node.get("/node").await;
    let listen = v["listen"].as_str().expect("a TCP address").to_string();
    let port: u16 = listen
        .strip_prefix("http://127.0.0.1:")
        .unwrap_or_else(|| panic!("a loopback address: {listen}"))
        .parse()
        .unwrap_or_else(|_| panic!("a port: {listen}"));
    assert_ne!(port, 0, "the port it bound, not the one it was asked for");

    let stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap_or_else(|e| panic!("nothing answers at {listen}: {e}"));
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .expect("handshake");
    tokio::spawn(conn);
    let request = Request::builder()
        .method("GET")
        .uri("/health")
        .header(hyper::header::HOST, format!("127.0.0.1:{port}"))
        .body(Full::new(Bytes::new()))
        .unwrap();
    let answer = sender.send_request(request).await.expect("an answer");
    assert_eq!(answer.status().as_u16(), 200);
    let body = answer.into_body().collect().await.unwrap().to_bytes();
    let health: Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(health["version"], v["version"], "the same node: {health}");
    node.shutdown().await;
}

/// `GET /node` says what this node is: the version `/health` says, the
/// process this test runs in, when it started, the socket it bound, the
/// workspace it keeps, whether it is paused and how many sessions are live.
#[tokio::test(flavor = "multi_thread")]
async fn the_node_describes_itself() {
    let node = Node::start().await;
    let v = node.get("/node").await;
    assert_eq!(v["version"], json!(env!("CARGO_PKG_VERSION")));
    assert_eq!(v["version"], node.get("/health").await["version"]);
    assert_eq!(v["pid"], json!(std::process::id()));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let started = v["started_at"].as_u64().unwrap();
    assert!(started <= now && now - started < 60, "{started} vs {now}");
    let socket = std::path::PathBuf::from(v["socket"].as_str().unwrap());
    assert!(socket.exists(), "the socket it bound: {}", socket.display());
    assert!(
        v.get("listen").is_none(),
        "no TCP address over the unix socket: {v}"
    );
    assert_eq!(
        std::path::Path::new(v["data_dir"].as_str().unwrap()),
        node.data_dir()
    );
    assert!(v["logs_dir"]
        .as_str()
        .unwrap()
        .starts_with(node.data_dir().to_str().unwrap()));
    assert_eq!(v["paused"], json!(false));
    assert_eq!(v["live_sessions"], json!(0));
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_definition_nobody_made_is_a_404_and_one_still_in_use_is_a_409() {
    let node = Node::start().await;
    // Unknown, well-formed ids: not found, never a malformed body.
    for path in [
        "/agents/nobody",
        "/teams/nobody",
        "/skills/nobody",
        "/mcp/nobody",
    ] {
        let (code, v) = node.req("GET", path, None).await;
        assert_eq!(code, 404, "GET {path}: {v}");
        let (code, v) = node.req("DELETE", path, None).await;
        assert_eq!(code, 404, "DELETE {path}: {v}");
    }
    // A skill an agent carries: the delete is a conflict naming the holder,
    // and the skill is still there afterwards.
    let skill = node
        .post(
            "/skills",
            json!({"id": "house-style", "name": "House style", "description": "how we write",
                   "markdown": "# Style\n"}),
        )
        .await;
    assert_eq!(skill["skill"]["id"], json!("house-style"), "{skill}");
    let agent = node
        .post(
            "/agents",
            json!({"name": "Writer", "system_prompt": "You write.",
                   "harness": "claude-code", "skills": ["house-style"]}),
        )
        .await;
    assert_eq!(agent["agent"]["id"], json!("writer"), "{agent}");
    let (code, v) = node.req("DELETE", "/skills/house-style", None).await;
    assert_eq!(code, 409, "{v}");
    assert!(
        v["error"].as_str().unwrap().contains("agent Writer"),
        "the holder is named as a person reads it: {v}"
    );
    let (code, _) = node.req("GET", "/skills/house-style", None).await;
    assert_eq!(code, 200, "a refused delete changed nothing");
    node.shutdown().await;
}

/// An edit the node refuses changes nothing. Standing an agent or a team
/// down is a transition with consequences — it leaves every channel it was
/// in, and the channel says so — so an edit that carries it beside something
/// the node refuses must not have stood anybody down, nor said that it did,
/// by the time the refusal is answered.
#[tokio::test(flavor = "multi_thread")]
async fn an_edit_that_is_refused_stands_nobody_down() {
    let node = Node::start().await;
    let v = node
        .post(
            "/agents",
            json!({"name": "Scout", "system_prompt": "You look ahead.", "harness": "claude-code"}),
        )
        .await;
    let scout = v["agent"]["id"].as_str().unwrap().to_string();
    assert_eq!(v["agent"]["enabled"], json!(true));
    // What the one room everybody is in was told about who came and went.
    let told = || async {
        let v = node.get("/channels/general/messages").await;
        v["messages"]
            .as_array()
            .unwrap_or_else(|| panic!("the room's messages: {v}"))
            .iter()
            .filter(|m| m["body_kind"] == json!("membership"))
            .count()
    };
    let before = told().await;
    for refused in [
        json!({"enabled": false, "skills": ["a-skill-nobody-wrote"]}),
        json!({"enabled": false, "mcps": ["a-server-nobody-installed"]}),
        json!({"enabled": false, "respond": "anybody"}),
        json!({"enabled": false, "name": ""}),
    ] {
        let (code, v) = node
            .req("PATCH", &format!("/agents/{scout}"), Some(refused.clone()))
            .await;
        assert_eq!(code, 400, "{refused}: {v}");
        let v = node.get(&format!("/agents/{scout}")).await;
        assert_eq!(
            v["agent"]["enabled"],
            json!(true),
            "{refused} was refused, and stood the agent down all the same"
        );
    }

    let v = node
        .post(
            "/teams",
            json!({"name": "Survey", "purpose": "looks ahead", "members": [{"agent": scout}]}),
        )
        .await;
    let team = v["team"]["id"].as_str().unwrap().to_string();
    for refused in [
        json!({"enabled": false, "members": [{"robot": "nobody"}]}),
        json!({"enabled": false, "members": [{"agent": "an-agent-nobody-made"}]}),
    ] {
        let (code, v) = node
            .req("PATCH", &format!("/teams/{team}"), Some(refused.clone()))
            .await;
        assert_eq!(code, 400, "{refused}: {v}");
        let v = node.get(&format!("/teams/{team}")).await;
        assert_eq!(
            v["team"]["enabled"],
            json!(true),
            "{refused} was refused, and stood the team down all the same"
        );
    }
    assert_eq!(
        told().await,
        before,
        "an edit that was refused told the room that somebody left"
    );

    // And an edit that goes through does both, in one write said once: stood
    // down, and edited.
    let v = node
        .post_patch(
            &format!("/agents/{scout}"),
            json!({"enabled": false, "description": "looks ahead"}),
        )
        .await;
    assert_eq!(v["agent"]["enabled"], json!(false));
    assert_eq!(v["agent"]["description"], json!("looks ahead"));
    assert_eq!(told().await, before + 1, "said once, where it happened");
    // The same edit again is no transition: nothing more is said.
    node.post_patch(&format!("/agents/{scout}"), json!({"enabled": false}))
        .await;
    assert_eq!(told().await, before + 1);
    node.shutdown().await;
}

/// A record of the library that is there and cannot be read is said as that,
/// by its file — never as a reference to something nobody wrote, which would
/// send a person looking for a typo — and the lists go on without it.
#[tokio::test(flavor = "multi_thread")]
async fn a_record_that_cannot_be_read_is_never_said_to_be_unknown() {
    let node = Node::start().await;
    let v = node
        .post(
            "/agents",
            json!({"name": "Scout", "system_prompt": "You look ahead.", "harness": "claude-code"}),
        )
        .await;
    let scout = v["agent"]["id"].as_str().unwrap().to_string();
    let v = node
        .post(
            "/agents",
            json!({"name": "Mapper", "system_prompt": "You draw.", "harness": "claude-code"}),
        )
        .await;
    let mapper = v["agent"]["id"].as_str().unwrap().to_string();
    node.post(
        "/skills",
        json!({"id": "survey", "name": "Survey", "description": "when", "markdown": "# walk"}),
    )
    .await;

    // Written over by something that is no record — another shape of the
    // code, a disk that lost the end of the file.
    let paths = node.ws.paths();
    let skill = paths.skill_file(&bisa_core::SkillId::new("survey").unwrap());
    let agent = paths.agent_file(&AgentId::new(&mapper).unwrap());
    for file in [&skill, &agent] {
        assert!(file.is_file(), "{}", file.display());
        std::fs::write(file, b"{ \"id\": ").unwrap();
    }

    let said = |v: &serde_json::Value| v["error"].as_str().unwrap_or_default().to_string();
    let (code, v) = node
        .req(
            "PATCH",
            &format!("/agents/{scout}"),
            Some(json!({"skills": ["survey"]})),
        )
        .await;
    assert_eq!(code, 404, "{v}");
    assert!(
        said(&v).contains("survey.json") && !said(&v).contains("unknown"),
        "{v}"
    );
    let (code, v) = node
        .req(
            "POST",
            "/teams",
            Some(json!({"name": "Survey", "members": [{"agent": mapper}]})),
        )
        .await;
    assert_eq!(code, 404, "{v}");
    assert!(
        said(&v).contains(&format!("{mapper}.json")) && !said(&v).contains("unknown"),
        "{v}"
    );
    for (path, file) in [
        ("/skills/survey".to_string(), "survey.json".to_string()),
        (format!("/agents/{mapper}"), format!("{mapper}.json")),
    ] {
        let (code, v) = node.req("GET", &path, None).await;
        assert_eq!(code, 404, "GET {path}: {v}");
        assert!(said(&v).contains(&file), "GET {path} names its file: {v}");
    }
    // What nobody wrote is still what nobody wrote.
    let (code, v) = node
        .req(
            "PATCH",
            &format!("/agents/{scout}"),
            Some(json!({"skills": ["a-skill-nobody-wrote"]})),
        )
        .await;
    assert_eq!(code, 400, "{v}");

    // The lists go on without what they cannot read.
    let v = node.get("/agents").await;
    let ids: Vec<&str> = v["agents"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| a["id"].as_str())
        .collect();
    assert!(
        ids.contains(&scout.as_str()) && !ids.contains(&mapper.as_str()),
        "{ids:?}"
    );
    let v = node.get("/skills").await;
    assert_eq!(v["skills"], json!([]), "{v}");
    node.shutdown().await;
}

/// A PATCH leaves alone what it does not name, and empties what it names as
/// `null`: the line under an agent's name and what a team is for can be taken
/// away as they were given.
#[tokio::test(flavor = "multi_thread")]
async fn what_an_edit_names_as_nothing_is_taken_away() {
    let node = Node::start().await;
    let v = node
        .post(
            "/agents",
            json!({
                "name": "Scout",
                "description": "looks ahead",
                "system_prompt": "You look ahead.",
                "harness": "claude-code",
            }),
        )
        .await;
    let scout = v["agent"]["id"].as_str().unwrap().to_string();
    let v = node
        .post_patch(&format!("/agents/{scout}"), json!({"name": "Scout II"}))
        .await;
    assert_eq!(v["agent"]["description"], json!("looks ahead"), "{v}");
    let v = node
        .post_patch(&format!("/agents/{scout}"), json!({"description": null}))
        .await;
    assert_eq!(v["agent"]["description"], json!(null), "{v}");
    assert_eq!(v["agent"]["name"], json!("Scout II"));

    let v = node
        .post(
            "/teams",
            json!({"name": "Survey", "purpose": "looks ahead", "members": [{"agent": scout}]}),
        )
        .await;
    let team = v["team"]["id"].as_str().unwrap().to_string();
    let v = node
        .post_patch(&format!("/teams/{team}"), json!({"name": "Surveyors"}))
        .await;
    assert_eq!(v["team"]["purpose"], json!("looks ahead"), "{v}");
    let v = node
        .post_patch(&format!("/teams/{team}"), json!({"purpose": null}))
        .await;
    assert_eq!(v["team"]["purpose"], json!(null), "{v}");
    assert_eq!(v["team"]["name"], json!("Surveyors"));
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_fresh_workspace_has_the_two_core_agents_and_nothing_else() {
    let node = Node::start().await;

    let v = node.get("/agents").await;
    let agents = v["agents"].as_array().unwrap();
    assert_eq!(agents.len(), 2, "two agents, and only two: {v}");
    let ids: Vec<&str> = agents.iter().filter_map(|a| a["id"].as_str()).collect();
    for core in AgentId::CORE {
        assert!(ids.contains(&core), "{core} is missing: {ids:?}");
    }
    for a in agents {
        assert_eq!(a["origin"], json!("core"));
        assert_eq!(
            a["enabled"],
            json!(true),
            "the agents that are always there are always on"
        );
    }
    // No workflows are installed until somebody asks; the catalog holds the
    // templates.
    let v = node.get("/workflows").await;
    assert!(v["workflows"].as_array().unwrap().is_empty(), "{v}");

    for (path, key) in [("/teams", "teams"), ("/skills", "skills"), ("/mcp", "mcp")] {
        let v = node.get(path).await;
        assert!(
            v[key].as_array().unwrap().is_empty(),
            "GET {path} seeded something: {v}"
        );
    }
    // The one room every workspace has, and nothing beside it.
    let v = node.get("/channels").await;
    let channels = v["channels"].as_array().unwrap();
    assert_eq!(channels.len(), 1, "{v}");
    assert_eq!(channels[0]["channel"]["id"], json!("general"));
    assert_eq!(
        channels[0]["channel"]["roster"]["policy"],
        json!("everyone")
    );

    node.shutdown().await;
}

/// The core agent's guards, as the desktop meets them: a 409 for the two
/// things it can never be (removed, disabled), a 400 with the store's own
/// message for a field it may not change — never a 500 and never a silent
/// restore.
///
/// A silent restore is the failure this is really about. The store compares
/// field by field against the stored record, so a client that PATCHes the
/// whole object back unchanged still succeeds — and a client that changes a
/// field it may not change is told *which* one, because an editor that reports
/// a saved prompt the store threw away is worse than one that reports an
/// error.
#[tokio::test(flavor = "multi_thread")]
async fn the_core_agents_are_the_ones_that_cannot_be_removed_or_muted() {
    let node = Node::start().await;
    for id in AgentId::CORE {
        let (code, v) = node.req("DELETE", &format!("/agents/{id}"), None).await;
        assert_eq!(code, 409, "{v}");
        assert!(
            v["error"].as_str().unwrap().contains(id),
            "a refusal names what it refused: {v}"
        );

        let (code, v) = node
            .req(
                "PATCH",
                &format!("/agents/{id}"),
                Some(json!({"enabled": false})),
            )
            .await;
        assert_eq!(code, 409, "{v}");
        assert!(
            v["error"].as_str().unwrap().contains("disabled"),
            "switching it off is refused on its own terms: {v}"
        );
    }
    let id = CORE_AGENT_ID;

    let (code, v) = node
        .req(
            "PATCH",
            &format!("/agents/{id}"),
            Some(json!({"system_prompt": "You are something else now."})),
        )
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(
        v["error"].as_str().unwrap().contains("system_prompt"),
        "the refusal names the field that changed: {v}"
    );

    // The two that are editable. A harness swap and a model plan are the whole
    // vocabulary: this agent has to run on whatever the owner actually has
    // installed, and it has to survive a quota wall.
    let (code, v) = node
        .req(
            "PATCH",
            &format!("/agents/{id}"),
            Some(json!({"harness": "codex"})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["agent"]["harness"], json!("codex"));

    let (code, v) = node
        .req(
            "PATCH",
            &format!("/agents/{id}"),
            Some(json!({"models": {"strategy": "fallback", "models": [
                {"model": "opus", "weight": 1, "enabled": true},
                {"model": "sonnet", "weight": 1, "enabled": true}
            ]}})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["agent"]["models"]["models"][0]["model"], json!("opus"));

    // The whole object, sent back untouched. This is what a desktop editor
    // does when the user opens the agent and presses save without typing, and
    // it has to succeed — otherwise the two editable fields are unreachable
    // through any UI that PATCHes the form it rendered.
    let agent = node.get(&format!("/agents/{id}")).await["agent"].clone();
    let (code, v) = node
        .req(
            "PATCH",
            &format!("/agents/{id}"),
            Some(json!({
                "name": agent["name"],
                "description": agent["description"],
                "system_prompt": agent["system_prompt"],
                "harness": agent["harness"],
                "models": agent["models"],
                "skills": agent["skills"],
                "mcps": agent["mcps"],
                "tags": agent["tags"],
                "respond": agent["respond"],
                "enabled": agent["enabled"],
            })),
        )
        .await;
    assert_eq!(code, 200, "an unchanged field was not changed: {v}");

    node.shutdown().await;
}

/// A key the body does not know is a refusal, on the way in and on an edit:
/// dropped in silence, a switch spelled wrong would read as a switch left off
/// and an edit that changed nothing would answer 200.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_body_carrying_a_key_it_does_not_know_is_refused() {
    let node = Node::start().await;
    let (code, v) = node
        .req(
            "POST",
            "/agents",
            Some(
                json!({"name": "Scout", "system_prompt": "look", "harness": "claude-code",
                        "judges": true}),
            ),
        )
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(v["error"].as_str().unwrap().contains("judges"), "{v}");
    let before = node.get("/agents").await["agents"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(before, 2, "nothing was made");

    let (code, v) = node
        .req(
            "PATCH",
            &format!("/agents/{CORE_AGENT_ID}"),
            Some(json!({"decision_making": true, "judges": true})),
        )
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(v["error"].as_str().unwrap().contains("judges"), "{v}");
    let agent = node.get(&format!("/agents/{CORE_AGENT_ID}")).await["agent"].clone();
    assert_eq!(
        agent["decision_making"],
        json!(false),
        "nothing was written"
    );

    // The switch under its own name is one of the three things a core agent
    // may change.
    let (code, v) = node
        .req(
            "PATCH",
            &format!("/agents/{CORE_AGENT_ID}"),
            Some(json!({"decision_making": true})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["agent"]["decision_making"], json!(true));

    node.shutdown().await;
}

/// The retired spelling of the switch is a key the body does not know.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_body_carrying_the_retired_switch_is_refused() {
    let node = Node::start().await;
    let (code, v) = node
        .req(
            "PATCH",
            &format!("/agents/{CORE_AGENT_ID}"),
            Some(json!({"decision_maker": true})), // terminology-lint-ignore: decision-maker - proves the retired word is refused
        )
        .await;
    assert_eq!(code, 400, "{v}");
    node.shutdown().await;
}

/// The catalog is browsable before anything is installed, and every row says
/// whether its id is already taken here — so one request draws the picker and
/// its checkmarks rather than four races against `/agents`, `/skills`,
/// `/teams` and `/channels`.
#[tokio::test(flavor = "multi_thread")]
async fn the_catalog_lists_what_a_workspace_may_install() {
    use bisa_store::CATALOG;
    let node = Node::start().await;

    // Counted from the bundle rather than from a literal, so shipping a new
    // agent is a bundle edit and not a test edit. The tally that used to sit
    // here was the same literal in prose, and it went stale the first time
    // the catalog grew — a comment restating what the next line computes is
    // just a copy nobody re-derives.
    let total = CATALOG.entry_count();
    let v = node.get("/catalog").await;
    let entries = v["entries"].as_array().unwrap();
    assert_eq!(entries.len(), total);
    assert!(
        entries.iter().all(|e| e["installed"] == json!(false)),
        "nothing is installed until somebody chooses it: {v}"
    );
    assert!(
        entries
            .iter()
            .all(|e| !e["name"].as_str().unwrap().is_empty()
                && !e["description"].as_str().unwrap().is_empty()),
        "a picker needs a name and a line of description for every row"
    );

    let v = node.get("/catalog?kind=agent").await;
    let agents = v["entries"].as_array().unwrap();
    assert_eq!(agents.len(), CATALOG.agents.len());
    assert!(agents.iter().all(|e| e["kind"] == json!("agent")));

    // `requires` is what the choice costs: an agent brings its skills.
    let dev = agents
        .iter()
        .find(|e| e["slug"] == json!("developer"))
        .expect("the catalog ships a Developer");
    assert!(
        !dev["requires"].as_array().unwrap().is_empty(),
        "a catalog agent carries skill *slugs*, not markdown: {dev}"
    );

    // A tag narrows the same list. Expected count is derived from the rows
    // themselves, so the assertion survives a re-tagged bundle.
    let expected = agents
        .iter()
        .filter(|e| {
            e["tags"]
                .as_array()
                .unwrap()
                .contains(&json!("engineering"))
        })
        .count();
    assert!(
        expected > 0 && expected < agents.len(),
        "pick a tag that actually narrows: {expected} of {}",
        agents.len()
    );
    let v = node.get("/catalog?kind=agent&tag=engineering").await;
    assert_eq!(v["entries"].as_array().unwrap().len(), expected);

    // A kind nobody ships is a 400 that spells out the kinds that exist. A
    // filter that quietly matched nothing would be indistinguishable from one
    // that correctly matched nothing.
    let (code, v) = node.req("GET", "/catalog?kind=agents", None).await;
    assert_eq!(code, 400, "{v}");
    let msg = v["error"].as_str().unwrap();
    for kind in ["agent", "skill", "team", "channel"] {
        assert!(msg.contains(kind), "the refusal names {kind}: {msg}");
    }

    // One entry, for the panel a picker opens before committing.
    let v = node.get("/catalog/team/engineering").await;
    assert_eq!(v["entry"]["slug"], json!("engineering"));
    assert_eq!(v["entry"]["kind"], json!("team"));
    assert!(
        v["entry"].get("workflow").is_none(),
        "only a workflow template's row carries a definition: {v}"
    );
    let (code, v) = node.req("GET", "/catalog/team/no-such-team", None).await;
    assert_eq!(code, 400, "a slug this build does not ship: {v}");

    // A workflow template's row carries its whole definition — the graph a
    // gallery draws before anything is installed — on a fresh workspace, the
    // templates whose `spawn` steps name an uninstalled one included.
    let v = node.get("/catalog?kind=workflow").await;
    let templates = v["entries"].as_array().unwrap();
    assert_eq!(templates.len(), CATALOG.workflows.len());
    for e in templates {
        let steps = e["workflow"]["steps"]
            .as_array()
            .unwrap_or_else(|| panic!("{}: a template's row carries its steps", e["slug"]));
        assert!(!steps.is_empty(), "{e}");
        assert!(
            steps
                .iter()
                .any(|s| s["then"].as_array().is_some_and(|t| !t.is_empty())),
            "{}: a definition has flows",
            e["slug"]
        );
        assert_eq!(e["workflow"]["name"], e["name"]);
    }
    assert!(
        agents.iter().all(|e| e.get("workflow").is_none()),
        "an agent's row carries no definition"
    );
    let v = node.get("/catalog/workflow/bug-fix").await;
    assert!(
        v["entry"]["workflow"]["steps"]
            .as_array()
            .is_some_and(|s| !s.is_empty()),
        "the one-entry read carries it too: {v}"
    );

    node.shutdown().await;
}

/// An install is transitive, idempotent, and visible afterwards.
///
/// Transitive because a team is useless without the agents it names and an
/// agent is diminished without its procedures; idempotent because a picker
/// whose second click is an error is a picker people stop trusting; and the
/// answer lists only what was *created*, so an owner can undo a one-line
/// request that produced fourteen objects.
#[tokio::test(flavor = "multi_thread")]
async fn installing_a_team_brings_its_agents_and_their_skills() {
    let node = Node::start().await;

    // What the catalog says the choice costs, read before making it.
    let team_entry = node.get("/catalog/team/engineering").await;
    let want_agents: Vec<String> = team_entry["entry"]["requires"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect();
    assert!(want_agents.len() > 1, "the Engineering team is a team");
    let mut want_skills: Vec<String> = Vec::new();
    for slug in &want_agents {
        let v = node.get(&format!("/catalog/agent/{slug}")).await;
        for s in v["entry"]["requires"].as_array().unwrap() {
            let s = s.as_str().unwrap().to_string();
            if !want_skills.contains(&s) {
                want_skills.push(s);
            }
        }
    }
    assert!(!want_skills.is_empty());

    let v = node
        .post(
            "/catalog/install",
            json!({"kind": "team", "slug": "engineering"}),
        )
        .await;
    let made = &v["installed"];
    assert_eq!(made["teams"], json!(["engineering"]));
    let mut got_agents: Vec<String> = made["agents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect();
    let mut got_skills: Vec<String> = made["skills"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect();
    got_agents.sort();
    got_skills.sort();
    let (mut want_agents_sorted, mut want_skills_sorted) = (want_agents.clone(), want_skills);
    want_agents_sorted.sort();
    want_skills_sorted.sort();
    assert_eq!(
        got_agents, want_agents_sorted,
        "the team brought its agents"
    );
    assert_eq!(
        got_skills, want_skills_sorted,
        "and those agents brought their procedures"
    );
    assert!(
        made["channels"].as_array().unwrap().is_empty(),
        "a team is not a channel: {v}"
    );

    // The same request again creates nothing. Not an error — that is what
    // idempotent means, reported honestly.
    let v = node
        .post(
            "/catalog/install",
            json!({"kind": "team", "slug": "engineering"}),
        )
        .await;
    for kind in ["agents", "skills", "teams", "channels"] {
        assert!(
            v["installed"][kind].as_array().unwrap().is_empty(),
            "a second install created a {kind}: {v}"
        );
    }

    // The catalog now says so, in the same row a picker already drew.
    let v = node.get("/catalog").await;
    let entries = v["entries"].as_array().unwrap();
    let installed = |kind: &str, slug: &str| {
        entries
            .iter()
            .find(|e| e["kind"] == json!(kind) && e["slug"] == json!(slug))
            .unwrap_or_else(|| panic!("no catalog row for {kind} {slug}"))["installed"]
            == json!(true)
    };
    assert!(installed("team", "engineering"));
    for slug in &want_agents {
        assert!(installed("agent", slug), "{slug} reads as uninstalled");
    }
    assert!(
        !installed("team", "design"),
        "an install installs what was asked for and nothing else"
    );

    // An unknown kind on the install body gets the same refusal `?kind=` does.
    let (code, v) = node
        .req(
            "POST",
            "/catalog/install",
            Some(json!({"kind": "agents", "slug": "developer"})),
        )
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(v["error"].as_str().unwrap().contains("channel"), "{v}");

    // A slug held by something the owner made is a collision, refused by name
    // rather than overwritten — the id already belongs to a decision somebody
    // took.
    node.post(
        "/skills",
        json!({"id": "red-teaming", "name": "Mine",
               "description": "The one I wrote, under a name the catalog also uses.",
               "markdown": "# Mine"}),
    )
    .await;
    let (code, v) = node
        .req(
            "POST",
            "/catalog/install",
            Some(json!({"kind": "skill", "slug": "red-teaming"})),
        )
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(
        v["error"].as_str().unwrap().contains("red-teaming"),
        "the refusal names the id at stake: {v}"
    );
    assert_eq!(
        node.get("/skills/red-teaming").await["skill"]["name"],
        json!("Mine"),
        "and the owner's skill is untouched"
    );

    node.shutdown().await;
}

/// The core agent belongs to every team, and is stored in none of them.
///
/// A list you can edit is a list you can empty, so membership is added at read
/// time instead. `GET /teams/{id}` reports it through `team_participants`; the
/// work-routing pool (`team_agents`) deliberately excludes it, because an
/// agent whose job is to delegate must not win a race to implement a work
/// item.
#[tokio::test(flavor = "multi_thread")]
async fn an_installed_team_has_the_core_agents_without_storing_them() {
    let node = Node::start().await;
    node.post(
        "/catalog/install",
        json!({"kind": "team", "slug": "engineering"}),
    )
    .await;

    let core = json!({"agent": CORE_AGENT_ID});
    let v = node.get("/teams/engineering").await;
    let members = v["team"]["members"].as_array().unwrap();
    for id in AgentId::CORE {
        assert!(
            members.contains(&json!({"agent": id})),
            "an agent every team can reach is missing ({id}): {v}"
        );
    }

    // Nothing stored it: the truth file holds exactly the roster the catalog
    // named.
    let engineering = TeamId::new("engineering").unwrap();
    let stored = node.ws.get_team(&engineering).unwrap();
    assert!(
        !stored
            .members
            .iter()
            .any(|m| m.as_agent().is_some_and(|a| AgentId::CORE.contains(&a))),
        "membership is added at read time, not written: {stored:?}"
    );

    // And a PATCH that sends the rendered member list straight back is not a
    // way to store it either — the store strips it on every write.
    let (code, v) = node
        .req(
            "PATCH",
            "/teams/engineering",
            Some(json!({"members": members})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
    let stored = node.ws.get_team(&engineering).unwrap();
    assert!(!stored
        .members
        .iter()
        .any(|m| m.as_agent() == Some(CORE_AGENT_ID)));
    assert!(node.get("/teams/engineering").await["team"]["members"]
        .as_array()
        .unwrap()
        .contains(&core));

    node.shutdown().await;
}

// ---------------------------------------------------------------------------
// The skill library and the MCP registry
// ---------------------------------------------------------------------------

/// A procedure is written once in the library and referenced by id. Attaching
/// and detaching is the whole vocabulary an agent has for skills — there is no
/// route that puts markdown on an agent, because that is how twenty agents
/// end up with twenty drifting copies of one checklist.
#[tokio::test(flavor = "multi_thread")]
async fn skill_library_and_agent_wiring() {
    let node = Node::start().await;

    let v = node.get("/skills").await;
    assert!(
        v["skills"].as_array().unwrap().is_empty(),
        "the library is a catalog now: empty until somebody installs from it"
    );

    let v = node
        .post(
            "/skills",
            json!({"id": "map-the-code", "name": "Map the code",
                   "description": "Use when a codebase needs a map before work starts.",
                   "tags": ["Engineering"], "markdown": "# How to map\n1. Read."}),
        )
        .await;
    assert_eq!(v["skill"]["id"], json!("map-the-code"));
    assert_eq!(v["skill"]["tags"], json!(["engineering"]));
    assert_eq!(
        v["skill"]["origin"],
        json!("local"),
        "provenance is recorded, never claimed by the caller"
    );

    // The description is the only line a model reads before opening a skill,
    // so it is required rather than decorative.
    let (code, _) = node
        .req(
            "POST",
            "/skills",
            Some(json!({"id": "blank", "name": "n", "description": "  ",
                        "markdown": "x"})),
        )
        .await;
    assert_eq!(code, 400);

    let (code, _) = node
        .req(
            "POST",
            "/skills",
            Some(json!({"id": "map-the-code", "name": "again",
                        "description": "d", "markdown": "x"})),
        )
        .await;
    assert_eq!(code, 400, "an id is claimed once");

    let v = node.get("/skills/map-the-code").await;
    assert_eq!(v["skill"]["name"], json!("Map the code"));
    assert!(v["agents"].as_array().unwrap().is_empty(), "nobody yet");

    let (code, v) = node
        .req(
            "PATCH",
            "/skills/map-the-code",
            Some(json!({"markdown": "# How to map\n1. Read.\n2. Draw."})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
    assert!(v["skill"]["markdown"].as_str().unwrap().contains("Draw"));
    assert_eq!(
        v["skill"]["tags"],
        json!(["engineering"]),
        "a patch that omits tags keeps them"
    );

    // Wiring: attach by id, and the agent reports it.
    let v = node
        .post(
            "/agents",
            json!({"name": "Cartographer", "system_prompt": "You map code.",
                   "harness": "claude-code"}),
        )
        .await;
    let agent = v["agent"]["id"].as_str().unwrap().to_string();

    let v = node
        .post(&format!("/agents/{agent}/skills/map-the-code"), json!({}))
        .await;
    assert_eq!(v["agent"]["skills"], json!(["map-the-code"]));
    // Idempotent: attaching twice is one reference, not two.
    let v = node
        .post(&format!("/agents/{agent}/skills/map-the-code"), json!({}))
        .await;
    assert_eq!(v["agent"]["skills"], json!(["map-the-code"]));
    let v = node.get(&format!("/agents/{agent}")).await;
    assert_eq!(v["agent"]["skills"], json!(["map-the-code"]));
    let v = node.get("/skills/map-the-code").await;
    assert_eq!(
        v["agents"][0]["id"],
        json!(agent),
        "the library says who follows it"
    );

    let (code, v) = node
        .req("POST", &format!("/agents/{agent}/skills/no-such"), None)
        .await;
    assert_eq!(code, 404, "a skill nobody made is unknown: {v}");

    let (code, v) = node
        .req(
            "DELETE",
            &format!("/agents/{agent}/skills/map-the-code"),
            None,
        )
        .await;
    assert_eq!(code, 200, "{v}");
    assert!(
        node.get(&format!("/agents/{agent}")).await["agent"]["skills"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    // Deleting a skill an agent still follows is refused, and the refusal
    // names the agent. The old behaviour detached it everywhere first, which
    // meant one delete could quietly rewrite twenty definitions — a data loss
    // with no error and nothing on screen to read.
    node.post(&format!("/agents/{agent}/skills/map-the-code"), json!({}))
        .await;
    let before = node.get(&format!("/agents/{agent}")).await;
    let (code, v) = node.req("DELETE", "/skills/map-the-code", None).await;
    assert_eq!(code, 409, "{v}");
    assert!(
        v["error"].as_str().unwrap().contains("Cartographer"),
        "the refusal names who still follows it: {v}"
    );
    assert_eq!(
        node.get(&format!("/agents/{agent}")).await,
        before,
        "a refused delete changed nothing"
    );

    // `GET /usage` answers the same question before the refusal does.
    let v = node.get("/usage/skill/map-the-code").await;
    assert_eq!(v["usage"][0]["kind"], json!("agent"));
    assert_eq!(v["usage"][0]["id"], json!(agent));

    // Detach, and the delete goes through.
    let (code, _) = node
        .req(
            "DELETE",
            &format!("/agents/{agent}/skills/map-the-code"),
            None,
        )
        .await;
    assert_eq!(code, 200);
    let (code, v) = node.req("DELETE", "/skills/map-the-code", None).await;
    assert_eq!(code, 200, "{v}");
    let (code, _) = node.req("GET", "/skills/map-the-code", None).await;
    assert_eq!(code, 404, "a deleted skill is unknown, not malformed");

    node.shutdown().await;
}

/// The registry is local by construction: an id travels, a command line does
/// not. One name is refused outright, because the engine's own server answers
/// to it in every session.
#[tokio::test(flavor = "multi_thread")]
async fn mcp_registry_and_agent_wiring() {
    let node = Node::start().await;

    assert!(
        node.get("/mcp").await["mcp"].as_array().unwrap().is_empty(),
        "nothing is registered until you register it"
    );

    let v = node
        .post(
            "/mcp",
            json!({"id": "filesystem", "description": "local files",
                   "tags": ["Ops"],
                   "transport": {"transport": "stdio", "name": "fs",
                                 "command": "mcp-fs", "args": ["--root", "/tmp"],
                                 "env": {"FS_TOKEN": "hunter2"}}}),
        )
        .await;
    assert_eq!(v["mcp"]["id"], json!("filesystem"));
    assert_eq!(
        v["mcp"]["transport"]["env"]["FS_TOKEN"],
        json!(bisa_core::MCP_MASK),
        "a secret is written once and never read back: {v}"
    );
    assert_eq!(
        v["mcp"]["health"]["state"],
        json!("unknown"),
        "nothing was probed yet"
    );
    assert_eq!(
        v["mcp"]["name"],
        json!("fs"),
        "the server name comes from the transport, so the two cannot disagree"
    );
    assert_eq!(v["mcp"]["tags"], json!(["ops"]));
    assert_eq!(v["mcp"]["enabled"], json!(true));

    // The reserved name is the caller's mistake, not a server fault.
    let (code, v) = node
        .req(
            "POST",
            "/mcp",
            Some(json!({"id": "shadow",
                        "transport": {"transport": "stdio", "name": "bisa",
                                      "command": "true"}})),
        )
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(
        v["error"].as_str().unwrap().contains("reserved"),
        "the refusal says why: {v}"
    );

    let v = node.get("/mcp").await;
    assert_eq!(v["mcp"].as_array().unwrap().len(), 1);
    let v = node.get("/mcp/filesystem").await;
    assert_eq!(v["mcp"]["transport"]["command"], json!("mcp-fs"));
    assert_eq!(
        v["mcp"]["transport"]["env"]["FS_TOKEN"],
        json!(bisa_core::MCP_MASK)
    );

    // An edit that sends the mask back keeps the stored value: the key is
    // still there afterwards, and a new one rides beside it.
    let (code, v) = node
        .req(
            "PATCH",
            "/mcp/filesystem",
            Some(json!({"transport": {"transport": "stdio", "name": "fs", "command": "mcp-fs",
                                      "args": ["--root", "/tmp"],
                                      "env": {"FS_TOKEN": bisa_core::MCP_MASK, "FS_ROOT": "/srv"}}})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
    let env = v["mcp"]["transport"]["env"].as_object().unwrap();
    assert_eq!(env.len(), 2, "the masked key stays, the new one lands: {v}");
    assert!(env.values().all(|x| x == &json!(bisa_core::MCP_MASK)));

    let (code, v) = node
        .req(
            "PATCH",
            "/mcp/filesystem",
            Some(json!({"enabled": false, "description": "switched off"})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["mcp"]["enabled"], json!(false));
    assert_eq!(
        v["mcp"]["tags"],
        json!(["ops"]),
        "an omitted patch field holds"
    );

    let (code, v) = node
        .req(
            "PATCH",
            "/mcp/filesystem",
            Some(json!({"transport": {"transport": "http", "name": "bisa",
                                      "url": "http://127.0.0.1:1"}})),
        )
        .await;
    assert_eq!(code, 400, "the reserved name is refused on edit too: {v}");

    // Wiring, mirroring skills exactly.
    let v = node
        .post(
            "/agents",
            json!({"name": "Filer", "system_prompt": "You file.",
                   "harness": "claude-code"}),
        )
        .await;
    let agent = v["agent"]["id"].as_str().unwrap().to_string();
    let v = node
        .post(&format!("/agents/{agent}/mcps/filesystem"), json!({}))
        .await;
    assert_eq!(v["agent"]["mcps"], json!(["filesystem"]));
    assert_eq!(
        node.get("/mcp/filesystem").await["agents"][0]["id"],
        json!(agent)
    );
    let (code, _) = node
        .req("DELETE", &format!("/agents/{agent}/mcps/filesystem"), None)
        .await;
    assert_eq!(code, 200);
    assert!(node.get(&format!("/agents/{agent}")).await["agent"]["mcps"]
        .as_array()
        .unwrap()
        .is_empty());

    // Removing a registered server an agent still holds is refused, mirroring
    // skills exactly — including that a refused delete leaves the definition
    // untouched rather than detaching it first.
    node.post(&format!("/agents/{agent}/mcps/filesystem"), json!({}))
        .await;
    let before = node.get(&format!("/agents/{agent}")).await;
    let (code, v) = node.req("DELETE", "/mcp/filesystem", None).await;
    assert_eq!(code, 409, "{v}");
    assert!(
        v["error"].as_str().unwrap().contains("Filer"),
        "the refusal names who still holds it: {v}"
    );
    assert_eq!(
        node.get(&format!("/agents/{agent}")).await,
        before,
        "a refused delete changed nothing"
    );

    let v = node.get("/usage/mcp/filesystem").await;
    assert_eq!(v["usage"][0]["kind"], json!("agent"));
    assert_eq!(v["usage"][0]["id"], json!(agent));

    let (code, _) = node
        .req("DELETE", &format!("/agents/{agent}/mcps/filesystem"), None)
        .await;
    assert_eq!(code, 200);
    let (code, v) = node.req("DELETE", "/mcp/filesystem", None).await;
    assert_eq!(code, 200, "{v}");

    node.shutdown().await;
}

/// A probe of a transport nobody registered is a report and registers
/// nothing; a probe of an id is 404 when unknown, a bad transport is 400,
/// and a server that would not answer is a 200 whose report says how far
/// the conversation got — a result, never an HTTP error.
#[tokio::test(flavor = "multi_thread")]
async fn a_probe_is_a_report_never_an_error_and_registers_nothing() {
    let node = Node::start().await;
    let (code, v) = node
        .req(
            "POST",
            "/mcp/probe",
            Some(json!({"transport": {"transport": "stdio", "name": "ghost",
                                      "command": "/definitely/not/a/binary"},
                        "timeout_secs": 3})),
        )
        .await;
    assert_eq!(code, 200, "a server that would not start is a result: {v}");
    assert_eq!(v["report"]["ok"], json!(false));
    assert_eq!(v["report"]["stage"], json!("spawn"), "{v}");
    assert_eq!(v["report"]["transport"], json!("stdio"));
    assert!(
        node.get("/mcp").await["mcp"].as_array().unwrap().is_empty(),
        "a draft probe registers nothing"
    );

    let (code, v) = node
        .req(
            "POST",
            "/mcp/probe",
            Some(json!({"transport": {"transport": "http", "name": "docs", "url": "mcp.example.test/mcp"}})),
        )
        .await;
    assert_eq!(
        code, 400,
        "a URL that is not one is the caller's mistake: {v}"
    );
    assert!(v["error"].as_str().unwrap().contains("http://"), "{v}");

    let (code, v) = node.req("POST", "/mcp/nobody/probe", Some(json!({}))).await;
    assert_eq!(code, 404, "{v}");

    // A registered server that is switched off is answered in words, never dialed.
    node.post(
        "/mcp",
        json!({"id": "off", "transport": {"transport": "http", "name": "off",
                                          "url": "http://127.0.0.1:1/mcp",
                                          "headers": {"Authorization": "Bearer t"}}}),
    )
    .await;
    let (code, v) = node
        .req("PATCH", "/mcp/off", Some(json!({"enabled": false})))
        .await;
    assert_eq!(code, 200, "{v}");
    let (code, v) = node.req("POST", "/mcp/off/probe", Some(json!({}))).await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["report"]["ok"], json!(false));
    assert_eq!(
        v["report"]["error"],
        json!(bisa_engine::mcp_health::DISABLED),
        "{v}"
    );
    assert_eq!(
        v["health"]["state"],
        json!("unknown"),
        "a refusal is not a health"
    );

    // Enabled, an unreachable URL fails before or at the handshake, and the
    // failure becomes the entry's health, read beside it and masked still.
    let (code, _) = node
        .req("PATCH", "/mcp/off", Some(json!({"enabled": true})))
        .await;
    assert_eq!(code, 200);
    let (code, v) = node
        .req("POST", "/mcp/off/probe", Some(json!({"timeout_secs": 3})))
        .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["report"]["ok"], json!(false));
    assert_eq!(v["health"]["state"], json!("failing"));
    let listed = node.get("/mcp").await;
    let row = &listed["mcp"][0];
    assert_eq!(row["health"]["state"], json!("failing"), "{listed}");
    assert_eq!(
        row["transport"]["headers"]["Authorization"],
        json!(bisa_core::MCP_MASK)
    );
    assert!(
        !listed.to_string().contains("Bearer t"),
        "no header value leaves the registry: {listed}"
    );

    node.shutdown().await;
}

// ---------------------------------------------------------------------------
// Tags: one filter, every list
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn tag_filters_narrow_every_list() {
    let node = Node::start().await;

    // A filter needs something to filter, and a fresh workspace has one agent
    // and nothing else. These two entries put a tagged agent, skill, team and
    // channel in the workspace between them — installed the way an owner would
    // install them, rather than reached for behind the API.
    for (kind, slug) in [("team", "engineering"), ("channel", "engineering")] {
        node.post("/catalog/install", json!({"kind": kind, "slug": slug}))
            .await;
    }

    let all = node.get("/agents").await["agents"]
        .as_array()
        .unwrap()
        .len();
    let engineering = node.get("/agents?tag=engineering").await;
    let filtered = engineering["agents"].as_array().unwrap();
    assert!(
        !filtered.is_empty(),
        "the catalog files agents under engineering"
    );
    assert!(
        filtered.len() < all,
        "a filter narrows: {} of {all}",
        filtered.len()
    );
    for a in filtered {
        assert!(
            a["tags"]
                .as_array()
                .unwrap()
                .contains(&json!("engineering")),
            "every row carries the tag it was filtered by: {a}"
        );
    }

    // `any` is the default and the wider answer; `all` narrows it. Both
    // spellings of a multi-tag filter agree.
    let any = node.get("/agents?tag=engineering&tag=code").await["agents"]
        .as_array()
        .unwrap()
        .len();
    let all_of = node.get("/agents?tag=engineering,code&match=all").await;
    let all_of = all_of["agents"].as_array().unwrap();
    assert!(
        all_of.len() < any,
        "match=all is narrower than match=any: {} vs {any}",
        all_of.len()
    );
    for a in all_of {
        let tags = a["tags"].as_array().unwrap();
        assert!(
            tags.contains(&json!("engineering")) && tags.contains(&json!("code")),
            "{a}"
        );
    }
    assert_eq!(
        node.get("/agents?tag=engineering,code").await["agents"]
            .as_array()
            .unwrap()
            .len(),
        any,
        "a comma-separated list and a repeated parameter are the same filter"
    );

    // A tag that cannot be a tag is a refusal naming it, never an empty list:
    // the two are indistinguishable to a caller otherwise.
    let (code, v) = node.req("GET", "/agents?tag=!nope", None).await;
    assert_eq!(code, 400);
    assert!(v["error"].as_str().unwrap().contains("!nope"), "{v}");
    let (code, _) = node.req("GET", "/agents?tag=code&match=some", None).await;
    assert_eq!(code, 400, "match takes any or all");

    // The same filter answers on every list, and an unmatched tag is an empty
    // list rather than an error.
    for path in [
        "/teams",
        "/channels",
        "/skills",
        "/goals",
        "/workflows",
        "/mcp",
    ] {
        let (code, v) = node
            .req("GET", &format!("{path}?tag=no-such-category"), None)
            .await;
        assert_eq!(code, 200, "{path}: {v}");
    }
    assert!(!node.get("/skills?tag=engineering").await["skills"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(!node.get("/teams?tag=engineering").await["teams"]
        .as_array()
        .unwrap()
        .is_empty());

    // The facets are what a filter bar is built from, and they agree with
    // what the list routes actually return.
    let v = node.get("/tags?entity=agent").await;
    let facets = v["tags"].as_array().unwrap();
    assert_eq!(v["entity"], json!("agent"));
    let counted = facets
        .iter()
        .find(|f| f["tag"] == json!("engineering"))
        .expect("engineering is in use");
    assert_eq!(
        counted["total"].as_u64().unwrap() as usize,
        filtered.len(),
        "a facet count and its list route answer the same question"
    );
    assert_eq!(counted["entities"][0]["entity"], json!("agent"));
    let totals: Vec<u64> = facets
        .iter()
        .map(|f| f["total"].as_u64().unwrap())
        .collect();
    assert!(totals.windows(2).all(|w| w[0] >= w[1]), "most-used first");

    // Unnarrowed, the facets span every kind that files anything.
    let v = node.get("/tags").await;
    assert_eq!(v["entity"], json!(null));
    let kinds: std::collections::HashSet<String> = v["tags"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|f| f["entities"].as_array().unwrap())
        .map(|e| e["entity"].as_str().unwrap().to_string())
        .collect();
    for kind in ["agent", "skill", "team", "channel"] {
        assert!(kinds.contains(kind), "the catalog files {kind}s: {kinds:?}");
    }
    let (code, _) = node.req("GET", "/tags?entity=nonsense", None).await;
    assert_eq!(code, 400);

    node.shutdown().await;
}

// ---------------------------------------------------------------------------
// Channel rosters: a directory, not a subscription
// ---------------------------------------------------------------------------

/// A channel handle addresses the whole roster in one token — and a message
/// with no mentions addresses nobody at all, however full the roster is.
///
/// The second half is the anti-stampede rule. Five rostered agents must never
/// mean five harness sessions per message, and the way that is guaranteed is
/// that a roster never becomes a `p` tag on its own. The enforcement that
/// turns a `p` tag into a session lives in
/// `bisa-engine/src/conversation.rs::dispatch`; what this asserts is the
/// input it is given — an unaddressed message addresses nobody.
#[tokio::test(flavor = "multi_thread")]
async fn a_channel_roster_addresses_nobody_until_it_is_addressed() {
    let node = Node::start().await;

    // A roster needs agents, and a fresh workspace has one. These two come
    // from the catalog, under their bare slugs.
    let roster = ["developer", "qa-engineer"];
    let mut roster_keys = Vec::new();
    for id in roster {
        node.post("/catalog/install", json!({"kind": "agent", "slug": id}))
            .await;
        let v = node.get(&format!("/agents/{id}")).await;
        roster_keys.push(v["agent"]["pubkey"].as_str().unwrap().to_string());
    }

    let v = node
        .post(
            "/channels",
            json!({"name": "shipping", "topic": "what is going out",
                   "agents": roster, "tags": ["engineering"]}),
        )
        .await;
    let channel = v["channel"]["id"].as_str().unwrap().to_string();
    assert_eq!(
        v["channel"]["roster"]["agents"].as_array().unwrap().len(),
        2
    );
    let v = node.get(&format!("/channels/{channel}")).await;
    assert!(
        v["roster_pubkeys"].as_array().unwrap().len() >= roster.len(),
        "the header needs the keys the handle expands to: {v}"
    );
    assert_eq!(
        v["members"].as_array().unwrap().len(),
        roster.len(),
        "the members are the roster, derived: {v}"
    );

    // A roster naming an agent that does not exist is refused, not stored.
    let (code, v) = node
        .req(
            "POST",
            "/channels",
            Some(json!({"name": "ghosts", "agents": ["no-such-agent"]})),
        )
        .await;
    assert_eq!(code, 400, "{v}");

    // The channel's own id is its handle: one token, the whole roster.
    node.post(
        &format!("/channels/{channel}/messages"),
        json!({"content": "status?", "mentions": [channel]}),
    )
    .await;
    for pk in &roster_keys {
        assert!(
            node.ws.mentioned_scopes(pk).unwrap().contains(&channel),
            "the handle expanded to the roster: {pk}"
        );
    }

    // A token that names nothing is a refusal: a mention that quietly
    // resolves to nobody is a message you believe you sent to someone.
    let (code, v) = node
        .req(
            "POST",
            &format!("/channels/{channel}/messages"),
            Some(json!({"content": "hello", "mentions": ["nobody-at-all"]})),
        )
        .await;
    assert_eq!(code, 400, "{v}");

    // The anti-stampede rule: a full roster, no mentions, nobody addressed.
    let v = node
        .post("/channels", json!({"name": "quiet", "agents": roster}))
        .await;
    let quiet = v["channel"]["id"].as_str().unwrap().to_string();
    node.post(
        &format!("/channels/{quiet}/messages"),
        json!({"content": "thinking out loud"}),
    )
    .await;
    for pk in &roster_keys {
        assert!(
            !node.ws.mentioned_scopes(pk).unwrap().contains(&quiet),
            "a roster is a directory, not a subscription — {pk} was addressed \
             by a message that mentioned nobody"
        );
    }

    // Editing a standing channel: topic, roster and tags, each optional.
    let (code, v) = node
        .req(
            "PATCH",
            &format!("/channels/{channel}"),
            Some(json!({"agents": ["developer"], "tags": ["engineering", "code"]})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["channel"]["roster"]["agents"], json!(["developer"]));
    assert_eq!(v["channel"]["tags"], json!(["code", "engineering"]));
    assert_eq!(
        v["channel"]["topic"],
        json!("what is going out"),
        "an omitted patch field holds"
    );

    // A DM has neither a roster nor a topic, and says so rather than failing.
    // Its other party is a person of this workspace first.
    let other = nostr::key::Keys::generate().public_key().to_hex();
    node.post(
        "/workspace/people",
        json!({"pubkey": other, "label": "bob", "role": "member"}),
    )
    .await;
    let dm = node.post("/dms", json!({"members": [other]})).await;
    let dm = dm["channel"]["id"].as_str().unwrap().to_string();
    let (code, v) = node
        .req(
            "PATCH",
            &format!("/channels/{dm}"),
            Some(json!({"topic": "x"})),
        )
        .await;
    assert_eq!(code, 400, "{v}");

    node.shutdown().await;
}

// ---------------------------------------------------------------------------
// Operator surface
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn admin_surface() {
    let node = Node::start().await;
    let id = node.new_goal("keep the service up").await;

    // Governance: owner-only by default, widened to the members.
    let v = node.get("/governance").await;
    assert_eq!(v["governance"]["approval"]["policy"], json!("owner"));
    let v = node
        .req(
            "PUT",
            "/governance",
            Some(json!({"approval": {"policy": "members"}})),
        )
        .await;
    assert_eq!(v.0, 200);
    assert_eq!(v.1["governance"]["approval"]["policy"], json!("members"));
    assert_eq!(
        v.1["governance"]["escalation"]["policy"],
        json!("owner"),
        "untouched"
    );
    assert_eq!(v.1["governance"]["publish"]["policy"], json!("owner"));
    // The three gates are the whole vocabulary: a fourth is refused.
    let (code, _) = node
        .req(
            "PUT",
            "/governance",
            Some(json!({"commit": {"policy": "members"}})),
        )
        .await;
    assert_eq!(code, 400, "an unknown gate is refused, not ignored");

    // Search finds the goal by its statement.
    let v = node.get("/search?q=service").await;
    assert!(
        v["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == json!(id.to_string())),
        "{v}"
    );

    // People: admit a person by hand, change their role, remove them.
    let person = nostr::key::Keys::generate().public_key().to_hex();
    let v = node
        .post(
            "/workspace/people",
            json!({"pubkey": person, "label": "bob", "role": "member"}),
        )
        .await;
    assert_eq!(v["people"].as_array().unwrap().len(), 1);
    assert_eq!(v["person"]["role"], json!("member"));
    let v = node.get("/workspace").await;
    assert_eq!(
        v["members"].as_array().unwrap().len(),
        2,
        "the owner and the person"
    );
    let (code, v) = node
        .req(
            "PUT",
            &format!("/workspace/people/{person}/role"),
            Some(json!({"role": "guest"})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["person"]["role"], json!("guest"));
    let (code, _) = node
        .req(
            "PUT",
            &format!("/workspace/people/{person}/role"),
            Some(json!({"role": "owner"})),
        )
        .await;
    assert_eq!(code, 400, "never a second owner");
    let (code, v) = node
        .req("DELETE", &format!("/workspace/people/{person}"), None)
        .await;
    assert_eq!(code, 200);
    assert!(v["people"].as_array().unwrap().is_empty());
    let (code, _) = node
        .req("POST", "/workspace/people", Some(json!({"pubkey": "nope"})))
        .await;
    assert_eq!(code, 400);

    // Pause / resume — the engine freeze the UI can finally reach, and the
    // read half a screen opening mid-pause needs.
    assert_eq!(node.get("/pause").await["paused"], json!(false));
    assert_eq!(node.post("/pause", json!({})).await["paused"], json!(true));
    assert_eq!(node.get("/pause").await["paused"], json!(true));
    assert_eq!(
        node.get("/node").await["paused"],
        json!(true),
        "the node's own description reads the flag too"
    );
    assert_eq!(
        node.post("/resume", json!({})).await["paused"],
        json!(false)
    );

    // Transcripts: an unknown session is a 404, not a panic.
    let (code, _) = node.req("GET", "/sessions/unknown/transcript", None).await;
    assert_eq!(code, 404);

    // A missing captured result is a 404 too — and so is an item nobody has.
    let item = WorkItemId::from_ulid(ulid::Ulid::from_datetime(std::time::SystemTime::now()));
    let (code, _) = node
        .req("GET", &format!("/work-items/{item}/result"), None)
        .await;
    assert_eq!(code, 404);

    // Workspace info, and the wire without a pump in this process.
    let v = node.get("/workspace").await;
    assert!(v["npub"].as_str().unwrap().starts_with("npub1"));
    assert!(
        v.get("relays").is_none(),
        "relays are the `sync.relays` setting"
    );
    let v = node.get("/sync").await;
    assert_eq!(v["running"], json!(false));
    assert_eq!(v["enabled"], json!(false), "off by default");
    let rows = v["relays"].as_array().unwrap();
    assert_eq!(
        rows.len(),
        4,
        "the four default relays are listed even without a pump: {v}"
    );
    assert!(rows
        .iter()
        .all(|r| r["status"] == json!("off") && r["connected"] == json!(false)));
    assert_eq!(rows[0]["url"], json!("wss://relay.nostr.com"));
    let (code, _) = node
        .req(
            "POST",
            "/sync/relays/check",
            Some(json!({"url": "http://nope"})),
        )
        .await;
    assert_eq!(code, 400, "a relay is a ws(s) URL");
    let (code, _) = node
        .req(
            "POST",
            "/sync/relays/check",
            Some(json!({"url": "wss://relay.example"})),
        )
        .await;
    assert_eq!(code, 409, "nobody to ask without a pump");

    // Harness catalog: shape only (what is installed varies by machine).
    let v = node.get("/harnesses").await;
    let rows = v["harnesses"].as_array().unwrap();
    assert!(!rows.is_empty());
    assert!(rows
        .iter()
        .all(|r| r["id"].is_string() && r["installed"].is_boolean()));
    // `launch` is what lets the desktop shell offer a harness in a terminal.
    // It is a description of a command; nothing in the node runs it. A row
    // either carries a program or is explicitly null — never a bare string the
    // caller would have to parse.
    for r in rows {
        let launch = &r["launch"];
        assert!(
            launch.is_null() || launch["program"].is_string(),
            "{}: launch is neither absent nor a program: {launch}",
            r["id"]
        );
    }

    node.shutdown().await;
}

// ---------------------------------------------------------------------------
// Work-item detail
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn work_item_detail() {
    let node = Node::start().await;
    let id = node.new_goal("detail me").await;
    let item = node.agent_item(id).await;
    let v = node.get(&format!("/work-items/{item}")).await;
    assert_eq!(v["item"]["id"], json!(item));
    assert_eq!(v["home"], json!({"home": "goal", "goal": id.to_string()}));
    assert_eq!(v["label"], json!("T"), "the goal's title");
    assert!(v["result"].is_null(), "nothing yielded yet");
    assert_eq!(v["has_result"], json!(false));
    let (code, _) = node
        .req("GET", &format!("/work-items/{item}/result"), None)
        .await;
    assert_eq!(code, 404, "no patch captured");

    // The list carries the same home and label beside every item.
    let rows = node.get("/work-items").await;
    let row = rows["work_items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["item"]["id"] == json!(item))
        .expect("listed");
    assert_eq!(row["home"], v["home"]);
    assert_eq!(row["label"], json!("T"));

    let (code, _) = node.req("GET", "/work-items/not-a-ulid", None).await;
    assert_eq!(code, 400);

    node.shutdown().await;
}

// ---------------------------------------------------------------------------
// Assignees
// ---------------------------------------------------------------------------

/// Every wire form `PUT /goals/{id}/assignees` accepts, and every one it
/// refuses.
///
/// This is the only test of the `human:<64 hex>` form and of the three ways a
/// list is rejected — its nearest sibling covers `team:` alone.
#[tokio::test(flavor = "multi_thread")]
async fn assignees_accept_every_wire_form_and_refuse_the_rest() {
    let node = Node::start().await;
    let parent = node.new_goal("something to assign").await;

    // A human assignee needs no prior registration: they are a bare pubkey.
    let human = format!("human:{}", "ab".repeat(32));
    let (code, v) = node
        .req(
            "PUT",
            &format!("/goals/{parent}/assignees"),
            Some(json!({"assignees": [human]})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["goal"]["assignees"][0]["human"], json!("ab".repeat(32)));

    // The wire form is the only accepted grammar; a bare id is ambiguous.
    let (code, _) = node
        .req(
            "PUT",
            &format!("/goals/{parent}/assignees"),
            Some(json!({"assignees": ["just-a-name"]})),
        )
        .await;
    assert_eq!(code, 400);
    // An unknown agent is refused too, rather than silently stored.
    let (code, _) = node
        .req(
            "PUT",
            &format!("/goals/{parent}/assignees"),
            Some(json!({"assignees": ["agent:nope"]})),
        )
        .await;
    assert_eq!(code, 400);

    // Clearing works.
    let (code, v) = node
        .req(
            "PUT",
            &format!("/goals/{parent}/assignees"),
            Some(json!({"assignees": []})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
    assert!(v["goal"]["assignees"]
        .as_array()
        .is_none_or(|a| a.is_empty()));

    node.shutdown().await;
}

// ---------------------------------------------------------------------------
// SSE
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn sse_streams_a_tagged_envelope() {
    let node = Node::start().await;
    let id = node.new_goal("watch me").await;
    node.ask(id).await;

    let socket = node.socket.clone();
    let sse = tokio::spawn(async move {
        let stream = UnixStream::connect(&socket).await.expect("sse connect");
        let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
            .await
            .expect("sse handshake");
        tokio::spawn(conn);
        let request = Request::builder()
            .method("GET")
            .uri("/events")
            .header(hyper::header::HOST, "localhost")
            .header(hyper::header::AUTHORIZATION, format!("Bearer {TOKEN}"))
            .body(Full::new(Bytes::new()))
            .unwrap();
        let resp = sender.send_request(request).await.expect("sse request");
        assert_eq!(resp.status().as_u16(), 200);
        let mut body = resp.into_body();
        let mut buf = String::new();
        while let Some(Ok(frame)) = body.frame().await {
            if let Some(data) = frame.data_ref() {
                buf.push_str(&String::from_utf8_lossy(data));
                if buf.contains("\"stream\"") {
                    return buf;
                }
            }
        }
        buf
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let run = node.current_run(id).await;
    node.post(
        &format!("/runs/{run}/steps/ask/answer"),
        json!({"answer": {"selected": ["sqlite"]}}),
    )
    .await;

    let buf = tokio::time::timeout(Duration::from_secs(10), sse)
        .await
        .expect("sse timed out")
        .expect("sse task");
    assert!(buf.contains("\"stream\":\"engine\""), "envelope: {buf}");
    assert!(
        buf.contains("gate_decided")
            || buf.contains("step_changed")
            || buf.contains("run_finished"),
        "payload: {buf}"
    );

    node.shutdown().await;
}

/// A stream is open for as long as the node is, and no longer: asked to
/// stop, the node ends every stream it holds — the listener reads the end —
/// and stops. A stop that waited on its listeners would wait for as long as
/// a desktop stays open.
#[tokio::test(flavor = "multi_thread")]
async fn a_node_asked_to_stop_ends_the_streams_it_holds_and_stops() {
    let node = Node::start().await;
    let open = |socket: PathBuf| async move {
        let stream = UnixStream::connect(&socket).await.expect("sse connect");
        let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
            .await
            .expect("sse handshake");
        tokio::spawn(conn);
        let request = Request::builder()
            .method("GET")
            .uri("/events")
            .header(hyper::header::HOST, "localhost")
            .header(hyper::header::AUTHORIZATION, format!("Bearer {TOKEN}"))
            .body(Full::new(Bytes::new()))
            .unwrap();
        let resp = sender.send_request(request).await.expect("sse request");
        assert_eq!(resp.status().as_u16(), 200);
        resp.into_body()
    };
    let mut first = open(node.socket.clone()).await;
    let mut second = open(node.socket.clone()).await;
    // Something is said, so both streams are being read when the stop comes.
    node.new_goal("watched while it stops").await;
    let said = tokio::time::timeout(Duration::from_secs(10), first.frame())
        .await
        .expect("a frame in time");
    assert!(
        matches!(said, Some(Ok(_))),
        "the stream carries what happens"
    );

    // `shutdown` holds the node to stopping within its own bound.
    node.shutdown().await;

    for body in [&mut first, &mut second] {
        let ended = tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(frame) = body.frame().await {
                if frame.is_err() {
                    break;
                }
            }
        })
        .await;
        assert!(ended.is_ok(), "the listener reads the end of its stream");
    }
}

// ---------------------------------------------------------------------------
// The goal's mode
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn manual_goals_opt_out_of_guidance() {
    let node = Node::start().await;
    let v = node
        .post(
            "/goals",
            json!({"statement": "I'll drive", "mode": "manual"}),
        )
        .await;
    let goal: Goal = serde_json::from_value(v["goal"].clone()).unwrap();
    assert_eq!(goal.mode, bisa_core::GoalMode::Manual);
    let v = node.get(&format!("/goals/{}", goal.id)).await;
    assert_eq!(v["guidance"]["mode"], json!("manual"));
    assert_eq!(
        v["guidance"]["phase"],
        json!("draft"),
        "nobody designs for a manual goal; the phase is its status"
    );
    // A word that is not a mode is refused at the body.
    let (code, v) = node
        .req(
            "POST",
            "/goals",
            Some(json!({"statement": "interactive?", "mode": "interactive"})),
        )
        .await;
    assert_eq!(code, 400, "{v}");
    node.shutdown().await;
}

/// A capture that names no mode takes `goals.default_mode`; the row and the
/// guidance carry the mode the goal got.
#[tokio::test(flavor = "multi_thread")]
async fn a_capture_without_a_mode_takes_the_workspace_default() {
    let node = Node::start().await;
    let (code, v) = node
        .req(
            "PUT",
            "/settings/workspace",
            Some(json!({"values": {"goals.default_mode": "guided"}})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
    let v = node
        .post("/goals", json!({"statement": "as the workspace likes"}))
        .await;
    let goal: Goal = serde_json::from_value(v["goal"].clone()).unwrap();
    assert_eq!(goal.mode, bisa_core::GoalMode::Guided);
    let rows = node.get("/goals").await;
    let row = rows["goals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == json!(goal.id.to_string()))
        .expect("the goal is listed");
    assert_eq!(row["mode"], json!("guided"));
    // The capture's own word wins over the default.
    let v = node
        .post("/goals", json!({"statement": "no, auto", "mode": "auto"}))
        .await;
    assert_eq!(v["goal"]["mode"], json!("auto"));
    node.shutdown().await;
}

/// A team can be put on a goal and taken off again, and the team's detail
/// shows what it is carrying — the relation the Teams UI has been promising.
#[tokio::test]
async fn team_assignment_round_trip() {
    let node = Node::start().await;

    let agent = node
        .post(
            "/agents",
            json!({"name": "Hand", "system_prompt": "work", "harness": "claude-code"}),
        )
        .await["agent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let team = node
        .post(
            "/teams",
            json!({"name": "Delivery", "members": [{"agent": agent}]}),
        )
        .await["team"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let goal = node
        .post("/goals", json!({"statement": "carry me", "mode": "manual"}))
        .await["goal"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Nothing carried yet.
    let v = node.get(&format!("/teams/{team}")).await;
    assert!(v["goals"].as_array().unwrap().is_empty());

    let (code, v) = node
        .req(
            "PUT",
            &format!("/goals/{goal}/assignees"),
            Some(json!({"assignees": [format!("team:{team}")]})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["goal"]["assignees"], json!([{"team": team}]));

    // The board row and the team detail both know.
    let rows = node.get("/goals").await;
    let row = rows["goals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == json!(goal))
        .unwrap()
        .clone();
    assert_eq!(row["assignees"], json!([{"team": team}]));
    let v = node.get(&format!("/teams/{team}")).await;
    assert_eq!(v["goals"].as_array().unwrap().len(), 1);
    assert_eq!(v["goals"][0]["id"], json!(goal));

    // An unknown team is refused, not silently stored.
    let (code, _) = node
        .req(
            "PUT",
            &format!("/goals/{goal}/assignees"),
            Some(json!({"assignees": ["team:nope".to_string()]})),
        )
        .await;
    assert!(code >= 400, "unknown team must be refused");

    // An empty list clears it.
    let (code, v) = node
        .req(
            "PUT",
            &format!("/goals/{goal}/assignees"),
            Some(json!({"assignees": []})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
    assert!(v["goal"]["assignees"]
        .as_array()
        .is_none_or(|a| a.is_empty()));
    assert!(node.get(&format!("/teams/{team}")).await["goals"]
        .as_array()
        .unwrap()
        .is_empty());
}

// ---------------------------------------------------------------------------
// Projects and workstreams
//
// **Every repository here is created by the test inside a `tempfile`
// directory and dies with it.** "origin" is a local bare repository addressed
// by its path, so the whole push path runs with no network, and GitHub is
// never reached — the pull-request tests run against an in-memory
// `FakeCodeHost` that claims the bare origin by name. No user repository is
// touched and no real pull request is created.
// ---------------------------------------------------------------------------

/// Raw git, for the fixture steps the platform deliberately does not expose
/// (`init --bare`, a repository-local identity, the first commit) and for
/// checking git's own view independently of ours.
fn raw_git(dir: &std::path::Path, args: &[&str]) -> String {
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
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Identity goes in the repository's *local* config: a global write would be
/// a side effect on the developer's machine.
fn set_identity(repo: &std::path::Path) {
    raw_git(repo, &["config", "user.name", "Bisa Test"]);
    raw_git(repo, &["config", "user.email", "test@example.invalid"]);
}

fn origin_branches(origin: &std::path::Path) -> Vec<String> {
    raw_git(
        origin,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
    )
    .lines()
    .map(str::to_string)
    .collect()
}

pub(crate) async fn until<T>(what: &str, mut probe: impl FnMut() -> Option<T>) -> T {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(v) = probe() {
            return v;
        }
        assert!(std::time::Instant::now() < deadline, "timed out: {what}");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

impl Node {
    /// One request with no `Authorization` at all — what a browser sent back
    /// to the OAuth callback presents, and what every other route refuses.
    pub(crate) async fn req_without_token(&self, method: &str, path: &str) -> (u16, Value) {
        let stream = UnixStream::connect(&self.socket).await.expect("connect");
        let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
            .await
            .expect("handshake");
        tokio::spawn(conn);
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header(hyper::header::HOST, "localhost")
            .body(Full::new(Bytes::new()))
            .unwrap();
        let resp = sender.send_request(request).await.expect("request");
        let status = resp.status().as_u16();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    pub(crate) async fn put(&self, path: &str, body: Value) -> Value {
        let (code, v) = self.req("PUT", path, Some(body)).await;
        assert_eq!(code, 200, "PUT {path} failed: {v}");
        v
    }

    pub(crate) async fn delete(&self, path: &str) -> Value {
        let (code, v) = self.req("DELETE", path, None).await;
        assert_eq!(code, 200, "DELETE {path} failed: {v}");
        v
    }

    /// A git project with one commit — the state a workstream can branch from.
    /// The first commit is made with raw git because the platform has no
    /// route for it: `git init` is as far as `POST /projects` goes.
    async fn git_project(&self, goal: &str, slug: &str, publish: &str) -> (String, PathBuf) {
        let v = self
            .post(
                &format!("/goals/{goal}/projects"),
                json!({"kind": "new", "slug": slug, "publish": publish}),
            )
            .await;
        let pid = v["project"]["id"].as_str().unwrap().to_string();
        let root = PathBuf::from(v["path"].as_str().unwrap());
        set_identity(&root);
        std::fs::write(root.join("README.md"), "baseline\n").unwrap();
        raw_git(&root, &["add", "-A"]);
        raw_git(&root, &["commit", "-m", "baseline", "--quiet"]);
        (pid, root)
    }

    /// A bare repository standing in for `origin`, wired to `root`.
    /// A bare origin for `root`, whose HEAD names the branch `root` is on:
    /// the node made `root` with git's own default — `init.defaultBranch` on
    /// this machine, `master` without one — and the sealed git here knows no
    /// such setting, so an origin made by its default alone could name a
    /// branch never pushed; a clone of it would then have no such branch to
    /// push back.
    fn origin_for(&self, root: &std::path::Path, slug: &str) -> PathBuf {
        let branch = raw_git(root, &["symbolic-ref", "--short", "HEAD"]);
        let origin = self.ws.root().join(format!("{slug}-origin.git"));
        std::fs::create_dir_all(&origin).unwrap();
        raw_git(
            &origin,
            &["init", "--bare", "--quiet", "--initial-branch", &branch],
        );
        raw_git(root, &["remote", "add", "origin", origin.to_str().unwrap()]);
        origin
    }
}

/// Create a project, look at its git state, open a workstream in it, commit
/// there, and close it — the whole loop over HTTP, with nothing but the
/// node's own routes.
#[tokio::test(flavor = "multi_thread")]
async fn project_lifecycle_over_http() {
    let node = Node::start().await;
    let goal = node.new_goal("own some code").await.to_string();

    // A managed root is a folder this node made, so it arrives as a
    // repository — no flag to pass and none to forget.
    let v = node
        .post(
            &format!("/goals/{goal}/projects"),
            json!({"kind": "new", "slug": "notes"}),
        )
        .await;
    let notes = PathBuf::from(v["path"].as_str().unwrap());
    assert!(notes.is_dir(), "the folder is created");
    assert!(
        notes.join(".git").is_dir(),
        "a managed root is initialised without being asked"
    );
    assert_eq!(v["project"]["vcs"]["type"], json!("git"));
    let notes_id = v["project"]["id"].as_str().unwrap().to_string();

    // Its status reads clean rather than erroring: a repository with an
    // unborn HEAD has no commits and nothing dirty.
    let v = node
        .get(&format!("/workstreams/{notes_id}/git/status"))
        .await;
    assert_eq!(v["status"]["git"], json!(true));
    assert_eq!(v["status"]["exists"], json!(true));
    assert_eq!(v["status"]["clean"], json!(true));
    assert_eq!(v["status"]["head"], Value::Null, "nothing committed yet");

    // Now one with a first commit, which is what a workstream can branch from.
    let (pid, root) = node.git_project(&goal, "storefront", "gated").await;
    assert!(root.join(".git").exists());
    let v = node.get(&format!("/workstreams/{pid}/git/status")).await;
    assert_eq!(v["status"]["git"], json!(true));
    assert_eq!(v["status"]["clean"], json!(true));
    assert!(v["status"]["branch"].as_str().is_some());
    assert_eq!(v["status"]["remote"], Value::Null, "no origin yet");

    // Open a workstream: a branch and a checkout, under the project's own
    // `workstreams/`, never inside its tree.
    let v = node
        .post(
            &format!("/projects/{pid}/workstreams"),
            json!({"label": "cart total"}),
        )
        .await;
    let wid = v["workstream"]["id"].as_str().unwrap().to_string();
    let wpath = PathBuf::from(v["path"].as_str().unwrap());
    assert_eq!(v["workstream"]["kind"]["kind"], json!("worktree"));
    let branch = v["workstream"]["kind"]["branch"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(branch.starts_with("work/cart-total"), "branch: {branch}");
    assert!(wpath.is_dir(), "the checkout exists");
    assert!(
        !wpath.starts_with(&root),
        "workstreams live beside the tree, never inside it: {}",
        wpath.display()
    );
    assert_eq!(
        v["workstream"]["goal"],
        Value::Null,
        "a workstream opened by hand with no goal named is for no goal"
    );
    assert_eq!(
        v["workstream"]["work_item"],
        Value::Null,
        "a workstream opened by hand has no work item to attribute to"
    );
    assert!(wpath.join("README.md").exists(), "branched from the base");

    // A clean workstream has an empty diff and says so.
    let v = node.get(&format!("/workstreams/{wid}/diff")).await;
    assert_eq!(v["clean"], json!(true));
    assert_eq!(v["diff"], json!(""));

    // Do some work in it.
    set_identity(&wpath);
    std::fs::write(wpath.join("cart.rs"), "fn total() {}\n").unwrap();
    let v = node.get(&format!("/workstreams/{wid}/diff")).await;
    assert_eq!(v["clean"], json!(false), "an untracked file is a change");
    // A brand-new file cannot appear in `git diff` without staging it, and a
    // GET does not stage: it is reported as untracked instead of silently
    // vanishing from the answer.
    assert_eq!(v["untracked"], json!(["cart.rs"]));
    assert_eq!(v["diff"], json!(""), "nothing tracked has changed yet");
    std::fs::write(wpath.join("README.md"), "baseline + cart\n").unwrap();
    let v = node.get(&format!("/workstreams/{wid}/diff")).await;
    assert!(v["diff"].as_str().unwrap().contains("README.md"));
    let files: Vec<String> = v["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["path"].as_str().unwrap().to_string())
        .collect();
    assert!(files.contains(&"README.md".to_string()), "{files:?}");

    // The detail view carries branch, base and distance travelled.
    let v = node.get(&format!("/workstreams/{wid}")).await;
    assert_eq!(v["branch"], json!(branch));
    assert_eq!(v["ahead_of_base"], json!(0), "nothing committed yet");
    assert_eq!(v["status"]["untracked"], json!(1));

    // Commit through the platform.
    let v = node
        .post(
            &format!("/workstreams/{wid}/commit"),
            json!({"message": "add the cart total"}),
        )
        .await;
    assert!(!v["short"].as_str().unwrap().is_empty());
    assert_eq!(v["workstream"]["state"]["state"], json!("committed"));
    let v = node.get(&format!("/workstreams/{wid}")).await;
    assert_eq!(v["ahead_of_base"], json!(1), "one commit ahead of base");
    assert_eq!(v["status"]["clean"], json!(true));

    // A clean tree refuses by name rather than making an empty commit.
    let (code, v) = node
        .req(
            "POST",
            &format!("/workstreams/{wid}/commit"),
            Some(json!({"message": "again"})),
        )
        .await;
    assert_eq!(code, 409, "{v}");
    assert!(v["error"].as_str().unwrap().contains("nothing to commit"));

    // The project lists its workstreams: the primary first, then the branch.
    let v = node.get(&format!("/projects/{pid}/workstreams")).await;
    let rows = v["workstreams"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["kind"]["kind"], json!("primary"));
    assert_eq!(
        rows[0]["id"],
        json!(pid),
        "the primary's id is the project's"
    );
    assert_eq!(rows[1]["id"], json!(wid));
    assert_eq!(v["checkouts"][0]["path"], json!(root.display().to_string()));

    // Closing keeps the checkout unless asked; then removes it.
    let v = node.delete(&format!("/workstreams/{wid}")).await;
    assert_eq!(v["workstream"]["state"]["state"], json!("closed"));
    assert_eq!(
        v["stopped_sessions"],
        json!(0),
        "a close says how many sessions it stopped: {v}"
    );
    assert!(wpath.is_dir(), "the checkout survives a plain close");
    let v = node.delete(&format!("/workstreams/{wid}?tree=true")).await;
    assert_eq!(v["removed_tree"], json!(true));
    assert!(!wpath.exists(), "?tree=true removes the checkout");

    node.shutdown().await;
}

/// Everything about a folder that a copy could disturb: every entry's
/// relative path, what kind of thing it is, its permission bits and its exact
/// bytes.
///
/// Taken before and after an import, an equal snapshot is the assertion the
/// whole feature rests on — that importing somebody's folder only *reads* it.
/// Comparing a file count or a `mtime` would pass for a copy that quietly
/// rewrote a line.
fn tree_snapshot(root: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    fn walk(base: &std::path::Path, dir: &std::path::Path, out: &mut Vec<(String, Vec<u8>)>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
            .map(|e| e.unwrap())
            .collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            let rel = path.strip_prefix(base).unwrap().display().to_string();
            let kind = entry.file_type().unwrap();
            if kind.is_symlink() {
                let target = std::fs::read_link(&path).unwrap();
                out.push((
                    format!("{rel} symlink"),
                    target.display().to_string().into_bytes(),
                ));
            } else if kind.is_dir() {
                out.push((format!("{rel} dir"), Vec::new()));
                walk(base, &path, out);
            } else {
                #[cfg(unix)]
                let mode = {
                    use std::os::unix::fs::PermissionsExt as _;
                    entry.metadata().unwrap().permissions().mode()
                };
                #[cfg(not(unix))]
                let mode = 0;
                out.push((
                    format!("{rel} file {mode:o}"),
                    std::fs::read(&path).unwrap(),
                ));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out
}

/// A git repository outside the workspace, with one commit — the folder
/// somebody points `import` or `adopt` at.
fn outside_repo(subject: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    raw_git(root, &["init", "--quiet"]);
    set_identity(root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(root.join("README.md"), "# storefront\n").unwrap();
    raw_git(root, &["add", "-A"]);
    raw_git(root, &["commit", "-m", subject, "--quiet"]);
    dir
}

/// Importing a folder copies the whole tree — `.git` included — into a
/// **managed** root, and the folder it copied from is byte-for-byte what it
/// was.
///
/// The `.git` half is not incidental. `bisa-iso`'s copy deliberately
/// skips it, which is right for a throwaway isolation tree and wrong here: a
/// repository imported without its history is not that repository, and the
/// `vcs` the record claims would be a guess rather than something read off the
/// copy.
#[tokio::test(flavor = "multi_thread")]
async fn an_import_copies_the_tree_and_leaves_the_source_folder_untouched() {
    let node = Node::start().await;
    let goal = node.new_goal("import some code").await.to_string();
    let src = outside_repo("the history that must survive");
    let source = src.path().to_path_buf();
    let before = tree_snapshot(&source);

    let v = node
        .post(
            &format!("/goals/{goal}/projects"),
            json!({"kind": "import", "slug": "storefront",
                   "path": source.display().to_string()}),
        )
        .await;

    // A managed root inside the workspace — a copy the goal owns, not a
    // reference to somebody's directory.
    assert_eq!(v["project"]["root"]["type"], json!("managed"));
    let dest = PathBuf::from(v["path"].as_str().unwrap());
    assert!(
        dest.starts_with(node.ws.root()),
        "{} is not inside the workspace",
        dest.display()
    );
    assert_ne!(dest, source);

    assert_eq!(
        std::fs::read_to_string(dest.join("src/main.rs")).unwrap(),
        "fn main() {}\n"
    );
    assert!(
        dest.join(".git").is_dir(),
        "an import without the history is not an import"
    );
    assert_eq!(v["project"]["vcs"]["type"], json!("git"));
    assert_eq!(
        raw_git(&dest, &["log", "--format=%s"]),
        "the history that must survive",
        "the copy is a repository with its history intact"
    );
    assert!(v["imported"]["files"].as_u64().unwrap() >= 2);

    // The one assertion the whole feature rests on.
    assert_eq!(
        tree_snapshot(&source),
        before,
        "an import only reads the folder it was given"
    );

    node.shutdown().await;
}

/// **The seam.** A project an agent makes is a project you can see.
///
/// `core_agent.rs` drives the intake ops and
/// stops at the store; every `GET /projects` test in this file builds its
/// fixtures over HTTP. So the two halves of "an agent created a project and
/// the desktop lists it" were each covered and the join between them was not
/// — which is exactly where the bug lived: the record was written correctly
/// the whole time, and the screen never heard about it.
///
/// This drives the op through the engine's own socket, the door a harness
/// session's `create_project` tool speaks through, and then reads the result
/// back over the HTTP control plane the desktop uses. Two surfaces, one
/// workspace, no shared fixture.
/// The two halves of "an agent wrote in my note and I can see it".
///
/// Each half is covered on its own — the store tests prove `append_note`, the
/// route tests prove `GET /notes/{id}` — and the join between them is exactly
/// where a bug would live: an id spelled differently on the two sides, or an
/// append that lands in a directory the read never looks in.
#[tokio::test(flavor = "multi_thread")]
async fn a_note_an_agent_appends_to_is_readable_over_http() {
    let node = Node::start().await;
    let goal = node.new_goal("the auth rework").await;

    let made = node
        .post(
            "/notes",
            json!({"scope": "goal", "id": goal.to_string(),
                   "title": "auth flow", "body": "the token is short-lived"}),
        )
        .await;
    let id = made["note"]["id"].as_str().unwrap().to_string();
    assert_eq!(made["note"]["scope"], json!("goal"));

    // The agent's door: the intake socket, exactly as a session speaks.
    let reply = node
        .intake_op(json!({"op": "note_append", "note": id,
                          "agent": "developer", "text": "refresh uses the cookie"}))
        .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");

    let read = node.get(&format!("/notes/{id}")).await;
    let body = read["note"]["body"].as_str().unwrap();
    assert!(body.contains("the token is short-lived"), "{body}");
    assert!(body.contains("refresh uses the cookie"), "{body}");
    // Attributed, so a reader can tell which half they wrote.
    assert!(body.contains("**developer**"), "{body}");

    // And the hash moved with it, so an editor holding the old one is refused
    // rather than being allowed to overwrite what the agent added.
    let stale = made["note"]["hash"].as_str().unwrap();
    let (status, _) = node
        .req(
            "PATCH",
            &format!("/notes/{id}"),
            Some(json!({"body": "mine only", "base_hash": stale})),
        )
        .await;
    assert_eq!(status, 409, "a stale edit must not silently win");

    // The 409 carries what is there now, which is what the client merges from.
    let (_, conflict) = node
        .req(
            "PATCH",
            &format!("/notes/{id}"),
            Some(json!({"body": "mine only", "base_hash": stale})),
        )
        .await;
    assert!(
        conflict["current"]
            .as_str()
            .unwrap_or_default()
            .contains("refresh uses the cookie"),
        "{conflict}"
    );

    // Deleted, the note is not found — read, edited or deleted again — which
    // is what a drawer standing on it reads as gone; an id that is no id is
    // a request that was wrong.
    let (status, v) = node.req("DELETE", &format!("/notes/{id}"), None).await;
    assert_eq!(status, 200, "{v}");
    for (method, body) in [
        ("GET", None),
        (
            "PATCH",
            Some(json!({"body": "too late", "base_hash": stale})),
        ),
        ("DELETE", None),
    ] {
        let (status, v) = node.req(method, &format!("/notes/{id}"), body).await;
        assert_eq!(status, 404, "{method}: {v}");
    }
    let (status, v) = node.req("GET", "/notes/not-an-id", None).await;
    assert_eq!(status, 400, "{v}");

    node.shutdown().await;
}

/// A write does not announce itself, and somebody else's write does.
///
/// **The invariant the overlay's stability rests on.** `note_changed` means
/// *somebody else wrote*; emitting it for the caller's own `PATCH` made the
/// writer its own audience, and the editor's refetch then replaced the text
/// being typed — which produced another write, and another frame, until the
/// app fell over. The route already returns the note, so the caller needs no
/// event; an agent appending is the case the event exists for.
#[tokio::test(flavor = "multi_thread")]
async fn a_note_write_is_silent_and_an_agent_append_is_not() {
    let mut node = Node::start().await;
    let made = node
        .post(
            "/notes",
            json!({"scope": "workspace", "title": "auth", "body": "mine"}),
        )
        .await;
    let id = made["note"]["id"].as_str().unwrap().to_string();
    let hash = made["note"]["hash"].as_str().unwrap().to_string();

    // Drain whatever creating it produced, so the assertions below are about
    // the two calls under test and not about the fixture.
    while node.bus.try_recv().is_ok() {}

    let patched = node
        .post_patch(
            &format!("/notes/{id}"),
            json!({"body": "mine, edited", "base_hash": hash}),
        )
        .await;
    assert_eq!(patched["note"]["body"], json!("mine, edited"));
    assert!(
        node.bus.try_recv().is_err(),
        "a PATCH announced itself, which is what drove the loop"
    );

    // The agent's door. This one *must* speak, or an appended answer would sit
    // in the file with nothing on screen saying so.
    let reply = node
        .intake_op(json!({"op": "note_append", "note": id,
                          "agent": "developer", "text": "theirs"}))
        .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");

    let mut announced = false;
    while let Ok(e) = node.bus.try_recv() {
        if let bisa_engine::events::EnginePayload::NoteChanged { note, .. } = &e.payload {
            if note == &id {
                announced = true;
            }
        }
    }
    assert!(announced, "an agent's append must reach the overlay");

    node.shutdown().await;
}

// ---------------------------------------------------------------------------
// Pets
// ---------------------------------------------------------------------------

/// Installing somebody else's file format, over the wire.
///
/// The store tests cover what a package may say; this covers the two things
/// only the routes can get wrong — that a folder inside the workspace is
/// refused, and that the sprite is typed from its own bytes rather than from
/// the manifest that named it. The webview has no CSP, so that second one is
/// the difference between an image route and a scripting vector.
#[tokio::test(flavor = "multi_thread")]
async fn a_pet_installs_from_a_folder_and_its_sheet_is_typed_from_its_bytes() {
    let node = Node::start().await;
    // The nine built-ins are there before anything is installed, Moonrice
    // among them, each stamped as shipped and carrying the pack's timing.
    let shipped = node.get("/pets").await["pets"].as_array().cloned().unwrap();
    assert_eq!(shipped.len(), 9, "{shipped:?}");
    assert!(shipped.iter().all(|p| p["origin"] == json!("catalog")));
    let moonrice = shipped
        .iter()
        .find(|p| p["id"] == json!("bisa-pets.midnight-shipping.moonrice"))
        .expect("Moonrice ships");
    assert_eq!(moonrice["displayName"], json!("Moonrice"));
    assert_eq!(moonrice["animations"]["idle"]["frames"], json!(6));
    assert_eq!(
        moonrice["x-bisa-pets"]["tagline"],
        json!("Brought you something warm.")
    );
    let (code, _) = node
        .req(
            "GET",
            "/pets/bisa-pets.midnight-shipping.moonrice/sprite",
            None,
        )
        .await;
    assert_eq!(code, 200, "a built-in's sheet comes from the bundle");
    let (code, _) = node
        .req("DELETE", "/pets/bisa-pets.midnight-shipping.moonrice", None)
        .await;
    assert_eq!(code, 409, "a built-in is put away, never removed");

    // A package somewhere outside the workspace, as a person's would be.
    let src = tempfile::tempdir().unwrap();
    std::fs::write(
        src.path().join("pet.json"),
        r#"{"id":"byte","displayName":"Byte","description":"a small friend","spritesheetPath":"spritesheet.webp"}"#,
    )
    .unwrap();
    let mut sheet = b"RIFF\0\0\0\0WEBPVP8 ".to_vec();
    sheet.extend(std::iter::repeat_n(0u8, 64));
    std::fs::write(src.path().join("spritesheet.webp"), &sheet).unwrap();

    let installed = node
        .post("/pets", json!({"path": src.path().to_string_lossy()}))
        .await;
    assert_eq!(installed["pet"]["id"], json!("byte"));
    // The manifest keeps its own field names on the way out, because the
    // format is somebody else's and this one speaks it.
    assert_eq!(installed["pet"]["displayName"], json!("Byte"));

    let listed = node.get("/pets").await["pets"].as_array().cloned().unwrap();
    assert_eq!(listed.len(), 10, "{listed:?}");
    assert_eq!(
        listed.last().unwrap()["id"],
        json!("byte"),
        "a person's pet lists after the nine"
    );
    assert!(
        listed.last().unwrap().get("origin").is_none(),
        "a local pet says nothing of its origin"
    );

    let (code, _) = node.req("GET", "/pets/byte/sprite", None).await;
    assert_eq!(code, 200);

    node.shutdown().await;
}

/// A package inside the workspace is refused, and so is one that is not there.
#[tokio::test(flavor = "multi_thread")]
async fn a_pet_source_outside_the_workspace_is_required() {
    let node = Node::start().await;

    // Relative, missing, and inside the workspace: three ways to be refused,
    // and the third is the one only this layer can catch — a package copied
    // from inside `pets/` would be a pet installing itself.
    for path in [
        "relative/path".to_string(),
        "/nonexistent/pet".to_string(),
        node.data_dir().display().to_string(),
    ] {
        let (code, body) = node.req("POST", "/pets", Some(json!({"path": path}))).await;
        assert_eq!(code, 400, "{path} -> {body}");
    }

    // Nothing was installed: the nine built-ins are all the list holds.
    let pets = node.get("/pets").await["pets"].clone();
    assert!(
        pets.as_array()
            .unwrap()
            .iter()
            .all(|p| p["origin"] == "catalog"),
        "{pets}"
    );
    node.shutdown().await;
}

/// A note's scope is checked rather than guessed at.
#[tokio::test(flavor = "multi_thread")]
async fn a_note_scope_that_does_not_pair_up_is_refused() {
    let node = Node::start().await;

    // A workspace note names nothing, so an id is a caller mistake worth
    // saying out loud rather than quietly dropping.
    let (status, body) = node
        .req(
            "POST",
            "/notes",
            Some(
                json!({"scope": "workspace", "id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
                        "title": "confused", "body": ""}),
            ),
        )
        .await;
    assert_eq!(status, 400, "{body}");

    let (status, _) = node
        .req(
            "POST",
            "/notes",
            Some(json!({"scope": "goal", "title": "no owner", "body": ""})),
        )
        .await;
    assert_eq!(status, 400, "a goal note with no goal");

    // The workspace scope is the default, and it works with no arguments —
    // the case the overlay hits on every screen that is not a goal.
    let made = node
        .post(
            "/notes",
            json!({"scope": "workspace", "title": "scratch", "body": ""}),
        )
        .await;
    assert_eq!(made["note"]["scope"], json!("workspace"));
    let listed = node.get("/notes?scope=workspace").await["notes"]
        .as_array()
        .cloned()
        .unwrap();
    assert_eq!(listed.len(), 1, "{listed:?}");

    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_project_an_agent_creates_is_listed_over_http() {
    let node = Node::start().await;
    let goal = node.new_goal("needs somewhere to work").await;

    // Nothing yet, so the row below cannot be something a fixture left.
    assert_eq!(node.get("/projects").await["projects"], json!([]));

    let reply = node
        .intake_op(json!({"op": "create_project", "agent": CORE_AGENT_ID,
                          "goal": goal.to_string(), "slug": "storefront"}))
        .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    let pid = reply["project"].as_str().unwrap().to_string();

    // The workspace-wide list — what the projects screen asks for.
    let rows = node.get("/projects").await["projects"]
        .as_array()
        .cloned()
        .unwrap();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["project"]["id"], json!(pid));
    assert_eq!(rows[0]["project"]["slug"], json!("storefront"));
    assert_eq!(rows[0]["goals"], json!([goal.to_string()]));
    assert_eq!(rows[0]["exists"], json!(true));

    // And the per-goal list: the attachments of that goal.
    let mine = node.get(&format!("/goals/{goal}/projects")).await["projects"]
        .as_array()
        .cloned()
        .unwrap();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0]["project"]["id"], json!(pid));

    // The detail route the screen opens next resolves it too, and the folder
    // the agent was told about is a repository on disk.
    let detail = node.get(&format!("/projects/{pid}")).await;
    let root = PathBuf::from(detail["path"].as_str().unwrap());
    assert_eq!(detail["exists"], json!(true));
    assert!(root.join(".git").is_dir(), "{}", root.display());
    assert_eq!(root, PathBuf::from(reply["path"].as_str().unwrap()));

    node.shutdown().await;
}

/// A managed root is a repository; an adopted one is not touched at all.
///
/// The rule stopped being "creating a project makes a directory and stops" and
/// became **never write into a folder you did not create** — so these two
/// halves have to be asserted together, or the change reads as permission to
/// write anywhere. The adopted folder is compared byte for byte before and
/// after, including its `.git`.
#[tokio::test(flavor = "multi_thread")]
async fn a_managed_root_is_a_repository_and_an_adopted_one_is_untouched() {
    let node = Node::start().await;
    let goal = node.new_goal("two roots, two rules").await.to_string();

    let v = node
        .post(
            &format!("/goals/{goal}/projects"),
            json!({"kind": "new", "slug": "storefront"}),
        )
        .await;
    let managed = PathBuf::from(v["path"].as_str().unwrap());
    assert_eq!(v["project"]["root"]["type"], json!("managed"));
    assert_eq!(v["project"]["vcs"]["type"], json!("git"));
    assert!(
        managed.join(".git").is_dir(),
        "a folder we made is a repository: {}",
        managed.display()
    );

    // A folder somebody else made, adopted where it lies. Not a repository,
    // so an unwanted `git init` would be visible rather than idempotent.
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("NOTES.md"), "not ours\n").unwrap();
    let source = outside.path().to_path_buf();
    let before = tree_snapshot(&source);

    let v = node
        .post(
            &format!("/goals/{goal}/projects"),
            json!({"kind": "adopt", "slug": "legacy",
                   "path": source.display().to_string()}),
        )
        .await;
    assert_eq!(v["project"]["root"]["type"], json!("external"));
    assert_eq!(
        v["project"]["vcs"]["type"],
        json!("none"),
        "git is read off the folder, never created in it"
    );
    assert!(
        !source.join(".git").exists(),
        "adopting must not initialise somebody's folder"
    );
    assert_eq!(
        tree_snapshot(&source),
        before,
        "an adopted folder is byte-identical afterwards"
    );

    node.shutdown().await;
}

/// Adopt writes nothing — until the person asks. `POST /projects/{pid}/git/init`
/// is the ask: the `.git` lands in their folder, the record turns git, the Git
/// status reads a repository, the bus carries the project's frame and its
/// primary's; a second ask is a conflict, an unknown project is not found.
#[tokio::test(flavor = "multi_thread")]
async fn initialising_a_repository_over_http_flips_the_record_and_tells_the_bus() {
    let node = Node::start().await;
    let goal = node
        .new_goal("a folder that becomes a repository")
        .await
        .to_string();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("NOTES.md"), "not ours\n").unwrap();
    let source = outside.path().to_path_buf();

    let v = node
        .post(
            &format!("/goals/{goal}/projects"),
            json!({"kind": "adopt", "slug": "legacy",
                   "path": source.display().to_string()}),
        )
        .await;
    let pid = v["project"]["id"].as_str().unwrap().to_string();
    let wid = v["project"]["id"].as_str().unwrap().to_string();
    assert_eq!(v["project"]["vcs"]["type"], json!("none"));
    assert!(!source.join(".git").exists());
    let status = node.get(&format!("/workstreams/{wid}/git/status")).await;
    assert_eq!(
        status["status"]["git"],
        json!(false),
        "neutral for a plain folder"
    );
    let mut events = node.events();

    let (code, v) = node
        .req(
            "POST",
            &format!("/projects/{pid}/git/init"),
            Some(json!({})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["project"]["project"]["vcs"]["type"], json!("git"));
    assert_eq!(
        v["project"]["project"]["root"]["type"],
        json!("external"),
        "still their folder"
    );
    assert!(
        v["committer"].is_string() || v["committer"].is_null(),
        "{v}"
    );
    assert!(source.join(".git").is_dir(), "the one write adopt allows");
    assert_eq!(
        std::fs::read_to_string(source.join("NOTES.md")).unwrap(),
        "not ours\n",
        "their files untouched"
    );
    let status = node.get(&format!("/workstreams/{wid}/git/status")).await;
    assert_eq!(status["status"]["git"], json!(true));
    let shown = node.get(&format!("/projects/{pid}")).await;
    assert_eq!(shown["project"]["vcs"]["type"], json!("git"));

    let (mut project_changed, mut primary_changed) = (false, false);
    while let Ok(ev) = events.try_recv() {
        match &ev.payload {
            bisa_engine::EnginePayload::ProjectChanged { project }
                if project.to_string() == pid =>
            {
                project_changed = true
            }
            bisa_engine::EnginePayload::WorkstreamChanged { workstream, .. }
                if workstream.to_string() == wid =>
            {
                primary_changed = true
            }
            _ => {}
        }
    }
    assert!(project_changed, "the project's frame");
    assert!(
        primary_changed,
        "the primary's frame, so the rail's mark turns"
    );

    let (code, v) = node
        .req(
            "POST",
            &format!("/projects/{pid}/git/init"),
            Some(json!({})),
        )
        .await;
    assert_eq!(code, 409, "already a repository: {v}");
    assert!(v["error"]
        .as_str()
        .unwrap()
        .contains("already a repository"));

    let (code, _) = node
        .req(
            "POST",
            &format!("/projects/{}/git/init", ulid::Ulid::generate()),
            Some(json!({})),
        )
        .await;
    assert_eq!(code, 404);

    node.shutdown().await;
}

/// The other placement for the same folder: link it where it lies. Still the
/// promise it always was — Bisa writes nothing into it, ever.
#[tokio::test(flavor = "multi_thread")]
async fn linking_a_folder_in_place_writes_nothing_into_it() {
    let node = Node::start().await;
    let goal = node.new_goal("borrow some code").await.to_string();
    let src = outside_repo("left exactly as it was");
    let source = src.path().to_path_buf();
    let before = tree_snapshot(&source);

    let v = node
        .post(
            &format!("/goals/{goal}/projects"),
            json!({"kind": "adopt", "slug": "legacy",
                   "path": source.display().to_string()}),
        )
        .await;

    assert_eq!(v["project"]["root"]["type"], json!("external"));
    let recorded = PathBuf::from(v["project"]["root"]["path"].as_str().unwrap());
    assert_eq!(recorded, source.canonicalize().unwrap());
    assert_eq!(
        v["project"]["vcs"]["type"],
        json!("git"),
        "git is read, not created"
    );
    assert_eq!(v["imported"], Value::Null, "nothing was copied");

    assert_eq!(
        tree_snapshot(&source),
        before,
        "adopting a folder writes nothing into it"
    );

    node.shutdown().await;
}

/// The three shapes of source an import refuses, and the record that must not
/// survive the refusal.
#[tokio::test(flavor = "multi_thread")]
async fn an_import_refuses_a_source_it_has_no_business_copying() {
    let node = Node::start().await;
    let goal = node
        .new_goal("try to import the wrong things")
        .await
        .to_string();
    let route = format!("/goals/{goal}/projects");

    // Inside the Bisa workspace: a project rooted at workspace truth
    // could have its workstreams land on the journal.
    let inside = node.ws.root().join("not-a-project");
    std::fs::create_dir_all(&inside).unwrap();
    let (code, v) = node
        .req(
            "POST",
            &route,
            Some(json!({"kind": "import", "slug": "inside",
                        "path": inside.display().to_string()})),
        )
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(
        v["error"]
            .as_str()
            .unwrap()
            .contains("inside the Bisa workspace"),
        "{v}"
    );

    // Relative: the caller and the daemon do not share a working directory, so
    // a relative path names two different folders depending on who resolves it.
    let (code, v) = node
        .req(
            "POST",
            &route,
            Some(json!({"kind": "import", "slug": "relative", "path": "../elsewhere"})),
        )
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(
        v["error"].as_str().unwrap().contains("absolute path"),
        "{v}"
    );

    // A file is not a folder.
    let file = tempfile::NamedTempFile::new().unwrap();
    let (code, v) = node
        .req(
            "POST",
            &route,
            Some(json!({"kind": "import", "slug": "afile",
                        "path": file.path().display().to_string()})),
        )
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(
        v["error"].as_str().unwrap().contains("not a directory"),
        "{v}"
    );

    // None of the three left a record behind.
    let listed = node.get(&route).await;
    assert!(
        listed["projects"].as_array().unwrap().is_empty(),
        "a refused import must not leave a project record: {listed}"
    );

    node.shutdown().await;
}

/// A destination that already holds something is refused rather than merged
/// into: the result of a merge is neither the source nor what was there, and
/// nothing afterwards can say which file came from where.
#[tokio::test(flavor = "multi_thread")]
async fn an_import_refuses_an_existing_destination_rather_than_merging_into_it() {
    let node = Node::start().await;
    let goal = node.new_goal("import twice").await.to_string();
    let route = format!("/goals/{goal}/projects");

    // A folder under the slug an import is about to want. `project rm` forgets
    // the record and deliberately leaves the tree, which is exactly how a
    // person arrives at this state.
    let v = node
        .post(&route, json!({"kind": "new", "slug": "storefront"}))
        .await;
    let occupied = PathBuf::from(v["path"].as_str().unwrap());
    std::fs::write(occupied.join("theirs.txt"), b"already here").unwrap();
    let pid = v["project"]["id"].as_str().unwrap().to_string();
    node.delete(&format!("/projects/{pid}")).await;

    let src = outside_repo("would have been merged in");
    let (code, v) = node
        .req(
            "POST",
            &route,
            Some(json!({"kind": "import", "slug": "storefront",
                        "path": src.path().display().to_string()})),
        )
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(
        v["error"].as_str().unwrap().contains("already exists"),
        "{v}"
    );

    assert_eq!(
        std::fs::read(occupied.join("theirs.txt")).unwrap(),
        b"already here",
        "the refusal left the destination alone"
    );
    assert!(
        !occupied.join("README.md").exists(),
        "nothing was merged in"
    );
    assert!(
        node.get(&route).await["projects"]
            .as_array()
            .unwrap()
            .is_empty(),
        "the record was rolled back with the failed copy"
    );

    node.shutdown().await;
}

/// A project is visible to exactly the goals it is attached to — no owner,
/// no inheritance through `parent`. Attaching and detaching move
/// no bytes, and the same route that creates a project attaches an existing
/// one.
#[tokio::test(flavor = "multi_thread")]
async fn project_visibility_is_attachment_and_nothing_else() {
    let node = Node::start().await;
    let first = node.new_goal("works on the storefront").await;
    let other = node.new_goal("also needs it").await;

    // A child of `other`: under the old model it would have inherited; now a
    // parent's project is not a child's until somebody attaches it.
    let child = node
        .ws
        .create_goal(bisa_store::NewGoal {
            origin: bisa_core::GoalOrigin::Spawned { parent: other },
            ..bisa_store::NewGoal::captured("a child of the other goal")
        })
        .unwrap()
        .id;

    let v = node
        .post(
            &format!("/goals/{first}/projects"),
            json!({"kind": "new", "slug": "storefront"}),
        )
        .await;
    let pid = v["project"]["id"].as_str().unwrap().to_string();
    assert_eq!(v["attached_to"], json!(first.to_string()));
    assert_eq!(v["goals"], json!([first.to_string()]));

    // The goal it was created for sees it; nobody else does yet.
    let v = node.get(&format!("/goals/{first}/projects")).await;
    assert_eq!(v["projects"].as_array().unwrap().len(), 1);
    assert_eq!(v["projects"][0]["goals"], json!([first.to_string()]));
    for goal in [other, child] {
        assert!(
            node.get(&format!("/goals/{goal}/projects")).await["projects"]
                .as_array()
                .unwrap()
                .is_empty(),
            "goal {goal} is not attached"
        );
    }

    // Attach it to the second goal.
    let v = node
        .post(
            &format!("/projects/{pid}/attach"),
            json!({"goal": other.to_string()}),
        )
        .await;
    assert_eq!(v["attached_to"], json!(other.to_string()));
    let v = node.get(&format!("/goals/{other}/projects")).await;
    assert_eq!(v["projects"].as_array().unwrap().len(), 1);
    let mut goals: Vec<String> = v["projects"][0]["goals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g.as_str().unwrap().to_string())
        .collect();
    goals.sort();
    let mut want = vec![first.to_string(), other.to_string()];
    want.sort();
    assert_eq!(goals, want, "both attachments are on the row");

    // The child inherits nothing.
    assert!(
        node.get(&format!("/goals/{child}/projects")).await["projects"]
            .as_array()
            .unwrap()
            .is_empty(),
        "a parent's attachment is not a child's"
    );

    // The same route creates *and* attaches, so one endpoint answers "add a
    // project here" whatever the source.
    let third = node.new_goal("a third").await;
    let v = node
        .post(
            &format!("/goals/{third}/projects"),
            json!({"kind": "attach", "project": pid}),
        )
        .await;
    assert_eq!(v["attached_to"], json!(third.to_string()));
    assert_eq!(
        node.get(&format!("/goals/{third}/projects")).await["projects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // Detaching takes it away again, and never touches the other goals or
    // the folder.
    let (code, _) = node
        .req(
            "DELETE",
            &format!("/projects/{pid}/attach"),
            Some(json!({"goal": other.to_string()})),
        )
        .await;
    assert_eq!(code, 200);
    assert!(
        node.get(&format!("/goals/{other}/projects")).await["projects"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        node.get(&format!("/goals/{first}/projects")).await["projects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let detail = node.get(&format!("/projects/{pid}")).await;
    assert_eq!(detail["exists"], json!(true), "detaching moved no bytes");

    node.shutdown().await;
}

/// `GET /projects` answers the question the per-goal route cannot: what
/// exists in this workspace at all — including a project attached to no
/// goal, which is an ordinary thing for a project to be.
///
/// The rule it must not break is **once each**. A project attached to two
/// goals is one row naming two goals — not two rows. That is exactly the fold
/// the desktop used to do client-side over N requests, and getting it wrong
/// here would show the same project twice on the projects screen and twice in
/// a picker.
#[tokio::test(flavor = "multi_thread")]
async fn workspace_project_list_is_one_row_per_project() {
    let node = Node::start().await;
    let first = node.new_goal("works on the storefront").await;
    let second = node.new_goal("also needs it").await;

    // Empty until there is something to list.
    assert!(node.get("/projects").await["projects"]
        .as_array()
        .unwrap()
        .is_empty());

    let v = node
        .post(
            &format!("/goals/{first}/projects"),
            json!({"kind": "new", "slug": "storefront", "name": "Storefront"}),
        )
        .await;
    let pid = v["project"]["id"].as_str().unwrap().to_string();
    // A second project, attached to the second goal only.
    node.post(
        &format!("/goals/{second}/projects"),
        json!({"kind": "new", "slug": "notes"}),
    )
    .await;
    // And a third attached to nothing at all: the first-run shape, where a
    // workspace opens on Projects and a goal comes later.
    let v = node
        .post("/projects", json!({"kind": "new", "slug": "standalone"}))
        .await;
    assert_eq!(v["goals"], json!([]));
    assert_eq!(v["attached_to"], Value::Null);

    node.post(
        &format!("/projects/{pid}/attach"),
        json!({"goal": second.to_string()}),
    )
    .await;

    // Two goals now see `storefront`; the second sees both of its own.
    assert_eq!(
        node.get(&format!("/goals/{first}/projects")).await["projects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        node.get(&format!("/goals/{second}/projects")).await["projects"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let v = node.get("/projects").await;
    let rows = v["projects"].as_array().unwrap();
    assert_eq!(
        rows.len(),
        3,
        "one row per project, however many goals it is attached to"
    );

    let storefront = rows
        .iter()
        .find(|r| r["project"]["id"] == json!(pid))
        .expect("the shared project is listed");

    // Enough to render a row and a picker without a second request.
    assert_eq!(storefront["project"]["slug"], json!("storefront"));
    assert_eq!(storefront["project"]["name"], json!("Storefront"));
    assert_eq!(storefront["project"]["vcs"]["type"], json!("git"));
    assert_eq!(storefront["project"]["root"]["type"], json!("managed"));
    // A managed root is the project's `tree/`.
    assert!(storefront["path"]
        .as_str()
        .unwrap()
        .ends_with("storefront/tree"));
    assert_eq!(storefront["exists"], json!(true));
    assert_eq!(
        storefront["workstreams"],
        json!(1),
        "the primary is a workstream"
    );
    let mut goals: Vec<String> = storefront["goals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g.as_str().unwrap().to_string())
        .collect();
    goals.sort();
    let mut want = vec![first.to_string(), second.to_string()];
    want.sort();
    assert_eq!(goals, want);

    let notes = rows
        .iter()
        .find(|r| r["project"]["slug"] == json!("notes"))
        .expect("the other project is listed too");
    assert_eq!(notes["goals"], json!([second.to_string()]));
    let standalone = rows
        .iter()
        .find(|r| r["project"]["slug"] == json!("standalone"))
        .expect("a project attached to nothing is still a project");
    assert_eq!(standalone["goals"], json!([]));

    // Detaching takes one goal off the row without touching the row itself.
    let (code, _) = node
        .req(
            "DELETE",
            &format!("/projects/{pid}/attach"),
            Some(json!({"goal": second.to_string()})),
        )
        .await;
    assert_eq!(code, 200);
    let v = node.get("/projects").await;
    assert_eq!(v["projects"].as_array().unwrap().len(), 3);
    let storefront = v["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["project"]["id"] == json!(pid))
        .unwrap();
    assert_eq!(storefront["goals"], json!([first.to_string()]));

    node.shutdown().await;
}

/// A slug becomes a directory name and an adopted path becomes a project
/// root: both are places a caller could try to escape to. Every shape is
/// refused *before* anything reaches the filesystem.
#[tokio::test(flavor = "multi_thread")]
async fn slugs_and_adopted_paths_cannot_escape() {
    let node = Node::start().await;
    let goal = node.new_goal("containment").await.to_string();
    let ws_root = node.ws.root().to_path_buf();

    let bad_slugs = [
        "..",
        "../evil",
        "../../etc",
        "a/../b",
        "/etc/passwd",
        "etc/passwd",
        "a/b",
        "a\\b",
        ".hidden",
        "C:\\Windows",
        "..%2f..",
        // Unicode lookalikes: a fullwidth solidus and a Cyrillic 'а' are
        // outside the ASCII allowlist, so neither can smuggle a separator nor
        // impersonate an existing slug.
        "a\u{ff0f}b",
        "\u{0430}pi",
        "café",
        "\u{202e}txt",
        "with space",
        "UPPER",
        "",
    ];
    for slug in bad_slugs {
        let (code, v) = node
            .req(
                "POST",
                &format!("/goals/{goal}/projects"),
                Some(json!({"kind": "new", "slug": slug})),
            )
            .await;
        assert_eq!(code, 400, "slug {slug:?} should be refused, got {v}");
    }
    // Nothing was created anywhere: the goal owns no projects, and the
    // workspace grew no stray directories.
    assert!(
        node.get(&format!("/goals/{goal}/projects")).await["projects"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(!ws_root.join("etc").exists());
    assert!(!ws_root.parent().unwrap().join("evil").exists());

    // --- adopt ------------------------------------------------------------
    let outside = tempfile::tempdir().unwrap();
    let real = outside.path().join("real-project");
    std::fs::create_dir_all(&real).unwrap();
    let file = outside.path().join("a-file.txt");
    std::fs::write(&file, "not a directory").unwrap();

    let refusals: [(&str, &str); 4] = [
        ("relative", "some/relative/path"),
        ("missing", "/definitely/not/here/at/all"),
        ("a file", file.to_str().unwrap()),
        // `..` inside an absolute path is resolved by canonicalization, and
        // what is on the other side does not exist.
        (
            "traversal",
            &format!("{}/../nowhere-at-all", real.display()),
        ),
    ];
    for (what, path) in refusals {
        let (code, v) = node
            .req(
                "POST",
                &format!("/goals/{goal}/projects"),
                Some(json!({"kind": "adopt", "slug": "adopted", "path": path})),
            )
            .await;
        assert_eq!(code, 400, "adopting {what} ({path}) should be refused: {v}");
    }

    // Inside the workspace: a project rooted at workspace truth could put a
    // working tree on top of the journal.
    let inside = node.ws.paths().goals_dir();
    std::fs::create_dir_all(&inside).unwrap();
    let (code, v) = node
        .req(
            "POST",
            &format!("/goals/{goal}/projects"),
            Some(json!({"kind": "adopt", "slug": "adopted", "path": inside.to_str().unwrap()})),
        )
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(v["error"]
        .as_str()
        .unwrap()
        .contains("inside the Bisa workspace"));

    // ...and a symlink is exactly how somebody would try to get there, so the
    // check runs on the canonical path rather than the one it was given.
    #[cfg(unix)]
    {
        let link = outside.path().join("innocent-looking");
        std::os::unix::fs::symlink(&inside, &link).unwrap();
        let (code, v) = node
            .req(
                "POST",
                &format!("/goals/{goal}/projects"),
                Some(json!({"kind": "adopt", "slug": "adopted",
                            "path": link.to_str().unwrap()})),
            )
            .await;
        assert_eq!(
            code, 400,
            "a symlink into the workspace must be refused: {v}"
        );
        assert!(v["error"]
            .as_str()
            .unwrap()
            .contains("inside the Bisa workspace"));

        // A symlink pointing somewhere legitimate is adopted as its *target*:
        // what is stored is the real directory, not the route taken to it.
        let good_link = outside.path().join("shortcut");
        std::os::unix::fs::symlink(&real, &good_link).unwrap();
        let v = node
            .post(
                &format!("/goals/{goal}/projects"),
                json!({"kind": "adopt", "slug": "adopted",
                       "path": good_link.to_str().unwrap()}),
            )
            .await;
        let stored = PathBuf::from(v["path"].as_str().unwrap());
        assert_eq!(stored, real.canonicalize().unwrap());
        assert_eq!(v["project"]["root"]["type"], json!("external"));
        // Adopting writes nothing into the folder it points at.
        assert_eq!(std::fs::read_dir(&real).unwrap().count(), 0);
    }

    node.shutdown().await;
}

/// Push passes the `Publish` gate, and the route says so instead of blocking:
/// `202` with the gate id, nothing on the remote, and the push completes only
/// once a human decides. "origin" is a local bare repository.
#[tokio::test(flavor = "multi_thread")]
async fn pushing_returns_the_gate_and_pushes_nothing_until_it_is_decided() {
    let node = Node::start().await;
    let goal = node.new_goal("ship the checkout").await.to_string();
    let (pid, root) = node.git_project(&goal, "storefront", "gated").await;
    let origin = node.origin_for(&root, "storefront");

    let v = node
        .post(
            &format!("/projects/{pid}/workstreams"),
            json!({"label": "checkout", "goal": goal}),
        )
        .await;
    let wid = v["workstream"]["id"].as_str().unwrap().to_string();
    let wpath = PathBuf::from(v["path"].as_str().unwrap());
    let branch = v["workstream"]["kind"]["branch"]
        .as_str()
        .unwrap()
        .to_string();
    set_identity(&wpath);
    std::fs::write(wpath.join("checkout.rs"), "fn checkout() {}\n").unwrap();
    node.post(
        &format!("/workstreams/{wid}/commit"),
        json!({"message": "checkout"}),
    )
    .await;

    // The request returns immediately with the gate — it does not wait for a
    // human, because an HTTP call that waits for a human is a call that times
    // out.
    let (code, v) = node
        .req("POST", &format!("/workstreams/{wid}/push"), Some(json!({})))
        .await;
    assert_eq!(code, 202, "a gated push answers 202: {v}");
    assert_eq!(v["pushed"], json!(false));
    assert_eq!(v["status"], json!("awaiting_publish_gate"));
    let gate = v["gate"].as_str().expect("gate id").to_string();

    // Nothing has left the machine.
    assert!(
        origin_branches(&origin).is_empty(),
        "the remote must be untouched until the gate is decided"
    );
    // ...and the gate is in the inbox, next to any other pending decision.
    let inbox = node.get("/inbox").await;
    let found = inbox["rows"].as_array().unwrap().iter().any(|r| {
        r["needs_action"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["gate_kind"] == json!("publish") && n["gate_id"] == json!(gate))
    });
    assert!(found, "the publish gate belongs in the inbox: {inbox}");

    // Decide it, and the push it was holding goes out.
    node.post(
        &format!("/goals/{goal}/decide"),
        json!({"approve": true, "gate": gate}),
    )
    .await;
    let branches = until("the branch to reach origin", || {
        let b = origin_branches(&origin);
        (!b.is_empty()).then_some(b)
    })
    .await;
    assert_eq!(branches, vec![branch.clone()]);

    // ...and the workstream records it, so the UI stops offering "push".
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let v = node.get(&format!("/workstreams/{wid}")).await;
        if v["workstream"]["state"]["state"] == json!("pushed") {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the workstream never recorded the push: {v}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    node.shutdown().await;
}

/// What a person approved and then did not go out is said. By the time the
/// push runs nobody is waiting on the call — the route answered `202` long
/// before — so a push the remote refuses after its gate was approved is a
/// fact of the workstream: a notice on its row of the Inbox, a line of the
/// feed, with what was asked and why it failed. "origin" is a folder that is
/// no repository.
#[tokio::test(flavor = "multi_thread")]
async fn a_push_that_fails_after_its_gate_was_approved_is_said() {
    let node = Node::start().await;
    let goal = node.new_goal("ship the checkout").await.to_string();
    let (pid, root) = node.git_project(&goal, "storefront", "gated").await;
    let nowhere = node.ws.root().join("nowhere.git");
    raw_git(
        &root,
        &["remote", "add", "origin", nowhere.to_str().unwrap()],
    );

    let v = node
        .post(
            &format!("/projects/{pid}/workstreams"),
            json!({"label": "checkout", "goal": goal}),
        )
        .await;
    let wid = v["workstream"]["id"].as_str().unwrap().to_string();
    let wpath = PathBuf::from(v["path"].as_str().unwrap());
    let branch = v["workstream"]["kind"]["branch"]
        .as_str()
        .unwrap()
        .to_string();
    set_identity(&wpath);
    std::fs::write(wpath.join("checkout.rs"), "fn checkout() {}\n").unwrap();
    node.post(
        &format!("/workstreams/{wid}/commit"),
        json!({"message": "checkout"}),
    )
    .await;
    let (code, v) = node
        .req("POST", &format!("/workstreams/{wid}/push"), Some(json!({})))
        .await;
    assert_eq!(code, 202, "{v}");
    let gate = v["gate"].as_str().expect("gate id").to_string();
    node.post(
        &format!("/goals/{goal}/decide"),
        json!({"approve": true, "gate": gate}),
    )
    .await;

    // The notice, on the workstream's own row.
    let mut row = Value::Null;
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while row.is_null() {
        let inbox = node.get("/inbox").await;
        row = inbox["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| {
                r["key"] == json!(wid)
                    && r["notices"]
                        .as_array()
                        .is_some_and(|all| all.iter().any(|n| n["notice"] == "publish_failed"))
            })
            .cloned()
            .unwrap_or(Value::Null);
        assert!(
            !row.is_null() || std::time::Instant::now() < deadline,
            "an approved push that failed was never said: {inbox}\nthe feed: {}",
            node.get("/pulse?limit=50").await
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(row["kind"], json!("workstream"));

    // The fact itself: what was asked, and the remote's own refusal.
    let feed = node.get("/pulse?limit=50&concept=projects").await;
    let fact = feed["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "workstream_publish_failed")
        .unwrap_or_else(|| panic!("the feed keeps it: {feed}"));
    assert_eq!(fact["event"]["workstream"], json!(wid));
    assert_eq!(fact["event"]["project"], json!(pid));
    assert_eq!(fact["event"]["what"], json!(format!("push {branch}")));
    assert!(
        fact["event"]["reason"]
            .as_str()
            .is_some_and(|why| !why.is_empty()),
        "{fact}"
    );

    // Nothing moved: the record still offers the push.
    let v = node.get(&format!("/workstreams/{wid}")).await;
    assert_eq!(v["workstream"]["state"]["state"], json!("committed"), "{v}");
    node.shutdown().await;
}

/// A declined gate pushes nothing, and says which workstream it refused.
#[tokio::test(flavor = "multi_thread")]
async fn a_declined_publish_gate_pushes_nothing() {
    let node = Node::start().await;
    let goal = node.new_goal("do not ship this").await.to_string();
    let (pid, root) = node.git_project(&goal, "storefront", "gated").await;
    let origin = node.origin_for(&root, "storefront");

    let v = node
        .post(
            &format!("/projects/{pid}/workstreams"),
            json!({"label": "risky", "goal": goal}),
        )
        .await;
    let wid = v["workstream"]["id"].as_str().unwrap().to_string();
    let wpath = PathBuf::from(v["path"].as_str().unwrap());
    set_identity(&wpath);
    std::fs::write(wpath.join("risky.rs"), "fn risky() {}\n").unwrap();
    node.post(
        &format!("/workstreams/{wid}/commit"),
        json!({"message": "risky"}),
    )
    .await;

    let (code, v) = node
        .req("POST", &format!("/workstreams/{wid}/push"), Some(json!({})))
        .await;
    assert_eq!(code, 202, "{v}");
    let gate = v["gate"].as_str().unwrap().to_string();

    node.post(
        &format!("/goals/{goal}/decide"),
        json!({"approve": false, "gate": gate}),
    )
    .await;

    // Nothing goes out, and the workstream never claims it was pushed.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(origin_branches(&origin).is_empty());
    let v = node.get(&format!("/workstreams/{wid}")).await;
    assert_eq!(v["workstream"]["state"]["state"], json!("committed"));

    node.shutdown().await;
}

/// `PublishPolicy::Manual` refuses outright — no gate opens, because there is
/// no decision that would make Bisa push for you.
#[tokio::test(flavor = "multi_thread")]
async fn manual_publishing_is_refused_rather_than_gated() {
    let node = Node::start().await;
    let goal = node.new_goal("manual only").await.to_string();
    let (pid, root) = node.git_project(&goal, "storefront", "manual").await;
    let origin = node.origin_for(&root, "storefront");

    let v = node
        .post(
            &format!("/projects/{pid}/workstreams"),
            json!({"label": "hand", "goal": goal}),
        )
        .await;
    let wid = v["workstream"]["id"].as_str().unwrap().to_string();
    let wpath = PathBuf::from(v["path"].as_str().unwrap());
    set_identity(&wpath);
    std::fs::write(wpath.join("hand.rs"), "fn hand() {}\n").unwrap();
    node.post(
        &format!("/workstreams/{wid}/commit"),
        json!({"message": "hand"}),
    )
    .await;

    let (code, v) = node
        .req("POST", &format!("/workstreams/{wid}/push"), Some(json!({})))
        .await;
    assert_eq!(code, 409, "{v}");
    assert!(v["error"].as_str().unwrap().contains("publishes manually"));
    assert_eq!(
        v["code"],
        json!("publish_manual"),
        "the refusal is named, not left to be guessed from the status: {v}"
    );
    assert!(node
        .get("/inbox")
        .await
        .to_string()
        .find("publish")
        .is_none());
    assert!(origin_branches(&origin).is_empty());

    node.shutdown().await;
}

/// `PublishPolicy::Auto` skips the gate: the route answers 200 with the work
/// already done.
#[tokio::test(flavor = "multi_thread")]
async fn auto_publishing_answers_200_with_the_push_done() {
    let node = Node::start().await;
    let goal = node.new_goal("ship continuously").await.to_string();
    let (pid, root) = node.git_project(&goal, "storefront", "auto").await;
    let origin = node.origin_for(&root, "storefront");

    let v = node
        .post(
            &format!("/projects/{pid}/workstreams"),
            json!({"label": "auto", "goal": goal}),
        )
        .await;
    let wid = v["workstream"]["id"].as_str().unwrap().to_string();
    let wpath = PathBuf::from(v["path"].as_str().unwrap());
    let branch = v["workstream"]["kind"]["branch"]
        .as_str()
        .unwrap()
        .to_string();
    set_identity(&wpath);
    std::fs::write(wpath.join("auto.rs"), "fn auto() {}\n").unwrap();
    node.post(
        &format!("/workstreams/{wid}/commit"),
        json!({"message": "auto"}),
    )
    .await;

    let (code, v) = node
        .req("POST", &format!("/workstreams/{wid}/push"), Some(json!({})))
        .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["pushed"], json!(true));
    assert_eq!(v["workstream"]["state"]["state"], json!("pushed"));
    assert_eq!(origin_branches(&origin), vec![branch]);

    node.shutdown().await;
}

/// Forgetting a project is a record operation: the folder stays on disk
/// unless the caller says `?tree=true`, and an adopted folder is never
/// deleted at all.
#[tokio::test(flavor = "multi_thread")]
async fn deleting_a_project_leaves_the_folder_unless_asked() {
    let node = Node::start().await;
    let goal = node.new_goal("delete me").await.to_string();

    let v = node
        .post(
            &format!("/goals/{goal}/projects"),
            json!({"kind": "new", "slug": "kept"}),
        )
        .await;
    let kept_id = v["project"]["id"].as_str().unwrap().to_string();
    let kept = PathBuf::from(v["path"].as_str().unwrap());
    std::fs::write(kept.join("work.txt"), "somebody's only copy").unwrap();

    let v = node.delete(&format!("/projects/{kept_id}")).await;
    assert_eq!(v["removed_tree"], json!(false));
    assert!(kept.is_dir(), "the folder survives a plain DELETE");
    assert!(kept.join("work.txt").exists());
    let (code, _) = node.req("GET", &format!("/projects/{kept_id}"), None).await;
    assert!(code >= 400, "the record is gone");

    // With `?tree=true` the managed folder goes too.
    let v = node
        .post(
            &format!("/goals/{goal}/projects"),
            json!({"kind": "new", "slug": "temporary"}),
        )
        .await;
    let temp_id = v["project"]["id"].as_str().unwrap().to_string();
    let temp = PathBuf::from(v["path"].as_str().unwrap());
    // Unlinked, not the OS Trash: a fixture never leaves its tempdir.
    node.req(
        "PUT",
        "/settings/machine",
        Some(json!({"values": {"editor.delete.trash": false}})),
    )
    .await;
    let v = node.delete(&format!("/projects/{temp_id}?tree=true")).await;
    assert_eq!(v["removed_tree"], json!(true));
    assert!(!temp.exists());

    // An adopted folder is refused: Bisa did not create it.
    let outside = tempfile::tempdir().unwrap();
    let real = outside.path().join("theirs");
    std::fs::create_dir_all(&real).unwrap();
    std::fs::write(real.join("theirs.txt"), "not ours").unwrap();
    let v = node
        .post(
            &format!("/goals/{goal}/projects"),
            json!({"kind": "adopt", "slug": "theirs", "path": real.to_str().unwrap()}),
        )
        .await;
    let theirs_id = v["project"]["id"].as_str().unwrap().to_string();
    let (code, v) = node
        .req("DELETE", &format!("/projects/{theirs_id}?tree=true"), None)
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(v["error"].as_str().unwrap().contains("adopted folder"));
    assert!(real.join("theirs.txt").exists());
    // A folder is one project's: the same folder, one inside it and one
    // around it are each refused by the name of the project that holds it.
    let inside = real.join("packages");
    std::fs::create_dir_all(&inside).unwrap();
    for (slug, path) in [
        ("theirs-again", real.clone()),
        ("theirs-inside", inside),
        ("theirs-around", outside.path().to_path_buf()),
    ] {
        let (code, v) = node
            .req(
                "POST",
                &format!("/goals/{goal}/projects"),
                Some(json!({"kind": "adopt", "slug": slug, "path": path.to_str().unwrap()})),
            )
            .await;
        assert_eq!(code, 400, "{slug}: {v}");
        assert!(
            v["error"]
                .as_str()
                .unwrap()
                .contains("a folder is one project's"),
            "{slug}: {v}"
        );
    }
    // Without the flag it is forgotten, and the folder is untouched — and free again.
    node.delete(&format!("/projects/{theirs_id}")).await;
    assert!(real.join("theirs.txt").exists());
    let (code, v) = node
        .req(
            "POST",
            &format!("/goals/{goal}/projects"),
            Some(json!({"kind": "adopt", "slug": "theirs-again", "path": real.to_str().unwrap()})),
        )
        .await;
    assert_eq!(code, 200, "a forgotten project lets its folder go: {v}");

    node.shutdown().await;
}

/// Cloning is explicit, and the "remote" is a local bare repository — the
/// whole clone path with no network.
#[tokio::test(flavor = "multi_thread")]
async fn cloning_records_what_it_became() {
    let node = Node::start().await;
    let goal = node.new_goal("clone something").await.to_string();

    // A source repository with one commit, built by the test.
    let src_dir = tempfile::tempdir().unwrap();
    let src = src_dir.path().join("source");
    std::fs::create_dir_all(&src).unwrap();
    raw_git(&src, &["init", "--quiet"]);
    set_identity(&src);
    std::fs::write(src.join("README.md"), "source\n").unwrap();
    raw_git(&src, &["add", "-A"]);
    raw_git(&src, &["commit", "-m", "source", "--quiet"]);

    let v = node
        .post(
            &format!("/goals/{goal}/projects"),
            json!({"kind": "clone", "slug": "cloned", "url": src.to_str().unwrap()}),
        )
        .await;
    let path = PathBuf::from(v["path"].as_str().unwrap());
    assert!(path.join("README.md").exists());
    assert_eq!(v["project"]["vcs"]["type"], json!("git"));
    assert_eq!(
        v["project"]["vcs"]["remote"],
        json!(src.to_str().unwrap()),
        "the clone remembers where it came from"
    );

    // A clone that fails leaves no half-project behind.
    let (code, _) = node
        .req(
            "POST",
            &format!("/goals/{goal}/projects"),
            Some(json!({"kind": "clone", "slug": "broken",
                        "url": "/definitely/not/a/repository"})),
        )
        .await;
    assert!(code >= 400);
    let listed = node.get(&format!("/goals/{goal}/projects")).await;
    let slugs: Vec<String> = listed["projects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["project"]["slug"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(slugs, vec!["cloned".to_string()]);

    node.shutdown().await;
}

/// Assignees on a project, and the record edits around them.
#[tokio::test(flavor = "multi_thread")]
async fn project_assignees_and_patch() {
    let node = Node::start().await;
    let goal = node.new_goal("who carries this").await.to_string();
    let agent = node
        .post(
            "/agents",
            json!({"name": "Hand", "system_prompt": "work", "harness": "claude-code"}),
        )
        .await["agent"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let pid = node
        .post(
            &format!("/goals/{goal}/projects"),
            json!({"kind": "new", "slug": "carried"}),
        )
        .await["project"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let v = node
        .put(
            &format!("/projects/{pid}/assignees"),
            json!({"assignees": [format!("agent:{agent}")]}),
        )
        .await;
    assert_eq!(v["project"]["assignees"], json!([{"agent": agent}]));

    // A bad entry is refused whole rather than partially applied.
    let (code, _) = node
        .req(
            "PUT",
            &format!("/projects/{pid}/assignees"),
            Some(json!({"assignees": [format!("agent:{agent}"), "nonsense"]})),
        )
        .await;
    assert_eq!(code, 400);
    let v = node.get(&format!("/projects/{pid}")).await;
    assert_eq!(v["project"]["assignees"], json!([{"agent": agent}]));

    // PATCH is partial: the name changes, the assignment stays.
    let (code, v) = node
        .req(
            "PATCH",
            &format!("/projects/{pid}"),
            Some(json!({"name": "Carried Things", "publish": "auto"})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["project"]["name"], json!("Carried Things"));
    assert_eq!(v["project"]["publish"], json!("auto"));
    assert_eq!(v["project"]["assignees"], json!([{"agent": agent}]));

    // An empty list clears it.
    let v = node
        .put(
            &format!("/projects/{pid}/assignees"),
            json!({"assignees": []}),
        )
        .await;
    assert!(v["project"]["assignees"]
        .as_array()
        .is_none_or(|a| a.is_empty()));

    node.shutdown().await;
}

/// The model-health ledger is readable over HTTP. It is live runtime state,
/// so a fresh node has nothing to report — and says so with an empty list
/// rather than an error.
#[tokio::test(flavor = "multi_thread")]
async fn model_health_is_readable() {
    let node = Node::start().await;
    let v = node.get("/models/health").await;
    assert!(v["models"].as_array().unwrap().is_empty());
    node.shutdown().await;
}

/// Opening a pull request expresses the gate exactly as pushing does — `202`
/// with the gate id — and the code host is not asked until the gate is decided.
/// The code host is in memory and claims the test's bare `origin` by name, so the
/// push is real and nothing leaves the process.
#[tokio::test(flavor = "multi_thread")]
async fn opening_a_pull_request_returns_the_gate_first() {
    let fake = std::sync::Arc::new(bisa_codehost::fake::FakeCodeHost::minimal(
        "storefront-origin",
    ));
    let node = Node::start_with(EngineConfig {
        design_enabled: false,
        code_hosts: Some(bisa_codehost::CodeHostRegistry::new(vec![fake.clone()])),
        ..Default::default()
    })
    .await;
    let goal = node.new_goal("open a pr").await.to_string();
    let (pid, root) = node.git_project(&goal, "storefront", "gated").await;
    let origin = node.origin_for(&root, "storefront");
    let repo = fake
        .detect(&bisa_codehost::RemoteUrl::parse(origin.to_str().unwrap()))
        .expect("the fake claims the bare origin");

    let v = node
        .post(
            &format!("/projects/{pid}/workstreams"),
            json!({"label": "pr", "goal": goal}),
        )
        .await;
    let wid = v["workstream"]["id"].as_str().unwrap().to_string();
    let wpath = PathBuf::from(v["path"].as_str().unwrap());
    set_identity(&wpath);
    std::fs::write(wpath.join("pr.rs"), "fn pr() {}\n").unwrap();
    node.post(
        &format!("/workstreams/{wid}/commit"),
        json!({"message": "pr"}),
    )
    .await;
    // A pull request is opened on pushed work, so the branch goes out first —
    // through its own Publish gate.
    let (code, v) = node
        .req("POST", &format!("/workstreams/{wid}/push"), Some(json!({})))
        .await;
    assert_eq!(code, 202, "{v}");
    let gate = v["gate"].as_str().unwrap().to_string();
    node.post(
        &format!("/goals/{goal}/decide"),
        json!({"approve": true, "gate": gate}),
    )
    .await;
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let v = node.get(&format!("/workstreams/{wid}")).await;
        if v["workstream"]["state"]["state"] == json!("pushed") {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the workstream was never pushed: {v}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let (code, v) = node
        .req(
            "POST",
            &format!("/workstreams/{wid}/pr"),
            Some(json!({"title": "Add the checkout", "body": "why"})),
        )
        .await;
    assert_eq!(code, 202, "{v}");
    assert_eq!(v["opened"], json!(false));
    let gate = v["gate"].as_str().unwrap().to_string();
    assert!(
        fake.list_prs(&repo, Default::default())
            .await
            .unwrap()
            .is_empty(),
        "the gate stands in front of the code host, not behind it"
    );

    node.post(
        &format!("/goals/{goal}/decide"),
        json!({"approve": true, "gate": gate}),
    )
    .await;

    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let v = node.get(&format!("/workstreams/{wid}")).await;
        if v["workstream"]["state"]["state"] == json!("pr_open") {
            assert_eq!(v["workstream"]["state"]["number"], json!(1));
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the workstream never recorded the pull request: {v}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        fake.list_prs(&repo, Default::default())
            .await
            .unwrap()
            .len(),
        1,
        "the code host is asked once the gate is approved"
    );

    node.shutdown().await;
}

/// A reply on a review thread over HTTP is the person's — their words reach
/// the code host untouched — and `resolve: true` resolves the thread in the
/// same act; the same thread replied on through the intake socket is an
/// agent's, signed with its id. The code host is in memory; the pull request
/// is put on the record directly, since opening one is the gate test above.
#[tokio::test(flavor = "multi_thread")]
async fn a_reply_on_a_review_thread_is_the_persons_over_http_and_an_agents_through_intake() {
    use bisa_codehost::{ReviewThread, ReviewThreadComment};
    let fake = std::sync::Arc::new(bisa_codehost::fake::FakeCodeHost::with_capabilities(
        "storefront-origin",
        bisa_codehost::CodeHostCapabilities {
            review_threads: true,
            review_thread_replies: true,
            ..Default::default()
        },
    ));
    let node = Node::start_with(EngineConfig {
        design_enabled: false,
        code_hosts: Some(bisa_codehost::CodeHostRegistry::new(vec![fake.clone()])),
        ..Default::default()
    })
    .await;
    let goal = node.new_goal("reply on a thread").await.to_string();
    let (pid, root) = node.git_project(&goal, "storefront", "auto").await;
    let origin = node.origin_for(&root, "storefront");
    let repo = fake
        .detect(&bisa_codehost::RemoteUrl::parse(origin.to_str().unwrap()))
        .expect("the fake claims the bare origin");
    let pr = fake.seed_pr(&repo, "alice", "work/reply", "main");
    let v = node
        .post(
            &format!("/projects/{pid}/workstreams"),
            json!({"label": "reply", "goal": goal}),
        )
        .await;
    let wid = v["workstream"]["id"].as_str().unwrap().to_string();
    let id: bisa_core::WorkstreamId = wid.parse().unwrap();
    for step in [
        bisa_core::WorkstreamTransition::Committed,
        bisa_core::WorkstreamTransition::Pushed,
        bisa_core::WorkstreamTransition::PrOpened {
            number: pr.number,
            url: pr.url.clone(),
        },
    ] {
        node.ws.transition_workstream(id, &step).unwrap();
    }
    for t in ["T1", "T2"] {
        fake.state.threads.lock().unwrap().push(ReviewThread {
            id: t.into(),
            path: Some("src/cart.rs".into()),
            line: Some(12),
            is_resolved: false,
            is_outdated: false,
            comments: vec![ReviewThreadComment {
                author: Some("alice".into()),
                body: "This rounds twice.".into(),
                created_at: None,
            }],
        });
    }

    let (code, v) = node
        .req(
            "POST",
            &format!("/workstreams/{wid}/pr/threads/T1/reply"),
            Some(json!({"body": "Fixed in abc123.", "resolve": true})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(
        (
            v["replied"].as_bool(),
            v["resolved"].as_bool(),
            v["thread"].as_str()
        ),
        (Some(true), Some(true), Some("T1"))
    );
    {
        let threads = fake.state.threads.lock().unwrap();
        assert_eq!(
            threads[0].comments[1].body, "Fixed in abc123.",
            "the person's words, untouched"
        );
        assert!(threads[0].is_resolved, "resolved in the same act");
    }
    let (code, v) = node
        .req(
            "POST",
            &format!("/workstreams/{wid}/pr/threads/T2/reply"),
            Some(json!({"body": ""})),
        )
        .await;
    assert_eq!(code, 400, "{v}");

    // Through the intake, the session is a turn of a conversation about the
    // workstream: its scope is the conversation's id, never the checkout's.
    let (code, v) = node
        .req(
            "POST",
            "/conversations",
            Some(json!({"origin": {"kind": "workstream", "id": wid, "project": pid}})),
        )
        .await;
    assert_eq!(code, 201, "{v}");
    let scope = v["conversation"]["id"]
        .as_str()
        .expect("a conversation")
        .to_string();
    let replied = node
        .intake_op(json!({"op": "pr_thread_reply", "scope": scope, "agent": "fixer", "thread": "T2", "body": "Left as is.", "resolve": false}))
        .await;
    assert_eq!(replied["ok"], true, "{replied}");
    {
        let threads = fake.state.threads.lock().unwrap();
        assert!(
            threads[1].comments[1]
                .body
                .starts_with(bisa_engine::codehost::AGENT_REPLY_MARK),
            "an agent's reply is signed: {}",
            threads[1].comments[1].body
        );
        assert!(!threads[1].is_resolved);
    }
    // What a screen reads next carries both, the cache dropped by the reply.
    let v = node.get(&format!("/workstreams/{wid}/pr/reviews")).await;
    assert_eq!(v["threads"][0]["is_resolved"], json!(true));
    assert_eq!(v["threads"][1]["comments"].as_array().unwrap().len(), 2);

    node.shutdown().await;
}

// ---------------------------------------------------------------------------
// Usage: what a delete refuses over, readable before the delete is tried
// ---------------------------------------------------------------------------

/// `GET /usage/{kind}/{id}` and `DELETE` must be two views of one answer. If
/// they could disagree, a client would grey out the wrong button — or worse,
/// enable the right-looking one and get a 400 it had no way to predict.
#[tokio::test(flavor = "multi_thread")]
async fn usage_agrees_with_what_delete_refuses() {
    let node = Node::start().await;

    let v = node
        .post(
            "/agents",
            json!({"name": "Scribe", "system_prompt": "You write.",
                   "harness": "claude-code"}),
        )
        .await;
    let agent = v["agent"]["id"].as_str().unwrap().to_string();

    // Nothing points at it yet: an empty list, and the delete would succeed.
    let v = node.get(&format!("/usage/agent/{agent}")).await;
    assert_eq!(v["usage"], json!([]));

    node.post(
        "/teams",
        json!({"name": "Engineering", "members": [{"agent": agent}]}),
    )
    .await;

    // The reference reads as a person would say it — the team's name, not its
    // ULID, which is the whole reason `label` exists.
    let v = node.get(&format!("/usage/agent/{agent}")).await;
    let usage = v["usage"].as_array().expect("a usage array");
    assert_eq!(usage.len(), 1, "{v}");
    assert_eq!(usage[0]["kind"], json!("team"));
    assert_eq!(usage[0]["label"], json!("Engineering"));

    // And the delete refuses with a 409 naming that same holder.
    let (code, v) = node.req("DELETE", &format!("/agents/{agent}"), None).await;
    assert_eq!(code, 409, "{v}");
    let msg = v["error"].as_str().unwrap();
    assert!(msg.contains("team Engineering"), "{msg}");

    // A skill nothing carries is deletable; one an agent carries is not, and
    // the refusal survives the round trip through the store's error mapping.
    node.post(
        "/skills",
        json!({"id": "house-style", "name": "House style",
               "description": "Use this when writing anything a human reads.",
               "markdown": "# House style\n\nBe terse."}),
    )
    .await;
    let v = node.get("/usage/skill/house-style").await;
    assert_eq!(v["usage"], json!([]));
    node.post(&format!("/agents/{agent}/skills/house-style"), json!({}))
        .await;
    let v = node.get("/usage/skill/house-style").await;
    assert_eq!(v["usage"][0]["kind"], json!("agent"));
    assert_eq!(v["usage"][0]["label"], json!("Scribe"));

    let (code, v) = node.req("DELETE", "/skills/house-style", None).await;
    assert_eq!(code, 409, "{v}");
    assert!(v["error"].as_str().unwrap().contains("agent Scribe"), "{v}");

    node.shutdown().await;
}

/// The vocabulary, not just "unknown": this is the surface where a hand-typed
/// path is mistyped, so the answer says what would have worked.
#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_usage_kind_is_a_400_naming_all_five() {
    let node = Node::start().await;
    let (code, v) = node.req("GET", "/usage/pet/whatever", None).await;
    assert_eq!(code, 400, "{v}");
    let msg = v["error"].as_str().unwrap();
    for kind in ["agent", "team", "skill", "mcp", "channel"] {
        assert!(msg.contains(kind), "{msg} omits {kind}");
    }

    // An id of a known kind that does not exist is a refusal too, and not an
    // empty list: "nothing references it" for a typo reads as permission.
    let (code, v) = node.req("GET", "/usage/agent/no-such-agent", None).await;
    assert_eq!(code, 404, "a usage question about nobody is not found: {v}");

    node.shutdown().await;
}

/// Per-file staging over HTTP: three files touched, one committed, and the
/// other two still sitting there afterwards.
///
/// This is the test the whole per-file surface exists for. Committing a
/// project used to mean `add --all`, which meant that anyone who had a second
/// edit open in the folder committed it too, under somebody else's message.
#[tokio::test(flavor = "multi_thread")]
async fn a_project_commits_the_files_a_person_picked_and_leaves_the_rest() {
    let node = Node::start().await;
    let goal = node.new_goal("keep a repository").await.to_string();
    let (pid, root) = node.git_project(&goal, "shop", "gated").await;

    // Three edits open at once, one of them under a name with a space and an
    // accent in it — not an edge case, a Tuesday.
    let accented = "notes de réunion.txt";
    std::fs::write(root.join("cart.rs"), "fn total() {}\n").unwrap();
    std::fs::write(root.join("README.md"), "baseline + cart\n").unwrap();
    std::fs::write(root.join(accented), "première ligne\n").unwrap();

    let v = node.get(&format!("/workstreams/{pid}/git/files")).await;
    assert_eq!(v["clean"], json!(false));
    let listed: Vec<String> = v["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(listed.len(), 3, "{listed:?}");
    assert!(listed.contains(&accented.to_string()), "{listed:?}");
    assert!(
        v["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["staged"] == json!(false) && f["unstaged"] == json!(true)),
        "nothing is staged until somebody stages it: {v}"
    );
    // Both halves of git's answer are kept: an edit to a tracked file and a
    // file git has never seen are both changes, and not the same kind.
    let by = |p: &str| {
        v["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["path"] == json!(p))
            .unwrap_or_else(|| panic!("no row for {p} in {v}"))
            .clone()
    };
    assert_eq!(by("README.md")["untracked"], json!(false));
    assert_eq!(by("README.md")["worktree"], json!("M"));
    assert_eq!(by("cart.rs")["untracked"], json!(true));

    // A GET must not stage: a brand-new file has no patch, and saying so is
    // the honest answer rather than quietly staging to produce one.
    let v = node
        .get(&format!("/workstreams/{pid}/git/diff?path=cart.rs"))
        .await;
    assert_eq!(v["untracked"], json!(true));
    assert_eq!(v["diff"], json!(""));
    assert_eq!(
        node.get(&format!("/workstreams/{pid}/git/files")).await["files"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["staged"] == json!(true))
            .count(),
        0,
        "reading a diff staged something"
    );

    // The two sides of a file git has never seen: nothing on the left, the
    // file on the right — and still nothing staged.
    let v = node
        .get(&format!("/workstreams/{pid}/git/sides?path=cart.rs"))
        .await;
    assert_eq!(v["original"], serde_json::Value::Null, "{v}");
    assert!(
        v["modified"].as_str().unwrap().contains("fn total()"),
        "{v}"
    );
    assert_eq!(v["binary"], json!(false));
    assert_eq!(v["truncated"], json!(false));
    assert_eq!(
        node.get(&format!("/workstreams/{pid}/git/files")).await["files"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["staged"] == json!(true))
            .count(),
        0,
        "reading the sides staged something"
    );
    // An edited tracked file: the index's text against the tree's.
    let v = node
        .get(&format!("/workstreams/{pid}/git/sides?path=README.md"))
        .await;
    assert!(
        v["original"].as_str().is_some(),
        "the index holds README.md: {v}"
    );
    assert_ne!(
        v["original"], v["modified"],
        "the tree moved on from the index"
    );

    // Stage exactly one.
    let v = node
        .post(
            &format!("/workstreams/{pid}/git/stage"),
            json!({"paths": ["cart.rs"]}),
        )
        .await;
    let staged_now: Vec<String> = v["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["staged"] == json!(true))
        .map(|f| f["path"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(staged_now, vec!["cart.rs".to_string()], "{v}");

    // Now it has a patch, and only its own.
    let v = node
        .get(&format!(
            "/workstreams/{pid}/git/diff?path=cart.rs&staged=true"
        ))
        .await;
    assert!(v["diff"].as_str().unwrap().contains("+fn total()"), "{v}");
    assert!(!v["diff"].as_str().unwrap().contains("baseline + cart"));
    // The staged sides: nothing at HEAD for a new file, the index on the right.
    let v = node
        .get(&format!(
            "/workstreams/{pid}/git/sides?path=cart.rs&staged=true"
        ))
        .await;
    assert_eq!(v["original"], serde_json::Value::Null, "{v}");
    assert!(
        v["modified"].as_str().unwrap().contains("fn total()"),
        "{v}"
    );

    // Unstaging puts it back, and does not touch the file.
    let v = node
        .post(
            &format!("/workstreams/{pid}/git/unstage"),
            json!({"paths": ["cart.rs"]}),
        )
        .await;
    assert!(
        v["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["staged"] == json!(false)),
        "{v}"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("cart.rs")).unwrap(),
        "fn total() {}\n",
        "unstaging must never write to the working tree"
    );

    // Committing with no paths and nothing staged refuses rather than
    // sweeping the folder in.
    let (code, v) = node
        .req(
            "POST",
            &format!("/workstreams/{pid}/git/commit"),
            Some(json!({"message": "everything, surely"})),
        )
        .await;
    assert_eq!(code, 409, "{v}");
    assert!(v["error"].as_str().unwrap().contains("nothing to commit"));
    assert_eq!(
        node.get(&format!("/workstreams/{pid}/git/files")).await["files"]
            .as_array()
            .unwrap()
            .len(),
        3,
        "a refused commit changed nothing"
    );

    // Commit the selection: two files named, the third left alone.
    let v = node
        .post(
            &format!("/workstreams/{pid}/git/commit"),
            json!({"message": "add the cart total", "paths": ["cart.rs", accented]}),
        )
        .await;
    assert!(!v["short"].as_str().unwrap().is_empty(), "{v}");

    let v = node.get(&format!("/workstreams/{pid}/git/files")).await;
    let left: Vec<String> = v["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(left, vec!["README.md".to_string()], "{v}");
    assert_eq!(
        std::fs::read_to_string(root.join("README.md")).unwrap(),
        "baseline + cart\n",
        "the file nobody picked is untouched"
    );
    let recorded = raw_git(&root, &["show", "--name-only", "--format=", "HEAD"]);
    assert!(recorded.contains("cart.rs"), "{recorded}");
    assert!(
        recorded.contains("de r"),
        "the accented name too: {recorded}"
    );
    assert!(!recorded.contains("README.md"), "{recorded}");

    // A clean tree refuses by name and changes nothing.
    node.post(
        &format!("/workstreams/{pid}/git/commit"),
        json!({"message": "the rest", "paths": ["README.md"]}),
    )
    .await;
    let head_before = raw_git(&root, &["rev-parse", "HEAD"]);
    let (code, v) = node
        .req(
            "POST",
            &format!("/workstreams/{pid}/git/commit"),
            Some(json!({"message": "again"})),
        )
        .await;
    assert_eq!(code, 409, "{v}");
    assert_eq!(raw_git(&root, &["rev-parse", "HEAD"]), head_before);
    assert_eq!(
        node.get(&format!("/workstreams/{pid}/git/files")).await["clean"],
        json!(true)
    );

    node.shutdown().await;
}

/// A commit message the agent could not write leaves the user an empty box
/// and a sentence — never a fabricated message.
///
/// These tests run with an empty harness catalog, which is exactly the
/// machine that has no agent runner installed. The route must still answer.
#[tokio::test(flavor = "multi_thread")]
async fn a_suggestion_with_no_harness_is_empty_and_says_why() {
    let node = Node::start().await;
    let goal = node.new_goal("write my messages").await.to_string();
    let (pid, root) = node.git_project(&goal, "drafts", "gated").await;

    // Nothing staged: there is no change to describe, and that is its own
    // answer rather than a guess.
    let v = node
        .post(&format!("/workstreams/{pid}/git/message"), json!({}))
        .await;
    assert_eq!(v["suggested"], json!(false));
    assert_eq!(v["message"], json!(""));
    assert!(
        v["error"].as_str().unwrap().contains("nothing is staged"),
        "{v}"
    );

    // Something staged, still no harness on the host.
    std::fs::write(root.join("draft.md"), "a change worth describing\n").unwrap();
    node.post(
        &format!("/workstreams/{pid}/git/stage"),
        json!({"paths": ["draft.md"]}),
    )
    .await;
    let v = node
        .post(&format!("/workstreams/{pid}/git/message"), json!({}))
        .await;
    assert_eq!(v["suggested"], json!(false));
    assert_eq!(v["message"], json!(""), "never a message nobody wrote");
    assert!(
        v["error"]
            .as_str()
            .unwrap()
            .contains("no session for agent"),
        "{v}"
    );
    assert_eq!(v["agent"], json!("general-agent"));

    // And asking did not commit anything.
    assert!(raw_git(&root, &["log", "--oneline"]).lines().count() == 1);

    node.shutdown().await;
}

/// The project's own root is its primary workstream: it shares the
/// project's id, it is listed first, its git surface answers under
/// `/workstreams/{wid}/git/…`, and it is never closed on its own.
#[tokio::test(flavor = "multi_thread")]
async fn the_primary_workstream_is_the_project_itself() {
    let node = Node::start().await;
    let goal = node.new_goal("ship the storefront").await.to_string();
    let (pid, root) = node.git_project(&goal, "storefront", "auto").await;

    // Listed first, under the project's own id, with the root as its checkout.
    let v = node.get(&format!("/projects/{pid}/workstreams")).await;
    let rows = v["workstreams"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "a fresh project has exactly its primary");
    assert_eq!(rows[0]["id"], json!(pid));
    assert_eq!(rows[0]["kind"]["kind"], json!("primary"));
    assert_eq!(v["checkouts"][0]["path"], json!(root.display().to_string()));
    assert_eq!(v["checkouts"][0]["exists"], json!(true));

    // Its status carries the project's policy and default branch.
    let v = node.get(&format!("/workstreams/{pid}/status")).await;
    assert_eq!(v["status"]["kind"]["kind"], json!("primary"));
    assert_eq!(v["status"]["publish"], json!("auto"));
    assert!(v["status"]["default_branch"].as_str().is_some());
    assert_eq!(v["status"]["base"], Value::Null, "the primary has no base");
    assert_eq!(v["status"]["running_agents"], json!(0));

    // The git surface is keyed by the workstream; the primary's id is the project's.
    let v = node.get(&format!("/workstreams/{pid}/git/files")).await;
    assert_eq!(v["workstream"], json!(pid));
    assert_eq!(v["clean"], json!(true));
    let v = node.get(&format!("/workstreams/{pid}/git/status")).await;
    assert_eq!(v["workstream"], json!(pid));
    assert_eq!(v["project"], json!(pid));

    // The project scope is gone from the tree routes; the primary answers instead.
    let (code, _) = node.req("GET", &format!("/tree/project/{pid}"), None).await;
    assert_eq!(code, 400, "`project` is not a scope any more");
    let v = node.get(&format!("/placement/workstream/{pid}")).await;
    assert_eq!(v["path"], json!(root.display().to_string()));

    // Deleting the primary is a 409 that says what to do instead.
    let (code, v) = node
        .req("DELETE", &format!("/workstreams/{pid}"), None)
        .await;
    assert_eq!(code, 409, "{v}");
    assert!(
        v["error"]
            .as_str()
            .unwrap()
            .contains("remove the project instead"),
        "{v}"
    );
    assert!(node.get(&format!("/workstreams/{pid}")).await["workstream"]["id"] == json!(pid));

    // Removing the project takes the primary's record with it.
    node.delete(&format!("/projects/{pid}")).await;
    let (code, _) = node.req("GET", &format!("/workstreams/{pid}"), None).await;
    assert_eq!(code, 404, "the primary went with its project");

    node.shutdown().await;
}

/// The person-editable identity of a project and a workstream (W2):
/// group and photo on the project, name / note / pinned on the workstream —
/// each set by a value and cleared by `null`, and nothing else editable there.
#[tokio::test(flavor = "multi_thread")]
async fn projects_and_workstreams_carry_a_persons_labels() {
    let node = Node::start().await;
    let goal = node.new_goal("label things").await.to_string();
    let (pid, _root) = node.git_project(&goal, "storefront", "auto").await;

    // A project joins a group and gets a picture — one this machine holds,
    // a picture by its bytes; both come back on the record.
    let png = node
        .ws
        .put_attachment(b"\x89PNG\r\n\x1a\nIHDR-tiny", "logo.png", "image/png")
        .unwrap();
    let sha = png.sha256.clone();
    let v = node
        .post_patch(
            &format!("/projects/{pid}"),
            json!({"group": " Shop ", "photo": {"sha256": sha, "name": "logo.png", "mime": "image/png", "size": png.size}}),
        )
        .await;
    assert_eq!(v["project"]["group"], json!("Shop"), "trimmed, kept");
    assert_eq!(v["project"]["photo"]["sha256"], json!(sha));
    let v = node.get(&format!("/projects/{pid}")).await;
    assert_eq!(v["project"]["group"], json!("Shop"));

    // A photo must be a content hash.
    let (code, _) = node
        .req(
            "PATCH",
            &format!("/projects/{pid}"),
            Some(json!({"photo": {"sha256": "nope", "name": "x", "mime": "image/png", "size": 1}})),
        )
        .await;
    assert_eq!(code, 400);

    // …that this machine holds, that is a picture, and that is small (ide/14 §Photos).
    let (code, v) = node
        .req(
            "PATCH",
            &format!("/projects/{pid}"),
            Some(json!({"photo": {"sha256": "b".repeat(64), "name": "x.png", "mime": "image/png", "size": 1}})),
        )
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(
        v["error"].as_str().unwrap().contains("not on this machine"),
        "{v}"
    );
    let text = node
        .ws
        .put_attachment(b"<html>not a picture</html>", "photo.png", "image/png")
        .unwrap();
    let (code, v) = node
        .req(
            "PATCH",
            &format!("/projects/{pid}"),
            Some(json!({"photo": {"sha256": text.sha256, "name": "photo.png", "mime": "image/png", "size": text.size}})),
        )
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(
        v["error"].as_str().unwrap().contains("not a picture"),
        "the bytes, never the name: {v}"
    );
    let mut big = b"\x89PNG\r\n\x1a\n".to_vec();
    big.resize((bisa_core::MAX_PHOTO_BYTES + 1) as usize, 0);
    let huge = node
        .ws
        .put_attachment(&big, "huge.png", "image/png")
        .unwrap();
    let (code, v) = node
        .req(
            "PATCH",
            &format!("/projects/{pid}"),
            Some(json!({"photo": {"sha256": huge.sha256, "name": "huge.png", "mime": "image/png", "size": huge.size}})),
        )
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(v["error"].as_str().unwrap().contains("the limit is"), "{v}");
    let v = node.get(&format!("/projects/{pid}")).await;
    assert_eq!(
        v["project"]["photo"]["sha256"],
        json!(sha),
        "a refusal changes nothing"
    );

    // `null` clears; an omitted field is left alone.
    let v = node
        .post_patch(&format!("/projects/{pid}"), json!({"group": null}))
        .await;
    assert_eq!(v["project"]["group"], Value::Null);
    assert_eq!(
        v["project"]["photo"]["sha256"],
        json!(sha),
        "photo untouched"
    );

    // The primary workstream takes a name and a note, and can be pinned.
    let v = node
        .post_patch(
            &format!("/workstreams/{pid}"),
            json!({"name": "Main line", "note": "where releases are cut", "pinned": true}),
        )
        .await;
    assert_eq!(v["workstream"]["name"], json!("Main line"));
    assert_eq!(v["workstream"]["note"], json!("where releases are cut"));
    assert_eq!(v["workstream"]["pinned"], json!(true));
    let v = node.get(&format!("/workstreams/{pid}/status")).await;
    assert_eq!(
        v["status"]["name"],
        json!("Main line"),
        "the status carries the label"
    );
    let v = node
        .post_patch(&format!("/workstreams/{pid}"), json!({"name": null}))
        .await;
    assert_eq!(v["workstream"]["name"], Value::Null, "back to its branch");
    assert_eq!(
        v["workstream"]["pinned"],
        json!(true),
        "pinned was not mentioned"
    );

    node.shutdown().await;
}

/// The Board over HTTP (ide/16): a due date joins the person-editable
/// fields, a place puts a card at an index in a column and answers every
/// record it rewrote, a bad day is refused with the reason, and a closed
/// workstream is Archived and nothing else.
#[tokio::test(flavor = "multi_thread")]
async fn the_board_places_cards_and_keeps_due_dates() {
    let node = Node::start().await;
    let goal = node.new_goal("board things").await.to_string();
    let (pid, _root) = node.git_project(&goal, "storefront", "auto").await;
    let v = node
        .post(
            &format!("/projects/{pid}/workstreams"),
            json!({"label": "checkout"}),
        )
        .await;
    let wid = v["workstream"]["id"]
        .as_str()
        .expect("a workstream")
        .to_string();

    // Fresh: no slot on the record at all.
    let v = node.get("/workstreams").await;
    let rows = v["workstreams"].as_array().unwrap();
    assert!(
        rows.iter().all(|r| r["workstream"].get("board").is_none()),
        "{v}"
    );

    // A due date, set and cleared; the other labels untouched.
    let v = node
        .post_patch(
            &format!("/workstreams/{wid}"),
            json!({"due": "2026-09-30", "pinned": true}),
        )
        .await;
    assert_eq!(v["workstream"]["board"]["due"], json!("2026-09-30"));
    assert_eq!(v["workstream"]["pinned"], json!(true));
    let (code, v) = node
        .req(
            "PATCH",
            &format!("/workstreams/{wid}"),
            Some(json!({"due": "2026-02-30"})),
        )
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(v["error"].as_str().unwrap().contains("YYYY-MM-DD"), "{v}");
    let v = node
        .post_patch(&format!("/workstreams/{wid}"), json!({"due": null}))
        .await;
    assert!(v["workstream"]["board"].get("due").is_none(), "{v}");
    assert_eq!(
        v["workstream"]["pinned"],
        json!(true),
        "pinned was not mentioned"
    );

    // Place the primary in Todo, then the branch before it.
    let v = node
        .put(
            &format!("/workstreams/{pid}/board/place"),
            json!({"column": "todo", "index": 0}),
        )
        .await;
    assert_eq!(v["workstreams"].as_array().unwrap().len(), 1);
    assert_eq!(
        v["workstreams"][0]["board"],
        json!({"column": "todo", "rank": 1024})
    );
    let v = node
        .put(
            &format!("/workstreams/{wid}/board/place"),
            json!({"column": "todo", "index": 0}),
        )
        .await;
    assert_eq!(
        v["workstreams"][0]["board"]["rank"],
        json!(512),
        "before the first"
    );
    assert_eq!(
        v["workstreams"][0]["state"]["state"],
        json!("open"),
        "a column is not the state"
    );

    // An unknown column, and a closed card asked for anywhere but Archived.
    let (code, _) = node
        .req(
            "PUT",
            &format!("/workstreams/{wid}/board/place"),
            Some(json!({"column": "later", "index": 0})),
        )
        .await;
    assert_eq!(code, 400);
    let (code, _) = node
        .req("DELETE", &format!("/workstreams/{wid}"), None)
        .await;
    assert_eq!(code, 200);
    let (code, v) = node
        .req(
            "PUT",
            &format!("/workstreams/{wid}/board/place"),
            Some(json!({"column": "doing", "index": 0})),
        )
        .await;
    assert_eq!(code, 400, "{v}");
    let v = node
        .put(
            &format!("/workstreams/{wid}/board/place"),
            json!({"column": "archived", "index": 0}),
        )
        .await;
    assert_eq!(v["workstreams"][0]["board"]["column"], json!("archived"));

    node.shutdown().await;
}

/// A proposal — recorded through `PUT /goals/{id}/workflow` with a
/// definition — is the goal's design: listed under the goal, absent from
/// the library, present under `?scope=all`, and named in the `GoalView`'s
/// `designs`.
#[tokio::test(flavor = "multi_thread")]
async fn a_goals_design_is_listed_under_its_goal_and_not_in_the_library() {
    let node = Node::start().await;
    let goal = node.new_goal("design me").await;
    let v = node
        .put(
            &format!("/goals/{goal}/workflow"),
            json!({"definition": human_workflow()}),
        )
        .await;
    let wfid = v["workflow"]["id"].as_str().unwrap().to_string();

    let library = node.get("/workflows").await;
    assert!(
        library["workflows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["workflow"]["id"] != json!(wfid)),
        "the library never shows a goal's design"
    );
    let all = node.get("/workflows?scope=all").await;
    assert!(all["workflows"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["workflow"]["id"] == json!(wfid)));
    let goals_own = node.get(&format!("/workflows?goal={goal}")).await;
    assert_eq!(goals_own["workflows"].as_array().unwrap().len(), 1);

    let view = node.get(&format!("/goals/{goal}")).await;
    assert_eq!(view["designs"].as_array().unwrap().len(), 1);
    assert_eq!(view["designs"][0]["workflow"]["id"], json!(wfid));
    assert_eq!(
        view["designs"][0]["workflow"]["origin"],
        json!({"origin": "goal", "goal": goal.to_string()})
    );
    node.shutdown().await;
}

/// Promote copies a design into the library under a new id; the original
/// stays the goal's, and a library workflow refuses with a 409.
#[tokio::test(flavor = "multi_thread")]
async fn promote_puts_a_copy_in_the_library_and_refuses_a_library_workflow() {
    let node = Node::start().await;
    let goal = node.new_goal("promote me").await;
    let v = node
        .put(
            &format!("/goals/{goal}/workflow"),
            json!({"definition": human_workflow()}),
        )
        .await;
    let wfid = v["workflow"]["id"].as_str().unwrap().to_string();

    let promoted = node
        .post(&format!("/workflows/{wfid}/promote"), json!({}))
        .await;
    let copy = promoted["workflow"]["id"].as_str().unwrap().to_string();
    assert_ne!(copy, wfid);
    assert_eq!(
        promoted["workflow"]["origin"],
        json!({"origin": "workspace"})
    );
    let library = node.get("/workflows").await;
    assert!(library["workflows"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["workflow"]["id"] == json!(copy)));

    let (code, v) = node
        .req(
            "POST",
            &format!("/workflows/{copy}/promote"),
            Some(json!({})),
        )
        .await;
    assert_eq!(code, 409, "already in the library: {v}");
    assert!(
        v["error"]
            .as_str()
            .unwrap()
            .contains("already in the library"),
        "{v}"
    );
    node.shutdown().await;
}

/// A body with a key the wire does not have is a 400 in the one error shape,
/// never a definition with the key silently dropped.
#[tokio::test(flavor = "multi_thread")]
async fn unknown_body_key_is_400() {
    let node = Node::start().await;
    let mut typo = human_workflow();
    typo["descripton"] = json!("typo");
    let (code, v) = node.req("POST", "/workflows", Some(typo)).await;
    assert_eq!(code, 400, "{v}");
    assert!(v["error"].as_str().unwrap().contains("descripton"), "{v}");

    // Inside a step, too.
    let mut step_typo = human_workflow();
    step_typo["steps"][0]["retires"] = json!(1);
    let (code, v) = node
        .req("POST", "/workflows/validate", Some(step_typo))
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(v["error"].as_str().unwrap().contains("retires"), "{v}");

    // And on a goal's body.
    let goal = node.new_goal("typo").await;
    let (code, v) = node
        .req(
            "PUT",
            &format!("/goals/{goal}/workflow"),
            Some(json!({"workflow": null, "extra": 1})),
        )
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(v["error"].as_str().unwrap().contains("extra"), "{v}");
    let (code, v) = node
        .req(
            "POST",
            &format!("/goals/{goal}/decide"),
            Some(json!({"approve": true, "approved": true})),
        )
        .await;
    assert_eq!(code, 400, "{v}");
    node.shutdown().await;
}

/// A goal's design is a draft: saved with its problems, pointed at, refused
/// only at start. A second save names the revision it edited.
#[tokio::test(flavor = "multi_thread")]
async fn design_with_problems_saves_and_returns_them() {
    let node = Node::start().await;
    let goal = node.new_goal("design me").await;
    let broken = json!({
        "name": "Half drawn",
        "steps": [{
            "id": "a", "name": "A", "kind": "human", "prompt": "?", "then": ["nowhere"]
        }]
    });
    let v = node
        .put(
            &format!("/goals/{goal}/workflow"),
            json!({"definition": broken}),
        )
        .await;
    let design = v["workflow"]["id"].as_str().unwrap().to_string();
    assert_eq!(v["workflow"]["revision"], json!(1));
    assert!(
        v["problems"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["kind"] == json!("unknown_step")),
        "{v}"
    );
    assert_eq!(v["goal"]["workflow"], json!(design));

    // A start refuses, with the problems typed.
    let (code, v) = node
        .req("POST", &format!("/goals/{goal}/run"), Some(json!({})))
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(v["error"].as_str().unwrap().contains("nowhere"), "{v}");
    assert_eq!(v["problems"][0]["kind"], json!("unknown_step"), "{v}");
    assert_eq!(v["problems"][0]["step"], json!("a"), "{v}");

    // The next save must name the revision it edited.
    let (code, v) = node
        .req(
            "PUT",
            &format!("/goals/{goal}/workflow"),
            Some(json!({"definition": human_workflow()})),
        )
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(
        v["error"].as_str().unwrap().contains("send the revision"),
        "{v}"
    );
    let (code, v) = node
        .req(
            "PUT",
            &format!("/goals/{goal}/workflow"),
            Some(json!({"definition": human_workflow(), "revision": 7})),
        )
        .await;
    assert_eq!(code, 409, "{v}");
    let v = node
        .put(
            &format!("/goals/{goal}/workflow"),
            json!({"definition": human_workflow(), "revision": 1}),
        )
        .await;
    assert_eq!(
        v["workflow"]["id"],
        json!(design),
        "the same design, edited"
    );
    assert_eq!(v["workflow"]["revision"], json!(2));
    assert!(v["problems"].as_array().unwrap().is_empty());
    node.post(&format!("/goals/{goal}/run"), json!({})).await;
    node.shutdown().await;
}

/// An agent step with no project on a goal with two is refused at start as
/// the caller's to fix — a 400 that names the step — not a 500.
#[tokio::test(flavor = "multi_thread")]
async fn project_ambiguous_is_400() {
    let node = Node::start().await;
    let goal = node.new_goal("two homes").await;
    for slug in ["alpha", "beta"] {
        node.post(
            &format!("/goals/{goal}/projects"),
            json!({"kind": "new", "slug": slug}),
        )
        .await;
    }
    let wf = node.workflow(agent_workflow()).await;
    node.req(
        "PUT",
        &format!("/goals/{goal}/workflow"),
        Some(json!({"workflow": wf.to_string()})),
    )
    .await;
    let (code, v) = node
        .req("POST", &format!("/goals/{goal}/run"), Some(json!({})))
        .await;
    assert_eq!(code, 400, "{v}");
    let message = v["error"].as_str().unwrap();
    assert!(message.contains("`build`"), "{message}");
    assert!(message.contains("names no project"), "{message}");
    assert_eq!(
        node.get(&format!("/goals/{goal}")).await["run"],
        json!(null),
        "nothing started"
    );
    node.shutdown().await;
}

/// An amendment over HTTP is validated like a save: problems come back typed
/// as a 400, and a new required input without a default is refused before
/// anything is written.
#[tokio::test(flavor = "multi_thread")]
async fn amend_over_http_validates_and_returns_problems() {
    let node = Node::start().await;
    let goal = node.new_goal("amend me").await;
    let held = json!({
        "name": "Held",
        "steps": [
            {"id": "hold", "name": "Hold", "kind": "wait", "until": {"until": "release"}, "then": ["old"]},
            {"id": "old", "name": "Old", "kind": "agent", "instructions": "old way",
             "harness": ["no-such-harness"]}
        ]
    });
    let wf = node.workflow(held).await;
    let run = node.start_run(goal, wf).await;
    let before = node.get(&format!("/goals/{goal}")).await["run"]["workflow"]["revision"].clone();

    // A dangling flow: the problems come back by step and kind.
    let dangling = json!({
        "name": "Held",
        "steps": [
            {"id": "hold", "name": "Hold", "kind": "wait", "until": {"until": "release"}, "then": ["nowhere"]}
        ]
    });
    let (code, v) = node
        .req(
            "POST",
            &format!("/goals/{goal}/amend"),
            Some(json!({"workflow": dangling})),
        )
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(
        v["problems"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["kind"] == json!("unknown_step") && p["step"] == json!("hold")),
        "{v}"
    );

    // A new required input with no default: the run cannot bind it.
    let needs_input = json!({
        "name": "Held",
        "inputs": [{"name": "who", "label": "Who", "kind": "text", "required": true}],
        "steps": [
            {"id": "hold", "name": "Hold", "kind": "wait", "until": {"until": "release"}, "then": ["ask"]},
            {"id": "ask", "name": "Ask", "kind": "approval", "prompt": "Ship for {inputs.who}?"}
        ]
    });
    let (code, v) = node
        .req(
            "POST",
            &format!("/goals/{goal}/amend"),
            Some(json!({"workflow": needs_input})),
        )
        .await;
    assert_eq!(code, 409, "the run refused the amendment: {v}");
    assert!(v["error"].as_str().unwrap().contains("who"), "{v}");
    let after = node.get(&format!("/goals/{goal}")).await;
    assert_eq!(after["run"]["id"], json!(run.to_string()));
    assert_eq!(
        after["run"]["workflow"]["revision"], before,
        "nothing was written"
    );

    // With a default it lands, and the run holds the value.
    let with_default = json!({
        "name": "Held",
        "inputs": [{"name": "who", "label": "Who", "kind": "text", "default": "everyone"}],
        "steps": [
            {"id": "hold", "name": "Hold", "kind": "wait", "until": {"until": "release"}, "then": ["ask"]},
            {"id": "ask", "name": "Ask", "kind": "approval", "prompt": "Ship for {inputs.who}?"}
        ]
    });
    let v = node
        .post(
            &format!("/goals/{goal}/amend"),
            json!({"workflow": with_default}),
        )
        .await;
    assert_eq!(v["run"]["inputs"]["who"], json!("everyone"));
    node.shutdown().await;
}

/// The three ways a project is born each leave their mark: the route's goal,
/// an agent step's provenance, or the workspace alone.
#[tokio::test(flavor = "multi_thread")]
async fn a_project_says_where_it_was_born() {
    let node = Node::start().await;
    // By hand, no goal: the workspace's.
    let hand = node
        .post("/projects", json!({"kind": "new", "slug": "by-hand"}))
        .await;
    assert_eq!(hand["project"]["origin"], json!({"origin": "workspace"}));

    // From a goal's route: the goal's.
    let goal = node.new_goal("give me a folder").await;
    let of_goal = node
        .post(
            &format!("/goals/{goal}/projects"),
            json!({"kind": "new", "slug": "from-goal"}),
        )
        .await;
    assert_eq!(
        of_goal["project"]["origin"],
        json!({"origin": "goal", "goal": goal.to_string()})
    );
    node.shutdown().await;
}

/// Every goal row carries its strip and its holder, `GET /goals/{id}` agrees
/// with the list, and a draft with a chosen workflow ghosts its steps.
/// The row a goal list draws its run verbs from, and the runs list.
fn goal_row_of(rows: &Value, goal: GoalId) -> Value {
    rows["goals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == json!(goal.to_string()))
        .expect("the goal's row")
        .clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_goal_is_stopped_restarted_and_its_queue_withdrawn_over_http() {
    let node = Node::start().await;
    let goal = node.new_goal("stop and restart me").await;
    let wf = node.workflow(human_workflow()).await;
    node.put(&format!("/goals/{goal}/workflow"), json!({"workflow": wf}))
        .await;
    let first = node.start_run(goal, wf).await;
    let v = node.post(&format!("/goals/{goal}/run"), json!({})).await;
    assert_eq!(v["status"], json!("queued"));
    let queued: RunId = serde_json::from_value(v["run"]["id"].clone()).unwrap();

    // The row says what the card's verbs need: the run's status, the queue.
    let row = goal_row_of(&node.get("/goals").await, goal);
    assert_eq!(row["run_status"], json!("waiting"));
    assert_eq!(row["queued"], json!(1));

    // Stop: the live run cancelled with the rationale, the queue withdrawn,
    // the goal open and a draft again.
    let v = node
        .post(
            &format!("/goals/{goal}/stop"),
            json!({"rationale": "enough"}),
        )
        .await;
    assert_eq!(v["stopped"], json!(first.to_string()));
    assert_eq!(v["withdrawn"], json!([queued.to_string()]));
    let v = node.get(&format!("/goals/{goal}")).await;
    assert_eq!(v["status"], json!("draft"));
    assert!(v["goal"]["closed"].is_null());
    assert_eq!(
        v["run"]["cancelled"],
        json!({"cause": "stopped", "rationale": "enough"})
    );
    let runs = node.get(&format!("/goals/{goal}/runs")).await;
    let runs = runs["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0]["cause"], json!({"cause": "withdrawn"}));
    assert_eq!(
        runs[1]["cause"],
        json!({"cause": "stopped", "rationale": "enough"})
    );
    assert!(runs.iter().all(|r| r["status"] == json!("cancelled")));
    let row = goal_row_of(&node.get("/goals").await, goal);
    assert_eq!(row["run_status"], json!("cancelled"));
    assert_eq!(row["queued"], json!(0));
    // Nothing left to stop: both empty, still 200.
    let v = node.post(&format!("/goals/{goal}/stop"), json!({})).await;
    assert!(v["stopped"].is_null() && v["withdrawn"] == json!([]), "{v}");

    // Restart: a new live run of the same workflow, at once.
    let v = node
        .post(&format!("/goals/{goal}/restart"), json!({}))
        .await;
    assert_eq!(v["status"], json!("waiting"), "{v}");
    let restarted: RunId = serde_json::from_value(v["run"]["id"].clone()).unwrap();
    assert_ne!(restarted, first);
    let v = node.get(&format!("/goals/{goal}")).await;
    assert_eq!(v["run"]["id"], json!(restarted.to_string()));
    assert_eq!(v["runs"].as_array().unwrap().len(), 3);
    // Restarting a live run cancels it as `restarted` and starts another.
    let v = node
        .post(&format!("/goals/{goal}/restart"), json!({}))
        .await;
    let again: RunId = serde_json::from_value(v["run"]["id"].clone()).unwrap();
    assert_ne!(again, restarted);
    let v = node.get(&format!("/goals/{goal}/runs/{restarted}")).await;
    assert_eq!(v["run"]["cancelled"], json!({"cause": "restarted"}));

    // Closed: nothing to stop or restart, said as a conflict.
    node.post(&format!("/goals/{goal}/close"), json!({})).await;
    let (code, _) = node
        .req("POST", &format!("/goals/{goal}/stop"), Some(json!({})))
        .await;
    assert_eq!(code, 409);
    let (code, _) = node
        .req("POST", &format!("/goals/{goal}/restart"), Some(json!({})))
        .await;
    assert_eq!(code, 409);
    // A withdrawal names a run of another goal: not found.
    let other = node.new_goal("other").await;
    let (code, _) = node
        .req("DELETE", &format!("/goals/{other}/runs/{again}"), None)
        .await;
    assert_eq!(code, 404);
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn goal_rows_carry_strip_holder_and_last_activity() {
    let node = Node::start().await;
    let goal = node.new_goal("walk the strip").await;
    let wf = node.workflow(human_workflow()).await;
    node.put(&format!("/goals/{goal}/workflow"), json!({"workflow": wf}))
        .await;

    // Chosen, not started: yours to start, every step ghosted pending.
    let rows = node.get("/goals").await;
    let row = rows["goals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == json!(goal.to_string()))
        .unwrap()
        .clone();
    assert_eq!(row["holder"], json!("you"));
    assert_eq!(
        row["strip"]["steps"][0]["state"],
        json!({"state": "pending"})
    );
    assert_eq!(row["strip"]["total"], json!(1));

    // Started: the human step waits on you, and the view agrees with the row.
    let run = node.start_run(goal, wf).await;
    let rows = node.get("/goals").await;
    let row = rows["goals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == json!(goal.to_string()))
        .unwrap()
        .clone();
    assert_eq!(row["holder"], json!("you"));
    assert_eq!(row["strip"]["current"], json!(["ask"]));
    assert_eq!(
        row["strip"]["steps"][0]["state"],
        json!({"state": "waiting"})
    );
    assert!(row["last_activity_at"].as_u64().unwrap() > 0);
    let view = node.get(&format!("/goals/{goal}")).await;
    assert_eq!(view["holder"], row["holder"]);
    assert_eq!(view["strip"], row["strip"]);

    // Answered: the run finishes and the holder does too.
    node.post(
        &format!("/runs/{run}/steps/ask/answer"),
        json!({"answer": {"selected": ["sqlite"]}}),
    )
    .await;
    let rows = node.get("/goals").await;
    let row = rows["goals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == json!(goal.to_string()))
        .unwrap()
        .clone();
    assert_eq!(row["holder"], json!("finished"));
    assert_eq!(row["strip"]["reached"], row["strip"]["total"]);
    node.shutdown().await;
}

/// A `git` that sees only a temp global config — never the developer's helpers
/// or keychain — with, optionally, one fake credential helper: a shell script
/// printing a fixed credential, the shape osxkeychain or `gh` answers with.
fn git_with_helper(dir: &std::path::Path, fake_password: Option<&str>) -> bisa_vcs::Git {
    let global = dir.join("global.gitconfig");
    match fake_password {
        Some(password) => {
            let script = dir.join("helper.sh");
            std::fs::write(
                &script,
                format!("#!/bin/sh\nprintf 'username=%s\\npassword=%s\\n' someone '{password}'\n"),
            )
            .unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            std::fs::write(
                &global,
                format!("[credential]\n\thelper = !sh {}\n", script.display()),
            )
            .unwrap();
        }
        None => {
            std::fs::write(&global, "").unwrap();
        }
    }
    bisa_vcs::Git::new()
        .with_env("GIT_CONFIG_GLOBAL", global)
        .with_env("GIT_CONFIG_NOSYSTEM", "1")
}

/// The account routes carry facts about one kind's accounts and never a
/// token: `GET /codehost/github/accounts` lists the stored logins with where
/// each token lives, whether the environment overrides them, git's helpers
/// and the username the helper holds, the login the CLI is signed in as, and
/// the default account; a stored login's check with no GitHub to ask is a
/// refusal or an outage, never a value. Both runs use an isolated `git` with
/// a fake helper or none, and an engine given no CLI runner, so nothing here
/// reads the developer's helpers, runs `gh`, or reaches a network.
#[tokio::test(flavor = "multi_thread")]
async fn the_account_routes_never_carry_a_token_and_run_no_cli_of_the_machines() {
    let env_token = std::env::var("BISA_GITHUB_TOKEN").is_ok_and(|v| !v.trim().is_empty());
    let dir = tempfile::tempdir().unwrap();

    // No helper, nothing stored.
    let node = Node::start_with(EngineConfig {
        design_enabled: false,
        git: Some(git_with_helper(dir.path(), None)),
        ..Default::default()
    })
    .await;
    let accounts = node.get("/codehost/github/accounts").await;
    for key in [
        "env_override",
        "store",
        "accounts",
        "helpers",
        "helper_username",
        "default",
    ] {
        assert!(accounts.get(key).is_some(), "{key} in {accounts}");
    }
    assert!(
        matches!(accounts["store"].as_str(), Some("file" | "keyring")),
        "{accounts}"
    );
    assert_eq!(accounts["accounts"], serde_json::json!([]), "{accounts}");
    assert_eq!(
        accounts["helpers"],
        serde_json::json!([]),
        "an isolated git names no helper: {accounts}"
    );
    assert!(
        accounts["helper_username"].is_null() && accounts["default"].is_null(),
        "{accounts}"
    );
    assert!(
        accounts.get("token").is_none() && accounts.get("prefer_cli").is_none(),
        "{accounts}"
    );
    assert_eq!(accounts["env_override"], serde_json::json!(env_token));
    if !env_token {
        let check = node
            .post(
                "/codehost/github/accounts/nobody/check",
                serde_json::json!({}),
            )
            .await;
        assert_eq!(
            check,
            serde_json::json!({"state": "no_token"}),
            "a login with no token costs no request"
        );
    }
    // A stored account is listed by login; the default is the global
    // `codehost.account`, set through its own route.
    bisa_codehost::creds::TokenStore::file_only(
        bisa_codehost::CodeHostKind::GitHub,
        "github.com",
        node.ws.paths().codehost_tokens_root().join("github"),
    )
    .store("Octocat", "ghp_stored_for_the_test")
    .unwrap();
    let accounts = node.get("/codehost/github/accounts").await;
    assert_eq!(
        accounts["accounts"],
        serde_json::json!([{"login": "octocat", "source": "file"}]),
        "{accounts}"
    );
    let set = node
        .put(
            "/codehost/github/default",
            serde_json::json!({"login": "octocat"}),
        )
        .await;
    assert_eq!(set, serde_json::json!({"default": "octocat"}));
    assert_eq!(
        node.get("/codehost/github/accounts").await["default"],
        serde_json::json!("octocat")
    );
    let cleared = node
        .put(
            "/codehost/github/default",
            serde_json::json!({"login": null}),
        )
        .await;
    assert_eq!(cleared, serde_json::json!({"default": null}));
    let text = node.get("/codehost/github/accounts").await.to_string();
    assert!(!text.contains("ghp_"), "no token crosses: {text}");
    node.delete("/codehost/github/accounts/octocat").await;
    assert_eq!(
        node.get("/codehost/github/accounts").await["accounts"],
        serde_json::json!([])
    );
    node.shutdown().await;

    // A fake helper: git answers, the helper and its username are named —
    // and the password itself never crosses the wire.
    let node = Node::start_with(EngineConfig {
        design_enabled: false,
        git: Some(git_with_helper(dir.path(), Some("ghp_fake_from_git"))),
        ..Default::default()
    })
    .await;
    let accounts = node.get("/codehost/github/accounts").await;
    assert_eq!(accounts["helpers"], serde_json::json!(["sh"]), "{accounts}");
    assert_eq!(
        accounts["helper_username"],
        serde_json::json!("someone"),
        "{accounts}"
    );
    let text = accounts.to_string();
    assert!(
        !text.contains("ghp_") && !text.contains("gho_"),
        "no token crosses: {text}"
    );
    node.shutdown().await;
}

/// One kind's health, its sign-in plan and a remote's inspection read what the
/// machine has through the CLI runner the engine was given — a scripted fake
/// here, so no program runs — and answer facts, never a token: the CLI is
/// installed and signed in as somebody, so that somebody is who requests go
/// as and who a new repository is suggested to speak as; a kind whose CLI is
/// absent is told to install it; Bitbucket, with no CLI, is a token; a kind
/// this build does not know is a 404 that names the three.
#[tokio::test(flavor = "multi_thread")]
async fn code_host_health_sign_in_and_inspection_read_the_cli_through_its_fake_and_never_a_token() {
    use bisa_codehost::cli::fake::FakeCli;
    use bisa_codehost::cli::CliProgram;
    let env_token = std::env::var("BISA_GITHUB_TOKEN").is_ok_and(|v| !v.trim().is_empty());
    let dir = tempfile::tempdir().unwrap();
    let status = "github.com\n  ✓ Logged in to github.com account octocat (keyring)\n  - Active account: true\n  - Git operations protocol: https\n  - Token: gho_************************************\n";
    let cli = Arc::new(
        FakeCli::new()
            .with_program(CliProgram::Gh, "/opt/homebrew/bin/gh")
            .always(
                CliProgram::Gh,
                &["--version"],
                0,
                "gh version 2.63.2 (2024-12-05)\n",
                "",
            )
            .always(CliProgram::Gh, &["auth", "status"], 0, status, "")
            .always(
                CliProgram::Gh,
                &["auth", "token"],
                0,
                "gho_never_crosses\n",
                "",
            ),
    );
    let node = Node::start_with(EngineConfig {
        design_enabled: false,
        git: Some(git_with_helper(dir.path(), None)),
        cli: Some(cli.clone()),
        ..Default::default()
    })
    .await;

    let health = node.get("/codehost/github/health").await;
    assert_eq!(health["kind"], json!("github"));
    assert_eq!(health["host"], json!("github.com"));
    assert_eq!(health["cli"]["installed"], json!(true), "{health}");
    assert_eq!(health["cli"]["version"], json!("2.63.2"));
    assert_eq!(health["cli"]["accounts"][0]["login"], json!("octocat"));
    assert_eq!(health["cli"]["accounts"][0]["active"], json!(true));
    assert_eq!(health["cli_login"], json!("octocat"));
    if !env_token {
        assert_eq!(
            health["resolves"],
            json!({"login": "octocat", "source": "cli"}),
            "with nothing stored, the CLI's account answers: {health}"
        );
    }
    let text = health.to_string();
    assert!(
        !text.contains("gho_"),
        "the CLI's token never crosses: {text}"
    );

    let plan = node.get("/codehost/github/login").await;
    assert_eq!(
        plan["kind"],
        json!("cli"),
        "the CLI is installed: its own browser sign-in, in a terminal: {plan}"
    );
    assert_eq!(plan["program"], json!("gh"));
    let plan = node.get("/codehost/gitlab/login").await;
    assert_eq!(
        plan["kind"],
        json!("install"),
        "glab is not on this fake's PATH: {plan}"
    );
    assert_eq!(plan["hints"]["brew"], json!("brew install glab"));
    let plan = node.get("/codehost/bitbucket/login").await;
    assert_eq!(plan["kind"], json!("token"), "Bitbucket has no CLI: {plan}");
    let bitbucket = node.get("/codehost/bitbucket/health").await;
    assert!(bitbucket["cli"].is_null(), "{bitbucket}");

    let inspected = node
        .post(
            "/codehost/inspect",
            json!({"url": "git@github.com:acme/web.git"}),
        )
        .await;
    assert_eq!(
        inspected["code_host"]["kind"],
        json!("github"),
        "{inspected}"
    );
    assert_eq!(inspected["remote"]["owner"], json!("acme"));
    assert_eq!(inspected["remote"]["protocol"], json!("scp"));
    if !env_token {
        assert_eq!(
            inspected["suggested"],
            json!("octocat"),
            "the CLI's account is the one a new repository speaks as: {inspected}"
        );
        assert_eq!(inspected["accounts"][0]["source"], json!("cli"));
        assert_eq!(inspected["cautions"], json!([]));
    }
    let gitlab = node
        .post(
            "/codehost/inspect",
            json!({"url": "https://gitlab.com/acme/platform/web.git"}),
        )
        .await;
    assert_eq!(gitlab["code_host"]["kind"], json!("gitlab"));
    assert_eq!(
        gitlab["remote"]["owner"],
        json!("acme/platform"),
        "a subgroup is the namespace: {gitlab}"
    );
    assert_eq!(
        gitlab["suggested"],
        serde_json::Value::Null,
        "nobody is signed in to GitLab here"
    );
    assert!(
        gitlab["cautions"][0].as_str().unwrap().contains("GitLab"),
        "{gitlab}"
    );
    let own = node
        .post(
            "/codehost/inspect",
            json!({"url": "git@git.acme.internal:acme/web.git"}),
        )
        .await;
    assert!(
        own["code_host"].is_null(),
        "a host nobody named a kind for: {own}"
    );
    assert!(own["cautions"][0]
        .as_str()
        .unwrap()
        .contains("codehost.kind"));

    let (status, unknown) = node.req("GET", "/codehost/gitea/accounts", None).await;
    assert_eq!(
        status, 404,
        "a kind this build does not know is a 404: {unknown}"
    );
    let words = unknown.to_string();
    assert!(
        words.contains("github") && words.contains("gitlab") && words.contains("bitbucket"),
        "an unknown kind names the three: {words}"
    );
    let lines = cli.lines();
    assert!(
        lines.iter().all(|l| l.starts_with("gh ")),
        "only gh was asked: {lines:?}"
    );
    assert!(
        !lines.iter().any(|l| l.contains("auth login")),
        "nothing here signs in for the person: {lines:?}"
    );
    node.shutdown().await;
}

/// The goal page and the inbox read one builder of "what waits on a person":
/// a question this daemon did not open — rebuilt from the run — shows on both.
#[tokio::test(flavor = "multi_thread")]
async fn a_goals_open_questions_match_its_inbox_row() {
    let node = Node::start().await;
    let goal = node.new_goal("ask me").await;
    let wf = node.workflow(human_workflow()).await;
    node.start_run(goal, wf).await;

    // Live: the gate this daemon opened.
    let view = node.get(&format!("/goals/{goal}")).await;
    let live = &view["guidance"]["open_questions"];
    assert_eq!(live.as_array().map(Vec::len), Some(1), "{view}");
    assert_eq!(live[0]["durable"], json!(false));
    let inbox = node.get("/inbox").await;
    let row = inbox["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == json!(goal.to_string()))
        .cloned()
        .unwrap_or_else(|| panic!("no inbox row for the goal: {inbox}"));
    assert_eq!(&row["needs_action"], live, "live: page and inbox agree");

    // Opened elsewhere: withdraw the live gate; the ask is rebuilt from the run.
    for gate in node.inner.gates.pending_for_goal(goal) {
        node.inner.gates.withdraw(&gate.id);
    }
    let view = node.get(&format!("/goals/{goal}")).await;
    let durable = &view["guidance"]["open_questions"];
    assert_eq!(durable.as_array().map(Vec::len), Some(1), "{view}");
    assert_eq!(durable[0]["durable"], json!(true));
    assert_eq!(durable[0]["expects"]["kind"], json!("answer"));
    assert!(durable[0]["gate_id"].is_null());
    let inbox = node.get("/inbox").await;
    let row = inbox["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == json!(goal.to_string()))
        .cloned()
        .unwrap();
    assert_eq!(
        &row["needs_action"], durable,
        "durable: page and inbox agree"
    );
}

/// The goal view carries where the Workflow Agent stands, from the goal's
/// journal: `off` on a node with guided mode off, and whatever the last
/// guidance fact says — a fact that claims work with nothing live behind it
/// reads as `stalled`.
#[tokio::test(flavor = "multi_thread")]
async fn goal_view_carries_the_design_status() {
    let node = Node::start().await;
    let goal = node.new_goal("design me").await;
    let v = node.get(&format!("/goals/{goal}")).await;
    assert_eq!(v["guidance"]["phase"], json!("design"));
    assert_eq!(v["guidance"]["design"]["status"], json!("off"), "{v}");
    assert_eq!(v["guidance"]["design"]["live"], json!(false));
    let (code, v) = node
        .req("POST", &format!("/goals/{goal}/design"), Some(json!({})))
        .await;
    assert_eq!(code, 409, "{v}");
    assert!(
        v["error"].as_str().unwrap().contains("designing is off"),
        "{v}"
    );

    // A fact written by a previous process that died mid-design.
    let owner = node.ws.owner_keys().clone();
    node.ws
        .append_journal(
            &bisa_core::Home::from(goal),
            JournalPayload::Guidance {
                phase: bisa_core::GuidancePhase::Design,
                status: bisa_core::GuidanceStatus::Working,
                detail: Some("on claude-code".into()),
                session: None,
            },
            &owner,
            None,
        )
        .unwrap();
    let v = node.get(&format!("/goals/{goal}")).await;
    assert_eq!(v["guidance"]["design"]["status"], json!("stalled"), "{v}");
    assert_eq!(v["guidance"]["design"]["live"], json!(false));
    node.ws
        .append_journal(
            &bisa_core::Home::from(goal),
            JournalPayload::Guidance {
                phase: bisa_core::GuidancePhase::Design,
                status: bisa_core::GuidanceStatus::Proposed,
                detail: Some("Ship it (3 steps)".into()),
                session: None,
            },
            &owner,
            None,
        )
        .unwrap();
    let v = node.get(&format!("/goals/{goal}")).await;
    assert_eq!(v["guidance"]["design"]["status"], json!("proposed"), "{v}");
    assert_eq!(
        v["guidance"]["design"]["detail"],
        json!("Ship it (3 steps)")
    );
    assert!(v["guidance"]["design"]["since"].is_u64());
    node.shutdown().await;
}

/// `POST /goals/{id}/design` refuses by name when there is nothing to design.
#[tokio::test(flavor = "multi_thread")]
async fn design_route_refusals() {
    let node = Node::start().await;
    let (code, _) = node
        .req(
            "POST",
            "/goals/01ARZ3NDEKTSV4RRFFQ69G5FAV/design",
            Some(json!({})),
        )
        .await;
    assert_eq!(code, 404);
    let v = node
        .post(
            "/goals",
            json!({"statement": "by hand", "title": "T", "mode": "manual"}),
        )
        .await;
    let manual = v["goal"]["id"].as_str().unwrap().to_string();
    let (code, v) = node
        .req("POST", &format!("/goals/{manual}/design"), Some(json!({})))
        .await;
    assert_eq!(code, 409, "{v}");
    assert!(v["error"].as_str().unwrap().contains("is manual"), "{v}");

    let wf = node.workflow(human_workflow()).await;
    let goal = node.new_goal("guided, then pointed").await;
    node.req(
        "PUT",
        &format!("/goals/{goal}/workflow"),
        Some(json!({"workflow": wf.to_string()})),
    )
    .await;
    let (code, v) = node
        .req("POST", &format!("/goals/{goal}/design"), Some(json!({})))
        .await;
    assert_eq!(code, 409, "{v}");
    assert!(
        v["error"]
            .as_str()
            .unwrap()
            .contains("already has a workflow"),
        "{v}"
    );
    node.shutdown().await;
}

/// A guided node with no harness to launch the Workflow Agent on records the
/// failure where the screen reads it, with the reason, and the goal's thread
/// hears why.
#[tokio::test(flavor = "multi_thread")]
async fn a_guided_node_with_no_harness_records_failed_over_http() {
    let node = Node::start_with(EngineConfig {
        design_enabled: true,
        events_enabled: false,
        ..Default::default()
    })
    .await;
    let goal = node.new_goal("nothing can run me").await;
    let mut design = json!(null);
    for _ in 0..200 {
        let v = node.get(&format!("/goals/{goal}")).await;
        if v["guidance"]["design"]["status"] == json!("failed") {
            design = v["guidance"]["design"].clone();
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(design["status"], json!("failed"), "{design}");
    assert!(
        design["detail"].as_str().unwrap_or_default().len() > 10,
        "{design}"
    );
    assert_eq!(design["live"], json!(false));
    let messages = node.get(&format!("/goals/{goal}/messages")).await;
    assert!(
        messages.to_string().contains("pick a workflow by hand"),
        "{messages}"
    );
    node.shutdown().await;
}

/// A run is reachable by its id under its own goal — whole, the same object
/// `GET /goals/{id}/run` answers while it is current — and never under
/// another goal's path.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_is_fetched_by_id_under_its_own_goal_only() {
    let node = Node::start().await;
    let id = node.new_goal("answer me").await;
    let other = node.new_goal("a different goal").await;
    let wf = node.workflow(human_workflow()).await;
    let run = node.start_run(id, wf).await;

    let current = node.get(&format!("/goals/{id}/run")).await;
    let by_id = node.get(&format!("/goals/{id}/runs/{run}")).await;
    assert_eq!(by_id["run"], current["run"], "the same run, whole");
    assert_eq!(by_id["run"]["id"], json!(run.to_string()));
    assert!(
        by_id["run"]["workflow"]["steps"].is_array()
            || by_id["run"]["workflow"]["steps"].is_object(),
        "{by_id}"
    );

    let (code, v) = node
        .req("GET", &format!("/goals/{other}/runs/{run}"), None)
        .await;
    assert_eq!(code, 404, "another goal's path: {v}");
    let (code, v) = node
        .req(
            "GET",
            &format!("/goals/{id}/runs/01ARZ3NDEKTSV4RRFFQ69G5FAV"),
            None,
        )
        .await;
    assert_eq!(code, 404, "a run that does not exist: {v}");
    let (code, _) = node
        .req("GET", &format!("/goals/{id}/runs/not-an-id"), None)
        .await;
    assert_eq!(code, 404);
}

// ---------------------------------------------------------------------------
// Sync from the Git tab: fetch, pull, revert, a remote re-pointed
// ---------------------------------------------------------------------------

/// A second clone of `origin`, standing in for a colleague's machine.
fn other_side(root: &std::path::Path, origin: &std::path::Path, name: &str) -> PathBuf {
    let other = root.join(name);
    raw_git(
        root,
        &[
            "clone",
            "--quiet",
            origin.to_str().unwrap(),
            other.to_str().unwrap(),
        ],
    );
    set_identity(&other);
    other
}

fn commit_file(repo: &std::path::Path, rel: &str, text: &str, message: &str) -> String {
    std::fs::write(repo.join(rel), text).unwrap();
    raw_git(repo, &["add", "-A"]);
    raw_git(repo, &["commit", "-m", message, "--quiet"]);
    raw_git(repo, &["rev-parse", "HEAD"])
}

#[tokio::test(flavor = "multi_thread")]
async fn fetch_pull_revert_and_a_re_pointed_remote_over_http() {
    let node = Node::start().await;
    let goal = node.new_goal("keep main current").await.to_string();
    let (pid, root) = node.git_project(&goal, "sync", "auto").await;
    let origin = node.origin_for(&root, "sync");
    let branch = raw_git(&root, &["symbolic-ref", "--short", "HEAD"]);

    // The primary publishes itself (auto: no gate), so it has an upstream.
    let (code, v) = node
        .req("POST", &format!("/workstreams/{pid}/push"), Some(json!({})))
        .await;
    assert_eq!(code, 200, "{v}");

    // A colleague pushes: fetch sees it, the status says behind, nothing moved.
    let other = other_side(node.ws.root(), &origin, "other");
    let theirs = commit_file(&other, "theirs.txt", "from the other side\n", "theirs");
    raw_git(&other, &["push", "--quiet", "origin", &branch]);
    let v = node
        .post(&format!("/workstreams/{pid}/git/fetch"), json!({}))
        .await;
    assert_eq!(v["fetched"], json!(true));
    assert_eq!(v["remote"], json!("origin"));
    assert_eq!(v["status"]["behind"], json!(1), "{v}");
    assert!(!root.join("theirs.txt").exists(), "a fetch moves nothing");

    // Pull, fast-forward only: the branch moves, the answer says from where to where.
    let v = node
        .post(
            &format!("/workstreams/{pid}/git/pull"),
            json!({"mode": "ff_only"}),
        )
        .await;
    assert_eq!(v["pull"]["moved"], json!(true), "{v}");
    assert_eq!(v["pull"]["to"], json!(theirs));
    assert_eq!(v["pull"]["upstream"], json!(format!("origin/{branch}")));
    assert!(v["recovery"]["ref_name"]
        .as_str()
        .unwrap()
        .contains("-pull"));
    assert_eq!(
        std::fs::read_to_string(root.join("theirs.txt")).unwrap(),
        "from the other side\n"
    );

    // Both sides move: fast-forward-only is refused by name, with the counts.
    let mine = commit_file(&root, "mine.txt", "mine\n", "mine");
    commit_file(&other, "theirs2.txt", "more\n", "theirs 2");
    raw_git(&other, &["pull", "--quiet", "--rebase", "origin", &branch]);
    raw_git(&other, &["push", "--quiet", "origin", &branch]);
    let (code, v) = node
        .req(
            "POST",
            &format!("/workstreams/{pid}/git/pull"),
            Some(json!({"mode": "ff_only"})),
        )
        .await;
    assert_eq!(code, 409, "{v}");
    assert_eq!(v["code"], json!("not_fast_forward"));
    assert_eq!(v["detail"], json!({"ahead": 1, "behind": 1}));
    assert_eq!(
        raw_git(&root, &["rev-parse", "HEAD"]),
        mine,
        "a refusal moves nothing"
    );

    // Rebase does it, and the graph stays linear.
    let v = node
        .post(
            &format!("/workstreams/{pid}/git/pull"),
            json!({"mode": "rebase"}),
        )
        .await;
    assert_eq!(v["pull"]["moved"], json!(true), "{v}");
    assert!(root.join("theirs2.txt").exists() && root.join("mine.txt").exists());

    // A conflict is named: its paths, and the operation left in progress —
    // which the status reports too until it is aborted or resolved.
    commit_file(&other, "README.md", "theirs\n", "theirs readme");
    raw_git(&other, &["pull", "--quiet", "--rebase", "origin", &branch]);
    raw_git(&other, &["push", "--quiet", "origin", &branch]);
    let head_before = commit_file(&root, "README.md", "mine\n", "mine readme");
    let (code, v) = node
        .req(
            "POST",
            &format!("/workstreams/{pid}/git/pull"),
            Some(json!({"mode": "merge"})),
        )
        .await;
    assert_eq!(code, 409, "{v}");
    assert_eq!(v["code"], json!("conflict"));
    assert_eq!(v["detail"]["paths"], json!(["README.md"]));
    assert_eq!(v["detail"]["in_progress"], json!("merge"));
    let v = node.get(&format!("/workstreams/{pid}/git/status")).await;
    assert_eq!(v["status"]["in_progress"], json!("merge"));
    assert_eq!(v["status"]["conflicted"], json!(1));
    let v = node.get(&format!("/workstreams/{pid}/status")).await;
    assert_eq!(
        v["status"]["in_progress"],
        json!("merge"),
        "the card's status says so too"
    );
    let (code, v) = node
        .req(
            "POST",
            &format!("/workstreams/{pid}/git/pull"),
            Some(json!({"mode": "rebase"})),
        )
        .await;
    assert_eq!(code, 409, "{v}");
    assert_eq!(v["code"], json!("in_progress"));
    assert_eq!(v["detail"]["in_progress"], json!("merge"));
    node.post(
        &format!("/workstreams/{pid}/git/abort"),
        json!({"what": "merge"}),
    )
    .await;
    let v = node.get(&format!("/workstreams/{pid}/git/status")).await;
    assert_eq!(v["status"]["in_progress"], Value::Null);
    assert_eq!(raw_git(&root, &["rev-parse", "HEAD"]), head_before);

    // Revert undoes a commit with a new one; the recovery ref is written first.
    let v = node
        .post(
            &format!("/workstreams/{pid}/git/revert"),
            json!({"commits": [head_before]}),
        )
        .await;
    assert!(
        v["recovery"]["ref_name"]
            .as_str()
            .unwrap()
            .contains("-revert"),
        "{v}"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("README.md")).unwrap(),
        "baseline\n"
    );
    assert_eq!(
        raw_git(&root, &["log", "-1", "--format=%s"]),
        "Revert \"mine readme\""
    );

    // A remote is re-pointed in place; `origin` keeps the project record true;
    // another name is simply added.
    let elsewhere = node.ws.root().join("elsewhere.git");
    std::fs::create_dir_all(&elsewhere).unwrap();
    raw_git(&elsewhere, &["init", "--bare", "--quiet"]);
    let there = elsewhere.to_str().unwrap().to_string();
    let v = node
        .put(
            &format!("/workstreams/{pid}/git/remotes/origin"),
            json!({"url": there}),
        )
        .await;
    let origin_row = v["remotes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "origin")
        .unwrap();
    assert_eq!(origin_row["url"], json!(there));
    let v = node.get(&format!("/projects/{pid}")).await;
    assert_eq!(
        v["project"]["vcs"]["remote"],
        json!(there),
        "the record follows origin"
    );
    let v = node
        .put(
            &format!("/workstreams/{pid}/git/remotes/mirror"),
            json!({"url": origin.to_str().unwrap()}),
        )
        .await;
    assert_eq!(v["remotes"].as_array().unwrap().len(), 2, "{v}");
    let (code, v) = node
        .req(
            "PUT",
            &format!("/workstreams/{pid}/git/remotes/origin"),
            Some(json!({"url": "  "})),
        )
        .await;
    assert_eq!(code, 400, "{v}");

    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_graph_is_searched_across_the_whole_log_not_the_loaded_page() {
    let node = Node::start().await;
    let goal = node.new_goal("search history").await.to_string();
    let (pid, root) = node.git_project(&goal, "ledger", "gated").await;
    set_identity(&root);
    // Thirty commits, one with a subject nobody else has, at the old end.
    for i in 0..30 {
        std::fs::write(root.join(format!("f{i}.txt")), format!("{i}\n")).unwrap();
        raw_git(&root, &["add", "."]);
        let subject = if i == 2 {
            "Rebalance the cart total".to_string()
        } else {
            format!("step {i}")
        };
        raw_git(&root, &["commit", "-q", "-m", &subject]);
    }
    // A page of five: the match is not on it.
    let (status, page) = request(
        &node.socket,
        "GET",
        &format!("/ide/graph/workstream/{pid}?from=0&count=5"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(page["rows"].as_array().unwrap().len(), 5);
    assert!(page["rows"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["subject"] != json!("Rebalance the cart total")));
    // The search sees the whole log.
    let (status, found) = request(
        &node.socket,
        "GET",
        &format!("/ide/graph/workstream/{pid}/search?q=CART"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{found}");
    let indices: Vec<u64> = found["indices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect();
    assert_eq!(indices.len(), 1, "{found}");
    let at = indices[0] as usize;
    assert!(at > 5, "the match is past the first page: {at}");
    assert_eq!(
        found["searched"],
        json!(31),
        "the project's first commit plus thirty"
    );
    assert_eq!(found["truncated"], json!(false));
    // Asking for the window that holds it lands on it.
    let (status, window) = request(
        &node.socket,
        "GET",
        &format!("/ide/graph/workstream/{pid}?from={at}&count=1"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{window}");
    assert_eq!(
        window["rows"][0]["subject"],
        json!("Rebalance the cart total")
    );
    // An author, an id prefix and an empty query.
    let (_, by_author) = request(
        &node.socket,
        "GET",
        &format!("/ide/graph/workstream/{pid}/search?q=bisa%20test&limit=3"),
        None,
    )
    .await;
    assert_eq!(by_author["indices"].as_array().unwrap().len(), 3);
    assert_eq!(by_author["truncated"], json!(true), "the cap stopped it");
    let short = window["rows"][0]["short"].as_str().unwrap().to_string();
    let (_, by_id) = request(
        &node.socket,
        "GET",
        &format!("/ide/graph/workstream/{pid}/search?q={}", &short[..5]),
        None,
    )
    .await;
    assert_eq!(by_id["indices"], json!([at]));
    let (status, _) = request(
        &node.socket,
        "GET",
        &format!("/ide/graph/workstream/{pid}/search?q="),
        None,
    )
    .await;
    assert_eq!(status, 400, "a search needs something to search for");

    // The one filter: `refs=head` leaves a branch nobody merged out, and an
    // unknown scope is refused by name.
    raw_git(&root, &["checkout", "-q", "-b", "side"]);
    std::fs::write(root.join("side.txt"), "side\n").unwrap();
    raw_git(&root, &["add", "."]);
    raw_git(&root, &["commit", "-q", "-m", "only on side"]);
    raw_git(&root, &["checkout", "-q", "-"]);
    let (status, all) = request(
        &node.socket,
        "GET",
        &format!("/ide/graph/workstream/{pid}?from=0&count=1&refs=all&refresh=true"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{all}");
    let (status, head) = request(
        &node.socket,
        "GET",
        &format!("/ide/graph/workstream/{pid}?from=0&count=1&refs=head"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{head}");
    assert_eq!(
        head["total"].as_u64().unwrap() + 1,
        all["total"].as_u64().unwrap(),
        "{all} vs {head}"
    );
    let (status, _) = request(
        &node.socket,
        "GET",
        &format!("/ide/graph/workstream/{pid}/search?q=side&refs=head"),
        None,
    )
    .await;
    assert_eq!(status, 200);
    let (status, _) = request(
        &node.socket,
        "GET",
        &format!("/ide/graph/workstream/{pid}?refs=mine"),
        None,
    )
    .await;
    assert_eq!(status, 400, "an unknown ref scope is refused");
}

// ---------------------------------------------------------------------------
// Artifacts: posted over HTTP, read back one by one and per conversation,
// and given a real name on disk for the file manager.
// ---------------------------------------------------------------------------

fn artifact_json(file: &bisa_core::AttachmentRef, title: &str) -> Value {
    let a = bisa_core::ArtifactRef::from_attachment(file.clone(), Some(title.into()), None);
    serde_json::to_value(a).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_post_with_an_unknown_artifact_is_400() {
    let node = Node::start().await;
    let ghost = json!({
        "sha256": "b".repeat(64), "name": "chart.svg", "mime": "image/svg+xml", "size": 3,
        "title": "Chart", "kind": "svg"
    });
    let (code, body) = node
        .req(
            "POST",
            "/channels/general/messages",
            Some(json!({"content": "see", "artifacts": [ghost]})),
        )
        .await;
    assert_eq!(code, 400, "{body}");
    assert!(
        body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("no artifact bytes"),
        "{body}"
    );
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_reads_back_with_its_artifacts_and_presence() {
    let node = Node::start().await;
    let file = node
        .ws
        .put_attachment(b"<h1>hi</h1>", "dashboard.html", "text/html")
        .unwrap();
    let posted = node
        .post(
            "/channels/general/messages",
            json!({"content": "", "artifacts": [artifact_json(&file, "Dashboard")]}),
        )
        .await;
    let id = posted["id"].as_str().expect("an id").to_string();

    let one = node.get(&format!("/messages/{id}")).await;
    let arts = one["message"]["artifacts"].as_array().expect("artifacts");
    assert_eq!(arts.len(), 1);
    assert_eq!(arts[0]["title"], json!("Dashboard"));
    assert_eq!(arts[0]["kind"], json!("html"));
    assert_eq!(arts[0]["sha256"], json!(file.sha256));
    assert_eq!(arts[0]["present"], json!(true));
    assert_eq!(one["message"]["scope_id"], json!("general"));

    let (code, _) = node.req("GET", "/messages/nope", None).await;
    assert_eq!(code, 404);
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn artifacts_of_a_scope_list_newest_first() {
    let node = Node::start().await;
    let first = node
        .ws
        .put_attachment(b"v1", "report.md", "text/markdown")
        .unwrap();
    let second = node
        .ws
        .put_attachment(b"v2", "report.md", "text/markdown")
        .unwrap();
    node.post(
        "/channels/general/messages",
        json!({"content": "v1", "artifacts": [artifact_json(&first, "Report")]}),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(1100)).await;
    node.post(
        "/channels/general/messages",
        json!({"content": "v2", "artifacts": [artifact_json(&second, "Report")]}),
    )
    .await;

    let listed = node.get("/artifacts/general").await;
    let rows = listed["artifacts"].as_array().expect("rows");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["sha256"], json!(second.sha256), "newest first");
    assert_eq!(rows[1]["sha256"], json!(first.sha256));
    assert!(rows
        .iter()
        .all(|r| r["title"] == json!("Report") && r["present"] == json!(true)));
    assert!(rows[0]["message_id"].is_string());
    let one = node.get("/artifacts/general?limit=1").await;
    assert_eq!(one["artifacts"].as_array().unwrap().len(), 1);
    let (code, _) = node.req("GET", "/artifacts/no-such-scope", None).await;
    assert_eq!(code, 400, "a scope that names no conversation is refused");
    node.shutdown().await;
}

/// An attachment is content-addressed, so it is served immutable: a year's
/// cache life and an ETag of its hash, and a client that holds it is told
/// so with no bytes read.
#[tokio::test(flavor = "multi_thread")]
async fn an_attachment_is_served_immutable_and_a_held_one_is_not_sent_again() {
    let node = Node::start().await;
    let file = node
        .ws
        .put_attachment(b"\x89PNG\r\n\x1a\nIHDR-tiny", "logo.png", "image/png")
        .unwrap();
    let path = format!("/attachments/{}?as=image", file.sha256);
    let (code, headers, len) = request_headers(&node.socket, "GET", &path, None).await;
    assert_eq!(code, 200);
    assert!(len > 0, "the bytes come the first time");
    assert_eq!(
        headers.get("cache-control").unwrap().to_str().unwrap(),
        "public, max-age=31536000, immutable"
    );
    let etag = headers.get("etag").unwrap().to_str().unwrap().to_string();
    assert_eq!(etag, format!("\"{}\"", file.sha256), "the hash is the tag");
    assert_eq!(headers.get("content-type").unwrap(), "image/png");

    let (code, headers, len) =
        request_headers(&node.socket, "GET", &path, Some(("if-none-match", &etag))).await;
    assert_eq!(code, 304, "held already: nothing to send");
    assert_eq!(len, 0);
    assert_eq!(headers.get("etag").unwrap().to_str().unwrap(), etag);

    let (code, _, _) = request_headers(
        &node.socket,
        "GET",
        &path,
        Some(("if-none-match", "\"other\"")),
    )
    .await;
    assert_eq!(code, 200, "another tag is another content");
    let gone = format!("/attachments/{}", "c".repeat(64));
    let (code, _, _) =
        request_headers(&node.socket, "GET", &gone, Some(("if-none-match", "*"))).await;
    assert_eq!(
        code, 404,
        "a match means nothing for bytes this machine never held"
    );
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn materialising_names_the_file_and_is_idempotent() {
    let node = Node::start().await;
    let file = node
        .ws
        .put_attachment(b"%PDF-1.4 tiny", "report.pdf", "application/pdf")
        .unwrap();
    let path = format!("/attachments/{}/file", file.sha256);
    let first = node.post(&path, json!({"name": "report.pdf"})).await;
    let named = PathBuf::from(first["path"].as_str().expect("a path"));
    assert!(named.is_file());
    assert_eq!(named.file_name().unwrap(), "report.pdf");
    assert!(named.starts_with(node.data_dir().join("attachments").join("named")));
    let again = node.post(&path, json!({"name": "report.pdf"})).await;
    assert_eq!(again["path"], first["path"]);

    let (code, _) = node.req("POST", &path, Some(json!({"name": ".."}))).await;
    assert_eq!(code, 400);
    let (code, _) = node
        .req(
            "POST",
            &format!("/attachments/{}/file", "c".repeat(64)),
            Some(json!({"name": "x.txt"})),
        )
        .await;
    assert_eq!(code, 404);
    let (code, _) = node
        .req(
            "POST",
            "/attachments/nope/file",
            Some(json!({"name": "x.txt"})),
        )
        .await;
    assert_eq!(code, 400);
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_artifact_is_served_on_a_port_of_its_own_once_and_stopped_by_id() {
    let node = Node::start().await;
    let page = node
        .ws
        .put_attachment(b"<h1>report</h1>", "q1 report.html", "text/html")
        .unwrap();
    let path = format!("/artifacts/{}/serve", page.sha256);
    let mut events = node.events();
    let first = node.post(&path, json!({"name": "q1 report.html"})).await;
    let id = first["id"].as_str().expect("an id").to_string();
    let port = first["port"].as_u64().expect("a port");
    assert_eq!(first["owner"]["kind"], json!("artifact"));
    assert_eq!(first["owner"]["name"], json!("q1 report.html"));
    assert_eq!(first["url"], json!(format!("http://127.0.0.1:{port}/")));
    assert_eq!(
        first["page"],
        json!(format!("http://127.0.0.1:{port}/q1%20report.html")),
        "the page is the file, its name escaped for a URL"
    );
    // The named copy is under the store, and it is what the port serves.
    let named = node
        .data_dir()
        .join("attachments")
        .join("named")
        .join(&page.sha256)
        .join("q1 report.html");
    assert!(named.is_file(), "{}", named.display());
    let again = node.post(&path, json!({"name": "q1 report.html"})).await;
    assert_eq!(
        again["id"], first["id"],
        "a second ask answers the server already up"
    );

    let all = node.get("/servers").await;
    let rows = all["servers"].as_array().expect("rows");
    assert!(rows.iter().any(|r| r["id"] == first["id"]));
    let (code, _) = node
        .req(
            "POST",
            &format!("/artifacts/{}/serve", "c".repeat(64)),
            Some(json!({"name": "x.html"})),
        )
        .await;
    assert_eq!(code, 404, "bytes this machine does not hold");
    let (code, _) = node
        .req("POST", &path, Some(json!({"name": "../x.html"})))
        .await;
    assert_eq!(code, 400, "not a file name");
    let (code, _) = node
        .req(
            "POST",
            "/artifacts/nope/serve",
            Some(json!({"name": "x.html"})),
        )
        .await;
    assert_eq!(code, 400);

    // Served once, said once: the second ask changed nothing, and an
    // artifact's page names no checkout.
    let mut said = Vec::new();
    while let Ok(ev) = events.try_recv() {
        if let bisa_engine::EnginePayload::ServerChanged { workstream } = ev.payload {
            said.push(workstream);
        }
    }
    assert_eq!(said, vec![None], "one start, one frame");

    let stopped = node.delete(&format!("/servers/{id}")).await;
    assert_eq!(stopped["stopped"]["id"], json!(id));
    let heard = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Ok(ev) = events.recv().await {
                if matches!(
                    ev.payload,
                    bisa_engine::EnginePayload::ServerChanged { workstream: None }
                ) {
                    return;
                }
            }
        }
    })
    .await;
    assert!(
        heard.is_ok(),
        "a stopped artifact page is said on the bus too"
    );
    let (code, _) = node.req("DELETE", &format!("/servers/{id}"), None).await;
    assert_eq!(code, 404, "stopped once");
    assert!(node.get("/servers").await["servers"]
        .as_array()
        .unwrap()
        .is_empty());
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_folder_asked_for_many_times_at_once_is_served_once_and_stopped_only_through_its_own_door(
) {
    let node = Node::start().await;
    let mut ids = Vec::new();
    for slug in ["web-app", "other-app"] {
        let made = node
            .post("/projects", json!({"kind": "new", "slug": slug}))
            .await;
        ids.push(
            made["project"]["id"]
                .as_str()
                .expect("a project")
                .to_string(),
        );
    }
    let (pid, other) = (ids[0].clone(), ids[1].clone());

    // Eight asks for the one checkout at the same moment: a double click, an
    // agent's retry, two windows.
    let door = format!("/workstreams/{pid}/servers");
    let asks = (0..8).map(|_| node.req("POST", &door, Some(json!({}))));
    let answers = futures::future::join_all(asks).await;
    let served: Vec<_> = answers.iter().filter(|(code, _)| *code == 200).collect();
    assert_eq!(served.len(), 1, "{answers:?}");
    assert!(
        answers.iter().all(|(code, _)| *code == 200 || *code == 409),
        "{answers:?}"
    );
    let listed = node.get(&format!("/workstreams/{pid}/servers")).await;
    assert_eq!(listed["servers"].as_array().unwrap().len(), 1, "{listed}");
    let id = served[0].1["id"].as_str().unwrap().to_string();

    // The agent's door at the same moment answers the server already up.
    let (code, v) = node
        .req(
            "POST",
            "/conversations",
            Some(json!({"origin": {"kind": "workstream", "id": pid, "project": pid}})),
        )
        .await;
    assert_eq!(code, 201, "{v}");
    let scope = v["conversation"]["id"].as_str().unwrap().to_string();
    let replies = futures::future::join_all(
        (0..4).map(|_| node.intake_op(json!({"op": "browser_serve", "scope": scope}))),
    )
    .await;
    for reply in &replies {
        assert_eq!(reply["result"]["id"], json!(id), "{reply}");
    }

    // Another checkout's door does not stop it; its own does, and after a
    // stop the folder is served afresh on a server of its own.
    let (code, _) = node
        .req(
            "DELETE",
            &format!("/workstreams/{other}/servers/{id}"),
            None,
        )
        .await;
    assert_eq!(code, 404, "not that checkout's server");
    assert_eq!(
        node.get("/servers").await["servers"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let (code, _) = node
        .req("DELETE", &format!("/workstreams/{pid}/servers/{id}"), None)
        .await;
    assert_eq!(code, 200);
    let again = node
        .post(&format!("/workstreams/{pid}/servers"), json!({}))
        .await;
    assert_ne!(again["id"], json!(id), "a new server, not the stopped one");
    node.shutdown().await;
}

// ---------------------------------------------------------------------------
// Goal documents — the files a person gives a goal as context
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn a_goal_captured_with_documents_keeps_them_under_its_documents_folder() {
    let node = Node::start().await;
    let brief = node
        .ws
        .put_attachment(b"%PDF-1.4 brief", "brief.pdf", "application/pdf")
        .unwrap();
    let mockup = node
        .ws
        .put_attachment(b"\x89PNG mock", "mockup.png", "image/png")
        .unwrap();
    let created = node
        .post(
            "/goals",
            json!({"statement": "ship the checkout", "mode": "manual", "documents": [brief, mockup]}),
        )
        .await;
    let goal = created["goal"]["id"].as_str().expect("a goal").to_string();

    let listed = node.get(&format!("/goals/{goal}/documents")).await;
    let rows = listed["documents"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{listed}");
    assert_eq!(rows[0]["name"], json!("brief.pdf"));
    assert_eq!(rows[0]["file"]["mime"], json!("application/pdf"));
    assert_eq!(rows[0]["present"], json!(true));
    let on_disk = node
        .ws
        .paths()
        .goal(goal.parse().unwrap())
        .document("mockup.png");
    assert_eq!(rows[1]["path"], json!(on_disk.display().to_string()));
    assert_eq!(std::fs::read(on_disk).unwrap(), b"\x89PNG mock");

    // The journal carries one fact per document, the descriptor verbatim.
    let journal = node.get(&format!("/goals/{goal}/journal")).await;
    let facts: Vec<&Value> = journal["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["payload"]["type"] == json!("document"))
        .collect();
    assert_eq!(facts.len(), 2, "{journal}");
    assert_eq!(facts[1]["payload"]["file"]["name"], json!("mockup.png"));

    // A later one lands the same way; the same name is numbered, not replaced.
    let again = node
        .ws
        .put_attachment(b"%PDF-1.4 revised brief", "brief.pdf", "application/pdf")
        .unwrap();
    let added = node
        .post(
            &format!("/goals/{goal}/documents"),
            json!({"documents": [again]}),
        )
        .await;
    assert_eq!(
        added["documents"][0]["name"],
        json!("brief (2).pdf"),
        "{added}"
    );
    let listed = node.get(&format!("/goals/{goal}/documents")).await;
    assert_eq!(listed["documents"].as_array().unwrap().len(), 3);

    // Bytes this node does not hold are refused: at capture and later alike.
    let ghost = json!({"sha256": "cd".repeat(32), "name": "ghost.pdf", "mime": "application/pdf", "size": 9});
    let (code, body) = node
        .req(
            "POST",
            &format!("/goals/{goal}/documents"),
            Some(json!({"documents": [ghost.clone()]})),
        )
        .await;
    assert_eq!(code, 400, "{body}");
    let (code, body) = node
        .req(
            "POST",
            "/goals",
            Some(json!({"statement": "with a ghost", "mode": "manual", "documents": [ghost]})),
        )
        .await;
    assert_eq!(code, 400, "{body}");
    let (code, _) = node
        .req(
            "POST",
            &format!("/goals/{goal}/documents"),
            Some(json!({"documents": []})),
        )
        .await;
    assert_eq!(code, 400, "an empty list is a mistake, not a no-op");
    node.shutdown().await;
}

/// A `wait` step a person is holding is owed: it is rebuilt as a `release:`
/// ask on the goal's row, and deciding it through the goal releases the
/// step — the one verb the card draws.
#[tokio::test(flavor = "multi_thread")]
async fn a_held_wait_step_is_a_release_ask_the_decide_route_releases() {
    let node = Node::start().await;
    let goal = node.new_goal("hold on").await;
    let wf = node
        .workflow(json!({
            "name": "Held",
            "steps": [
                {"id": "hold", "name": "Hold", "kind": "wait", "until": {"until": "release"}, "then": ["done"]},
                {"id": "done", "name": "Done", "kind": "end", "finish": "done"}
            ]
        }))
        .await;
    node.start_run(goal, wf).await;
    let mut asks = json!(null);
    for _ in 0..50 {
        let v = node.get("/inbox?filter=needs_you").await;
        if let Some(row) = v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["key"] == json!(goal.to_string()))
        {
            asks = row["needs_action"].clone();
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let ask = &asks[0];
    assert_eq!(ask["step"], json!("hold"), "{asks}");
    assert_eq!(ask["gate_kind"], json!("escalation"));
    assert_eq!(ask["expects"]["kind"], json!("decision"));
    assert!(
        ask["subject"]
            .as_str()
            .is_some_and(|s| s.starts_with("release:")),
        "the subject says what the one verb does: {asks}"
    );
    assert!(
        ask["durable"].as_bool().unwrap(),
        "rebuilt from the run, no live gate"
    );

    let (code, body) = node
        .req(
            "POST",
            &format!("/goals/{goal}/decide"),
            Some(json!({"approve": false, "step": "hold"})),
        )
        .await;
    assert_eq!(code, 400, "a held step has nothing to decline: {body}");

    let decided = node
        .post(
            &format!("/goals/{goal}/decide"),
            json!({"approve": true, "step": "hold"}),
        )
        .await;
    assert_eq!(decided["approve"], json!(true), "{decided}");
    let mut released = false;
    for _ in 0..50 {
        let v = node.get(&format!("/goals/{goal}")).await;
        if v["run"]["outcome"] == json!("done") {
            released = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(
        released,
        "the release let the run finish: {decided} → {}",
        node.get(&format!("/goals/{goal}")).await["run"]
    );
    let v = node.get("/inbox?filter=needs_you").await;
    assert!(
        v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["key"] != json!(goal.to_string())),
        "nothing owed once released: {v}"
    );
    node.shutdown().await;
}

/// A checkout has conversations, not a thread: an agent's message in a
/// conversation about the project's primary is unread there — a row of the
/// conversation's own kind under the **projects** source, where the
/// checkout's own rows live, saying what it is about, never under the
/// messages or the goals — and kept once read. A project's id is no message
/// scope, so the post has to go through the conversation.
#[tokio::test(flavor = "multi_thread")]
async fn a_conversation_about_a_checkout_that_moves_earns_a_row_under_projects_saying_what_it_is_about(
) {
    let node = Node::start().await;
    let made = node
        .post("/projects", json!({"kind": "new", "slug": "storefront"}))
        .await;
    let pid = made["project"]["id"]
        .as_str()
        .expect("a project")
        .to_string();
    let v = node
        .post(
            "/agents",
            json!({"name": "Scout", "system_prompt": "research", "harness": "claude-code"}),
        )
        .await;
    let agent = AgentId::new(v["agent"]["id"].as_str().unwrap()).unwrap();
    // The project's own id is not a scope a message can be posted in.
    let refused = node.ws.post_message(
        &pid,
        MessageBody::post("the build is green"),
        None,
        &[],
        &[],
        Some(&agent),
        PostOrigin::Asked,
    );
    assert!(
        matches!(refused, Err(bisa_store::StoreError::Invalid(ref e)) if e.to_string().contains("unknown scope")),
        "a project's id is no message scope: {refused:?}"
    );
    // A conversation about the project runs in its primary; that is where
    // the agent speaks.
    let (code, v) = node
        .req(
            "POST",
            "/conversations",
            Some(json!({"origin": {"kind": "project", "id": pid}})),
        )
        .await;
    assert_eq!(code, 201, "{v}");
    let conversation = v["conversation"]["id"]
        .as_str()
        .expect("a conversation")
        .to_string();
    node.ws
        .post_message(
            &conversation,
            MessageBody::post("the build is green"),
            None,
            &[],
            &[],
            Some(&agent),
            PostOrigin::Asked,
        )
        .expect("agent message in the conversation");

    let v = node.get("/inbox?source=projects").await;
    let row = v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == json!(conversation))
        .unwrap_or_else(|| panic!("the conversation's row: {v}"));
    assert_eq!(row["kind"], json!("conversation"));
    assert_eq!(
        row["source"],
        json!("projects"),
        "it sits under what it is about"
    );
    assert_eq!(
        row["origin"],
        json!({"kind": "project", "id": pid}),
        "and says which project"
    );
    assert_eq!(row["unread_count"], json!(1));
    assert_eq!(row["read"], json!(false));
    assert_eq!(row["needs_action"], json!([]), "a message is not owed");
    // The conversation's row is the projects source's alone — where the
    // checkout's own rows live (a committer wanted, a script that ran):
    // neither the messages nor the goals nor the workflows list it.
    for source in ["messages", "goals", "workflows"] {
        let rows = node.get(&format!("/inbox?source={source}")).await;
        assert!(
            rows["rows"]
                .as_array()
                .unwrap()
                .iter()
                .all(|r| r["key"] != json!(conversation) && r["kind"] != json!("conversation")),
            "not under the {source}: {rows}"
        );
    }
    // Read, it concerns you no longer: a conversation earns its row while it
    // names you or moves unread (13 — Conversations), and leaves once read.
    node.post("/read", json!({"scope": conversation})).await;
    let v = node.get("/inbox?source=projects").await;
    assert!(
        v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["key"] != json!(conversation)),
        "read, the conversation leaves the Inbox: {v}"
    );
    node.shutdown().await;
}

/// A conversation sits under what it is about. One about the workspace is a
/// message like any other; one about a goal is the goal's — under the goals
/// source, its row saying which goal — and never under the messages. Every
/// row says its source, and no row sits under two.
#[tokio::test(flavor = "multi_thread")]
async fn a_conversation_sits_under_what_it_is_about() {
    let node = Node::start().await;
    let v = node
        .post(
            "/agents",
            json!({"name": "Scout", "system_prompt": "research", "harness": "claude-code"}),
        )
        .await;
    let agent = AgentId::new(v["agent"]["id"].as_str().unwrap()).unwrap();
    let goal = node
        .post(
            "/goals",
            json!({"statement": "Ship the quarterly report", "mode": "manual"}),
        )
        .await["goal"]["id"]
        .as_str()
        .expect("a goal")
        .to_string();
    let mut opened = Vec::new();
    for origin in [
        json!({"kind": "goal", "id": goal}),
        json!({"kind": "workspace"}),
    ] {
        let (code, v) = node
            .req("POST", "/conversations", Some(json!({"origin": origin})))
            .await;
        assert_eq!(code, 201, "{v}");
        let id = v["conversation"]["id"]
            .as_str()
            .expect("a conversation")
            .to_string();
        node.ws
            .post_message(
                &id,
                MessageBody::post("news for you"),
                None,
                &[],
                &[],
                Some(&agent),
                PostOrigin::Asked,
            )
            .expect("agent message in the conversation");
        opened.push(id);
    }
    let (of_goal, of_workspace) = (&opened[0], &opened[1]);
    let rows = |v: &serde_json::Value| v["rows"].as_array().cloned().unwrap_or_default();

    let goals = node.get("/inbox?source=goals").await;
    let row = rows(&goals)
        .into_iter()
        .find(|r| r["key"] == json!(of_goal))
        .unwrap_or_else(|| panic!("the goal's conversation under the goals: {goals}"));
    assert_eq!(row["kind"], json!("conversation"));
    assert_eq!(row["source"], json!("goals"));
    assert_eq!(
        row["origin"],
        json!({"kind": "goal", "id": goal}),
        "the row says which goal it is about"
    );
    assert!(
        rows(&goals).iter().all(|r| r["key"] != json!(of_workspace)),
        "the workspace's conversation is no goal's: {goals}"
    );

    let messages = node.get("/inbox?source=messages").await;
    let row = rows(&messages)
        .into_iter()
        .find(|r| r["key"] == json!(of_workspace))
        .unwrap_or_else(|| panic!("the workspace's conversation under the messages: {messages}"));
    assert_eq!(row["source"], json!("messages"));
    assert_eq!(row["origin"], json!({"kind": "workspace"}));
    assert!(
        rows(&messages).iter().all(|r| r["key"] != json!(of_goal)),
        "the goal's conversation is never a message: {messages}"
    );

    // Every row says its source, one of the five the route takes.
    let all = node.get("/inbox").await;
    for r in rows(&all) {
        let source = r["source"].as_str().expect("a source on every row");
        assert!(
            ["messages", "projects", "workflows", "goals", "people"].contains(&source),
            "{r}"
        );
    }
    node.shutdown().await;
}

/// A workflow the Workflow Agent wrote is a row under the workflows source,
/// with no conversation, carrying its notices — the design and, later, its
/// being put away — and a person's own save of it earns nothing. The facts
/// are recorded the way the engine records them, so the row builder is what
/// is under test.
#[tokio::test(flavor = "multi_thread")]
async fn a_designed_workflow_is_an_inbox_row_under_workflows() {
    use bisa_core::activity::{ActivityConcept, ActivityFact, ActivitySource, ActivitySourceKind};
    let node = Node::start().await;
    let wf = node.workflow(human_workflow()).await;
    let key = wf.to_string();
    assert!(
        node.get("/inbox?source=workflows").await["rows"]
            .as_array()
            .unwrap()
            .is_empty(),
        "a person's own workflow is no news to them"
    );
    let fact = |at: u64, kind: &str, event: serde_json::Value| ActivityFact {
        at,
        concept: ActivityConcept::Workflows,
        kind: kind.into(),
        source: ActivitySource::new(ActivitySourceKind::Workflow, key.clone()),
        author: None,
        event,
    };
    node.ws
        .record_activity(
            &fact(
                1_700_000_000,
                "workflow_changed",
                json!({"type": "workflow_changed", "workflow": key, "revision": 2, "designed": false}),
            ),
            true,
        )
        .expect("the fact");
    assert!(
        node.get("/inbox?source=workflows").await["rows"]
            .as_array()
            .unwrap()
            .is_empty(),
        "a save by hand is not a notice"
    );
    node.ws
        .record_activity(
            &fact(
                1_700_000_001,
                "workflow_changed",
                json!({"type": "workflow_changed", "workflow": key, "revision": 3, "designed": true}),
            ),
            true,
        )
        .expect("the fact");
    node.ws
        .record_activity(
            &fact(
                1_700_000_002,
                "workflow_archived",
                json!({"type": "workflow_archived", "workflow": key, "archived": true}),
            ),
            true,
        )
        .expect("the fact");

    let v = node.get("/inbox?source=workflows").await;
    let rows = v["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{v}");
    let row = &rows[0];
    assert_eq!(row["kind"], json!("workflow"));
    assert_eq!(row["key"], json!(key));
    assert_eq!(row["title"], json!(human_workflow()["name"]));
    let notices: Vec<&serde_json::Value> = row["notices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| &n["notice"])
        .collect();
    assert_eq!(
        notices,
        vec![&json!("workflow_archived"), &json!("workflow_designed")],
        "newest first: {v}"
    );
    assert_eq!(row["unread_notices"], json!(2));
    assert_eq!(row["needs_action"], json!([]), "a design is not owed");
    assert_eq!(row["unread_count"], json!(0), "no conversation");
    assert!(
        node.get("/inbox?filter=needs_you").await["rows"]
            .as_array()
            .unwrap()
            .is_empty(),
        "nothing waits on you: a notice is never owed"
    );
    assert!(
        node.get("/inbox?source=goals").await["rows"]
            .as_array()
            .unwrap()
            .is_empty(),
        "the workflow's row is under its own source"
    );
    node.shutdown().await;
}

/// A session's `browser_serve` (ide/18) reaches the node's own served
/// folders: the checkout is up on a loopback port the Browser menu lists
/// too, and asked again the same server answers.
/// The boundary a real session crosses: a chat instance in a channel, whose
/// scope id rides as the candidate `goal` on every request, asks the browser
/// through the node's engine — and is answered in the browser's words, never
/// with an envelope the engine could not read.
#[tokio::test(flavor = "multi_thread")]
async fn a_channel_scoped_browser_request_reaches_the_engine_and_is_answered_in_its_words() {
    let node = Node::start().await;
    let reply = node
        .intake_op(json!({
            "op": "browser",
            "request": {"action": "tabs"},
            "scope": "general",
            "agent": "general-agent",
            "goal": "general"
        }))
        .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    assert_eq!(
        reply["result"]["error"],
        json!(bisa_engine::browser::NOBODY_HOME),
        "no desktop has read the list: the browser's own sentence, at once"
    );
    let posted = node
        .intake_op(json!({
            "op": "post_message",
            "scope": "general",
            "content": "from a channel",
            "agent": "general-agent",
            "goal": "general"
        }))
        .await;
    assert_eq!(posted["ok"], json!(true), "{posted}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sessions_browser_serve_puts_its_checkout_on_the_nodes_servers() {
    let node = Node::start().await;
    let made = node
        .post("/projects", json!({"kind": "new", "slug": "web-app"}))
        .await;
    let pid = made["project"]["id"]
        .as_str()
        .expect("a project")
        .to_string();
    let (code, v) = node
        .req(
            "POST",
            "/conversations",
            Some(json!({"origin": {"kind": "workstream", "id": pid, "project": pid}})),
        )
        .await;
    assert_eq!(code, 201, "{v}");
    let scope = v["conversation"]["id"].as_str().unwrap().to_string();

    let reply = node
        .intake_op(json!({"op": "browser_serve", "scope": scope}))
        .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    let url = reply["result"]["url"].as_str().unwrap().to_string();
    assert!(url.starts_with("http://127.0.0.1:"), "{url}");
    assert_eq!(reply["result"]["folder"], json!(""));
    let id = reply["result"]["id"].as_str().unwrap().to_string();

    let listed = node.get(&format!("/workstreams/{pid}/servers")).await;
    let servers = listed["servers"].as_array().unwrap();
    assert_eq!(servers.len(), 1, "{listed}");
    assert_eq!(servers[0]["id"], json!(id));
    assert_eq!(servers[0]["url"], json!(url));
    assert_eq!(servers[0]["owner"]["kind"], json!("workstream"));
    assert_eq!(servers[0]["owner"]["workstream"], json!(pid));

    // The same folder asked again is the server already up — an agent
    // that retries reads the URL, and no second listener opens.
    let again = node
        .intake_op(json!({"op": "browser_serve", "scope": scope}))
        .await;
    assert_eq!(again["ok"], json!(true), "{again}");
    assert_eq!(again["result"]["id"], json!(id));
    let listed = node.get(&format!("/workstreams/{pid}/servers")).await;
    assert_eq!(listed["servers"].as_array().unwrap().len(), 1);

    // A folder the checkout lacks is refused in the node's words.
    let missing = node
        .intake_op(json!({"op": "browser_serve", "scope": scope, "folder": "dist"}))
        .await;
    assert_eq!(missing["ok"], json!(false), "{missing}");
    node.shutdown().await;
}

/// A step's place on the canvas is the step's own fact: saved with the
/// definition, read back as written, absent when nobody placed it.
#[tokio::test(flavor = "multi_thread")]
async fn a_steps_position_is_saved_with_the_workflow_and_absent_until_placed() {
    let node = Node::start().await;
    let mut placed = human_workflow();
    placed["steps"][0]["position"] = json!({ "x": 40, "y": 120 });
    let wf = node.workflow(placed).await;
    let stored = node.get(&format!("/workflows/{wf}")).await["workflow"].clone();
    assert_eq!(stored["steps"][0]["position"], json!({ "x": 40, "y": 120 }));
    let mut edited = human_workflow();
    edited["revision"] = stored["revision"].clone();
    edited["steps"][0]["position"] = json!({ "x": 8, "y": 16 });
    let (code, v) = node
        .req("PUT", &format!("/workflows/{wf}"), Some(edited))
        .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(
        v["workflow"]["steps"][0]["position"],
        json!({ "x": 8, "y": 16 })
    );
    let mut unplaced = human_workflow();
    unplaced["revision"] = v["workflow"]["revision"].clone();
    let (code, v) = node
        .req("PUT", &format!("/workflows/{wf}"), Some(unplaced))
        .await;
    assert_eq!(code, 200, "{v}");
    assert!(
        v["workflow"]["steps"][0].get("position").is_none(),
        "unplaced again: the key is absent, never null — {v}"
    );
    node.shutdown().await;
}

/// Inspecting a folder asks the questions adopting it would — an absolute
/// path, a real one, a directory, outside the workspace — so the dialog that
/// leads to `adopt` cannot inspect what adopt then refuses; a folder that is
/// no repository inspects to no remote.
#[tokio::test(flavor = "multi_thread")]
async fn inspecting_a_folder_asks_the_same_questions_as_adopting_it() {
    let node = Node::start().await;
    let (status, v) = node
        .req(
            "POST",
            "/codehost/inspect",
            Some(json!({"path": "relative/dir"})),
        )
        .await;
    assert_eq!(status, 400, "{v}");
    let said = v["error"].as_str().unwrap();
    assert!(
        said.contains("absolute") && said.contains("inspecting"),
        "the verb has its own words: {said}"
    );
    let inside = node.data_dir().display().to_string();
    let (status, v) = node
        .req("POST", "/codehost/inspect", Some(json!({"path": inside})))
        .await;
    assert_eq!(status, 400, "{v}");
    assert!(v["error"].as_str().unwrap().contains("inside"), "{v}");
    let (status, v) = node
        .req(
            "POST",
            "/codehost/inspect",
            Some(json!({"path": "/definitely/not/here"})),
        )
        .await;
    assert_eq!(status, 400, "{v}");
    let outside = tempfile::tempdir().unwrap();
    let ok = node
        .post(
            "/codehost/inspect",
            json!({"path": outside.path().display().to_string()}),
        )
        .await;
    assert!(
        ok["remote"].is_null(),
        "a folder that is no repository has no remote: {ok}"
    );
    node.shutdown().await;
}

// ---------------------------------------------------------------------------
// A run of the workspace's work item, and the workflow routes' refusals
// ---------------------------------------------------------------------------

/// A run of the workspace's work item, from its id alone: its home is the
/// run and its label the workflow's name and the run's number; a result the
/// journal holds is read back; a captured patch is served as text; a
/// settled item is forgotten on request.
#[tokio::test(flavor = "multi_thread")]
async fn a_workspace_runs_item_is_labelled_read_and_forgotten() {
    let node = Node::start().await;
    let wf = node.workflow(agent_workflow()).await;
    let v = node.post(&format!("/workflows/{wf}/runs"), json!({})).await;
    let run: RunId = serde_json::from_value(v["run"]["id"].clone()).unwrap();
    let run_home = json!({"home": "run", "run": run.to_string()});
    // The item settles blocked on its own: no harness of that name exists.
    let mut item = String::new();
    for _ in 0..100 {
        let rows = node.get("/work-items").await;
        let found = rows["work_items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["home"] == run_home && r["item"]["state"]["state"] == json!("blocked"));
        if let Some(row) = found {
            item = row["item"]["id"].as_str().unwrap().to_string();
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(!item.is_empty(), "the run's item never settled");
    let v = node.get(&format!("/work-items/{item}")).await;
    assert_eq!(v["home"], run_home, "{v}");
    assert_eq!(v["label"], json!("Build it #1"), "{v}");
    assert!(v["result"].is_null(), "{v}");
    assert_eq!(v["has_result"], json!(false));

    // A result the journal holds is read back.
    let home = bisa_core::Home::Run { run };
    let item_id: WorkItemId = item.parse().unwrap();
    node.ws
        .append_journal(
            &home,
            JournalPayload::Result {
                work_item: item_id,
                output: json!({"ok": true}),
                artifacts: vec![],
            },
            node.ws.owner_keys(),
            None,
        )
        .unwrap();
    let v = node.get(&format!("/work-items/{item}")).await;
    assert_eq!(v["result"], json!({"ok": true}), "{v}");

    // A captured patch is served as text.
    let patch = node.ws.paths().home(&home).result(item_id);
    std::fs::create_dir_all(patch.parent().unwrap()).unwrap();
    std::fs::write(&patch, "--- a/x\n+++ b/x\n").unwrap();
    assert_eq!(
        node.get(&format!("/work-items/{item}")).await["has_result"],
        json!(true)
    );
    let (code, headers, len) = request_headers(
        node.socket(),
        "GET",
        &format!("/work-items/{item}/result"),
        None,
    )
    .await;
    assert_eq!(code, 200);
    assert_eq!(
        headers.get("content-type").and_then(|v| v.to_str().ok()),
        Some("text/x-patch")
    );
    assert_eq!(len, "--- a/x\n+++ b/x\n".len());

    // A blocked item is not settled: it is cancelled first, then forgotten.
    let (code, v) = node
        .req("DELETE", &format!("/work-items/{item}"), None)
        .await;
    assert_eq!(code, 400, "{v}");
    node.ws
        .transition_work_item(&home, item_id, &bisa_core::WorkItemTransition::Cancel)
        .unwrap();
    let (code, v) = node
        .req("DELETE", &format!("/work-items/{item}"), None)
        .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["ok"], json!(true));
    assert_eq!(v["home"], run_home);
    assert_eq!(v["work_item"], json!(item));
    let (code, _) = node.req("GET", &format!("/work-items/{item}"), None).await;
    assert_eq!(code, 404, "forgotten");
    node.shutdown().await;
}

/// The workflow routes refuse in words: a goal that is no id, a scope word
/// nobody knows, and a workflow nobody made on every route that names one.
#[tokio::test(flavor = "multi_thread")]
async fn the_workflow_routes_refuse_a_bad_goal_a_bad_scope_and_a_workflow_nobody_made() {
    let node = Node::start().await;
    let (code, v) = node.req("GET", "/workflows?goal=not-an-id", None).await;
    assert_eq!(code, 400, "{v}");
    let (code, v) = node.req("GET", "/workflows?scope=everything", None).await;
    assert_eq!(code, 400, "{v}");
    let nobody = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    for (method, path, body) in [
        ("GET", format!("/workflows/{nobody}"), None),
        ("GET", format!("/workflows/{nobody}/runs"), None),
        (
            "POST",
            format!("/workflows/{nobody}/retire"),
            Some(json!({"workflow": "archive", "projects": "keep"})),
        ),
        (
            "POST",
            format!("/workflows/{nobody}/archive"),
            Some(json!({"archived": true})),
        ),
    ] {
        let (code, v) = node.req(method, &path, body).await;
        assert_eq!(code, 404, "{method} {path}: {v}");
    }
    node.shutdown().await;
}

// ---------------------------------------------------------------------------
// A goal's routes at their edges
// ---------------------------------------------------------------------------

/// The goal routes' edges: a capture of a workflow that listens at once
/// shows its public hook's secret this once; a goal pointed at a workflow
/// and handed a definition in one body is refused; a start step named on
/// a goal with no workflow is refused; a queued run that is no id is 404;
/// a message posted on the goal's thread lands there.
#[tokio::test(flavor = "multi_thread")]
async fn the_goal_routes_at_their_edges() {
    let node = Node::start().await;
    let hears = node
        .workflow(json!({
            "name": "Hears a call",
            "steps": [
                {"id": "ticket", "name": "A ticket arrives", "kind": "start",
                 "on": {"event": "hook", "public": true}, "then": ["say"]},
                {"id": "say", "name": "Say", "kind": "notify", "template": "heard"}
            ]
        }))
        .await;
    let (code, made) = node
        .req(
            "POST",
            "/goals",
            Some(json!({"statement": "listen for tickets", "workflow": hears.to_string(), "inputs": {}})),
        )
        .await;
    assert_eq!(code, 200, "{made}");
    let secrets = made["secrets"]
        .as_array()
        .unwrap_or_else(|| panic!("the secrets, shown once: {made}"));
    assert_eq!(secrets.len(), 1, "{made}");
    assert_eq!(secrets[0]["step"], json!("ticket"));
    assert_eq!(secrets[0]["secret"].as_str().map(str::len), Some(64));

    let goal = node.new_goal("edges").await;
    let (code, v) = node
        .req(
            "PUT",
            &format!("/goals/{goal}/workflow"),
            Some(json!({"workflow": hears.to_string(), "definition": human_workflow()})),
        )
        .await;
    assert_eq!(code, 400, "{v}");
    let (code, v) = node
        .req(
            "POST",
            &format!("/goals/{goal}/run"),
            Some(json!({"start": "ticket"})),
        )
        .await;
    assert!(
        code == 400 || code == 409,
        "a goal with no workflow starts nothing: {code} {v}"
    );
    let (code, v) = node
        .req("DELETE", &format!("/goals/{goal}/runs/not-an-id"), None)
        .await;
    assert_eq!(code, 404, "{v}");
    let (code, v) = node
        .req(
            "POST",
            &format!("/goals/{goal}/messages"),
            Some(json!({"content": "a word on the goal"})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
    let thread = node.get(&format!("/goals/{goal}/messages")).await;
    assert!(
        thread.to_string().contains("a word on the goal"),
        "{thread}"
    );
    node.shutdown().await;
}
