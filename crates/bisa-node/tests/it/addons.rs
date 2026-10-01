//! Addons over the wire: a folder imported, listed, switched and granted;
//! the bundle served **without the token** under its walls; the broker's
//! refusals; a built-in installed from the catalog. Nothing leaves this
//! process — every fetch below stops at a rule.

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

struct Answer {
    status: u16,
    headers: hyper::HeaderMap,
    body: Vec<u8>,
}

impl Answer {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or(Value::Null)
    }
    fn header(&self, name: &str) -> Option<String> {
        self.headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    }
}

async fn send(
    socket: &std::path::Path,
    method: &str,
    path: &str,
    body: Option<Value>,
    token: bool,
) -> Answer {
    let stream = UnixStream::connect(socket).await.expect("connect");
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .expect("handshake");
    tokio::spawn(conn);
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(hyper::header::HOST, "localhost")
        .header(hyper::header::CONTENT_TYPE, "application/json");
    if token {
        builder = builder.header(hyper::header::AUTHORIZATION, format!("Bearer {TOKEN}"));
    }
    let request = builder
        .body(Full::new(Bytes::from(
            body.map(|b| b.to_string()).unwrap_or_default(),
        )))
        .unwrap();
    let resp = sender.send_request(request).await.expect("request");
    let status = resp.status().as_u16();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    Answer {
        status,
        headers,
        body: bytes.to_vec(),
    }
}

async fn call(socket: &std::path::Path, method: &str, path: &str, body: Value) -> Answer {
    send(socket, method, path, Some(body), true).await
}

async fn get(socket: &std::path::Path, path: &str) -> Answer {
    send(socket, "GET", path, None, true).await
}

async fn get_open(socket: &std::path::Path, path: &str) -> Answer {
    send(socket, "GET", path, None, false).await
}

/// A folder of the person's own: a page, a script, an image.
fn folder(root: &std::path::Path, id: &str) -> PathBuf {
    let dir = root.join("byte");
    std::fs::create_dir_all(dir.join("img")).unwrap();
    std::fs::write(
        dir.join("addon.json"),
        format!(
            r#"{{"id":"{id}","name":"Byte","description":"a widget","version":"1.0.0","license":"MIT",
                "permissions":["storage",{{"network":{{"hosts":["api.example.com"]}}}}]}}"#
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("index.html"),
        "<!doctype html><script src=\"bisa-addon.js\"></script><script src=\"main.js\"></script>",
    )
    .unwrap();
    std::fs::write(dir.join("main.js"), "// x").unwrap();
    std::fs::write(dir.join("img/a.png"), b"png").unwrap();
    dir
}

