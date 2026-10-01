//! Settings over HTTP: three scopes, one registry, refusals at write time.

use bisa_engine::{Engine, EngineConfig};
use bisa_harness::HarnessCatalog;
use bisa_node::{serve, NodeConfig};
use bisa_store::{FileKeyStore, NewProject, Paths, Workspace};
use http_body_util::{BodyExt as _, Full};
use hyper::body::Bytes;
use hyper::Request;
use hyper_util::rt::TokioIo;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;
use tokio::net::UnixStream;

const TOKEN: &str = "test-token-0123456789abcdef";

async fn boot() -> (
    tempfile::TempDir,
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
    let project = ws
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
    (dir, actual, project, stop)
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

fn find<'a>(rows: &'a Value, key: &str) -> &'a Value {
    rows["settings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == json!(key))
        .unwrap_or_else(|| panic!("{key} is not in the resolved set"))
}

#[tokio::test(flavor = "multi_thread")]
async fn the_registry_is_served_and_every_key_resolves_to_its_default_at_first() {
    let (_dir, socket, _project, _stop) = boot().await;
    let (status, registry) = request(&socket, "GET", "/settings/registry", None).await;
    assert_eq!(status, 200);
    let defs = registry["settings"].as_array().unwrap();
    assert_eq!(defs.len(), bisa_core::SETTINGS.len());
    let tab = defs
        .iter()
        .find(|d| d["key"] == json!("editor.tab_size"))
        .unwrap();
    assert_eq!(tab["kind"]["type"], json!("integer"));
    assert_eq!(
        tab["scopes"],
        json!(["project", "workspace"]),
        "scopes list in resolution order: the narrowest first"
    );
    assert_eq!(tab["group"], json!("editor"));

    let (status, resolved) = request(&socket, "GET", "/settings/resolved", None).await;
    assert_eq!(status, 200);
    assert_eq!(
        find(&resolved, "editor.tab_size")["origin"],
        json!("default")
    );
    assert_eq!(find(&resolved, "editor.tab_size")["value"], json!(4));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_write_lands_at_its_scope_and_resolution_walks_project_then_workspace() {
    let (_dir, socket, project, _stop) = boot().await;
    let (status, v) = request(
        &socket,
        "PUT",
        "/settings/workspace",
        Some(json!({"values": {"editor.tab_size": 2}})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (status, v) = request(
        &socket,
        "PUT",
        &format!("/settings/project?project={project}"),
        Some(json!({"values": {"editor.tab_size": 8}})),
    )
    .await;
    assert_eq!(status, 200, "{v}");

    let (_, for_project) = request(
        &socket,
        "GET",
        &format!("/settings/resolved?project={project}"),
        None,
    )
    .await;
    assert_eq!(find(&for_project, "editor.tab_size")["value"], json!(8));
    assert_eq!(
        find(&for_project, "editor.tab_size")["origin"],
        json!("project")
    );
    let (_, for_workspace) = request(&socket, "GET", "/settings/resolved", None).await;
    assert_eq!(find(&for_workspace, "editor.tab_size")["value"], json!(2));
    assert_eq!(
        find(&for_workspace, "editor.tab_size")["origin"],
        json!("workspace")
    );

    // The raw layer is exactly what was written there, nothing resolved into it.
    let (_, layer) = request(&socket, "GET", "/settings/workspace", None).await;
    assert_eq!(layer["values"], json!({"editor.tab_size": 2}));

    // Unset at project scope: the value falls back to the workspace's.
    let (status, v) = request(
        &socket,
        "DELETE",
        &format!("/settings/project/editor.tab_size?project={project}"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["setting"]["origin"], json!("workspace"));
    assert_eq!(v["setting"]["value"], json!(2));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_disallowed_scope_or_a_bad_value_is_refused_naming_the_rule_and_writes_nothing() {
    let (_dir, socket, project, _stop) = boot().await;
    // font_size is machine-only.
    let (status, v) = request(
        &socket,
        "PUT",
        &format!("/settings/project?project={project}"),
        Some(json!({"values": {"editor.font_size": 14}})),
    )
    .await;
    assert_eq!(status, 400, "{v}");
    let error = v["error"].as_str().unwrap();
    assert!(
        error.contains("editor.font_size") && error.contains("allowed: machine"),
        "the scopes in the words a person types: {error}"
    );

    // A batch with one bad key writes none of it.
    let (status, v) = request(
        &socket,
        "PUT",
        "/settings/workspace",
        Some(json!({"values": {"editor.tab_size": 3, "editor.font_size": 14}})),
    )
    .await;
    assert_eq!(status, 400, "{v}");
    let (_, layer) = request(&socket, "GET", "/settings/workspace", None).await;
    assert_eq!(layer["values"], json!({}), "half a batch was written");

    // Out of range, wrong type, unknown key, unknown scope.
    for (path, body) in [
        (
            "/settings/machine",
            json!({"values": {"editor.font_size": 99}}),
        ),
        (
            "/settings/machine",
            json!({"values": {"editor.minimap": "yes"}}),
        ),
        ("/settings/machine", json!({"values": {"editor.nope": 1}})),
        (
            "/settings/galaxy",
            json!({"values": {"editor.font_size": 12}}),
        ),
    ] {
        let (status, v) = request(&socket, "PUT", path, Some(body)).await;
        assert_eq!(status, 400, "{path}: {v}");
    }
    let (status, v) = request(
        &socket,
        "PUT",
        "/settings/project",
        Some(json!({"values": {"editor.tab_size": 2}})),
    )
    .await;
    assert_eq!(status, 400, "project scope with no project: {v}");
}
