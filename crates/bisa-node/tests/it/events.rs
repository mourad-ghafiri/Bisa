//! Listening, listeners, hooks and signals over the node's real listener.
//!
//! The routes are adapters: what a host may listen with, whether an
//! occurrence starts its run and what a guard does are the engine's and are
//! tested there. What is the node's is tested here — the wire: the shapes a
//! row, a listener and a signal are answered in, the statuses a refusal
//! maps to, a secret shown once and never again — and, above all, the one
//! route somebody outside this machine is ever pointed at. Every answer
//! `POST /hooks/{host}/{step}` can give is exercised — the machine's switch
//! off, a valid token, a valid signature, a forged one, no credentials at
//! all, a listener that does not exist, a host that is Off, a body over the
//! cap, a flood — together with the property that matters more than any
//! single status code: **a refused call enqueues nothing.**
//!
//! The listening runtime is driven by hand (`events_enabled: false`): the
//! worker is a call ([`Node::drain`]), the ticker a call with the clock as
//! its argument ([`Node::tick`]) — nothing waits for a timer.

use bisa_core::activity::{ActivityConcept, ActivityFact, ActivitySource, ActivitySourceKind};
use bisa_engine::{Engine, EngineConfig};
use bisa_harness::HarnessCatalog;
use bisa_node::{serve, NodeConfig};
use bisa_store::{FileKeyStore, Workspace};
use http_body_util::{BodyExt as _, Full, StreamBody};
use hyper::body::{Bytes, Frame};
use hyper::Request;
use hyper_util::rt::TokioIo;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::UnixStream;

/// The control-plane token every test node is started with. A caller on
/// this machine presents it; a caller outside never holds it.
const TOKEN: &str = "test-token-0123456789abcdef";

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

struct Node {
    socket: PathBuf,
    /// The store, for a fact no route records (an activity row, as the
    /// engine writes one).
    ws: Arc<Workspace>,
    /// The engine's inner state: the listening runtime is driven through it.
    inner: Arc<bisa_engine::Inner>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    server: Option<tokio::task::JoinHandle<anyhow::Result<()>>>,
    _dir: tempfile::TempDir,
}

/// One response, with the header the hook contract promises.
struct Resp {
    status: u16,
    retry_after: Option<String>,
    body: Value,
    raw: String,
}

