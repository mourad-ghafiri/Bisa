//! Reviewing an agent's changes over the node's socket (ide/20): a
//! conversation carries its mode, the review is read and settled by the
//! conversation's id, a hunk states the disk it was read from, and an ask is
//! answered where it waits. The "agent" is the test driving the engine's
//! tracker; every checkout is a temporary directory.

use bisa_core::{AgentId, ConversationMode};
use bisa_engine::changes::tracker::ChangeTracker;
use bisa_engine::changes::Checkout;
use bisa_engine::{Engine, EngineConfig};
use bisa_harness::{HarnessCatalog, InputRequest};
use bisa_node::{serve, NodeConfig};
use bisa_store::{FileKeyStore, Paths, Workspace};
use http_body_util::{BodyExt as _, Full};
use hyper::body::Bytes;
use hyper::Request;
use hyper_util::rt::TokioIo;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UnixStream;

const TOKEN: &str = "test-token-0123456789abcdef";

struct Node {
    _dir: tempfile::TempDir,
    socket: PathBuf,
    _stop: tokio::sync::oneshot::Sender<()>,
    /// The engine's own state, held before `serve` takes the engine.
    inner: Arc<bisa_engine::Inner>,
    root: PathBuf,
    workstream: bisa_core::WorkstreamId,
}

