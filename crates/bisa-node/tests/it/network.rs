//! The network routes: `GET /network` reflects a manual write with the login
//! masked and says what is in force; a bad URL is a 400 at the settings
//! route naming the key; `POST /network/check` refuses a URL that is not
//! `http(s)` or names this machine. Nothing here sends a request out.

use bisa_engine::{Engine, EngineConfig};
use bisa_harness::HarnessCatalog;
use bisa_node::{serve, NodeConfig};
use bisa_store::{FileKeyStore, Paths, Workspace};
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

async fn call(
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
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn the_status_reflects_a_manual_write_with_the_login_masked() {
    let (_dir, socket, _stop) = boot().await;

    let (status, body) = call(&socket, "GET", "/network", None).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["mode"], json!("environment"));
    assert_eq!(body["http1_only"], json!(false));
    assert!(body["problems"].as_array().unwrap().is_empty(), "{body}");

    let (status, written) = call(
        &socket,
        "PUT",
        "/settings/machine",
        Some(json!({ "values": {
            "network.proxy.mode": "manual",
            "network.proxy.https": "http://ada:s3cret@proxy.example:3128",
            "network.proxy.no_proxy": ".corp.example, 10.0.0.0/8",
            "network.http1_only": true,
        }})),
    )
    .await;
    assert_eq!(status, 200, "{written}");

    let (status, body) = call(&socket, "GET", "/network", None).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["mode"], json!("manual"));
    let shown = body["https"].as_str().unwrap();
    assert!(!shown.contains("s3cret"), "{shown}");
    assert!(shown.contains("ada:"), "{shown}");
    assert_eq!(body["no_proxy"], json!([".corp.example", "10.0.0.0/8"]));
    assert_eq!(body["http1_only"], json!(true));
    assert_eq!(body["in_force"]["kind"], json!("proxy"));
    assert!(!body["in_force"]["https"]
        .as_str()
        .unwrap()
        .contains("s3cret"));
    assert_eq!(
        body["in_force"]["no_proxy"],
        json!("localhost,127.0.0.1,::1,.corp.example,10.0.0.0/8")
    );
    assert!(body["problems"].as_array().unwrap().is_empty(), "{body}");
    assert!(
        !body.to_string().contains("s3cret"),
        "a password never leaves the node"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bad_proxy_url_is_a_400_naming_the_key_and_writes_nothing() {
    let (_dir, socket, _stop) = boot().await;
    let (status, body) = call(
        &socket,
        "PUT",
        "/settings/machine",
        Some(json!({ "values": { "network.proxy.http": "proxy.example:3128" } })),
    )
    .await;
    assert_eq!(status, 400, "{body}");
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("network.proxy.http"),
        "{body}"
    );
    let (_, layer) = call(&socket, "GET", "/settings/machine", None).await;
    assert!(
        layer["values"].get("network.proxy.http").is_none(),
        "{layer}"
    );
}

/// A relay entry the pool would only drop in silence is refused at the
/// door, naming the key and the entry, and nothing is written.
#[tokio::test(flavor = "multi_thread")]
async fn a_relay_that_is_no_url_is_a_400_naming_the_entry_and_writes_nothing() {
    let (_dir, socket, _stop) = boot().await;
    let (status, body) = call(
        &socket,
        "PUT",
        "/settings/machine",
        Some(json!({ "values": { "sync.relays": ["wss://relay.example", "not-a-url"] } })),
    )
    .await;
    assert_eq!(status, 400, "{body}");
    let said = body["error"].as_str().unwrap();
    assert!(
        said.contains("sync.relays") && said.contains("not-a-url"),
        "{body}"
    );
    let (_, layer) = call(&socket, "GET", "/settings/machine", None).await;
    assert!(layer["values"].get("sync.relays").is_none(), "{layer}");
    let (status, _) = call(
        &socket,
        "PUT",
        "/settings/machine",
        Some(json!({ "values": { "sync.relays": ["wss://relay.example"] } })),
    )
    .await;
    assert_eq!(status, 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_check_refuses_this_machine_and_a_scheme_that_is_not_http() {
    let (_dir, socket, _stop) = boot().await;
    for url in [
        "http://127.0.0.1:1/",
        "http://localhost/",
        "ftp://example.com/",
        "nope",
    ] {
        let (status, body) = call(
            &socket,
            "POST",
            "/network/check",
            Some(json!({ "url": url })),
        )
        .await;
        assert_eq!(status, 400, "{url}: {body}");
    }
}