/// Who a request comes from.
#[derive(Clone, Copy)]
enum Caller {
    /// This machine: the control-plane token rides along.
    Here,
    /// Outside: no token, only what the hook's own headers carry.
    Outside,
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
        let engine = Engine::start(
            ws,
            HarnessCatalog::new(),
            EngineConfig {
                design_enabled: false,
                events_enabled: false,
                ..Default::default()
            },
        )
        .expect("engine");
        let ws = Arc::clone(&engine.inner().ws);
        let inner = Arc::clone(engine.inner());

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
        let server = tokio::spawn(serve(engine, cfg, async {
            let _stopped_or_dropped = stop_rx.await;
        }));

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
            ws,
            inner,
            stop: Some(stop),
            server: Some(server),
            _dir: dir,
        }
    }

    /// Settle the signal queue to a standstill, as the worker would.
    async fn drain(&self) -> usize {
        bisa_engine::listen::dispatch::drain_signals(&self.inner).await
    }

    /// One pass of the listening ticker at `now`.
    async fn tick(&self, now: u64) {
        bisa_engine::listen::sources::tick(&self.inner, now).await;
    }

    async fn json(&self, method: &str, path: &str, body: Option<Value>) -> Resp {
        let body = body.map(|b| b.to_string()).unwrap_or_default();
        self.send(
            Caller::Here,
            method,
            path,
            &[("content-type", "application/json")],
            body,
        )
        .await
    }

    async fn ok(&self, method: &str, path: &str, body: Option<Value>) -> Value {
        let r = self.json(method, path, body).await;
        assert_eq!(r.status, 200, "{method} {path} failed: {}", r.raw);
        r.body
    }

    /// A call to a public hook from outside: no token, the headers given,
    /// the body verbatim.
    async fn outside(&self, path: &str, headers: &[(&str, &str)], body: String) -> Resp {
        self.send(Caller::Outside, "POST", path, headers, body)
            .await
    }

    /// A call to a hook from this machine, under the control-plane token.
    async fn local(&self, path: &str, headers: &[(&str, &str)], body: String) -> Resp {
        self.send(Caller::Here, "POST", path, headers, body).await
    }

    /// One request with exactly the headers given and a body sent verbatim.
    async fn send(
        &self,
        caller: Caller,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: String,
    ) -> Resp {
        let stream = UnixStream::connect(&self.socket).await.expect("connect");
        let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
            .await
            .expect("handshake");
        tokio::spawn(conn);
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header(hyper::header::HOST, "localhost");
        if let Caller::Here = caller {
            builder = builder.header(hyper::header::AUTHORIZATION, format!("Bearer {TOKEN}"));
        }
        for (k, v) in headers {
            builder = builder.header(*k, *v);
        }
        let req = builder.body(Full::new(Bytes::from(body))).unwrap();
        let resp = sender.send_request(req).await.expect("request");
        Resp::of(resp).await
    }

    /// A body streamed from outside in chunks with **no `Content-Length`** —
    /// the case that proves the cap is enforced while reading rather than
    /// after.
    async fn chunked(&self, path: &str, headers: &[(&str, &str)], chunks: Vec<Vec<u8>>) -> Resp {
        let stream = UnixStream::connect(&self.socket).await.expect("connect");
        let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
            .await
            .expect("handshake");
        tokio::spawn(conn);
        let body =
            StreamBody::new(futures::stream::iter(chunks.into_iter().map(|c| {
                Ok::<_, std::convert::Infallible>(Frame::data(Bytes::from(c)))
            })));
        let mut builder = Request::builder()
            .method("POST")
            .uri(path)
            .header(hyper::header::HOST, "localhost");
        for (k, v) in headers {
            builder = builder.header(*k, *v);
        }
        let req = builder.body(body).unwrap();
        let resp = sender.send_request(req).await.expect("request");
        Resp::of(resp).await
    }

    /// Record a workflow; returns its id.
    async fn workflow(&self, definition: Value) -> String {
        let v = self.ok("POST", "/workflows", Some(definition)).await;
        assert_eq!(v["problems"], json!([]), "the fixture is sound: {v}");
        v["workflow"]["id"]
            .as_str()
            .expect("workflow id")
            .to_string()
    }

    /// Turn a library workflow On with `body`; answers what the route did.
    async fn turn_on(&self, wf: &str, body: Value) -> Value {
        self.ok("PUT", &format!("/workflows/{wf}/listening"), Some(body))
            .await
    }

    /// A public hook workflow, On: `(workflow, secret)`.
    async fn public_hook(&self, name: &str) -> (String, String) {
        let wf = self.workflow(hook_workflow(name, true)).await;
        let on = self.turn_on(&wf, json!({})).await;
        let secret = on["secrets"][0]["secret"]
            .as_str()
            .expect("the secret, shown once")
            .to_string();
        assert_eq!(secret.len(), 64, "32 random bytes, hex");
        assert_eq!(on["secrets"][0]["path"], json!(public_path(&wf)));
        (wf, secret)
    }

    /// One machine setting, written as a person writes it.
    async fn set_machine(&self, key: &str, value: Value) {
        self.ok(
            "PUT",
            "/settings/machine",
            Some(json!({ "values": { (key): value } })),
        )
        .await;
    }

    /// This machine takes public hook calls, and queues them unscreened —
    /// so a call taken is a signal queued, with nothing behind it to wait
    /// for.
    async fn allow_public_hooks(&self) {
        self.set_machine("events.public_hooks", json!(true)).await;
        self.set_machine("security.content.screen", json!(false))
            .await;
    }

    /// A host's signals, newest first.
    async fn signals_of(&self, host: &str) -> Vec<Value> {
        let v = self
            .ok("GET", &format!("/signals?host={host}&limit=200"), None)
            .await;
        v.as_array().cloned().expect("a list of signals")
    }

    /// Read `path` until `done` answers something, a few seconds at most.
    async fn until<T>(&self, what: &str, path: &str, done: impl Fn(&Value) -> Option<T>) -> T {
        for _ in 0..100 {
            let v = self.ok("GET", path, None).await;
            if let Some(found) = done(&v) {
                return found;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("{what} never came: {}", self.ok("GET", path, None).await);
    }

    async fn shutdown(mut self) {
        let _server_gone = self.stop.take().unwrap().send(());
        let server = self.server.take().unwrap();
        tokio::time::timeout(Duration::from_secs(10), server)
            .await
            .expect("server did not stop")
            .expect("server task")
            .expect("serve error");
    }
}

impl Resp {
    async fn of<B>(resp: hyper::Response<B>) -> Self
    where
        B: hyper::body::Body<Data = Bytes> + Unpin,
        B::Error: std::fmt::Debug,
    {
        let status = resp.status().as_u16();
        let retry_after = resp
            .headers()
            .get(hyper::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let raw = String::from_utf8_lossy(&bytes).to_string();
        Self {
            status,
            retry_after,
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            raw,
        }
    }
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// A step a run stays live on until somebody releases it.
fn hold() -> Value {
    json!({"id": "hold", "name": "Hold", "kind": "wait", "until": {"until": "release"}})
}

/// A workflow only a call begins: one hook start, `ticket` — public or not
/// — that maps the body's subject, and a hold its runs stay live on.
fn hook_workflow(name: &str, public: bool) -> Value {
    json!({
        "name": name,
        "inputs": [{"name": "subject", "label": "Subject", "kind": "text", "required": true}],
        "steps": [
            {"id": "ticket", "name": "A ticket arrives", "kind": "start",
             "on": {"event": "hook", "public": public},
             "inputs": {"subject": "{event.payload.subject}"},
             "then": ["hold"]},
            hold(),
        ]
    })
}

/// A workflow with two ways in: by hand, and on a call.
fn two_ways_in(name: &str) -> Value {
    json!({
        "name": name,
        "steps": [
            {"id": "by-hand", "name": "By hand", "kind": "start",
             "on": {"event": "manual"}, "then": ["hold"]},
            {"id": "ticket", "name": "A ticket arrives", "kind": "start",
             "on": {"event": "hook"}, "then": ["hold"]},
            hold(),
        ]
    })
}

/// A workflow that begins every hour and says who its digest is for — an
/// input its event does not supply and no default fills.
fn hourly_digest() -> Value {
    json!({
        "name": "Hourly digest",
        "inputs": [
            {"name": "who", "label": "Who", "kind": "text", "required": true},
            {"name": "at", "label": "At", "kind": "number"},
        ],
        "steps": [
            {"id": "hourly", "name": "Every hour", "kind": "start",
             "on": {"event": "schedule", "every": 3600},
             "inputs": {"at": "{event.payload.at}"},
             "then": ["tell"]},
            {"id": "tell", "name": "Tell", "kind": "notify",
             "template": "the digest for {inputs.who}", "then": ["hold"]},
            hold(),
        ]
    })
}

/// A workflow that begins when the signal `name` is raised.
fn on_signal(name: &str) -> Value {
    json!({
        "name": format!("On {name}"),
        "steps": [
            {"id": "raised", "name": "The signal is raised", "kind": "start",
             "on": {"event": "signal", "name": name}, "then": ["hold"]},
            hold(),
        ]
    })
}

fn public_path(wf: &str) -> String {
    format!("/hooks/workspace:{wf}/ticket")
}

fn local_path(wf: &str) -> String {
    format!("/workflows/{wf}/hooks/ticket")
}

fn sign(secret: &str, body: &str) -> String {
    use hmac::{Hmac, Mac as _};
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body.as_bytes());
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

/// A goal on a recorded workflow, not started.
async fn goal_on(node: &Node, statement: &str, wf: &str) -> String {
    let v = node
        .ok(
            "POST",
            "/goals",
            Some(json!({"statement": statement, "title": "T", "mode": "manual"})),
        )
        .await;
    let goal = v["goal"]["id"].as_str().expect("goal id").to_string();
    node.ok(
        "PUT",
        &format!("/goals/{goal}/workflow"),
        Some(json!({"workflow": wf})),
    )
    .await;
    goal
}

/// The workflow's runs of the workspace once there are `n`, newest first.
async fn runs_of(node: &Node, wf: &str, n: usize) -> Vec<Value> {
    node.until(
        "the workflow's runs",
        &format!("/workflows/{wf}/runs"),
        |v| {
            let runs = v["runs"].as_array()?;
            (runs.len() >= n).then(|| runs.clone())
        },
    )
    .await
}

// ---------------------------------------------------------------------------
// A library workflow: On, Off, and what its row says
// ---------------------------------------------------------------------------

/// A workflow's row says how it begins and what turning it On asks; On, it
/// carries its standing — and the toggle is never a revision of it.
#[tokio::test(flavor = "multi_thread")]
async fn a_workflow_is_turned_on_and_off_and_its_row_says_where_it_stands() {
    let node = Node::start().await;
    let wf = node.workflow(hourly_digest()).await;

    let row = node.ok("GET", &format!("/workflows/{wf}"), None).await;
    assert_eq!(row["listening"], Value::Null, "it installs Off: {row}");
    assert_eq!(row["event_only"], json!(true), "no start by hand: {row}");
    assert_eq!(row["listening_needs"], json!(["who"]), "{row}");
    let starts = row["starts"].as_array().expect("starts");
    assert_eq!(starts.len(), 1, "{row}");
    assert_eq!(starts[0]["step"], json!("hourly"));
    assert_eq!(starts[0]["event"], json!("schedule"));
    assert!(
        starts[0]["summary"]["id"]
            .as_str()
            .is_some_and(|id| id.starts_with("step-summary-start-")),
        "a summary as data: {row}"
    );
    let revision = row["workflow"]["revision"].clone();

    // Turning On asks what its events do not supply, and takes nothing the
    // workflow does not declare.
    let refused = node
        .json(
            "PUT",
            &format!("/workflows/{wf}/listening"),
            Some(json!({})),
        )
        .await;
    assert_eq!(refused.status, 400, "{}", refused.raw);
    assert!(
        refused.body["error"]
            .as_str()
            .is_some_and(|e| e.contains("who")),
        "the refusal names the input: {}",
        refused.raw
    );
    let refused = node
        .json(
            "PUT",
            &format!("/workflows/{wf}/listening"),
            Some(json!({"inputs": {"who": "the team", "whom": "a typo"}})),
        )
        .await;
    assert_eq!(refused.status, 400, "{}", refused.raw);
    assert!(refused.raw.contains("whom"), "{}", refused.raw);
    let refused = node
        .json(
            "PUT",
            &format!("/workflows/{wf}/listening"),
            Some(json!({"inputs": {"who": "the team"}, "ceiling": 3})),
        )
        .await;
    assert_eq!(refused.status, 400, "a key nobody knows: {}", refused.raw);
    assert_eq!(
        node.ok("GET", &format!("/workflows/{wf}"), None).await["listening"],
        Value::Null,
        "a refusal turns nothing on"
    );

    let on = node
        .turn_on(
            &wf,
            json!({"inputs": {"who": "the team"}, "budget": {"max_tokens": 10000}}),
        )
        .await;
    assert_eq!(on["secrets"], json!([]), "no public hook, no secret");
    let standing = &on["workflow"]["listening"];
    assert_eq!(standing["inputs"], json!({"who": "the team"}), "{on}");
    assert_eq!(standing["budget"], json!({"max_tokens": 10000}), "{on}");
    assert!(standing["since"].is_u64(), "{on}");
    assert!(standing.get("paused").is_none(), "{on}");
    assert_eq!(
        on["workflow"]["workflow"]["revision"], revision,
        "a toggle is never a revision"
    );
    let listeners = on["listeners"].as_array().expect("listeners");
    assert_eq!(listeners.len(), 1, "{on}");
    let listener = &listeners[0];
    assert_eq!(
        listener["listener"],
        json!(format!("workspace:{wf}/hourly"))
    );
    assert_eq!(listener["host"], json!(format!("workspace:{wf}")));
    assert_eq!(listener["step"], json!("hourly"));
    assert_eq!(listener["event"], json!("schedule"));
    assert_eq!(listener["backlog"], json!(0));
    assert_eq!(listener["live_runs"], json!(0));
    for absent in ["local_hook", "public_hook", "failed", "last_fired_at"] {
        assert!(listener.get(absent).is_none(), "{absent}: {listener}");
    }

    // The first tick arms it: the listener says when it next comes due.
    let t0 = now();
    node.tick(t0).await;
    let listed = node
        .ok("GET", &format!("/workflows/{wf}/listeners"), None)
        .await;
    assert_eq!(listed[0]["next_due"], json!(t0 + 3600), "{listed}");
    let everyone = node.ok("GET", "/listeners", None).await;
    assert_eq!(
        everyone
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["listener"].clone())
            .collect::<Vec<_>>(),
        vec![json!(format!("workspace:{wf}/hourly"))]
    );

    // The library's row carries it too.
    let library = node.ok("GET", "/workflows", None).await;
    let listed = library["workflows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["workflow"]["id"] == json!(wf))
        .expect("the workflow's row");
    assert_eq!(listed["listening"]["inputs"], json!({"who": "the team"}));

    // An occurrence is a run of the workspace, by its event.
    node.tick(t0 + 3600).await;
    assert_eq!(node.drain().await, 1);
    let runs = runs_of(&node, &wf, 1).await;
    assert_eq!(
        runs[0]["started_by"],
        json!({"by": "event", "event": "schedule"}),
        "{runs:?}"
    );
    let listed = node
        .ok("GET", &format!("/workflows/{wf}/listeners"), None)
        .await;
    assert_eq!(listed[0]["live_runs"], json!(1), "{listed}");
    assert!(listed[0]["last_fired_at"].is_u64(), "{listed}");

    // Off: the standing goes, the listeners with it; Off again is nothing.
    let off = node
        .ok("DELETE", &format!("/workflows/{wf}/listening"), None)
        .await;
    assert_eq!(off["workflow"]["listening"], Value::Null, "{off}");
    assert_eq!(off["workflow"]["workflow"]["revision"], revision);
    assert_eq!(
        node.ok("GET", &format!("/workflows/{wf}/listeners"), None)
            .await,
        json!([])
    );
    assert_eq!(node.ok("GET", "/listeners", None).await, json!([]));
    node.ok("DELETE", &format!("/workflows/{wf}/listening"), None)
        .await;

    // A workflow nobody has, by every verb that names one.
    let ghost = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    for (method, path) in [
        ("PUT", format!("/workflows/{ghost}/listening")),
        ("DELETE", format!("/workflows/{ghost}/listening")),
        ("GET", format!("/workflows/{ghost}/listeners")),
        ("POST", format!("/workflows/{ghost}/hooks/ticket")),
        ("POST", format!("/workflows/{ghost}/hooks/ticket/secret")),
    ] {
        let r = node.json(method, &path, Some(json!({}))).await;
        assert_eq!(r.status, 404, "{method} {path}: {}", r.raw);
    }
    let r = node
        .json("PUT", "/workflows/not-an-id/listening", Some(json!({})))
        .await;
    assert_eq!(r.status, 400, "{}", r.raw);

    node.shutdown().await;
}

/// What cannot listen as it stands is refused in words — with its problems
/// when it has any — and nothing is turned on.
#[tokio::test(flavor = "multi_thread")]
async fn what_cannot_listen_is_refused_with_its_problems() {
    let node = Node::start().await;

    // A draft whose start flows into a step nobody wrote.
    let draft = node
        .ok(
            "POST",
            "/workflows",
            Some(json!({
                "name": "Half drawn",
                "steps": [
                    {"id": "ticket", "name": "A ticket arrives", "kind": "start",
                     "on": {"event": "hook"}, "then": ["nowhere"]},
                ]
            })),
        )
        .await;
    assert_ne!(draft["problems"], json!([]), "kept with its problems");
    let half_drawn = draft["workflow"]["id"].as_str().unwrap().to_string();
    let refused = node
        .json(
            "PUT",
            &format!("/workflows/{half_drawn}/listening"),
            Some(json!({})),
        )
        .await;
    assert_eq!(refused.status, 400, "{}", refused.raw);
    assert!(
        refused.body["problems"]
            .as_array()
            .is_some_and(|ps| ps.iter().any(|p| p["kind"] == json!("unknown_step"))),
        "the problems ride the refusal: {}",
        refused.raw
    );

    // Nothing to listen for.
    let by_hand = node
        .workflow(json!({
            "name": "By hand only",
            "steps": [
                {"id": "begin", "name": "Begin", "kind": "start",
                 "on": {"event": "manual"}, "then": ["hold"]},
                hold(),
            ]
        }))
        .await;
    let refused = node
        .json(
            "PUT",
            &format!("/workflows/{by_hand}/listening"),
            Some(json!({})),
        )
        .await;
    assert_eq!(refused.status, 400, "{}", refused.raw);
    assert!(
        refused.raw.contains("no start on an event"),
        "{}",
        refused.raw
    );
    let row = node.ok("GET", &format!("/workflows/{by_hand}"), None).await;
    assert_eq!(row["event_only"], json!(false));
    assert_eq!(row["starts"][0]["event"], json!("manual"));
    assert_eq!(row["listening_needs"], json!([]));

    // It reads a goal the workspace would not give it.
    let reads_goal = node
        .workflow(json!({
            "name": "Reads its goal",
            "steps": [
                {"id": "ticket", "name": "A ticket arrives", "kind": "start",
                 "on": {"event": "hook"}, "then": ["say"]},
                {"id": "say", "name": "Say", "kind": "notify",
                 "template": "working on {goal.statement}"},
            ]
        }))
        .await;
    let refused = node
        .json(
            "PUT",
            &format!("/workflows/{reads_goal}/listening"),
            Some(json!({})),
        )
        .await;
    assert_eq!(refused.status, 400, "{}", refused.raw);
    assert!(
        refused.body["problems"]
            .as_array()
            .is_some_and(|ps| ps.iter().any(|p| p["kind"] == json!("needs_goal"))),
        "{}",
        refused.raw
    );

    // It is put away.
    let archived = node.workflow(hook_workflow("Put away", false)).await;
    node.ok(
        "POST",
        &format!("/workflows/{archived}/archive"),
        Some(json!({"archived": true})),
    )
    .await;
    let refused = node
        .json(
            "PUT",
            &format!("/workflows/{archived}/listening"),
            Some(json!({})),
        )
        .await;
    assert_eq!(refused.status, 400, "{}", refused.raw);
    assert!(refused.raw.contains("archived"), "{}", refused.raw);

    // It is a goal's own design: its goal listens, never the design.
    let goal = node
        .ok(
            "POST",
            "/goals",
            Some(json!({"statement": "design me", "title": "T", "mode": "manual"})),
        )
        .await["goal"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let designed = node
        .ok(
            "PUT",
            &format!("/goals/{goal}/workflow"),
            Some(json!({"definition": hook_workflow("The goal's own", false)})),
        )
        .await;
    let design = designed["workflow"]["id"].as_str().unwrap().to_string();
    let refused = node
        .json(
            "PUT",
            &format!("/workflows/{design}/listening"),
            Some(json!({})),
        )
        .await;
    assert_eq!(refused.status, 400, "{}", refused.raw);

    assert_eq!(node.ok("GET", "/listeners", None).await, json!([]));
    node.shutdown().await;
}

/// A public hook's secret is answered by the turn that mints it and by a
/// rotation — and by nothing else, ever.
#[tokio::test(flavor = "multi_thread")]
async fn a_hooks_secret_is_shown_once_and_never_again() {
    let node = Node::start().await;
    node.allow_public_hooks().await;
    let (wf, secret) = node.public_hook("From outside").await;

    // The listener says a secret is minted, never which.
    let listeners = node
        .json("GET", &format!("/workflows/{wf}/listeners"), None)
        .await;
    assert_eq!(listeners.status, 200);
    let listener = &listeners.body[0];
    assert_eq!(listener["event"], json!("hook"));
    assert_eq!(listener["local_hook"], json!(local_path(&wf)));
    assert_eq!(
        listener["public_hook"],
        json!({"path": public_path(&wf), "has_secret": true})
    );
    for path in [
        format!("/workflows/{wf}/listeners"),
        format!("/workflows/{wf}"),
        "/workflows".to_string(),
        "/listeners".to_string(),
        "/signals".to_string(),
    ] {
        let read = node.json("GET", &path, None).await;
        assert!(!read.raw.contains(&secret), "GET {path} leaked the secret");
    }

    // Off and On again: the secret is kept, and not shown a second time.
    node.ok("DELETE", &format!("/workflows/{wf}/listening"), None)
        .await;
    let again = node.turn_on(&wf, json!({})).await;
    assert_eq!(again["secrets"], json!([]), "shown once");
    let kept = node
        .outside(&public_path(&wf), &[("x-bisa-token", &secret)], "{}".into())
        .await;
    assert_eq!(kept.status, 202, "kept across Off and On: {}", kept.raw);

    // Rotation is the only recovery, and it invalidates the old secret.
    let rotated = node
        .ok(
            "POST",
            &format!("/workflows/{wf}/hooks/ticket/secret"),
            None,
        )
        .await;
    assert_eq!(rotated["step"], json!("ticket"));
    assert_eq!(rotated["path"], json!(public_path(&wf)));
    let new_secret = rotated["secret"].as_str().unwrap().to_string();
    assert_eq!(new_secret.len(), 64);
    assert_ne!(new_secret, secret);
    let old = node
        .outside(&public_path(&wf), &[("x-bisa-token", &secret)], "{}".into())
        .await;
    assert_eq!(old.status, 401, "the rotated-away secret still worked");
    let new = node
        .outside(
            &public_path(&wf),
            &[("x-bisa-token", &new_secret)],
            "{}".into(),
        )
        .await;
    assert_eq!(new.status, 202, "{}", new.raw);

    // A start that is not public has nothing to rotate, and no public call.
    let private = node.workflow(hook_workflow("From here only", false)).await;
    let on = node.turn_on(&private, json!({})).await;
    assert_eq!(on["secrets"], json!([]));
    assert_eq!(
        on["listeners"][0]["local_hook"],
        json!(local_path(&private))
    );
    assert!(on["listeners"][0].get("public_hook").is_none(), "{on}");
    let r = node
        .json(
            "POST",
            &format!("/workflows/{private}/hooks/ticket/secret"),
            None,
        )
        .await;
    assert_eq!(r.status, 400, "{}", r.raw);

    node.shutdown().await;
}

// ---------------------------------------------------------------------------
// The local hook door
// ---------------------------------------------------------------------------

/// A hook called from this machine is taken while its host listens — 202
/// and a signal, nothing about a run — and refused in words while it does
/// not.
#[tokio::test(flavor = "multi_thread")]
async fn a_hook_is_called_from_this_machine_while_its_workflow_is_on() {
    let node = Node::start().await;
    let wf = node.workflow(hook_workflow("Tickets", false)).await;
    let host = format!("workspace:{wf}");
    let body = json!({"subject": "printer on fire"}).to_string();
    let json_body = [("content-type", "application/json")];

    // Off: refused, and nothing is written down.
    let off = node.local(&local_path(&wf), &json_body, body.clone()).await;
    assert_eq!(off.status, 409, "{}", off.raw);
    assert!(
        off.body["error"]
            .as_str()
            .is_some_and(|e| e.contains("not listening")),
        "{}",
        off.raw
    );
    assert_eq!(
        off.body["text"]["id"],
        json!("error-node-hooks-host-not-listening")
    );
    assert!(node.signals_of(&host).await.is_empty());

    node.turn_on(&wf, json!({})).await;
    let taken = node.local(&local_path(&wf), &json_body, body.clone()).await;
    assert_eq!(taken.status, 202, "{}", taken.raw);
    let signal = taken.body["signal"].as_str().expect("a signal").to_string();
    // The caller learns a signal id and nothing about what it will cause.
    assert_eq!(taken.body, json!({ "signal": signal }));

    let queued = node.signals_of(&host).await;
    assert_eq!(queued.len(), 1, "{queued:?}");
    assert_eq!(queued[0]["id"], json!(signal));
    assert_eq!(queued[0]["listener"], json!(format!("{host}/ticket")));
    assert_eq!(queued[0]["source"], json!("hook"));
    assert_eq!(queued[0]["state"], json!("queued"));
    assert_eq!(queued[0]["scope"], json!({"scope": "workspace"}));
    assert!(queued[0]["at"].is_u64());
    assert!(
        queued[0].get("payload").is_none(),
        "a list of signals never carries a payload: {queued:?}"
    );
    let listeners = node
        .ok("GET", &format!("/workflows/{wf}/listeners"), None)
        .await;
    assert_eq!(listeners[0]["backlog"], json!(1), "{listeners}");
    assert_eq!(
        node.ok("GET", &format!("/workflows/{wf}/runs"), None).await["runs"],
        json!([]),
        "an event never acts"
    );

    // The worker starts its run, with what the start's mapping read.
    assert_eq!(node.drain().await, 1);
    let runs = runs_of(&node, &wf, 1).await;
    assert_eq!(runs[0]["scope"], json!("workspace"));
    assert_eq!(
        runs[0]["started_by"],
        json!({"by": "event", "event": "hook"})
    );
    let run = node
        .ok(
            "GET",
            &format!("/runs/{}", runs[0]["id"].as_str().unwrap()),
            None,
        )
        .await;
    assert_eq!(run["run"]["start"], json!("ticket"));
    assert_eq!(run["run"]["dispatched"], json!(signal));
    assert_eq!(run["run"]["inputs"]["subject"], json!("printer on fire"));
    assert_eq!(run["summary"]["started_by"]["by"], json!("event"));
    assert_eq!(node.signals_of(&host).await[0]["state"], json!("done"));

    // A step that is no hook start of it, and a word that is no step.
    let r = node
        .local(
            &format!("/workflows/{wf}/hooks/hold"),
            &json_body,
            "{}".into(),
        )
        .await;
    assert_eq!(r.status, 404, "{}", r.raw);
    let r = node
        .local(
            &format!("/workflows/{wf}/hooks/Not%20A%20Step"),
            &json_body,
            "{}".into(),
        )
        .await;
    assert_eq!(r.status, 400, "{}", r.raw);

    // Over the cap: refused as the body it is.
    let big = format!(
        "{{\"subject\":\"{}\"}}",
        "a".repeat(bisa_core::MAX_SIGNAL_PAYLOAD_BYTES + 1024)
    );
    let r = node.local(&local_path(&wf), &json_body, big).await;
    assert_eq!(r.status, 413, "{}", r.raw);

    // `events.enabled` off: the hooks take nothing, and say why.
    node.set_machine("events.enabled", json!(false)).await;
    let r = node.local(&local_path(&wf), &json_body, body).await;
    assert_eq!(r.status, 409, "{}", r.raw);
    assert_eq!(
        r.body["text"]["id"],
        json!("error-node-hooks-listening-off-on-this-machine")
    );
    assert_eq!(node.signals_of(&host).await.len(), 1);

    node.shutdown().await;
}

/// A redelivery is the same occurrence: the caller's key — or a code host's
/// delivery id — makes a second call the first one's signal.
/// A caller on this machine that calls faster than the work goes fills the
/// start's backlog: past it the call is refused as *too many* — never taken
/// and dropped — and it is taken again once a run ended and made room.
#[tokio::test(flavor = "multi_thread")]
async fn a_local_call_over_a_full_backlog_is_429_until_a_run_makes_room() {
    let node = Node::start().await;
    let wf = node.workflow(hook_workflow("Busy desk", false)).await;
    let host = format!("workspace:{wf}");
    let json_body = [("content-type", "application/json")];
    let ticket = |n: usize| json!({ "subject": format!("ticket {n}") }).to_string();
    node.turn_on(&wf, json!({})).await;
    let backlog = bisa_core::EventSettings::default().backlog_per_listener as usize;

    // The first call begins a run, which holds; the start runs one at a
    // time, so what comes after waits behind it.
    let first = node.local(&local_path(&wf), &json_body, ticket(0)).await;
    assert_eq!(first.status, 202, "{}", first.raw);
    node.drain().await;
    let runs = runs_of(&node, &wf, 1).await;
    for n in 1..=backlog {
        let r = node.local(&local_path(&wf), &json_body, ticket(n)).await;
        assert_eq!(r.status, 202, "call {n}: {}", r.raw);
        node.drain().await;
    }
    let before = node.signals_of(&host).await.len();
    assert_eq!(before, backlog + 1);

    let over = node
        .local(&local_path(&wf), &json_body, ticket(backlog + 1))
        .await;
    assert_eq!(over.status, 429, "{}", over.raw);
    assert_eq!(
        over.body["text"]["id"],
        json!("error-node-hooks-backlog-full"),
        "{}",
        over.raw
    );
    assert_eq!(
        node.signals_of(&host).await.len(),
        before,
        "a call that was refused wrote nothing down"
    );
    assert_eq!(
        node.ok("GET", &format!("/workflows/{wf}/runs"), None).await["runs"]
            .as_array()
            .map(Vec::len),
        Some(1),
        "and one run goes, as its guard says"
    );

    // The run ends: the next in line begins, and there is room for a call.
    let run = runs[0]["id"].as_str().unwrap();
    node.ok(
        "POST",
        &format!("/runs/{run}/steps/hold/release"),
        Some(json!({})),
    )
    .await;
    // This node's worker is driven by hand: one pass takes up what the
    // run's end let go.
    node.drain().await;
    node.until(
        "the next run to begin",
        &format!("/workflows/{wf}/runs"),
        |v| (v["runs"].as_array()?.len() == 2).then_some(()),
    )
    .await;
    let room = node
        .local(&local_path(&wf), &json_body, ticket(backlog + 2))
        .await;
    assert_eq!(room.status, 202, "{}", room.raw);
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_redelivery_is_deduplicated() {
    let node = Node::start().await;
    node.allow_public_hooks().await;
    let (wf, secret) = node.public_hook("From outside").await;
    let host = format!("workspace:{wf}");
    let body = json!({"subject": "toner low"}).to_string();

    let first = node
        .local(
            &local_path(&wf),
            &[("idempotency-key", "order-17")],
            body.clone(),
        )
        .await;
    assert_eq!(first.status, 202, "{}", first.raw);
    let again = node
        .local(
            &local_path(&wf),
            &[("idempotency-key", "order-17")],
            body.clone(),
        )
        .await;
    assert_eq!(again.status, 202, "{}", again.raw);
    assert_eq!(again.body["signal"], first.body["signal"], "one occurrence");
    assert_eq!(node.signals_of(&host).await.len(), 1);

    // The same from outside, under a code host's delivery id.
    let delivery = [
        ("x-bisa-token", secret.as_str()),
        ("x-github-delivery", "72d3162e-cc78-11e3-81ab-4c9367dc0958"),
    ];
    let pushed = node
        .outside(&public_path(&wf), &delivery, body.clone())
        .await;
    assert_eq!(pushed.status, 202, "{}", pushed.raw);
    let redelivered = node
        .outside(&public_path(&wf), &delivery, body.clone())
        .await;
    assert_eq!(redelivered.status, 202, "{}", redelivered.raw);
    assert_eq!(redelivered.body["signal"], pushed.body["signal"]);
    assert_ne!(pushed.body["signal"], first.body["signal"]);
    assert_eq!(node.signals_of(&host).await.len(), 2);

    // No key: every call is its own occurrence.
    let plain = node
        .outside(
            &public_path(&wf),
            &[("x-bisa-token", &secret)],
            body.clone(),
        )
        .await;
    assert_eq!(plain.status, 202, "{}", plain.raw);
    assert_eq!(node.signals_of(&host).await.len(), 3);

    // One occurrence, one run.
    node.drain().await;
    let listeners = node
        .ok("GET", &format!("/workflows/{wf}/listeners"), None)
        .await;
    assert_eq!(
        listeners[0]["live_runs"].as_u64().unwrap() + listeners[0]["backlog"].as_u64().unwrap(),
        3,
        "{listeners}"
    );

    node.shutdown().await;
}

// ---------------------------------------------------------------------------
// The public hook door
// ---------------------------------------------------------------------------

/// While this machine allows no public hooks the door answers nobody
/// anything: 404, whatever is asked and whoever asks.
#[tokio::test(flavor = "multi_thread")]
async fn the_public_door_is_closed_until_the_machine_opens_it() {
    let node = Node::start().await;
    let (wf, secret) = node.public_hook("From outside").await;
    let host = format!("workspace:{wf}");

    let with_the_secret = node
        .outside(&public_path(&wf), &[("x-bisa-token", &secret)], "{}".into())
        .await;
    assert_eq!(with_the_secret.status, 404, "{}", with_the_secret.raw);
    let with_nothing = node.outside(&public_path(&wf), &[], "{}".into()).await;
    let nobody = node
        .outside(
            "/hooks/workspace:01ARZ3NDEKTSV4RRFFQ69G5FAV/ticket",
            &[("x-bisa-token", &secret)],
            "{}".into(),
        )
        .await;
    for r in [&with_nothing, &nobody] {
        assert_eq!(r.status, with_the_secret.status);
        assert_eq!(r.body, with_the_secret.body, "one answer for all");
    }
    assert_eq!(with_the_secret.body, json!({"error": "not found"}));
    assert!(node.signals_of(&host).await.is_empty());

    // A call from this machine is taken all the while.
    let local = node
        .local(
            &local_path(&wf),
            &[],
            json!({"subject": "mine"}).to_string(),
        )
        .await;
    assert_eq!(local.status, 202, "{}", local.raw);

    // Opened, the same call is taken; with `events.enabled` off, none is.
    node.allow_public_hooks().await;
    let opened = node
        .outside(
            &public_path(&wf),
            &[("x-bisa-token", &secret)],
            json!({"subject": "theirs"}).to_string(),
        )
        .await;
    assert_eq!(opened.status, 202, "{}", opened.raw);
    node.set_machine("events.enabled", json!(false)).await;
    let stopped = node
        .outside(&public_path(&wf), &[("x-bisa-token", &secret)], "{}".into())
        .await;
    assert_eq!(stopped.status, 404, "{}", stopped.raw);
    assert_eq!(node.signals_of(&host).await.len(), 2);

    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_valid_token_and_a_valid_signature_both_get_in() {
    let node = Node::start().await;
    node.allow_public_hooks().await;
    let (wf, secret) = node.public_hook("From a code host").await;
    let host = format!("workspace:{wf}");
    let path = public_path(&wf);

    let body = json!({"subject": "merged", "pull_request": {"merged": true}}).to_string();
    let by_token = node
        .outside(
            &path,
            &[
                ("content-type", "application/json"),
                ("x-bisa-token", &secret),
            ],
            body.clone(),
        )
        .await;
    assert_eq!(by_token.status, 202, "{}", by_token.raw);
    let signal = by_token.body["signal"].as_str().expect("a signal");
    // The caller learns a signal id and nothing about what it caused.
    assert_eq!(by_token.body, json!({ "signal": signal }));

    let by_hmac = node
        .outside(
            &path,
            &[
                ("content-type", "application/json"),
                ("x-hub-signature-256", &sign(&secret, &body)),
            ],
            body.clone(),
        )
        .await;
    assert_eq!(by_hmac.status, 202, "{}", by_hmac.raw);

    // A right signature survives a wrong token sent alongside it.
    let both = node
        .outside(
            &path,
            &[
                ("x-bisa-token", "nope"),
                ("x-hub-signature-256", &sign(&secret, &body)),
            ],
            body.clone(),
        )
        .await;
    assert_eq!(both.status, 202, "{}", both.raw);

    let signals = node.signals_of(&host).await;
    assert_eq!(signals.len(), 3);
    assert!(signals.iter().all(|s| s["source"] == json!("hook")));
    assert!(signals.iter().all(|s| s["state"] == json!("queued")));

    // What the body carried is the event of the run it begins.
    node.drain().await;
    let runs = runs_of(&node, &wf, 1).await;
    let oldest = runs.last().expect("a run");
    let run = node
        .ok(
            "GET",
            &format!("/runs/{}", oldest["id"].as_str().unwrap()),
            None,
        )
        .await;
    assert_eq!(run["run"]["event"]["source"], json!("hook"));
    assert_eq!(
        run["run"]["event"]["payload"]["pull_request"]["merged"],
        json!(true)
    );
    assert_eq!(run["run"]["inputs"]["subject"], json!("merged"));

    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn every_authentication_failure_is_one_401_and_none_enqueues_anything() {
    let node = Node::start().await;
    node.allow_public_hooks().await;
    let (wf, secret) = node.public_hook("From outside").await;
    let host = format!("workspace:{wf}");
    let path = public_path(&wf);
    let body = json!({"subject": "opened"}).to_string();
    let wrong = "f".repeat(64);

    // A signature forged over another body, and one with no `sha256=`.
    let forged = sign(&secret, "{\"subject\":\"something else\"}");
    let naked = sign(&secret, &body)
        .trim_start_matches("sha256=")
        .to_string();
    let guessed = sign(&wrong, &body);
    let cases: Vec<(&str, Vec<(&str, &str)>)> = vec![
        ("no credentials at all", vec![]),
        ("a wrong token", vec![("x-bisa-token", wrong.as_str())]),
        (
            "a token of a different length",
            vec![("x-bisa-token", "short")],
        ),
        (
            "a signature that is no signature",
            vec![("x-hub-signature-256", "sha256=deadbeef")],
        ),
        (
            "a right signature over a different body",
            vec![("x-hub-signature-256", forged.as_str())],
        ),
        (
            "a signature made with another secret",
            vec![("x-hub-signature-256", guessed.as_str())],
        ),
        (
            "a bare signature with no sha256= prefix",
            vec![("x-hub-signature-256", naked.as_str())],
        ),
        (
            "the control-plane token, which is no hook's secret",
            vec![("x-bisa-token", TOKEN)],
        ),
    ];
    for (what, headers) in cases {
        let r = node.outside(&path, &headers, body.clone()).await;
        assert_eq!(r.status, 401, "{what} should be 401, got {}", r.raw);
        assert_eq!(r.body, json!({"error": "unauthorized"}), "{what}");
    }

    // A listener that does not exist is byte-for-byte the same answer as a
    // bad secret: the route is not an oracle for which hosts and steps are
    // real — nor for which starts are hooks, or public.
    let reference = node
        .outside(&path, &[("x-bisa-token", &wrong)], body.clone())
        .await;
    let private = node.workflow(hook_workflow("From here only", false)).await;
    node.turn_on(&private, json!({})).await;
    let goal = goal_on(&node, "listens too", &private).await;
    for unknown in [
        "/hooks/workspace:01ARZ3NDEKTSV4RRFFQ69G5FAV/ticket".to_string(),
        "/hooks/goal:01ARZ3NDEKTSV4RRFFQ69G5FAV/ticket".to_string(),
        "/hooks/workspace:not-a-ulid/ticket".to_string(),
        "/hooks/nobody/ticket".to_string(),
        format!("/hooks/workspace:{wf}/nowhere"),
        format!("/hooks/workspace:{wf}/hold"),
        format!("/hooks/workspace:{private}/ticket"),
        format!("/hooks/goal:{goal}/ticket"),
    ] {
        let r = node
            .outside(&unknown, &[("x-bisa-token", &secret)], body.clone())
            .await;
        assert_eq!(r.status, reference.status, "{unknown} differs by status");
        assert_eq!(r.body, reference.body, "{unknown} differs by body");
    }

    assert!(
        node.signals_of(&host).await.is_empty(),
        "a refused call enqueued a signal"
    );
    assert!(node
        .signals_of(&format!("workspace:{private}"))
        .await
        .is_empty());

    node.shutdown().await;
}

/// A host that is Off refuses even the holder of its secret — and says so
/// only to them.
#[tokio::test(flavor = "multi_thread")]
async fn a_host_that_is_off_refuses_the_holder_of_its_secret() {
    let node = Node::start().await;
    node.allow_public_hooks().await;
    let (wf, secret) = node.public_hook("From outside").await;
    let host = format!("workspace:{wf}");
    node.ok("DELETE", &format!("/workflows/{wf}/listening"), None)
        .await;

    let r = node
        .outside(&public_path(&wf), &[("x-bisa-token", &secret)], "{}".into())
        .await;
    // 403, not 401: this caller already holds the secret, so naming the
    // reason tells them nothing they did not know. A caller without it never
    // reaches this branch and so never learns the listener is real.
    assert_eq!(r.status, 403, "{}", r.raw);
    assert!(node.signals_of(&host).await.is_empty());

    // A wrong secret on a host that is Off is still the plain 401.
    let r = node
        .outside(
            &public_path(&wf),
            &[("x-bisa-token", &"0".repeat(64))],
            "{}".into(),
        )
        .await;
    assert_eq!(r.status, 401);
    assert_eq!(r.body, json!({"error": "unauthorized"}));

    node.turn_on(&wf, json!({})).await;
    let r = node
        .outside(
            &public_path(&wf),
            &[("x-bisa-token", &secret)],
            json!({"subject": "back on"}).to_string(),
        )
        .await;
    assert_eq!(r.status, 202, "{}", r.raw);

    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_oversized_body_is_413_whether_or_not_it_declares_its_length() {
    let node = Node::start().await;
    node.allow_public_hooks().await;
    let (wf, secret) = node.public_hook("From outside").await;
    let host = format!("workspace:{wf}");
    let path = public_path(&wf);
    let cap = bisa_core::MAX_SIGNAL_PAYLOAD_BYTES;

    // Declared: refused before a byte of body is read.
    let big = format!("{{\"subject\":\"{}\"}}", "a".repeat(cap + 1024));
    let r = node
        .outside(
            &path,
            &[
                ("content-type", "application/json"),
                ("x-bisa-token", &secret),
            ],
            big,
        )
        .await;
    assert_eq!(r.status, 413, "{}", r.raw);

    // Undeclared: chunked, no Content-Length, and far larger than anything
    // this process should ever hold. The frame loop stops early — if the cap
    // were applied after buffering, this test would allocate 8 MiB.
    let chunk = vec![b'a'; 64 * 1024];
    let chunks: Vec<Vec<u8>> = std::iter::repeat_n(chunk, 128).collect();
    let r = node
        .chunked(&path, &[("x-bisa-token", &secret)], chunks)
        .await;
    assert_eq!(r.status, 413, "{}", r.raw);

    // Within the cap as it arrived, over it once every quote is escaped.
    let quotes = "\"".repeat(cap - 1);
    let r = node
        .outside(&path, &[("x-bisa-token", &secret)], quotes)
        .await;
    assert_eq!(r.status, 413, "{}", r.raw);
    assert!(
        node.signals_of(&host).await.is_empty(),
        "a body refused is a call not taken"
    );

    // A body under the cap is taken.
    let ok = node
        .outside(
            &path,
            &[("x-bisa-token", &secret)],
            json!({"subject": "x".repeat(1024)}).to_string(),
        )
        .await;
    assert_eq!(ok.status, 202, "{}", ok.raw);
    assert_eq!(node.signals_of(&host).await.len(), 1);

    node.shutdown().await;
}

/// A body that is not a JSON object still reaches the start that reads it:
/// text under `text`, any other JSON under `value`.
#[tokio::test(flavor = "multi_thread")]
async fn a_body_that_is_not_an_object_is_carried_under_a_key_of_its_own() {
    let node = Node::start().await;
    let wf = node
        .workflow(json!({
            "name": "Whatever arrives",
            "inputs": [{"name": "said", "label": "Said", "kind": "text"}],
            "steps": [
                {"id": "ticket", "name": "Something arrives", "kind": "start",
                 "on": {"event": "hook"},
                 "inputs": {"said": "{event.payload.text}"},
                 "then": ["hold"]},
                hold(),
            ]
        }))
        .await;
    node.turn_on(&wf, json!({})).await;

    let taken = node
        .local(&local_path(&wf), &[], "not json at all".into())
        .await;
    assert_eq!(taken.status, 202, "{}", taken.raw);
    assert_eq!(node.drain().await, 1);
    let runs = runs_of(&node, &wf, 1).await;
    let run = node
        .ok(
            "GET",
            &format!("/runs/{}", runs[0]["id"].as_str().unwrap()),
            None,
        )
        .await;
    assert_eq!(
        run["run"]["event"]["payload"],
        json!({"text": "not json at all"})
    );
    assert_eq!(run["run"]["inputs"]["said"], json!("not json at all"));

    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_flood_costs_the_rate_limit_and_nothing_more() {
    let node = Node::start().await;
    node.allow_public_hooks().await;
    let (wf, secret) = node.public_hook("From outside").await;
    let host = format!("workspace:{wf}");
    let path = public_path(&wf);

    let mut accepted = 0;
    let mut backlog_full = 0;
    let mut limited = 0;
    let mut retry_after = None;
    for _ in 0..60 {
        let r = node
            .outside(
                &path,
                &[("x-bisa-token", &secret)],
                json!({"subject": "again"}).to_string(),
            )
            .await;
        match (r.status, &r.retry_after) {
            (202, _) => accepted += 1,
            // The rate limit says when to come back; a full backlog cannot.
            (429, Some(after)) => {
                limited += 1;
                retry_after = retry_after.or(Some(after.clone()));
                assert!(r.body["retry_after"].is_number(), "{}", r.raw);
            }
            (429, None) => backlog_full += 1,
            (other, _) => panic!("unexpected status {other}: {}", r.raw),
        }
    }
    assert!(
        accepted > 0 && limited > 0,
        "{accepted} in, {backlog_full} over the backlog, {limited} over the rate"
    );
    let retry: u64 = retry_after
        .expect("a rate-limited 429 carries Retry-After")
        .parse()
        .expect("Retry-After is a number of seconds");
    assert!(retry >= 1);

    // Only the calls taken became signals — the refused ones cost a token
    // bucket lookup and nothing durable.
    assert_eq!(node.signals_of(&host).await.len(), accepted);

    // A flood on invented listeners cannot mute a real one: a second hook
    // still answers on its own budget.
    for i in 0..50 {
        let _refused = node
            .outside(
                &format!("/hooks/workspace:{wf}/invented-{i}"),
                &[("x-bisa-token", "nope")],
                "{}".to_string(),
            )
            .await;
    }
    let (other, other_secret) = node.public_hook("A second door").await;
    let r = node
        .outside(
            &public_path(&other),
            &[("x-bisa-token", &other_secret)],
            json!({"subject": "fine"}).to_string(),
        )
        .await;
    assert_eq!(r.status, 202, "{}", r.raw);

    node.shutdown().await;
}

// ---------------------------------------------------------------------------
// Signals
// ---------------------------------------------------------------------------

/// A named signal raised by hand is kept for the waits that replay it and
/// written for every start that hears it; the list says where each stands.
#[tokio::test(flavor = "multi_thread")]
async fn a_signal_is_raised_by_hand_and_listed() {
    let node = Node::start().await;
    let wf = node.workflow(on_signal("report.ready")).await;
    let host = format!("workspace:{wf}");

    // Nothing listening is a normal answer.
    let quiet = node
        .ok(
            "POST",
            "/signals",
            Some(json!({"name": "report.ready", "payload": {"url": "https://example.test/1"}})),
        )
        .await;
    assert!(quiet["signal"].is_string(), "{quiet}");
    assert_eq!(quiet["listeners"], json!([]), "{quiet}");

    node.turn_on(&wf, json!({})).await;
    let raised = node
        .ok(
            "POST",
            "/signals",
            Some(json!({"name": "report.ready", "payload": {"url": "https://example.test/2"}})),
        )
        .await;
    let record = raised["signal"].as_str().expect("the record").to_string();
    let heard = raised["listeners"].as_array().expect("listeners").clone();
    assert_eq!(heard.len(), 1, "{raised}");
    // Another name is nobody's.
    let other = node
        .ok("POST", "/signals", Some(json!({"name": "report.late"})))
        .await;
    assert_eq!(other["listeners"], json!([]));

    let all = node.ok("GET", "/signals", None).await;
    let all = all.as_array().expect("a list");
    assert_eq!(all.len(), 4, "three records and one for the listener");
    let kept = all
        .iter()
        .find(|s| s["id"] == json!(record))
        .expect("the record");
    assert!(kept.get("listener").is_none(), "{kept}");
    assert_eq!(kept["source"], json!("signal"));
    assert_eq!(kept["name"], json!("report.ready"));
    assert_eq!(kept["state"], json!("done"), "kept, never claimed");
    assert_eq!(kept["scope"], json!({"scope": "workspace"}));
    let for_listener = all
        .iter()
        .find(|s| s["id"] == heard[0])
        .expect("the listener's");
    assert_eq!(for_listener["listener"], json!(format!("{host}/raised")));
    assert_eq!(for_listener["name"], json!("report.ready"));
    assert_eq!(for_listener["state"], json!("queued"));

    // One host's, and no more than asked.
    let of_host = node.signals_of(&host).await;
    assert_eq!(of_host.len(), 1, "{of_host:?}");
    assert_eq!(of_host[0]["id"], heard[0]);
    let one = node.ok("GET", "/signals?limit=1", None).await;
    assert_eq!(one.as_array().unwrap().len(), 1);
    let r = node.json("GET", "/signals?host=everything", None).await;
    assert_eq!(r.status, 400, "{}", r.raw);
    assert!(r.raw.contains("everything"), "{}", r.raw);

    // The run it begins says which signal began it.
    assert_eq!(node.drain().await, 1);
    let runs = runs_of(&node, &wf, 1).await;
    assert_eq!(
        runs[0]["started_by"],
        json!({"by": "event", "event": "signal", "detail": "report.ready"})
    );

    // On a goal when one is named; refused for one nobody has.
    let goal = goal_on(&node, "raise on me", &wf).await;
    let scoped = node
        .ok(
            "POST",
            "/signals",
            Some(json!({"name": "report.ready", "goal": goal})),
        )
        .await;
    let scoped_record = scoped["signal"].clone();
    let all = node.ok("GET", "/signals", None).await;
    let kept = all
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == scoped_record)
        .expect("the record");
    assert_eq!(kept["scope"], json!({"scope": "goal", "goal": goal}));
    let r = node
        .json(
            "POST",
            "/signals",
            Some(json!({"name": "report.ready", "goal": "01ARZ3NDEKTSV4RRFFQ69G5FAV"})),
        )
        .await;
    assert_eq!(r.status, 404, "{}", r.raw);

    // A name that is no name, a body that is no body, a payload over the cap.
    for bad in ["", "  ", "Report.Ready", "a..b", "sp ace"] {
        let r = node
            .json("POST", "/signals", Some(json!({"name": bad})))
            .await;
        assert_eq!(r.status, 400, "{bad:?}: {}", r.raw);
    }
    let r = node
        .json("POST", "/signals", Some(json!({"payload": {}})))
        .await;
    assert_eq!(r.status, 400, "no name: {}", r.raw);
    let r = node
        .json(
            "POST",
            "/signals",
            Some(json!({"name": "report.ready", "scope": "workspace"})),
        )
        .await;
    assert_eq!(r.status, 400, "a key nobody knows: {}", r.raw);
    let r = node
        .json(
            "POST",
            "/signals",
            Some(json!({
                "name": "report.ready",
                "payload": {"blob": "x".repeat(bisa_core::MAX_SIGNAL_PAYLOAD_BYTES)},
            })),
        )
        .await;
    assert_eq!(r.status, 413, "{}", r.raw);

    node.shutdown().await;
}

/// What comes from outside is read before it can start anything: with the
/// content screen on and nobody to give a verdict — the classifier is off
/// here, so the screen answers at once — a public call is held, saying why,
/// until a person lets it through.
#[tokio::test(flavor = "multi_thread")]
async fn a_held_signal_is_let_through_by_a_person() {
    let node = Node::start().await;
    node.set_machine("events.public_hooks", json!(true)).await;
    node.set_machine("security.classifier.enabled", json!(false))
        .await;
    let (wf, secret) = node.public_hook("From outside").await;

    let taken = node
        .outside(
            &public_path(&wf),
            &[("x-bisa-token", &secret)],
            json!({"subject": "read me first"}).to_string(),
        )
        .await;
    assert_eq!(taken.status, 202, "{}", taken.raw);
    let signal = taken.body["signal"].as_str().unwrap().to_string();

    // Held, and once the screen has had its say, saying why.
    let held = node
        .until("the screen's word on the signal", "/signals", |v| {
            v.as_array()?.iter().find_map(|s| {
                let said = s["note"].as_str()?;
                (s["id"] == json!(signal) && said.starts_with("held by the content screen"))
                    .then(|| s.clone())
            })
        })
        .await;
    assert_eq!(held["state"], json!("held"), "{held}");
    assert_eq!(node.drain().await, 0, "held starts nothing");
    assert_eq!(
        node.ok("GET", &format!("/workflows/{wf}/runs"), None).await["runs"],
        json!([])
    );

    // The person read it and lets it through.
    let released = node
        .ok("POST", &format!("/signals/{signal}/release"), None)
        .await;
    assert_eq!(released["signal"]["id"], json!(signal));
    assert_eq!(released["signal"]["state"], json!("queued"), "{released}");
    assert_eq!(node.drain().await, 1);
    runs_of(&node, &wf, 1).await;

    // Only a held signal is let through.
    let again = node
        .json("POST", &format!("/signals/{signal}/release"), None)
        .await;
    assert_eq!(again.status, 409, "{}", again.raw);
    let nobody = node
        .json("POST", "/signals/no-such-signal/release", None)
        .await;
    assert_eq!(nobody.status, 404, "{}", nobody.raw);

    // A call from this machine is a person's own: never held.
    let local = node
        .local(
            &local_path(&wf),
            &[],
            json!({"subject": "mine"}).to_string(),
        )
        .await;
    assert_eq!(local.status, 202, "{}", local.raw);
    let mine = local.body["signal"].clone();
    let listed = node.ok("GET", "/signals", None).await;
    let mine = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == mine)
        .expect("the local call's signal");
    assert_eq!(mine["state"], json!("queued"), "{mine}");

    node.shutdown().await;
}

// ---------------------------------------------------------------------------
// A goal that listens
// ---------------------------------------------------------------------------

/// A goal whose workflow begins on events listens when it is started: each
/// occurrence is a run on the goal, a run by hand is *Run now*, and the goal
/// stops and listens again through its own routes.
#[tokio::test(flavor = "multi_thread")]
async fn a_goal_listens_runs_on_each_occurrence_and_listens_again() {
    let node = Node::start().await;
    let wf = node.workflow(two_ways_in("Two ways in")).await;
    let goal = goal_on(&node, "answer every ticket", &wf).await;
    let host = format!("goal:{goal}");
    let hook = format!("/goals/{goal}/hooks/ticket");

    // Not started: it listens to nothing.
    let page = node.ok("GET", &format!("/goals/{goal}"), None).await;
    assert_eq!(page["listening"], Value::Null, "{page}");
    assert_eq!(
        node.ok("GET", &format!("/goals/{goal}/listeners"), None)
            .await,
        json!([])
    );
    let r = node.local(&hook, &[], "{}".into()).await;
    assert_eq!(r.status, 409, "{}", r.raw);

    // A person's start arms its events rather than running: the answer is
    // where the goal stands, and no run.
    let begun = node
        .ok("POST", &format!("/goals/{goal}/run"), Some(json!({})))
        .await;
    assert_eq!(
        begun,
        json!({"status": "waiting"}),
        "armed: no run, and no secret where no hook is public"
    );
    let listeners = node
        .ok("GET", &format!("/goals/{goal}/listeners"), None)
        .await;
    let listeners = listeners.as_array().expect("listeners");
    assert_eq!(listeners.len(), 1, "the start by hand is no listener");
    assert_eq!(listeners[0]["listener"], json!(format!("{host}/ticket")));
    assert_eq!(listeners[0]["host"], json!(host));
    assert_eq!(listeners[0]["event"], json!("hook"));
    assert_eq!(listeners[0]["local_hook"], json!(hook));

    let page = node.ok("GET", &format!("/goals/{goal}"), None).await;
    assert!(page["listening"]["since"].is_u64(), "{page}");
    assert_eq!(page["goal"]["listening"], page["listening"]);
    assert_eq!(page["status"], json!("waiting"), "on its next event");
    assert_eq!(page["holder"], json!("world"));
    assert_eq!(page["run"], Value::Null);
    let rows = node.ok("GET", "/goals", None).await;
    let row = rows["goals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["id"] == json!(goal))
        .expect("the goal's row");
    assert_eq!(row["listening"], page["listening"]);
    assert_eq!(row["status"], json!("waiting"));
    assert_eq!(
        node.ok("GET", "/listeners", None).await[0]["host"],
        json!(host)
    );

    // An occurrence is a run on the goal, by its event.
    let taken = node.local(&hook, &[], "{}".into()).await;
    assert_eq!(taken.status, 202, "{}", taken.raw);
    let signal = taken.body["signal"].clone();
    let queued = node.signals_of(&host).await;
    assert_eq!(queued[0]["id"], signal);
    assert_eq!(
        queued[0]["scope"],
        json!({"scope": "goal", "goal": goal}),
        "a goal's occurrence is its goal's"
    );
    assert_eq!(node.drain().await, 1);
    let runs = node
        .until("the goal's run", &format!("/goals/{goal}/runs"), |v| {
            v["runs"].as_array().filter(|r| !r.is_empty()).cloned()
        })
        .await;
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["scope"], json!("goal"));
    assert_eq!(
        runs[0]["started_by"],
        json!({"by": "event", "event": "hook"})
    );

    // *Run now*: at the start by hand, queued behind the live run.
    let by_hand = node
        .ok(
            "POST",
            &format!("/goals/{goal}/run"),
            Some(json!({"start": "by-hand"})),
        )
        .await;
    assert_eq!(by_hand["status"], json!("queued"), "{by_hand}");
    assert_eq!(by_hand["run"]["event"], Value::Null);
    let runs = node.ok("GET", &format!("/goals/{goal}/runs"), None).await;
    assert_eq!(runs["runs"][0]["started_by"], json!({"by": "you"}));
    assert_eq!(runs["runs"][0]["position"], json!(1));

    // It stops listening; a live run goes on.
    let stopped = node
        .ok("DELETE", &format!("/goals/{goal}/listening"), None)
        .await;
    assert_eq!(stopped["goal"]["listening"], Value::Null, "{stopped}");
    assert_eq!(stopped["goal"]["goal"]["id"], json!(goal));
    assert!(stopped["goal"]["run"].is_object(), "{stopped}");
    let r = node.local(&hook, &[], "{}".into()).await;
    assert_eq!(r.status, 409, "{}", r.raw);
    assert_eq!(node.ok("GET", "/listeners", None).await, json!([]));

    // And listens again.
    let again = node
        .ok(
            "PUT",
            &format!("/goals/{goal}/listening"),
            Some(json!({"inputs": {}})),
        )
        .await;
    assert!(again["goal"]["listening"]["since"].is_u64(), "{again}");
    assert_eq!(again["listeners"].as_array().unwrap().len(), 1);
    assert_eq!(again["secrets"], json!([]));
    let r = node.local(&hook, &[], "{}".into()).await;
    assert_eq!(r.status, 202, "{}", r.raw);
    let r = node
        .json(
            "PUT",
            &format!("/goals/{goal}/listening"),
            Some(json!({"budget": {"max_tokens": 1}})),
        )
        .await;
    assert_eq!(
        r.status, 400,
        "a goal's runs spend against the goal's budget: {}",
        r.raw
    );

    // A goal nobody has.
    let ghost = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    for (method, path) in [
        ("PUT", format!("/goals/{ghost}/listening")),
        ("DELETE", format!("/goals/{ghost}/listening")),
        ("GET", format!("/goals/{ghost}/listeners")),
        ("POST", format!("/goals/{ghost}/hooks/ticket")),
        ("POST", format!("/goals/{ghost}/hooks/ticket/secret")),
    ] {
        let r = node.json(method, &path, Some(json!({}))).await;
        assert_eq!(r.status, 404, "{method} {path}: {}", r.raw);
    }

    node.shutdown().await;
}

/// A goal whose workflow begins on a public hook is shown the hook's secret
/// when it starts listening — once.
#[tokio::test(flavor = "multi_thread")]
async fn a_goals_public_hook_is_shown_its_secret_when_the_goal_starts() {
    let node = Node::start().await;
    node.allow_public_hooks().await;
    let wf = node.workflow(hook_workflow("From outside", true)).await;
    let goal = goal_on(&node, "answer the outside", &wf).await;
    let path = format!("/hooks/goal:{goal}/ticket");

    let begun = node
        .ok("POST", &format!("/goals/{goal}/run"), Some(json!({})))
        .await;
    assert_eq!(begun["status"], json!("waiting"), "{begun}");
    assert!(begun.get("run").is_none(), "arming makes no run: {begun}");
    let secret = begun["secrets"][0]["secret"]
        .as_str()
        .expect("the secret, shown once")
        .to_string();
    assert_eq!(begun["secrets"][0]["step"], json!("ticket"));
    assert_eq!(begun["secrets"][0]["path"], json!(path));
    let listeners = node
        .json("GET", &format!("/goals/{goal}/listeners"), None)
        .await;
    assert_eq!(
        listeners.body[0]["public_hook"],
        json!({"path": path, "has_secret": true})
    );
    assert!(!listeners.raw.contains(&secret), "its listener leaked it");
    let page = node.json("GET", &format!("/goals/{goal}"), None).await;
    assert!(!page.raw.contains(&secret), "the goal's page leaked it");

    let r = node
        .outside(
            &path,
            &[("x-bisa-token", &secret)],
            json!({"subject": "from outside"}).to_string(),
        )
        .await;
    assert_eq!(r.status, 202, "{}", r.raw);

    let rotated = node
        .ok("POST", &format!("/goals/{goal}/hooks/ticket/secret"), None)
        .await;
    assert_eq!(rotated["path"], json!(path));
    assert_ne!(rotated["secret"], json!(secret));
    let r = node
        .outside(&path, &[("x-bisa-token", &secret)], "{}".into())
        .await;
    assert_eq!(r.status, 401, "the rotated-away secret still worked");

    node.shutdown().await;
}

// ---------------------------------------------------------------------------
// The Inbox
// ---------------------------------------------------------------------------

/// A listener that failed has no row of its own: its trouble is a notice on
/// its host's row — the workflow's under the workflows source, the goal's
/// under the goals. The facts are recorded the way the engine records them,
/// so the row builder is what is under test.
#[tokio::test(flavor = "multi_thread")]
async fn a_listener_that_failed_is_a_notice_on_its_hosts_row() {
    let node = Node::start().await;
    let wf = node.workflow(hook_workflow("Tickets", false)).await;
    node.turn_on(&wf, json!({})).await;
    let goal = goal_on(&node, "listens too", &wf).await;
    for source in ["workflows", "goals"] {
        assert!(
            node.ok("GET", &format!("/inbox?source={source}"), None)
                .await["rows"]
                .as_array()
                .unwrap()
                .is_empty(),
            "nothing failed yet under {source}"
        );
    }

    let failed = |concept, kind, id: &str, listener: String| ActivityFact {
        at: now(),
        concept,
        kind: "listener_failed".into(),
        source: ActivitySource::new(kind, id),
        author: None,
        event: json!({
            "type": "listener_failed",
            "listener": listener,
            "signal": "01J0SIG",
            "error": "the event does not fill the run's inputs",
        }),
    };
    node.ws
        .record_activity(
            &failed(
                ActivityConcept::Workflows,
                ActivitySourceKind::Workflow,
                &wf,
                format!("workspace:{wf}/ticket"),
            ),
            true,
        )
        .expect("the workflow's fact");
    node.ws
        .record_activity(
            &failed(
                ActivityConcept::Goals,
                ActivitySourceKind::Goal,
                &goal,
                format!("goal:{goal}/ticket"),
            ),
            true,
        )
        .expect("the goal's fact");

    let v = node.ok("GET", "/inbox?source=workflows", None).await;
    let rows = v["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{v}");
    let row = &rows[0];
    assert_eq!(row["kind"], json!("workflow"));
    assert_eq!(row["key"], json!(wf));
    assert_eq!(row["title"], json!("Tickets"));
    assert_eq!(row["notices"][0]["notice"], json!("listener_failed"));
    assert_eq!(
        row["notices"][0]["event"]["listener"],
        json!(format!("workspace:{wf}/ticket"))
    );
    assert_eq!(row["unread_notices"], json!(1));
    assert_eq!(row["needs_action"], json!([]), "a failure is not owed");
    assert_eq!(row["unread_count"], json!(0), "no conversation");

    let v = node.ok("GET", "/inbox?source=goals", None).await;
    let rows = v["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{v}");
    assert_eq!(rows[0]["kind"], json!("goal"));
    assert_eq!(rows[0]["key"], json!(goal));
    assert_eq!(rows[0]["notices"][0]["notice"], json!("listener_failed"));

    assert!(
        node.ok("GET", "/inbox?filter=needs_you", None).await["rows"]
            .as_array()
            .unwrap()
            .is_empty(),
        "nothing waits on you: a notice is never owed"
    );
    // The sources are five, and a word that names none is refused.
    let r = node.json("GET", "/inbox?source=listeners", None).await;
    assert_eq!(r.status, 400, "{}", r.raw);
    for source in ["goals", "projects", "workflows", "messages", "people"] {
        let r = node
            .json("GET", &format!("/inbox?source={source}"), None)
            .await;
        assert_eq!(r.status, 200, "{source}: {}", r.raw);
        assert!(r.body["rows"].is_array(), "{source}: {}", r.raw);
    }

    node.shutdown().await;
}

/// A run begun by another run's end says which run ended: that run's
/// workflow by name, and its number among the workflow's runs.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_begun_by_a_runs_end_names_the_run_that_ended() {
    let node = Node::start().await;
    let nightly = node
        .workflow(json!({
            "name": "Nightly report",
            "steps": [{"id": "say", "name": "Say", "kind": "notify", "template": "reported"}]
        }))
        .await;
    let follow = node
        .workflow(json!({
            "name": "After the report",
            "steps": [
                {"id": "ended", "name": "A report ended", "kind": "start",
                 "on": {"event": "run", "workflow": nightly, "outcome": "done"},
                 "then": ["hold"]},
                hold(),
            ]
        }))
        .await;
    node.turn_on(&follow, json!({})).await;
    node.ok(
        "POST",
        &format!("/workflows/{nightly}/runs"),
        Some(json!({})),
    )
    .await;
    let ended = node
        .until(
            "the report to end",
            &format!("/workflows/{nightly}/runs"),
            |v| {
                let run = v["runs"].as_array()?.first()?;
                (run["outcome"] == json!("done")).then(|| run["id"].as_str().unwrap().to_string())
            },
        )
        .await;
    // The ear is fed by hand here (`events_enabled: false`): the end of the
    // run, as the bus would say it.
    let run = node.ws.get_run(ended.parse().unwrap()).unwrap();
    bisa_engine::listen::ear::on_event(
        &node.inner,
        &bisa_engine::EngineEvent::of_run(
            &run,
            None,
            bisa_engine::EnginePayload::RunFinished {
                run: run.id,
                workflow: run.workflow.id,
                outcome: run.outcome.unwrap(),
            },
        ),
    );
    node.drain().await;
    let runs = runs_of(&node, &follow, 1).await;
    assert_eq!(runs[0]["started_by"]["by"], json!("event"), "{}", runs[0]);
    let detail = runs[0].to_string();
    assert!(detail.contains("Nightly report #1"), "{detail}");
    node.shutdown().await;
}

/// A goal with no workflow has no listeners to show; a public call to a
/// hook start that takes none has no secret to be verified against and is
/// refused as unauthorized, saying no more.
#[tokio::test(flavor = "multi_thread")]
async fn a_goal_with_no_workflow_lists_no_listeners_and_a_local_hook_takes_no_public_call() {
    let node = Node::start().await;
    let made = node
        .ok(
            "POST",
            "/goals",
            Some(json!({"statement": "listens to nothing"})),
        )
        .await;
    let goal = made["goal"]["id"].as_str().expect("the goal").to_string();
    let listeners = node
        .ok("GET", &format!("/goals/{goal}/listeners"), None)
        .await;
    assert_eq!(listeners, json!([]), "{listeners}");

    node.set_machine("events.public_hooks", json!(true)).await;
    let wf = node.workflow(hook_workflow("local only", false)).await;
    node.turn_on(&wf, json!({})).await;
    let r = node
        .outside(
            &public_path(&wf),
            &[],
            json!({"subject": "knock"}).to_string(),
        )
        .await;
    assert_eq!(r.status, 401, "{}", r.raw);
    node.shutdown().await;
}
