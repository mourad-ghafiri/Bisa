//! `GET /tree`, `GET /file` and `GET /placement` over the node's real
//! listener.
//!
//! Two things are worth testing here and they pull in opposite directions.
//!
//! The first is that a listing **says what things are**: a goal's folder
//! read back should show a journal, a project, a workstream and a work
//! directory, not eight rows all called `file`. That is the feature.
//!
//! The second is that these are the only routes on the control plane that
//! take a filesystem path from a caller, so every way out of the root is
//! exercised — `..`, an absolute path, and a symlink pointing out, which is
//! the one a lexical check would miss — against every route that accepts a
//! path. A refusal has to *name the boundary*, because "invalid path" leaves
//! the caller unable to tell a bug from a policy.

use bisa_core::{Goal, GoalId, Workstream, WorkstreamId, WorkstreamKind, WorkstreamState};
use bisa_engine::{Engine, EngineConfig};
use bisa_harness::HarnessCatalog;
use bisa_node::{serve, NodeConfig};
use bisa_store::{FileKeyStore, Workspace, FILE_MAX_BYTES};
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

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

struct Node {
    socket: PathBuf,
    /// Kept across `serve` so a test can put a record the API has no route
    /// for — a workstream's checkout is the executor's, not a caller's.
    ws: Arc<Workspace>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    server: Option<tokio::task::JoinHandle<anyhow::Result<()>>>,
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
        let engine = Engine::start(
            ws,
            HarnessCatalog::new(),
            EngineConfig {
                design_enabled: false,
                ..Default::default()
            },
        )
        .expect("engine");
        let ws = Arc::clone(&engine.inner().ws);

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
            stop: Some(stop),
            server: Some(server),
            _dir: dir,
        }
    }

    async fn req(&self, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
        request(&self.socket, method, path, body).await
    }

    async fn get(&self, path: &str) -> Value {
        let (code, v) = self.req("GET", path, None).await;
        assert_eq!(code, 200, "GET {path} failed: {v}");
        v
    }

    async fn post(&self, path: &str, body: Value) -> Value {
        let (code, v) = self.req("POST", path, Some(body)).await;
        assert_eq!(code, 200, "POST {path} failed: {v}");
        v
    }

    async fn new_goal(&self, statement: &str) -> GoalId {
        let v = self
            .post("/goals", json!({"statement": statement, "title": "T"}))
            .await;
        let goal: Goal = serde_json::from_value(v["goal"].clone()).expect("goal");
        goal.id
    }

    /// A work item on the goal: the one an `agent` step becomes when the
    /// goal runs a one-step workflow on a harness nobody has. The item exists
    /// the moment the step starts; that it then fails is beside the point of
    /// a placement read.
    async fn agent_item(&self, id: GoalId) -> String {
        let v = self
            .post(
                "/workflows",
                json!({"name": "Build", "steps": [{
                    "id": "build", "name": "Build", "kind": "agent",
                    "instructions": "fix the cart total", "harness": ["no-such-harness"]
                }]}),
            )
            .await;
        let wf = v["workflow"]["id"]
            .as_str()
            .expect("workflow id")
            .to_string();
        self.req(
            "PUT",
            &format!("/goals/{id}/workflow"),
            Some(json!({"workflow": wf})),
        )
        .await;
        self.post(&format!("/goals/{id}/run"), json!({})).await;
        for _ in 0..50 {
            let v = self.get(&format!("/goals/{id}")).await;
            if let Some(item) = v["work_items"][0]["id"].as_str() {
                return item.to_string();
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("the agent step never became a work item");
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

async fn request(
    socket: &std::path::Path,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (u16, Value) {
    let stream = UnixStream::connect(socket).await.expect("connect");
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .expect("handshake");
    tokio::spawn(conn);
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header(hyper::header::HOST, "localhost")
        .header(hyper::header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(hyper::header::CONTENT_TYPE, "application/json")
        .body(Full::new(Bytes::from(
            body.map(|b| b.to_string()).unwrap_or_default(),
        )))
        .unwrap();
    let resp = sender.send_request(request).await.expect("request");
    let status = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

/// The `kind` a listing gave one path, or `None` when it was not listed.
fn kind_of<'a>(tree: &'a Value, path: &str) -> Option<&'a str> {
    tree["entries"]
        .as_array()?
        .iter()
        .find(|e| e["path"] == json!(path))?["kind"]
        .as_str()
}

// ---------------------------------------------------------------------------
// The listing, and what it says things are
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn an_goals_folder_reads_as_its_life() {
    let node = Node::start().await;
    let id = node.new_goal("ship the demo").await;

    // A project through the API, so the layout is the one the platform makes
    // rather than one the test invented.
    node.post(
        &format!("/goals/{id}/projects"),
        json!({"kind": "new", "slug": "web"}),
    )
    .await;

    // The rest by hand: a workstream's checkout is the executor's job, and a
    // result is written when a work item settles.
    let paths = node.ws.paths().goal(id);
    std::fs::create_dir_all(paths.scratch()).unwrap();
    std::fs::write(paths.scratch().join("notes.md"), "# notes\n").unwrap();
    std::fs::create_dir_all(paths.results()).unwrap();
    std::fs::write(paths.results().join("01K2.patch"), "diff\n").unwrap();
    std::fs::create_dir_all(paths.documents()).unwrap();
    std::fs::write(paths.documents().join("brief.md"), "# brief\n").unwrap();
    // A file the layout does not name. Written by hand because a fresh goal
    // has spent nothing yet, and the point is that it lists as what it is.
    std::fs::write(paths.ledger(), "").unwrap();

    let tree = node.get(&format!("/tree/goal/{id}")).await;
    assert_eq!(tree["scope"], json!("goal"));
    assert_eq!(tree["path"], json!(""));

    assert_eq!(kind_of(&tree, "journal.jsonl"), Some("journal"));
    assert_eq!(kind_of(&tree, "state"), Some("state"));
    assert_eq!(kind_of(&tree, "scratch"), Some("work"));
    assert_eq!(kind_of(&tree, "results"), Some("result"));
    assert_eq!(kind_of(&tree, "documents"), Some("document"));
    // A project is not under its goal: it lives at `projects/<slug>/` of the
    // workspace and reaches the tree as its primary workstream.
    assert_eq!(kind_of(&tree, "projects"), None);
    // Everything the layout does not name is honestly a file.
    assert_eq!(kind_of(&tree, "ledger.jsonl"), Some("file"));

    // One level by default: the children of `documents/` are not here yet.
    assert_eq!(kind_of(&tree, "documents/brief.md"), None);
    assert_eq!(tree["depth"], json!(1));
    assert_eq!(tree["deeper"], json!(true), "there is more below");
    assert_eq!(
        tree["truncated"],
        json!(false),
        "the depth bound is not a cut"
    );

    // Two levels: a child of an index *is* one of the things it indexes.
    let tree = node.get(&format!("/tree/goal/{id}?depth=2")).await;
    assert_eq!(
        kind_of(&tree, "documents/brief.md"),
        Some("document"),
        "a child of an index is one of the things it indexes"
    );
    assert_eq!(kind_of(&tree, "results/01K2.patch"), Some("result"));
    // A working directory is not an index: what an agent left in it is
    // somebody's file, and calling it `work` would be a lie.
    assert_eq!(kind_of(&tree, "scratch/notes.md"), Some("file"));

    // A sub-path lists from the root's frame of reference, so the paths that
    // come back can be handed straight back in.
    let tree = node.get(&format!("/tree/goal/{id}?path=scratch")).await;
    assert_eq!(tree["path"], json!("scratch"));
    assert_eq!(kind_of(&tree, "scratch/notes.md"), Some("file"));

    node.shutdown().await;
}

/// A project's root is somebody's source tree, so nothing in it is annotated.
/// The alternative — inheriting the goal's layout names — would report a
/// repository that happens to contain a `state/` directory as holding
/// snapshots of a goal it has never heard of.
#[tokio::test(flavor = "multi_thread")]
async fn a_projects_own_files_are_not_annotated() {
    let node = Node::start().await;
    let id = node.new_goal("ship the demo").await;
    let v = node
        .post(
            &format!("/goals/{id}/projects"),
            json!({"kind": "new", "slug": "web"}),
        )
        .await;
    let pid = v["project"]["id"].as_str().expect("project id").to_string();

    // Where the project lives, asked of the route whose job that is.
    let root = PathBuf::from(
        node.get(&format!("/placement/workstream/{pid}")).await["path"]
            .as_str()
            .expect("placement path"),
    );
    std::fs::create_dir_all(root.join("state")).unwrap();
    std::fs::write(root.join("README.md"), "hi\n").unwrap();

    let tree = node.get(&format!("/tree/workstream/{pid}?depth=2")).await;
    assert_eq!(kind_of(&tree, "state"), Some("dir"));
    assert_eq!(kind_of(&tree, "README.md"), Some("file"));

    node.shutdown().await;
}

// ---------------------------------------------------------------------------
// Containment
// ---------------------------------------------------------------------------

/// Three ways out of the root, on every route that takes a path.
///
/// The symlink is the one that matters: it never looks like an escape, and
/// only canonicalization notices. The refusal has to name the boundary — a
/// caller cannot tell a bug from a policy from the words "invalid path".
#[tokio::test(flavor = "multi_thread")]
async fn a_path_out_of_the_tree_is_refused_and_the_refusal_names_the_boundary() {
    let node = Node::start().await;
    let id = node.new_goal("ship the demo").await;
    let root = node.ws.paths().goal(id).dir().to_path_buf();
    // Two spellings of one directory. `resolve_within` refuses a `..`
    // component before it canonicalizes anything — there is nothing to
    // canonicalize against yet — so a component rejection names the base as
    // handed in, while a symlink rejection names the resolved one. On macOS
    // those differ (`/var` is itself a link to `/private/var`), and the
    // contract being tested is that the refusal names *the boundary*, not
    // which of its two names it happened to use.
    let spellings = [
        root.display().to_string(),
        root.canonicalize().unwrap().display().to_string(),
    ];

    let mut escapes = vec!["../../identity", "..", "/etc/passwd"];
    if cfg!(unix) {
        // Lexically inside the goal; actually the workspace's identity
        // folder. This is the one a string prefix test would wave through.
        #[cfg(unix)]
        std::os::unix::fs::symlink(node.ws.paths().identity_dir(), root.join("out")).unwrap();
        escapes.push("out");
        escapes.push("out/owner.key");
    }

    for bad in escapes {
        for route in ["tree", "file"] {
            let (code, v) = node
                .req("GET", &format!("/{route}/goal/{id}?path={bad}"), None)
                .await;
            assert_eq!(code, 400, "{route} {bad:?} was not refused: {v}");
            let error = v["error"].as_str().unwrap_or_default();
            assert!(
                error.contains("leaves") && spellings.iter().any(|b| error.contains(b)),
                "{route} {bad:?}: the refusal does not name the boundary: {error}"
            );
        }
    }

    // `/placement` takes no path at all — there is nothing to escape from,
    // which is the point of splitting "where is this" off from "what is in
    // it". A path handed to it is ignored, not honoured.
    let v = node
        .get(&format!("/placement/goal/{id}?path=../../identity"))
        .await;
    assert_eq!(v["path"], json!(root.display().to_string()));

    node.shutdown().await;
}

/// The same boundary, one scope over — and this is the case that explains why
/// `resolve_within` takes a base instead of being fixed to the workspace root.
/// An adopted project's root is legitimately outside the workspace, so the
/// boundary named here is the project's, not the workspace's.
#[tokio::test(flavor = "multi_thread")]
async fn a_projects_boundary_is_its_own_root() {
    let node = Node::start().await;
    let id = node.new_goal("ship the demo").await;
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret"), "s3cret\n").unwrap();

    let v = node
        .post(
            &format!("/goals/{id}/projects"),
            json!({"kind": "adopt", "slug": "legacy",
                   "path": outside.path().canonicalize().unwrap()}),
        )
        .await;
    let pid = v["project"]["id"].as_str().expect("project id").to_string();
    let root = outside.path().canonicalize().unwrap();

    // Inside the adopted folder, which is outside the workspace: allowed,
    // because the root a path is reached through is the root it is checked
    // against.
    let v = node
        .get(&format!("/file/workstream/{pid}?path=secret"))
        .await;
    assert_eq!(v["text"], json!("s3cret\n"));

    let (code, v) = node
        .req(
            "GET",
            &format!("/file/workstream/{pid}?path=../elsewhere"),
            None,
        )
        .await;
    assert_eq!(code, 400);
    assert!(
        v["error"]
            .as_str()
            .unwrap_or_default()
            .contains(&root.display().to_string()),
        "the refusal names the workspace instead of the project: {v}"
    );

    node.shutdown().await;
}

// ---------------------------------------------------------------------------
// Reading one file
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn a_binary_file_is_named_and_an_oversized_one_is_truncated() {
    let node = Node::start().await;
    let id = node.new_goal("ship the demo").await;
    let work = node.ws.paths().goal(id).scratch();
    std::fs::create_dir_all(&work).unwrap();

    // A NUL byte and a name that says "text": detection is from the bytes.
    std::fs::write(work.join("logo.txt"), [0x89, b'P', b'N', b'G', 0x00, 0x1a]).unwrap();
    let v = node
        .get(&format!("/file/goal/{id}?path=scratch/logo.txt"))
        .await;
    assert_eq!(v["binary"], json!(true));
    assert_eq!(v["text"], json!(null), "no bytes are served for a binary");
    assert_eq!(v["size"], json!(6));

    let big = vec![b'a'; FILE_MAX_BYTES as usize + 4096];
    std::fs::write(work.join("big.log"), &big).unwrap();
    let v = node
        .get(&format!("/file/goal/{id}?path=scratch/big.log"))
        .await;
    assert_eq!(v["truncated"], json!(true));
    assert_eq!(v["binary"], json!(false));
    assert_eq!(
        v["size"],
        json!(big.len()),
        "size is the file's, not the reply's"
    );
    assert_eq!(
        v["text"].as_str().unwrap().len(),
        FILE_MAX_BYTES as usize,
        "truncation is a cap, not an error"
    );

    std::fs::write(work.join("notes.md"), "# hello\n").unwrap();
    let v = node
        .get(&format!("/file/goal/{id}?path=scratch/notes.md"))
        .await;
    assert_eq!(v["text"], json!("# hello\n"));
    assert_eq!(v["truncated"], json!(false));
    assert_eq!(
        v["path"],
        json!("scratch/notes.md"),
        "the reply names what was asked"
    );

    // A directory is not a file, and the refusal says which mistake was made.
    let (code, v) = node
        .req("GET", &format!("/file/goal/{id}?path=scratch"), None)
        .await;
    assert_eq!(code, 400);
    assert!(v["error"]
        .as_str()
        .unwrap_or_default()
        .contains("directory"));

    // A bodiless `?path=` is the root, which is also a directory — answered
    // here rather than by the store, so the message names the parameter.
    let (code, v) = node.req("GET", &format!("/file/goal/{id}"), None).await;
    assert_eq!(code, 400);
    assert!(v["error"].as_str().unwrap_or_default().contains("?path="));

    node.shutdown().await;
}

// ---------------------------------------------------------------------------
// Placement
// ---------------------------------------------------------------------------

/// The two routes must agree about where a scope is rooted, or a client that
/// resolves a path against `placement` will build one the tree route refuses.
#[tokio::test(flavor = "multi_thread")]
async fn placement_agrees_with_where_the_tree_rooted_itself() {
    let node = Node::start().await;
    let id = node.new_goal("ship the demo").await;
    let v = node
        .post(
            &format!("/goals/{id}/projects"),
            json!({"kind": "new", "slug": "web"}),
        )
        .await;
    let pid = v["project"]["id"].as_str().expect("project id").to_string();

    // A workstream's checkout has no HTTP route that creates one — it is the
    // executor's, on the path a work item takes — so the record and the
    // directory go in through the store.
    let wid = WorkstreamId::from_ulid(ulid::Ulid::from_datetime(std::time::SystemTime::now()));
    // A workstream lives under its project, never under a goal.
    let web = bisa_core::Slug::new("web").unwrap();
    let checkout = node.ws.paths().project(&web).workstream_dir(wid);
    std::fs::create_dir_all(checkout.join("src")).unwrap();
    std::fs::write(checkout.join("src/main.rs"), "fn main() {}\n").unwrap();
    node.ws
        .put_workstream(&Workstream {
            id: wid,
            project: pid.parse().unwrap(),
            name: None,
            note: None,
            pinned: false,
            goal: Some(id),
            work_item: None,
            kind: WorkstreamKind::Copy,
            agent: None,
            state: WorkstreamState::Open,
            created_at: 0,
            board: Default::default(),
        })
        .expect("workstream");

    for (scope, sid) in [
        ("goal", id.to_string()),
        ("workstream", pid.clone()),
        ("workstream", wid.to_string()),
    ] {
        let placement = node.get(&format!("/placement/{scope}/{sid}")).await;
        let tree = node.get(&format!("/tree/{scope}/{sid}")).await;
        assert_eq!(
            placement["path"], tree["root"],
            "{scope} disagrees with itself about where it lives"
        );
        assert_eq!(placement["exists"], json!(true));
    }

    // The workstream's root is its checkout, not the `<id>.json` record beside
    // it. Rooting at the record would list the wrong thing and read as a
    // one-file project.
    let tree = node.get(&format!("/tree/workstream/{wid}?depth=2")).await;
    assert_eq!(kind_of(&tree, "src"), Some("dir"));
    assert_eq!(kind_of(&tree, "src/main.rs"), Some("file"));

    // A place that has nothing in it yet is an answer, not an error: a record
    // can outlive its checkout, and a copy workstream's tree is torn down as soon
    // as its patch is captured.
    //
    // A second record whose directory was never created, rather than deleting
    // the one above. The route's behaviour turns on `path.exists()`, and a
    // directory that was never made satisfies that exactly as well as one that
    // was removed — while keeping a recursive delete out of the test suite.
    let ghost = WorkstreamId::from_ulid(ulid::Ulid::from_datetime(std::time::SystemTime::now()));
    node.ws
        .put_workstream(&Workstream {
            id: ghost,
            project: pid.parse().unwrap(),
            name: None,
            note: None,
            pinned: false,
            goal: Some(id),
            work_item: None,
            kind: WorkstreamKind::Copy,
            agent: None,
            state: WorkstreamState::Open,
            created_at: 0,
            board: Default::default(),
        })
        .expect("workstream");

    let placement = node.get(&format!("/placement/workstream/{ghost}")).await;
    assert_eq!(placement["exists"], json!(false));
    let (code, v) = node
        .req("GET", &format!("/tree/workstream/{ghost}"), None)
        .await;
    assert_eq!(code, 400, "listing a directory that is not there: {v}");
    assert!(v["error"]
        .as_str()
        .unwrap_or_default()
        .contains("no directory"));

    node.shutdown().await;
}

/// A work item is the one scope that is not a directory: it names a *thing*,
/// and where that thing works is a rule.
///
/// The rule has three states and this walks all of them, because the middle one
/// is the whole reason the scope exists and the last one is the reason it
/// cannot be "the workstream" and stop there. A copy workstream's tree is deleted
/// once its patch is captured, so an item that finished still has to answer
/// with somewhere real — and the only honest answer left is the goal's own
/// `work/`, which is exactly where the engine runs a `check` step's command.
#[tokio::test(flavor = "multi_thread")]
async fn a_work_item_is_rooted_where_its_work_actually_happens() {
    let node = Node::start().await;
    let id = node.new_goal("ship the demo").await;
    let v = node
        .post(
            &format!("/goals/{id}/projects"),
            json!({"kind": "new", "slug": "web"}),
        )
        .await;
    let pid = v["project"]["id"].as_str().expect("project id").to_string();

    let item = node.agent_item(id).await;

    // 1. No workstream yet — the goal's own `work/`, whether or not anything
    //    has written into it. `exists: false` is an answer, not a failure.
    let work_dir = node.ws.paths().goal(id).scratch();
    let placement = node.get(&format!("/placement/work_item/{item}")).await;
    assert_eq!(placement["path"], json!(work_dir.display().to_string()));

    // 2. A workstream **record** for it whose checkout is not on disk — which is
    //    the state a copy workstream is left in once its patch is captured and
    //    its tree torn down. The record alone must not move the item: rooting
    //    it at a directory that is not there would point every later read at
    //    nothing.
    //
    //    Written as a directory that was never created rather than one this
    //    test deletes. The branch under test is `path.is_dir() == false`, and
    //    that is what "never created" means — a test suite has no business
    //    running a recursive delete to reach a state it can simply not build.
    let wid = WorkstreamId::from_ulid(ulid::Ulid::from_datetime(std::time::SystemTime::now()));
    let web = bisa_core::Slug::new("web").unwrap();
    let checkout = node.ws.paths().project(&web).workstream_dir(wid);
    node.ws
        .put_workstream(&Workstream {
            id: wid,
            project: pid.parse().unwrap(),
            name: None,
            note: None,
            pinned: false,
            goal: Some(id),
            work_item: Some(item.parse().unwrap()),
            kind: WorkstreamKind::Copy,
            agent: None,
            state: WorkstreamState::Open,
            created_at: 0,
            board: Default::default(),
        })
        .expect("workstream");

    let placement = node.get(&format!("/placement/work_item/{item}")).await;
    assert_eq!(
        placement["path"],
        json!(work_dir.display().to_string()),
        "a workstream record with no tree must not capture the item"
    );

    // 3. The checkout on disk — now it wins, because that is where the run
    //    happened. Same record, same id; the only thing that changed is that
    //    the directory exists.
    std::fs::create_dir_all(checkout.join("src")).unwrap();
    std::fs::write(checkout.join("src/main.rs"), "fn main() {}\n").unwrap();

    let placement = node.get(&format!("/placement/work_item/{item}")).await;
    assert_eq!(placement["path"], json!(checkout.display().to_string()));
    assert_eq!(placement["exists"], json!(true));

    // The tree route agrees, and lists the checkout rather than the record.
    let tree = node.get(&format!("/tree/work_item/{item}?depth=2")).await;
    assert_eq!(placement["path"], tree["root"]);
    assert_eq!(kind_of(&tree, "src/main.rs"), Some("file"));
    // Nothing under a work-item root is classified: it is somebody's source
    // tree, exactly as under a project or a workstream.
    assert_eq!(kind_of(&tree, "src"), Some("dir"));

    let file = node
        .get(&format!("/file/work_item/{item}?path=src/main.rs"))
        .await;
    assert_eq!(file["text"], json!("fn main() {}\n"));

    node.shutdown().await;
}

// ---------------------------------------------------------------------------
// The two ways to get the address wrong
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_scope_names_the_four_that_work() {
    let node = Node::start().await;
    let id = node.new_goal("ship the demo").await;

    for route in ["tree", "file", "placement"] {
        let (code, v) = node.req("GET", &format!("/{route}/agent/{id}"), None).await;
        assert_eq!(code, 400, "{route}: {v}");
        let error = v["error"].as_str().unwrap_or_default();
        for valid in ["goal", "workstream", "work_item"] {
            assert!(error.contains(valid), "{route}: {error} omits {valid}");
        }
    }

    node.shutdown().await;
}

/// An id nothing knows about, and the status it gets.
///
/// These are not the same across scopes, which is a property of the store's
/// error taxonomy rather than of these routes: a goal and a work item are
/// their own `NotFound` variants (404), and so does a missing workstream; a
/// malformed id never reaches the store's lookup and is a 400 by name.
#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_id_is_refused_by_the_stores_own_taxonomy() {
    let node = Node::start().await;
    let ghost = ulid::Ulid::from_datetime(std::time::SystemTime::now()).to_string();

    for route in ["tree", "file", "placement"] {
        for scope in ["goal", "work_item", "workstream"] {
            let (code, _) = node
                .req("GET", &format!("/{route}/{scope}/{ghost}?path=x"), None)
                .await;
            assert_eq!(code, 404, "{route}/{scope}: an unknown id is a 404");
        }
    }

    // A malformed id never reaches the store's lookup at all.
    let (code, v) = node.req("GET", "/placement/goal/not-a-ulid", None).await;
    assert_eq!(code, 400);
    assert!(v["error"]
        .as_str()
        .unwrap_or_default()
        .contains("is not a goal id"));
    let (code, v) = node
        .req("GET", "/placement/work_item/not-a-ulid", None)
        .await;
    assert_eq!(code, 400);
    assert!(v["error"]
        .as_str()
        .unwrap_or_default()
        .contains("is not a work item id"));

    node.shutdown().await;
}
