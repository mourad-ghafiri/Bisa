//! The cache management surface over HTTP: stats are read and the
//! caches clear. A request warms a cache; `/cache/stats` reports it;
//! `/cache/clear` empties the entries while the counters stand.

use bisa_engine::{Engine, EngineConfig};
use bisa_harness::HarnessCatalog;
use bisa_node::{serve, NodeConfig};
use bisa_store::{FileKeyStore, Paths, Workspace};
use http_body_util::{BodyExt as _, Full};
use hyper::body::Bytes;
use hyper::Request;
use hyper_util::rt::TokioIo;
use serde_json::Value;
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

async fn request(socket: &std::path::Path, method: &str, path: &str) -> (u16, Value) {
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

#[tokio::test(flavor = "multi_thread")]
async fn stats_report_caches_and_clear_empties_them() {
    let (_dir, socket, _stop) = boot().await;

    // Warm a cache: two `GET /harnesses` share the harness listing (the second
    // is a hit within the 30 s default TTL).
    let _ = request(&socket, "GET", "/harnesses").await;
    let _ = request(&socket, "GET", "/harnesses").await;

    let (status, stats) = request(&socket, "GET", "/cache/stats").await;
    assert_eq!(status, 200, "{stats}");
    let rows = stats.as_array().expect("an array of cache stats");
    let listing = rows
        .iter()
        .find(|r| r["name"] == "harness.listing")
        .expect("the harness listing cache is registered");
    assert_eq!(listing["entries"], Value::from(1), "the listing is held");
    assert!(
        listing["hits"].as_u64().unwrap() >= 1,
        "the second request was a hit"
    );

    let (status, cleared) = request(&socket, "POST", "/cache/clear").await;
    assert_eq!(status, 200, "{cleared}");
    assert_eq!(cleared["cleared"], Value::Bool(true));

    let (_status, after) = request(&socket, "GET", "/cache/stats").await;
    let listing = after
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "harness.listing")
        .expect("still registered after a clear");
    assert_eq!(listing["entries"], Value::from(0), "cleared to empty");
    assert!(
        listing["hits"].as_u64().unwrap() >= 1,
        "counters stand across a clear"
    );
}
