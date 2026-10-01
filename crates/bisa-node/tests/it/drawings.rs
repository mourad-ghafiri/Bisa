//! Drawings over HTTP (19 — Drawings): the rows and the detail, a guarded
//! patch whose 409 carries the current scene, a scope that does not pair
//! refused, the agents' parked requests listed and answered, and the
//! drawings repository — status, who commits, commit, origin, push, fetch,
//! and no pull — against a local bare origin, no network, no real agent.

use bisa_engine::{Engine, EngineConfig};
use bisa_harness::HarnessCatalog;
use bisa_node::{serve, NodeConfig};
use bisa_store::{FileKeyStore, Paths, Workspace};
use bisa_vcs::Git;
use http_body_util::{BodyExt as _, Full};
use hyper::body::Bytes;
use hyper::Request;
use hyper_util::rt::TokioIo;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;
use tokio::net::UnixStream;

const TOKEN: &str = "test-token-0123456789abcdef";

async fn boot() -> (tempfile::TempDir, PathBuf, tokio::sync::oneshot::Sender<()>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let data = dir.path().to_path_buf();
    let ws = Workspace::open_with_keystore(
        &data,
        Box::new(FileKeyStore::new(Paths::new(&data).identity_dir())),
    )
    .expect("workspace");
    let git = Git::new()
        .with_env("GIT_CONFIG_GLOBAL", data.join("no-global.gitconfig"))
        .with_env("GIT_CONFIG_NOSYSTEM", "1");
    let engine = Engine::start(
        ws,
        HarnessCatalog::new(),
        EngineConfig {
            design_enabled: false,
            git: Some(git),
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
        #[cfg(feature = "a2a")]
        a2a: None,
        token: Some(TOKEN.to_string()),
    };
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
    (dir, actual, stop)
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
    let bytes = body
        .map(|b| Bytes::from(serde_json::to_vec(&b).unwrap()))
        .unwrap_or_default();
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header(hyper::header::HOST, "localhost")
        .header(hyper::header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(hyper::header::CONTENT_TYPE, "application/json")
        .body(Full::new(bytes))
        .unwrap();
    let resp = sender.send_request(request).await.expect("request");
    let status = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn bare_origin(dir: &std::path::Path) -> PathBuf {
    let origin = dir.join("origin.git");
    let out = std::process::Command::new("git")
        .args(["init", "--bare", "--quiet", origin.to_str().unwrap()])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git on PATH");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    origin
}

#[tokio::test(flavor = "multi_thread")]
async fn drawings_over_http_with_their_repository() {
    let (dir, socket, _stop) = boot().await;

    // A scope that does not pair is refused before anything is written.
    let (status, v) = request(
        &socket,
        "POST",
        "/drawings",
        Some(json!({"title": "Orders", "scope": "goal"})),
    )
    .await;
    assert_eq!(status, 400, "{v}");

    let (status, v) = request(
        &socket,
        "POST",
        "/drawings",
        Some(json!({"title": "Orders", "scope": "workspace"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let id = v["drawing"]["id"].as_str().unwrap().to_string();
    let hash = v["drawing"]["hash"].as_str().unwrap().to_string();
    assert_eq!(v["drawing"]["element_count"], 0);
    assert!(v["drawing"]["scene"]["elements"]
        .as_array()
        .unwrap()
        .is_empty());

    // The list carries no scene; the detail does.
    let (status, v) = request(&socket, "GET", "/drawings?scope=workspace", None).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["drawings"].as_array().unwrap().len(), 1);
    assert!(v["drawings"][0].get("scene").is_none());
    let (status, v) = request(&socket, "GET", &format!("/drawings/{id}"), None).await;
    assert_eq!(status, 200, "{v}");
    assert!(v["drawing"]["scene"].is_object());

    // A scene change with the right hash lands; a stale one is a 409 carrying the scene.
    let scene = json!({"elements": [{"id": "a", "type": "rectangle", "x": 0, "y": 0, "width": 160, "height": 80}],
                       "app_state": {"view_background_color": "#ffffff", "grid": true}});
    let (status, v) = request(
        &socket,
        "PATCH",
        &format!("/drawings/{id}"),
        Some(json!({"scene": scene, "base_hash": hash})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["drawing"]["element_count"], 1);
    assert!(v["drawing"]["scene"]["app_state"]["grid"]
        .as_bool()
        .unwrap());
    let fresh = v["drawing"]["hash"].as_str().unwrap().to_string();
    let (status, v) = request(
        &socket,
        "PATCH",
        &format!("/drawings/{id}"),
        Some(json!({"scene": {"elements": []}, "base_hash": hash})),
    )
    .await;
    assert_eq!(status, 409, "{v}");
    assert_eq!(v["current_hash"], json!(fresh));
    assert_eq!(v["current"]["elements"].as_array().unwrap().len(), 1);
    // A title needs no hash.
    let (status, v) = request(
        &socket,
        "PATCH",
        &format!("/drawings/{id}"),
        Some(json!({"title": "Order flow", "pinned": true})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["drawing"]["title"], "Order flow");
    // An image is refused by name.
    let (status, v) = request(
        &socket,
        "PATCH",
        &format!("/drawings/{id}"),
        Some(json!({"scene": {"elements": [{"id": "i", "type": "image", "x": 0, "y": 0}]}, "base_hash": fresh})),
    )
    .await;
    assert_eq!(status, 400, "{v}");

    // The agents' parked requests: none yet; an answer to nothing is a 404.
    let (status, v) = request(&socket, "GET", "/drawings/requests", None).await;
    assert_eq!(status, 200, "{v}");
    assert!(v["requests"].as_array().unwrap().is_empty());
    let (status, v) = request(
        &socket,
        "POST",
        "/drawings/requests/01NOTHING",
        Some(json!({"ok": true})),
    )
    .await;
    assert_eq!(status, 404, "{v}");

    // The repository: one change, nobody set, then a commit, an origin, a push, a fetch — and no pull.
    let (status, v) = request(&socket, "GET", "/drawings/git", None).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["changed"], 1, "{v}");
    assert!(v["last_commit"].is_null());
    let (status, v) = request(
        &socket,
        "POST",
        "/drawings/git/commit",
        Some(json!({"message": "Add the first drawing"})),
    )
    .await;
    assert!((400..500).contains(&status), "{status} {v}");
    assert!(v["error"].as_str().unwrap().contains("Who commits"), "{v}");
    let (status, v) = request(
        &socket,
        "PUT",
        "/drawings/git/identity",
        Some(json!({"name": "Ada", "email": "ada@example.com"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (status, v) = request(
        &socket,
        "POST",
        "/drawings/git/commit",
        Some(json!({"message": "Add the first drawing"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["subject"], "Add the first drawing");
    let (_, v) = request(&socket, "GET", "/drawings/git", None).await;
    assert_eq!(v["changed"], 0, "the snapshot folder is ignored: {v}");
    let root = Paths::new(dir.path()).drawings_dir();
    let exclude = std::fs::read_to_string(root.join(".git").join("info").join("exclude")).unwrap();
    assert!(
        exclude.lines().any(|l| l == "state/"),
        "the snapshots are excluded by the repository's own file, never a tracked one: {exclude}"
    );
    let origin = bare_origin(dir.path());
    let (status, v) = request(
        &socket,
        "PUT",
        "/drawings/git/remote",
        Some(json!({"url": origin.to_str().unwrap()})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (status, v) = request(&socket, "POST", "/drawings/git/push", None).await;
    assert_eq!(status, 200, "{v}");
    assert!(v["upstream"].as_str().unwrap().starts_with("origin/"));
    let (status, v) = request(&socket, "POST", "/drawings/git/fetch", None).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["behind"], 0);
    let (status, _) = request(&socket, "POST", "/drawings/git/pull", None).await;
    assert_eq!(status, 404, "the drawings repository offers no pull");
    let (status, v) = request(&socket, "POST", "/drawings/git/message", None).await;
    assert_eq!(status, 200, "{v}");
    assert!(v["suggested"].is_boolean());

    // Deleting takes the record, the snapshot and the file.
    let (status, _) = request(&socket, "DELETE", &format!("/drawings/{id}"), None).await;
    assert_eq!(status, 200);
    let (status, _) = request(&socket, "GET", &format!("/drawings/{id}"), None).await;
    assert_eq!(status, 404);
    let (_, v) = request(&socket, "GET", "/drawings/git", None).await;
    assert_eq!(v["changed"], 1, "the deletion is a change to commit: {v}");
}
