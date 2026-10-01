//! The diagnostic log's routes: `GET /logs` names the workspace's folder
//! and, on a node that installed no log, every family with nothing and no
//! report; a planted report is listed, summarised and read back whole by
//! `GET /logs/crashes/{name}`, and a name that is not a report's is 404 —
//! and `/workspace` says where the folder is, so the desktop shell never
//! spells it.
//!
//! The fixture's workspace is a tempdir and the engine has no log handle,
//! so nothing writes a file and nothing is read off this machine; the one
//! report is written by the test, under the tempdir.

use bisa_engine::{Engine, EngineConfig};
use bisa_harness::HarnessCatalog;
use bisa_log::{CrashKind, CrashReport, Process};
use bisa_node::{serve, NodeConfig};
use bisa_store::{FileKeyStore, Paths, Workspace};
use http_body_util::{BodyExt as _, Full};
use hyper::body::Bytes;
use hyper::Request;
use hyper_util::rt::TokioIo;
use serde_json::Value;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
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

async fn get(socket: &std::path::Path, path: &str) -> (u16, Value) {
    let stream = UnixStream::connect(socket).await.expect("connect");
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .expect("handshake");
    tokio::spawn(conn);
    let request = Request::builder()
        .method("GET")
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
async fn the_logs_route_names_the_folder_and_a_node_without_a_log_lists_nothing() {
    let (dir, socket, _stop) = boot().await;
    let expected = Paths::new(dir.path()).logs_dir();

    let (status, logs) = get(&socket, "/logs").await;
    assert_eq!(status, 200, "{logs}");
    assert_eq!(logs["dir"], Value::from(expected.display().to_string()));
    let families = logs["families"].as_array().unwrap();
    assert_eq!(families.len(), Process::ALL.len());
    for (family, process) in families.iter().zip(Process::ALL) {
        assert_eq!(family["process"], process.prefix());
        assert_eq!(
            family["dir"],
            Value::from(process.dir(&expected).display().to_string())
        );
        assert_eq!(family["files"], Value::Array(vec![]));
    }
    assert_eq!(logs["crashes"], Value::Array(vec![]));
    assert_eq!(
        logs["crashes_dir"],
        Value::from(bisa_log::crashes_dir(&expected).display().to_string())
    );
    assert_eq!(logs["bytes"], Value::from(0));
    assert_eq!(logs["latest_crash"], Value::Null);
    assert!(!expected.exists(), "listing makes no folder");

    let (status, ws) = get(&socket, "/workspace").await;
    assert_eq!(status, 200, "{ws}");
    assert_eq!(ws["logs_dir"], Value::from(expected.display().to_string()));
    assert_eq!(
        ws["data_dir"],
        Value::from(dir.path().display().to_string())
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_crash_report_is_listed_summarised_and_read_back_whole_and_a_stranger_is_404() {
    let (dir, socket, _stop) = boot().await;
    let root = Paths::new(dir.path()).logs_dir();
    let mut report = CrashReport::new(CrashKind::Panic, "one bad row")
        .with_location(Some("src/x.rs:12".to_string()))
        .with_thread(Some("main".to_string()))
        .with_backtrace("0: x\n1: y".to_string());
    report.process = Process::Node;
    report.version = "0.0.0-test".to_string();
    report.pid = 4242;
    report.at = "2026-09-11T10:22:33Z".to_string();
    let at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_789_000_000);
    let path = bisa_log::write_crash(&root, &report, at).unwrap();
    let name = path.file_name().unwrap().to_string_lossy().into_owned();

    let (status, logs) = get(&socket, "/logs").await;
    assert_eq!(status, 200, "{logs}");
    assert_eq!(logs["crashes"][0]["name"], name);
    assert!(logs["bytes"].as_u64().unwrap() > 0);
    let latest = &logs["latest_crash"];
    assert_eq!(latest["name"], name);
    assert_eq!(latest["process"], "node");
    assert_eq!(latest["kind"], "panic");
    assert_eq!(latest["at"], "2026-09-11T10:22:33Z");
    assert_eq!(latest["message"], "one bad row");

    let (status, whole) = get(&socket, &format!("/logs/crashes/{name}")).await;
    assert_eq!(status, 200, "{whole}");
    assert_eq!(whole["process"], "node");
    assert_eq!(whole["version"], "0.0.0-test");
    assert_eq!(whole["pid"], 4242);
    assert_eq!(whole["kind"], "panic");
    assert_eq!(whole["location"], "src/x.rs:12");
    assert_eq!(whole["thread"], "main");
    assert_eq!(whole["backtrace"], "0: x\n1: y");
    assert_eq!(whole["child"], Value::Null);
    assert_eq!(whole["recent"], Value::Array(vec![]));

    let (status, _) = get(&socket, "/logs/crashes/node.20260910T060640Z.9.json").await;
    assert_eq!(status, 404, "a report that is not there");
    let (status, _) = get(&socket, "/logs/crashes/notes.json").await;
    assert_eq!(status, 404, "a name that is not a report's");
}
