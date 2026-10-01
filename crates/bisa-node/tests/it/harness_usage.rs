//! `GET /harnesses/{id}/usage` over HTTP: a mock harness's report comes
//! back as percentages and labels, an unknown harness says it reports
//! nothing, and `harness.usage.reads` off answers `off`. No binary, no
//! provider, no credential — the catalog holds one mock.

use bisa_engine::{Engine, EngineConfig};
use bisa_harness::mock::MockAdapter;
use bisa_harness::{
    HarnessCatalog, UsageAccount, UsageReport, UsageSource, UsageState, UsageWindow,
};
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

fn mock_report() -> UsageState {
    UsageState::Report {
        report: UsageReport {
            harness: "mock".into(),
            read_at: 1_788_775_200,
            source: UsageSource::Endpoint,
            account: Some(UsageAccount {
                plan: Some("max".into()),
                login: None,
            }),
            windows: vec![
                UsageWindow {
                    id: "five_hour".into(),
                    label: "5h".into(),
                    scope: None,
                    used_percent: 23.5,
                    resets_at: Some(1_788_782_400),
                },
                UsageWindow {
                    id: "weekly:fable".into(),
                    label: "Fable".into(),
                    scope: Some("Fable".into()),
                    used_percent: 8.5,
                    resets_at: None,
                },
            ],
            extras: vec![],
        },
    }
}

async fn boot() -> (tempfile::TempDir, PathBuf, tokio::sync::oneshot::Sender<()>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let data = dir.path().to_path_buf();
    let ws = Workspace::open_with_keystore(
        &data,
        Box::new(FileKeyStore::new(Paths::new(&data).identity_dir())),
    )
    .expect("workspace");
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::new(MockAdapter {
        id: "mock".into(),
        usage: Some(mock_report()),
        ..Default::default()
    }));
    let engine = Engine::start(
        ws,
        catalog,
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

#[tokio::test(flavor = "multi_thread")]
async fn a_harness_usage_is_read_once_said_honestly_and_switched_off() {
    let (_dir, socket, _stop) = boot().await;

    // The mock's report, as the desktop reads it.
    let (status, v) = request(&socket, "GET", "/harnesses/mock/usage", None).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["harness"], "mock");
    assert_eq!(v["usage"]["state"], "report");
    let report = &v["usage"]["report"];
    assert_eq!(report["source"], "endpoint");
    assert_eq!(report["account"]["plan"], "max");
    assert_eq!(report["windows"][0]["label"], "5h");
    assert_eq!(report["windows"][0]["used_percent"], 23.5);
    assert_eq!(report["windows"][1]["label"], "Fable");
    assert_eq!(
        report["windows"][1]["scope"], "Fable",
        "a model week is scoped to its model"
    );
    assert!(
        report["windows"][1].get("resets_at").is_none(),
        "an unknown reset is omitted, not null"
    );
    let text = v.to_string();
    assert!(
        !text.contains("token") && !text.contains("Token"),
        "nothing of a credential is on the wire: {text}"
    );

    // A harness the catalog does not hold reports nothing, in its own name.
    let (status, v) = request(&socket, "GET", "/harnesses/nobody/usage", None).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["usage"]["state"], "unsupported");
    assert!(
        v["usage"]["reason"]
            .as_str()
            .unwrap()
            .starts_with("nobody reports no usage limits"),
        "{v}"
    );

    // Reads off: every harness answers off, the mock included.
    let (status, v) = request(
        &socket,
        "PUT",
        "/settings/machine",
        Some(json!({"values": {"harness.usage.reads": false}})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (status, v) = request(&socket, "GET", "/harnesses/mock/usage?refresh=true", None).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["usage"]["state"], "off");
}