async fn boot() -> Node {
    let dir = tempfile::tempdir().expect("tempdir");
    let data = dir.path().to_path_buf();
    let ws = Workspace::open_with_keystore(
        &data,
        Box::new(FileKeyStore::new(Paths::new(&data).identity_dir())),
    )
    .expect("workspace");
    // The editor's delete goes to the OS trash by default; a test unlinks.
    // A machine's key: whether this computer has a Trash is not a workspace's to say.
    ws.set_setting(
        bisa_core::SettingScope::Machine,
        None,
        "editor.delete.trash",
        json!(false),
    )
    .expect("setting");
    let project = ws
        .create_project(bisa_store::NewProject::managed("web-app").unwrap())
        .unwrap();
    let workstream = ws.primary_workstream(project.id).unwrap();
    let root = ws.checkout_in(&project, &workstream);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("file.txt"), "one\ntwo\nthree\n").unwrap();
    let engine = Engine::start(
        ws,
        HarnessCatalog::new(),
        EngineConfig {
            design_enabled: false,
            ..Default::default()
        },
    )
    .expect("engine");
    let socket = Paths::new(&data).node_socket();
    let (stop, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let cfg = NodeConfig {
        socket: socket.clone(),
        http: None,
        data_dir: data,
        collab: None,
        fetch_attachment: None,
        token: Some(TOKEN.to_string()),
        #[cfg(feature = "a2a")]
        a2a: None,
    };
    let inner = Arc::clone(engine.inner());
    tokio::spawn(serve(engine, cfg, async {
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
    Node {
        _dir: dir,
        socket: actual,
        _stop: stop,
        inner,
        root,
        workstream: workstream.id,
    }
}

async fn request(node: &Node, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
    let stream = UnixStream::connect(&node.socket).await.expect("connect");
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .expect("handshake");
    tokio::spawn(conn);
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header(hyper::header::HOST, "localhost")
        .header(hyper::header::CONTENT_TYPE, "application/json")
        .header(hyper::header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .body(Full::new(Bytes::from(
            body.map(|b| b.to_string()).unwrap_or_default(),
        )))
        .unwrap();
    let resp = sender.send_request(request).await.expect("request");
    let status = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    };
    (status, value)
}

/// Start a conversation about the checkout and answer its id.
async fn conversation(node: &Node, mode: Option<&str>) -> (String, Value) {
    let mut body = json!({"origin": {
        "kind": "workstream",
        "id": node.workstream.to_string(),
        "project": node.inner.ws.get_workstream(node.workstream).unwrap().project.to_string(),
    }});
    if let Some(mode) = mode {
        body["mode"] = json!(mode);
    }
    let (code, made) = request(node, "POST", "/conversations", Some(body)).await;
    assert_eq!(code, 201, "{made}");
    let id = made["conversation"]["id"].as_str().unwrap().to_string();
    (id, made["conversation"].clone())
}

/// One turn in which the agent rewrites line two.
fn agent_rewrites_line_two(node: &Node, conversation: &str) {
    let inner = &node.inner;
    let tracker = ChangeTracker::new(
        inner,
        Checkout {
            conversation: conversation.parse().unwrap(),
            workstream: node.workstream,
            root: node.root.clone(),
        },
        AgentId::general(),
        node.root.clone(),
        true,
    );
    tracker
        .begin_turn(inner, None, ConversationMode::Manual)
        .unwrap();
    tracker.allowed(&InputRequest::permission(
        "req-1",
        "Edit",
        bisa_core::ToolTier::Write,
        "file.txt",
        json!({"file_path": "file.txt"}),
    ));
    std::fs::write(node.root.join("file.txt"), "one\nTWO\nthree\n").unwrap();
    tracker.tool_ended(inner, "Edit").unwrap();
    tracker.end_turn(inner, None).unwrap();
}

/// *Build this plan* is one act of the node's: the conversation leaves
/// `plan` for the mode it came from — the record remembers it, so no screen
/// has to — and for the default when it began in a plan. Out of a plan there
/// is nothing to build.
#[tokio::test(flavor = "multi_thread")]
async fn a_plan_is_built_back_into_the_mode_it_came_from() {
    let node = boot().await;
    let (id, made) = conversation(&node, Some("auto")).await;
    assert_eq!(made["mode"], "auto");
    let path = format!("/conversations/{id}");
    let build = format!("/conversations/{id}/plan/build");

    let (code, refused) = request(&node, "POST", &build, None).await;
    assert_eq!(code, 409, "not in a plan, nothing to build: {refused}");

    let (code, _) = request(&node, "PATCH", &path, Some(json!({"mode": "plan"}))).await;
    assert_eq!(code, 200);
    let (code, built) = request(&node, "POST", &build, None).await;
    assert_eq!(code, 200, "{built}");
    assert_eq!(
        built["conversation"]["mode"], "auto",
        "back where it came from: {built}"
    );
    let (_, read) = request(&node, "GET", &path, None).await;
    assert_eq!(read["conversation"]["mode"], "auto");
    let (code, _) = request(&node, "POST", &build, None).await;
    assert_eq!(code, 409, "built once");

    // Begun in a plan, it builds into the default.
    let (planned, made) = conversation(&node, Some("plan")).await;
    assert_eq!(made["mode"], "plan");
    let (code, built) = request(
        &node,
        "POST",
        &format!("/conversations/{planned}/plan/build"),
        None,
    )
    .await;
    assert_eq!(code, 200, "{built}");
    assert_eq!(built["conversation"]["mode"], "manual");

    // A conversation nobody has, and one that has no mode at all.
    let (code, _) = request(
        &node,
        "POST",
        "/conversations/01ARZ3NDEKTSV4RRFFQ69G5FAV/plan/build",
        None,
    )
    .await;
    assert_eq!(code, 404);
    let (_, elsewhere) = request(
        &node,
        "POST",
        "/conversations",
        Some(json!({"origin": {"kind": "workspace"}})),
    )
    .await;
    let other = elsewhere["conversation"]["id"].as_str().unwrap();
    let (code, _) = request(
        &node,
        "POST",
        &format!("/conversations/{other}/plan/build"),
        None,
    )
    .await;
    assert_eq!(code, 400, "only a checkout's conversation has a mode");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_conversation_starts_in_manual_and_its_mode_is_changed_in_place() {
    let node = boot().await;
    let (id, made) = conversation(&node, None).await;
    assert_eq!(made["mode"], "manual", "the registry's default");
    let (_, asked) = conversation(&node, Some("auto")).await;
    assert_eq!(asked["mode"], "auto");

    let path = format!("/conversations/{id}");
    let (code, planned) = request(&node, "PATCH", &path, Some(json!({"mode": "plan"}))).await;
    assert_eq!(code, 200, "{planned}");
    assert_eq!(planned["conversation"]["mode"], "plan");
    let (code, refused) = request(&node, "PATCH", &path, Some(json!({"mode": "guided"}))).await;
    assert!((400..500).contains(&code), "{code} {refused}");

    // A conversation about anything but a checkout has no mode to set and
    // no changes to read.
    let (_, elsewhere) = request(
        &node,
        "POST",
        "/conversations",
        Some(json!({"origin": {"kind": "workspace"}})),
    )
    .await;
    let other = elsewhere["conversation"]["id"].as_str().unwrap();
    let (code, _) = request(
        &node,
        "PATCH",
        &format!("/conversations/{other}"),
        Some(json!({"mode": "auto"})),
    )
    .await;
    assert_eq!(code, 400);
    let (code, _) = request(
        &node,
        "GET",
        &format!("/conversations/{other}/changes"),
        None,
    )
    .await;
    assert_eq!(code, 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_change_is_read_by_its_conversation_and_a_hunk_is_undone_against_the_disk_it_was_read_from(
) {
    let node = boot().await;
    let (id, _) = conversation(&node, None).await;
    let base = format!("/conversations/{id}/changes");
    let (code, empty) = request(&node, "GET", &base, None).await;
    assert_eq!(code, 200, "{empty}");
    assert_eq!(
        (empty["pending"].as_u64(), empty["owed"].as_bool()),
        (Some(0), Some(false))
    );

    agent_rewrites_line_two(&node, &id);
    let (_, seen) = request(&node, "GET", &base, None).await;
    assert_eq!(seen["pending"], 1);
    assert_eq!(seen["owed"], true);
    assert_eq!(seen["turns"][0]["files"][0]["path"], "file.txt");
    assert_eq!(seen["turns"][0]["files"][0]["state"], "pending");

    let (code, file) = request(&node, "GET", &format!("{base}/file?path=file.txt"), None).await;
    assert_eq!(code, 200, "{file}");
    let hunk = file["file"]["hunks"][0].clone();
    assert_eq!(hunk["added"], "TWO\n");
    assert_eq!(file["file"]["base_text"], "one\ntwo\nthree\n");

    // A hunk read from a disk that is not this one is refused, 409.
    let stale = json!({"verdict": "undo", "target": {
        "grain": "hunk", "path": "file.txt", "hunk": hunk["id"], "disk_hash": "0".repeat(64),
    }});
    let (code, _) = request(&node, "POST", &format!("{base}/settle"), Some(stale)).await;
    assert_eq!(code, 409);

    let undo = json!({"verdict": "undo", "target": {
        "grain": "hunk", "path": "file.txt", "hunk": hunk["id"],
        "disk_hash": file["file"]["disk_hash"],
    }});
    let (code, settled) = request(&node, "POST", &format!("{base}/settle"), Some(undo)).await;
    assert_eq!(code, 200, "{settled}");
    assert_eq!(
        (settled["files"].as_u64(), settled["pending"].as_u64()),
        (Some(1), Some(0))
    );
    assert_eq!(
        std::fs::read_to_string(node.root.join("file.txt")).unwrap(),
        "one\ntwo\nthree\n"
    );
    let (_, gone) = request(&node, "GET", &format!("{base}/file?path=file.txt"), None).await;
    assert_eq!(gone["file"], Value::Null);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_kept_turn_is_still_gone_back_to_before() {
    let node = boot().await;
    let (id, _) = conversation(&node, None).await;
    agent_rewrites_line_two(&node, &id);
    let base = format!("/conversations/{id}/changes");
    let keep = json!({"verdict": "keep", "target": {"grain": "all"}});
    let (code, kept) = request(&node, "POST", &format!("{base}/settle"), Some(keep)).await;
    assert_eq!(code, 200, "{kept}");
    let (_, seen) = request(&node, "GET", &base, None).await;
    let turn = seen["turns"][0]["turn"].clone();
    assert_eq!(seen["turns"][0]["files"][0]["state"], "kept");

    let (code, restored) = request(
        &node,
        "POST",
        &format!("{base}/restore"),
        Some(json!({"turn": turn})),
    )
    .await;
    assert_eq!(code, 200, "{restored}");
    assert_eq!(restored["files"], 1);
    assert_eq!(
        std::fs::read_to_string(node.root.join("file.txt")).unwrap(),
        "one\ntwo\nthree\n"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn nothing_waits_until_a_turn_asks_and_an_ask_that_is_not_waiting_is_a_sentence() {
    let node = boot().await;
    let (id, _) = conversation(&node, None).await;
    let (code, asks) = request(&node, "GET", &format!("/conversations/{id}/asks"), None).await;
    assert_eq!(code, 200, "{asks}");
    assert_eq!(asks["asks"], json!([]));
    let (code, refused) = request(
        &node,
        "POST",
        &format!("/conversations/{id}/asks/01JNOTWAITING"),
        Some(json!({"answer": "allow", "scope": "conversation"})),
    )
    .await;
    assert_eq!(code, 400, "{refused}");
    assert!(refused["error"]
        .as_str()
        .unwrap_or_default()
        .contains("no longer waiting"));
}

fn refused(code: u16) -> bool {
    (400..500).contains(&code)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_reviews_door_refuses_in_words_and_changes_nothing() {
    let node = boot().await;
    let (id, _) = conversation(&node, None).await;
    agent_rewrites_line_two(&node, &id);
    let base = format!("/conversations/{id}/changes");
    let nobody = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

    // A name that is no id, and an id that names nothing.
    let (code, said) = request(&node, "GET", "/conversations/not-an-id/changes", None).await;
    assert_eq!(code, 400, "{said}");
    let (code, _) = request(
        &node,
        "GET",
        &format!("/conversations/{nobody}/changes"),
        None,
    )
    .await;
    assert_eq!(code, 404);
    let (code, _) = request(&node, "GET", &format!("/conversations/{nobody}/asks"), None).await;
    assert_eq!(code, 404);

    // A path that leaves the checkout is not a path.
    for path in ["../outside.txt", "/etc/hosts", ""] {
        let (code, said) = request(&node, "GET", &format!("{base}/file?path={path}"), None).await;
        assert_eq!(code, 400, "{path:?}: {said}");
    }
    // A file nobody changed has nothing under review — an answer, not an error.
    let (code, none) = request(
        &node,
        "GET",
        &format!("{base}/file?path=untouched.txt"),
        None,
    )
    .await;
    assert_eq!((code, &none["file"]), (200, &Value::Null));

    // The words are keep and undo; nothing else is one.
    for verdict in ["accept", "reject", "KEEP", ""] {
        let body = json!({"verdict": verdict, "target": {"grain": "all"}});
        let (code, said) = request(&node, "POST", &format!("{base}/settle"), Some(body)).await;
        assert!(refused(code), "{verdict:?} answered {code}: {said}");
    }
    for target in [
        json!({"grain": "everything"}),
        json!({"grain": "turn", "turn": "not-a-turn"}),
        json!({"grain": "file", "path": "../outside.txt"}),
        json!({"grain": "hunk", "path": "file.txt"}),
    ] {
        let body = json!({"verdict": "undo", "target": target});
        let (code, said) =
            request(&node, "POST", &format!("{base}/settle"), Some(body.clone())).await;
        assert!(refused(code), "{body} answered {code}: {said}");
    }
    let ghost = json!({"verdict": "undo", "target": {"grain": "turn", "turn": nobody}});
    let (code, said) = request(&node, "POST", &format!("{base}/settle"), Some(ghost)).await;
    assert_eq!(code, 400, "{said}");
    assert!(said.to_string().contains("no such turn"), "{said}");
    let (code, said) = request(
        &node,
        "POST",
        &format!("{base}/restore"),
        Some(json!({"turn": nobody})),
    )
    .await;
    assert_eq!(code, 400, "{said}");

    // An answer that is neither allow nor deny, and a scope that is no scope.
    for answer in [
        json!({"answer": "maybe"}),
        json!({"answer": "allow", "scope": "forever"}),
        json!({}),
    ] {
        let (code, said) = request(
            &node,
            "POST",
            &format!("/conversations/{id}/asks/{nobody}"),
            Some(answer.clone()),
        )
        .await;
        assert!(refused(code), "{answer} answered {code}: {said}");
    }

    // Through all of it the change still waits and the file is the agent's.
    let (_, seen) = request(&node, "GET", &base, None).await;
    assert_eq!(seen["pending"], 1);
    assert_eq!(
        std::fs::read_to_string(node.root.join("file.txt")).unwrap(),
        "one\nTWO\nthree\n"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_deleted_conversation_takes_its_review_with_it_and_leaves_the_files() {
    let node = boot().await;
    let (id, _) = conversation(&node, None).await;
    agent_rewrites_line_two(&node, &id);
    let (code, _) = request(&node, "DELETE", &format!("/conversations/{id}"), None).await;
    assert!((200..300).contains(&code), "{code}");

    for (method, path, body) in [
        ("GET", format!("/conversations/{id}/changes"), None),
        ("GET", format!("/conversations/{id}/asks"), None),
        (
            "POST",
            format!("/conversations/{id}/changes/settle"),
            Some(json!({"verdict": "undo", "target": {"grain": "all"}})),
        ),
    ] {
        let (code, said) = request(&node, method, &path, body).await;
        assert_eq!(code, 404, "{method} {path}: {said}");
    }
    assert_eq!(
        std::fs::read_to_string(node.root.join("file.txt")).unwrap(),
        "one\nTWO\nthree\n",
        "what the agent wrote stays: only its record went"
    );
}
