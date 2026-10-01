//! The notes repository over HTTP: status, who commits, commit, origin, push,
//! fetch — against a local bare origin, no network, no real agent.

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
async fn the_notes_repository_over_http() {
    let (dir, socket, _stop) = boot().await;

    // A note, then the repository's status: one change, no commit, no origin.
    let (status, v) = request(
        &socket,
        "POST",
        "/notes",
        Some(json!({"title": "First", "body": "words", "scope": "workspace"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (status, v) = request(&socket, "GET", "/notes/git", None).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["changed"], 1);
    assert!(v["last_commit"].is_null());
    assert!(v["remote"].is_null());
    assert_eq!(v["identity"]["source"], "none");

    // Nobody set to commit is a refusal with the notes' own door, not a 500.
    let (status, v) = request(
        &socket,
        "POST",
        "/notes/git/commit",
        Some(json!({"message": "Add the first note"})),
    )
    .await;
    assert!((400..500).contains(&status), "{status} {v}");
    assert!(v["error"].as_str().unwrap().contains("Who commits"), "{v}");

    let (status, v) = request(
        &socket,
        "PUT",
        "/notes/git/identity",
        Some(json!({"name": "Ada", "email": "ada@example.com"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["identity"]["source"], "local");
    let (status, v) = request(
        &socket,
        "POST",
        "/notes/git/commit",
        Some(json!({"message": "Add the first note"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["subject"], "Add the first note");
    let (_, v) = request(&socket, "GET", "/notes/git", None).await;
    assert_eq!(v["changed"], 0);
    assert_eq!(v["last_commit"]["subject"], "Add the first note");

    // Push needs an origin; set one by path and push.
    let (status, v) = request(&socket, "POST", "/notes/git/push", None).await;
    assert!(
        (400..500).contains(&status),
        "no origin is a refusal: {status} {v}"
    );
    let origin = bare_origin(dir.path());
    let (status, v) = request(
        &socket,
        "PUT",
        "/notes/git/remote",
        Some(json!({"url": origin.to_str().unwrap()})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (status, v) = request(&socket, "POST", "/notes/git/push", None).await;
    assert_eq!(status, 200, "{v}");
    assert!(
        v["upstream"].as_str().unwrap().starts_with("origin/"),
        "{v}"
    );
    assert_eq!(v["ahead"], 0);
    let (status, v) = request(&socket, "POST", "/notes/git/fetch", None).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["behind"], 0);

    // The suggestion route always answers 200; with no agent runnable here it says why.
    let (status, v) = request(&socket, "POST", "/notes/git/message", None).await;
    assert_eq!(status, 200, "{v}");
    assert!(v["suggested"].is_boolean());
}