#[tokio::test(flavor = "multi_thread")]
async fn an_addon_is_imported_switched_granted_and_removed_over_the_wire() {
    let (_dir, socket, _stop) = boot().await;
    let src = tempfile::tempdir().unwrap();
    let from = folder(src.path(), "acme.byte");

    let a = get(&socket, "/addons").await;
    assert_eq!(a.status, 200, "{}", String::from_utf8_lossy(&a.body));
    assert_eq!(a.json()["addons"], json!([]));
    assert_eq!(a.json()["addons_enabled"], json!(true));

    // Validate first, as the import dialog does: a clean folder, no problems.
    let v = call(
        &socket,
        "POST",
        "/addons/validate",
        json!({ "path": from.to_string_lossy() }),
    )
    .await;
    assert_eq!(v.status, 200, "{}", String::from_utf8_lossy(&v.body));
    assert_eq!(v.json()["problems"], json!([]));
    assert_eq!(v.json()["manifest"]["id"], "acme.byte");

    // The catalog's offer, whole manifests and all, before anything is installed.
    let offers = get(&socket, "/addons/offer").await;
    assert_eq!(
        offers.status,
        200,
        "{}",
        String::from_utf8_lossy(&offers.body)
    );
    let offered = offers.json()["offers"].as_array().unwrap().clone();
    assert_eq!(offered.len(), 13);
    assert!(offered.iter().all(|o| o["installed"] == false));
    let weather = offered.iter().find(|o| o["slug"] == "weather").unwrap();
    assert_eq!(
        weather["manifest"]["permissions"].as_array().unwrap().len(),
        2
    );

    // A grant never declared refuses the import by word.
    let refused = call(
        &socket,
        "POST",
        "/addons",
        json!({ "path": from.to_string_lossy(), "granted": ["notify"], "enabled": true }),
    )
    .await;
    assert_eq!(
        refused.status,
        400,
        "{}",
        String::from_utf8_lossy(&refused.body)
    );
    assert!(refused.json()["error"].as_str().unwrap().contains("notify"));

    let imported = call(
        &socket,
        "POST",
        "/addons",
        json!({ "path": from.to_string_lossy(), "granted": ["storage"], "enabled": false }),
    )
    .await;
    assert_eq!(
        imported.status,
        200,
        "{}",
        String::from_utf8_lossy(&imported.body)
    );
    let addon = &imported.json()["addon"];
    assert_eq!(addon["id"], "acme.byte");
    assert_eq!(addon["origin"], "local");
    assert_eq!(addon["enabled"], false);
    assert_eq!(addon["granted"], json!(["storage"]));
    assert_eq!(addon["files_present"], true);
    assert_eq!(addon["active"], false);
    assert_eq!(addon["manifest"]["name"], "Byte");

    // A disabled addon serves nothing, with or without the token.
    assert_eq!(
        get_open(&socket, "/addons/acme.byte/files/index.html")
            .await
            .status,
        404
    );
    assert_eq!(
        get(&socket, "/addons/acme.byte/files/index.html")
            .await
            .status,
        404
    );

    // Nothing to patch is a 400; the switch and the grants each move.
    assert_eq!(
        call(&socket, "PATCH", "/addons/acme.byte", json!({}))
            .await
            .status,
        400
    );
    let on = call(
        &socket,
        "PATCH",
        "/addons/acme.byte",
        json!({ "enabled": true }),
    )
    .await;
    assert_eq!(on.status, 200, "{}", String::from_utf8_lossy(&on.body));
    assert_eq!(on.json()["addon"]["active"], true);
    let widened = call(
        &socket,
        "PATCH",
        "/addons/acme.byte",
        json!({ "granted": [{ "network": { "hosts": ["evil.example"] } }] }),
    )
    .await;
    assert_eq!(widened.status, 400, "a widened grant is refused");
    let granted = call(
        &socket,
        "PATCH",
        "/addons/acme.byte",
        json!({ "granted": ["storage", { "network": { "hosts": ["api.example.com"] } }] }),
    )
    .await;
    assert_eq!(
        granted.status,
        200,
        "{}",
        String::from_utf8_lossy(&granted.body)
    );
    assert_eq!(
        granted.json()["addon"]["granted"].as_array().unwrap().len(),
        2
    );

    let one = get(&socket, "/addons/acme.byte").await;
    assert_eq!(one.status, 200);
    assert_eq!(one.json()["addon"]["enabled"], true);
    assert_eq!(get(&socket, "/addons/nope").await.status, 404);
    assert_eq!(get(&socket, "/addons/Not%20An%20Id").await.status, 400);

    // Every addon route but the files answers 401 without the token.
    assert_eq!(get_open(&socket, "/addons").await.status, 401);
    assert_eq!(get_open(&socket, "/addons/acme.byte").await.status, 401);
    assert_eq!(
        send(
            &socket,
            "POST",
            "/addons/acme.byte/fetch",
            Some(json!({"url": "https://api.example.com/"})),
            false
        )
        .await
        .status,
        401
    );

    let removed = call(&socket, "DELETE", "/addons/acme.byte", json!({})).await;
    assert_eq!(removed.status, 200);
    assert_eq!(get(&socket, "/addons/acme.byte").await.status, 404);
    assert_eq!(get(&socket, "/addons").await.json()["addons"], json!([]));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bundle_is_served_open_under_its_walls() {
    let (dir, socket, _stop) = boot().await;
    let src = tempfile::tempdir().unwrap();
    let from = folder(src.path(), "acme.byte");
    let imported = call(
        &socket,
        "POST",
        "/addons",
        json!({ "path": from.to_string_lossy(), "enabled": true }),
    )
    .await;
    assert_eq!(
        imported.status,
        200,
        "{}",
        String::from_utf8_lossy(&imported.body)
    );

    let page = get_open(&socket, "/addons/acme.byte/files/index.html").await;
    assert_eq!(page.status, 200);
    assert!(String::from_utf8_lossy(&page.body).contains("bisa-addon.js"));
    assert_eq!(
        page.header("content-type").as_deref(),
        Some("text/html; charset=utf-8")
    );
    assert_eq!(
        page.header("x-content-type-options").as_deref(),
        Some("nosniff")
    );
    assert_eq!(
        page.header("content-disposition").as_deref(),
        Some("inline")
    );
    assert_eq!(page.header("cache-control").as_deref(), Some("no-store"));
    assert_eq!(
        page.header("cross-origin-resource-policy").as_deref(),
        Some("cross-origin")
    );
    let csp = page
        .header("content-security-policy")
        .expect("a policy on every file");
    assert_eq!(csp, bisa_node::addons::ADDON_FILES_CSP);
    for wall in [
        "connect-src 'none'",
        "frame-src 'none'",
        "form-action 'none'",
        "base-uri 'none'",
        "sandbox allow-scripts",
    ] {
        assert!(csp.contains(wall), "{csp}");
    }

    // The library is the platform's, served at the root of every bundle.
    let sdk = get_open(&socket, "/addons/acme.byte/files/bisa-addon.js").await;
    assert_eq!(sdk.status, 200);
    assert_eq!(
        sdk.header("content-type").as_deref(),
        Some("text/javascript; charset=utf-8")
    );
    assert!(String::from_utf8_lossy(&sdk.body).contains("bisa:hello"));

    // A nested file, by its own type; the path never leaves the bundle.
    let png = get_open(&socket, "/addons/acme.byte/files/img/a.png").await;
    assert_eq!(png.status, 200);
    assert_eq!(png.header("content-type").as_deref(), Some("image/png"));
    for escape in [
        "/addons/acme.byte/files/../addon.json",
        "/addons/acme.byte/files/%2e%2e/addon.json",
        "/addons/acme.byte/files/img/../../addon.json",
        "/addons/acme.byte/files/.hidden",
        "/addons/acme.byte/files/nope.js",
        "/addons/nope/files/index.html",
        "/addons/Not%20An%20Id/files/index.html",
    ] {
        let a = get_open(&socket, escape).await;
        assert!(a.status == 404 || a.status == 400, "{escape}: {}", a.status);
        assert!(
            !String::from_utf8_lossy(&a.body).contains("acme.byte\"") || a.status == 404,
            "{escape}: a bare answer"
        );
    }

    // A file planted after the install with an extension no bundle may carry
    // is a download, never run.
    let planted = Paths::new(dir.path())
        .addon_files_dir(&bisa_core::AddonId::new("acme.byte").unwrap())
        .join("tool.exe");
    std::fs::write(&planted, b"MZ").unwrap();
    let exe = get_open(&socket, "/addons/acme.byte/files/tool.exe").await;
    assert_eq!(exe.status, 200);
    assert_eq!(
        exe.header("content-type").as_deref(),
        Some("application/octet-stream")
    );
    assert_eq!(
        exe.header("content-disposition").as_deref(),
        Some("attachment")
    );

    // The machine's switch closes the files too.
    let off = call(
        &socket,
        "PUT",
        "/settings/machine",
        json!({ "values": { "addons.enabled": false } }),
    )
    .await;
    assert!(
        off.status == 200 || off.status == 204,
        "{}: {}",
        off.status,
        String::from_utf8_lossy(&off.body)
    );
    assert_eq!(
        get_open(&socket, "/addons/acme.byte/files/index.html")
            .await
            .status,
        404
    );
    assert_eq!(
        get(&socket, "/addons").await.json()["addons_enabled"],
        json!(false)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_broker_refuses_and_a_built_in_installs_from_the_catalog() {
    let (_dir, socket, _stop) = boot().await;
    let installed = call(
        &socket,
        "POST",
        "/catalog/install",
        json!({ "kind": "addon", "slug": "weather" }),
    )
    .await;
    assert_eq!(
        installed.status,
        200,
        "{}",
        String::from_utf8_lossy(&installed.body)
    );
    assert_eq!(installed.json()["installed"]["addons"], json!(["weather"]));
    let listed = get(&socket, "/catalog?kind=addon").await;
    assert_eq!(listed.status, 200);
    let entries = listed.json()["entries"].as_array().unwrap().clone();
    assert_eq!(entries.len(), 13);
    let weather = entries.iter().find(|e| e["slug"] == "weather").unwrap();
    assert_eq!(weather["installed"], true);
    assert_eq!(weather["kind"], "addon");

    let one = get(&socket, "/addons/weather").await;
    assert_eq!(
        one.json()["addon"]["origin"],
        json!({ "catalog": { "slug": "weather" } })
    );
    assert_eq!(one.json()["addon"]["active"], true);

    let refuse = |url: &str| {
        let url = url.to_string();
        let socket = socket.clone();
        async move {
            let a = call(
                &socket,
                "POST",
                "/addons/weather/fetch",
                json!({ "url": url }),
            )
            .await;
            assert_eq!(a.status, 400, "{url}: {}", String::from_utf8_lossy(&a.body));
            a.json()["error"].as_str().unwrap_or_default().to_string()
        }
    };
    assert!(refuse("http://api.open-meteo.com/v1/forecast")
        .await
        .contains("https"));
    assert!(refuse("https://127.0.0.1:4477/health")
        .await
        .contains("this machine"));
    assert!(refuse("https://example.com/").await.contains("declare"));
    assert!(refuse("nonsense").await.contains("not a URL"));

    let ungranted = call(
        &socket,
        "PATCH",
        "/addons/weather",
        json!({ "granted": [] }),
    )
    .await;
    assert_eq!(ungranted.status, 200);
    assert!(refuse("https://api.open-meteo.com/v1/forecast")
        .await
        .contains("not granted"));

    // A person's own may not take a built-in's id.
    let src = tempfile::tempdir().unwrap();
    let from = folder(src.path(), "clock");
    let taken = call(
        &socket,
        "POST",
        "/addons",
        json!({ "path": from.to_string_lossy() }),
    )
    .await;
    assert_eq!(taken.status, 400);
    assert!(taken.json()["error"]
        .as_str()
        .unwrap()
        .contains("ships with the platform"));
}
