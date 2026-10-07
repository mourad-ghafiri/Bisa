//! The cache management surface over HTTP: stats are read and the
//! caches clear. A request warms a cache; `/cache/stats` reports it;
//! `/cache/clear` empties the entries while the counters stand.
//!
//! The registry the surface reads is the process's: every node this test
//! binary has booted registers a `harness.listing` of its own under the one
//! name, and any of them may be read while this runs. So what is asserted
//! here holds whoever else is reading — sums over every row of the name, and
//! counters that only grow — never one row's exact entry count.

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

    /// Every `harness.listing` row summed: entries, hits, misses.
    fn listing(stats: &Value) -> (u64, u64, u64) {
        stats
            .as_array()
            .expect("an array of cache stats")
            .iter()
            .filter(|r| r["name"] == "harness.listing")
            .fold((0, 0, 0), |(e, h, m), r| {
                (
                    e + r["entries"].as_u64().unwrap_or(0),
                    h + r["hits"].as_u64().unwrap_or(0),
                    m + r["misses"].as_u64().unwrap_or(0),
                )
            })
    }

    let (status, before) = request(&socket, "GET", "/cache/stats").await;
    assert_eq!(status, 200, "{before}");
    assert!(
        before
            .as_array()
            .is_some_and(|rows| rows.iter().any(|r| r["name"] == "harness.listing")),
        "the harness listing cache is registered: {before}"
    );
    let (_, hits_before, _) = listing(&before);

    // Warm the cache: two `GET /harnesses` share the harness listing (the
    // second is a hit within the 30 s default TTL).
    let _ = request(&socket, "GET", "/harnesses").await;
    let _ = request(&socket, "GET", "/harnesses").await;
    let (_, warmed) = request(&socket, "GET", "/cache/stats").await;
    let (entries, hits, misses) = listing(&warmed);
    assert!(entries >= 1, "the listing is held: {warmed}");
    assert!(hits > hits_before, "the second request was a hit: {warmed}");

    let (status, cleared) = request(&socket, "POST", "/cache/clear").await;
    assert_eq!(status, 200, "{cleared}");
    assert_eq!(cleared["cleared"], Value::Bool(true));

    // Cleared: the next read computes again — a miss of this node's own,
    // whatever another node in the process is doing — and the counters stand.
    let _ = request(&socket, "GET", "/harnesses").await;
    let (_status, after) = request(&socket, "GET", "/cache/stats").await;
    let (_, hits_after, misses_after) = listing(&after);
    assert!(
        misses_after > misses,
        "cleared to empty: the read after the clear computed again: {after}"
    );
    assert!(hits_after >= hits, "counters stand across a clear: {after}");
}
