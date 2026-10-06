//! `GET /updates` over the socket: the latest release as the engine read it
//! from a stub, `?refresh=true` asking again, the token required, and a node
//! started without a source answering *off*.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::routing::any;
use axum::{Json, Router};
use bisa_engine::updates::UpdatesSource;
use bisa_engine::{Engine, EngineConfig};
use bisa_harness::HarnessCatalog;
use bisa_node::{serve, NodeConfig};
use bisa_store::{FileKeyStore, Paths, Workspace};
use http_body_util::{BodyExt as _, Full};
use hyper::body::Bytes;
use hyper::Request;
use hyper_util::rt::TokioIo;
use serde_json::{json, Value};
use tokio::net::UnixStream;

const TOKEN: &str = "test-token-0123456789abcdef";

/// A stub standing in for GitHub's *latest release*, counting its hits.
struct Stub {
    base_url: String,
    hits: Arc<AtomicU32>,
}

async fn handle(State(hits): State<Arc<AtomicU32>>) -> Json<Value> {
    hits.fetch_add(1, Ordering::SeqCst);
    Json(json!({
        "tag_name": "v0.3.0",
        "name": "Bisa 0.3.0",
        "html_url": "https://github.com/mourad-ghafiri/Bisa/releases/tag/v0.3.0",
        "body": "### Added\n\n- The Update dialog.\n",
        "published_at": "2026-10-10T09:00:00Z",
        "assets": [{"name": "Bisa-0.3.0-macos-universal.dmg", "browser_download_url": "https://example.test/Bisa-0.3.0-macos-universal.dmg", "size": 1}]
    }))
}

impl Stub {
    async fn start() -> Self {
        let hits = Arc::new(AtomicU32::new(0));
        let app = Router::new()
            .fallback(any(handle))
            .with_state(Arc::clone(&hits));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _served = axum::serve(listener, app).await;
        });
        Self {
            base_url: format!("http://{addr}"),
            hits,
        }
    }

    fn source(&self) -> UpdatesSource {
        UpdatesSource::at(format!("{}/repos/o/r/releases/latest", self.base_url))
    }
}

async fn boot(
    updates: Option<UpdatesSource>,
) -> (tempfile::TempDir, PathBuf, tokio::sync::oneshot::Sender<()>) {
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
            events_enabled: false,
            updates,
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

async fn get(socket: &std::path::Path, path: &str, token: Option<&str>) -> (u16, Value) {
    let stream = UnixStream::connect(socket).await.expect("connect");
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .expect("handshake");
    tokio::spawn(conn);
    let mut request = Request::builder()
        .method("GET")
        .uri(path)
        .header(hyper::header::HOST, "localhost");
    if let Some(token) = token {
        request = request.header(hyper::header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let request = request.body(Full::new(Bytes::new())).unwrap();
    let resp = sender.send_request(request).await.expect("request");
    let status = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn the_latest_release_is_read_held_and_asked_again_on_refresh() {
    let stub = Stub::start().await;
    let (_dir, socket, _stop) = boot(Some(stub.source())).await;

    let (status, v) = get(&socket, "/updates", Some(TOKEN)).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["state"], "latest");
    assert_eq!(v["release"]["tag"], "v0.3.0");
    assert_eq!(v["release"]["version"], "0.3.0");
    assert_eq!(v["release"]["name"], "Bisa 0.3.0");
    assert_eq!(v["release"]["published_at"], 1_791_622_800u64);
    assert_eq!(
        v["release"]["url"],
        "https://github.com/mourad-ghafiri/Bisa/releases/tag/v0.3.0"
    );
    assert_eq!(
        v["release"]["assets"][0]["name"],
        "Bisa-0.3.0-macos-universal.dmg"
    );
    assert!(v["checked_at"].as_u64().unwrap() > 0);
    assert_eq!(stub.hits.load(Ordering::SeqCst), 1);

    let (status, again) = get(&socket, "/updates", Some(TOKEN)).await;
    assert_eq!(status, 200, "{again}");
    assert_eq!(again, v, "held for the TTL");
    assert_eq!(stub.hits.load(Ordering::SeqCst), 1);

    let (status, fresh) = get(&socket, "/updates?refresh=true", Some(TOKEN)).await;
    assert_eq!(status, 200, "{fresh}");
    assert_eq!(fresh["state"], "latest");
    assert_eq!(stub.hits.load(Ordering::SeqCst), 2, "a refresh asks again");

    let (status, refused) = get(&socket, "/updates?refresh=seven", Some(TOKEN)).await;
    assert_eq!(status, 400, "{refused}");

    let (status, _) = get(&socket, "/updates", None).await;
    assert_eq!(status, 401, "the token, like every route but /health");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_node_started_without_a_source_answers_off_and_dials_nobody() {
    let (_dir, socket, _stop) = boot(None).await;
    let (status, v) = get(&socket, "/updates", Some(TOKEN)).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v, json!({"state": "off"}));
}
