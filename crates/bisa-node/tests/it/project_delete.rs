//! Deleting a project's folder takes it off disk the way `editor.delete.trash`
//! says — the OS Trash by default, unlinked when off: the managed directory
//! leaves its place with `?tree=true`, and a plain forget leaves every file
//! where it was. The fixture turns the Trash off: a test never writes outside
//! its tempdir, and Finder's Trash is somebody's.

use bisa_engine::{Engine, EngineConfig};
use bisa_harness::HarnessCatalog;
use bisa_node::{serve, NodeConfig};
use bisa_store::{FileKeyStore, NewProject, Paths, Workspace};
use http_body_util::{BodyExt as _, Full};
use hyper::body::Bytes;
use hyper::Request;
use hyper_util::rt::TokioIo;
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;
use tokio::net::UnixStream;

const TOKEN: &str = "test-token-0123456789abcdef";

/// Boot a node over a temp workspace holding one managed project; return the
/// data dir, the socket, and the project id.
async fn boot() -> (
    tempfile::TempDir,
    PathBuf,
    PathBuf,
    String,
    tokio::sync::oneshot::Sender<()>,
) {
    let dir = tempfile::tempdir().expect("tempdir");
    let data = dir.path().to_path_buf();
    let ws = Workspace::open_with_keystore(
        &data,
        Box::new(FileKeyStore::new(Paths::new(&data).identity_dir())),
    )
    .expect("workspace");
    let pid = ws
        .create_project(NewProject::managed("web").unwrap())
        .unwrap()
        .id
        .to_string();
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
        data_dir: data.clone(),
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
    (dir, data, actual, pid, stop)
}

async fn request(socket: &std::path::Path, method: &str, path: &str) -> (u16, Value) {
    request_with(socket, method, path, None).await
}

async fn request_with(
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
    let bytes = body.map(|b| Bytes::from(b.to_string())).unwrap_or_default();
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header(hyper::header::HOST, "localhost")
        .header(hyper::header::CONTENT_TYPE, "application/json")
        .header(hyper::header::AUTHORIZATION, format!("Bearer {TOKEN}"))
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

#[tokio::test(flavor = "multi_thread")]
async fn tree_true_moves_the_managed_folder_off_disk() {
    let (_dir, data, socket, pid, _stop) = boot().await;
    let projects_dir = data.join("projects");
    assert!(
        std::fs::read_dir(&projects_dir).unwrap().count() >= 1,
        "the managed project folder exists before the delete"
    );

    let (status, v) = request_with(
        &socket,
        "PUT",
        "/settings/machine",
        Some(serde_json::json!({"values": {"editor.delete.trash": false}})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (status, body) = request(&socket, "DELETE", &format!("/projects/{pid}?tree=true")).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["removed_tree"], Value::Bool(true));
    let path = body["path"].as_str().expect("a path in the response");
    assert!(
        !std::path::Path::new(path).exists(),
        "the project folder is gone from its place on disk (moved to Trash): {path}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_plain_forget_leaves_the_folder_on_disk() {
    let (_dir, data, socket, pid, _stop) = boot().await;
    let projects_dir = data.join("projects");
    let before = std::fs::read_dir(&projects_dir).unwrap().count();

    let (status, body) = request(&socket, "DELETE", &format!("/projects/{pid}")).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["removed_tree"], Value::Bool(false));
    assert_eq!(
        std::fs::read_dir(&projects_dir).unwrap().count(),
        before,
        "forget keeps every file where it was"
    );
}
