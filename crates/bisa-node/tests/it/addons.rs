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
    send_with(socket, method, path, body, token, &[]).await
}

/// `send`, with headers of the caller's own — an `Origin`, a `Range`.
async fn send_with(
    socket: &std::path::Path,
    method: &str,
    path: &str,
    body: Option<Value>,
    token: bool,
    headers: &[(&str, &str)],
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
    for (name, value) in headers {
        builder = builder.header(*name, *value);
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
    assert_eq!(offered.len(), 18);
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
    // Both fields in one patch are judged whole: a grant never declared
    // refuses the patch, and the switch beside it stays where it was.
    let half = call(
        &socket,
        "PATCH",
        "/addons/acme.byte",
        json!({ "enabled": false, "granted": ["notify"] }),
    )
    .await;
    assert_eq!(half.status, 400, "{}", String::from_utf8_lossy(&half.body));
    assert_eq!(
        get(&socket, "/addons/acme.byte").await.json()["addon"]["enabled"],
        true,
        "the switch beside a refused grant did not move"
    );
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
    // A folder of the bundle is no file to serve.
    assert_eq!(
        get_open(&socket, "/addons/acme.byte/files/img")
            .await
            .status,
        404
    );
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
    assert_eq!(entries.len(), 18);
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

/// A bundle that declares `media` and whose person granted it wears a
/// `media-src` widened to the hosts of the grant; declared but not
/// granted, it wears the constant like every other bundle.
#[tokio::test(flavor = "multi_thread")]
async fn a_media_addon_s_files_wear_a_wider_media_src_and_an_ordinary_one_the_constant() {
    let (dir, socket, _stop) = boot().await;
    let src = tempfile::tempdir().unwrap();
    let from = src.path().join("player");
    std::fs::create_dir_all(&from).unwrap();
    std::fs::write(
        from.join("addon.json"),
        r#"{"id":"acme.player","name":"Player","description":"plays","version":"1.0.0","license":"MIT",
            "permissions":["storage",{"media":{"hosts":["*"],"youtube":true}}]}"#,
    )
    .unwrap();
    std::fs::write(
        from.join("index.html"),
        "<!doctype html><script src=\"bisa-addon.js\"></script><script src=\"main.js\"></script>",
    )
    .unwrap();
    std::fs::write(from.join("main.js"), "// x").unwrap();

    // Declared, not granted: the constant.
    let imported = call(
        &socket,
        "POST",
        "/addons",
        json!({ "path": from.to_string_lossy(), "granted": ["storage"], "enabled": true }),
    )
    .await;
    assert_eq!(
        imported.status,
        200,
        "{}",
        String::from_utf8_lossy(&imported.body)
    );
    let page = get_open(&socket, "/addons/acme.player/files/index.html").await;
    assert_eq!(page.status, 200);
    assert_eq!(
        page.header("content-security-policy").as_deref(),
        Some(bisa_node::addons::ADDON_FILES_CSP)
    );

    // Granted: `media-src` reaches every https host; the other walls stand.
    let granted = call(
        &socket,
        "PATCH",
        "/addons/acme.player",
        json!({ "granted": ["storage", { "media": { "hosts": ["*"], "youtube": true } }] }),
    )
    .await;
    assert_eq!(
        granted.status,
        200,
        "{}",
        String::from_utf8_lossy(&granted.body)
    );
    let page = get_open(&socket, "/addons/acme.player/files/index.html").await;
    assert_eq!(page.status, 200);
    let csp = page
        .header("content-security-policy")
        .expect("a policy on every file");
    assert!(
        csp.contains("media-src 'self' data: blob: https:;"),
        "{csp}"
    );
    for wall in [
        "connect-src 'none'",
        "frame-src 'none'",
        "sandbox allow-scripts",
    ] {
        assert!(csp.contains(wall), "{wall}: {csp}");
    }
    drop(dir);
}

/// A folder declaring the `media` permission given, and nothing else of note.
fn media_folder(root: &std::path::Path, id: &str, media: &str) -> PathBuf {
    let dir = root.join(id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("addon.json"),
        format!(
            r#"{{"id":"{id}","name":"Player","description":"plays","version":"1.0.0","license":"MIT",
                "permissions":["storage",{{"media":{media}}}]}}"#
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("index.html"),
        "<!doctype html><script src=\"bisa-addon.js\"></script><script src=\"main.js\"></script>",
    )
    .unwrap();
    std::fs::write(dir.join("main.js"), "// x").unwrap();
    dir
}

/// The player page opens, token-less, for an active addon granted `media`
/// with `youtube`; a declaration alone, a grant without the player, a
/// disabled addon, an unknown one, the machine's switch off, and an id of
/// any other shape each meet the files' bare 404 — or, for a shape the
/// exemption does not know, the gate's 401.
#[tokio::test(flavor = "multi_thread")]
async fn the_youtube_player_page_opens_for_a_granted_addon_alone() {
    let (dir, socket, _stop) = boot().await;
    let src = tempfile::tempdir().unwrap();
    let with_player = media_folder(
        src.path(),
        "acme.player",
        r#"{"hosts":["*"],"youtube":true}"#,
    );
    let hosts_only = media_folder(src.path(), "acme.links", r#"{"hosts":["cdn.example"]}"#);
    for (from, granted) in [(&with_player, json!(["storage"])), (&hosts_only, json!([]))] {
        let imported = call(
            &socket,
            "POST",
            "/addons",
            json!({ "path": from.to_string_lossy(), "granted": granted, "enabled": true }),
        )
        .await;
        assert_eq!(
            imported.status,
            200,
            "{}",
            String::from_utf8_lossy(&imported.body)
        );
    }
    let page_path = "/addons/acme.player/player/youtube/dQw4w9WgXcQ";

    // Declared, not granted: closed.
    let page = get_open(&socket, page_path).await;
    assert_eq!(page.status, 404, "{}", String::from_utf8_lossy(&page.body));
    assert!(
        !String::from_utf8_lossy(&page.body).contains("youtube"),
        "a bare answer"
    );

    // Granted: the page, under its own headers, the id in its one hole.
    let granted = call(
        &socket,
        "PATCH",
        "/addons/acme.player",
        json!({ "granted": ["storage", { "media": { "hosts": ["*"], "youtube": true } }] }),
    )
    .await;
    assert_eq!(
        granted.status,
        200,
        "{}",
        String::from_utf8_lossy(&granted.body)
    );
    let page = get_open(&socket, page_path).await;
    assert_eq!(page.status, 200, "{}", String::from_utf8_lossy(&page.body));
    let body = String::from_utf8_lossy(&page.body);
    assert!(body.contains("data-video=\"dQw4w9WgXcQ\""), "{body}");
    assert!(
        body.contains("https://www.youtube.com/iframe_api"),
        "{body}"
    );
    assert!(!body.contains("{{VIDEO}}"), "the hole is filled");
    assert_eq!(
        page.header("content-type").as_deref(),
        Some("text/html; charset=utf-8")
    );
    assert_eq!(
        page.header("x-content-type-options").as_deref(),
        Some("nosniff")
    );
    assert_eq!(page.header("cache-control").as_deref(), Some("no-store"));
    assert_eq!(
        page.header("referrer-policy").as_deref(),
        Some("strict-origin-when-cross-origin")
    );
    assert_eq!(
        page.header("content-security-policy").as_deref(),
        Some(bisa_node::addons::PLAYER_PAGE_CSP)
    );

    // An id of another shape: the files' 404 with the token, the gate's 401
    // without — the exemption is the route's exact shape.
    let short = "/addons/acme.player/player/youtube/dQw4w9WgXc";
    assert_eq!(get(&socket, short).await.status, 404);
    assert_eq!(get_open(&socket, short).await.status, 401);
    let odd = "/addons/acme.player/player/youtube/dQw4w9WgXc!";
    assert_eq!(get(&socket, odd).await.status, 404);

    // A grant of hosts alone has no player.
    let links = call(
        &socket,
        "PATCH",
        "/addons/acme.links",
        json!({ "granted": ["storage", { "media": { "hosts": ["cdn.example"] } }] }),
    )
    .await;
    assert_eq!(
        links.status,
        200,
        "{}",
        String::from_utf8_lossy(&links.body)
    );
    assert_eq!(
        get_open(&socket, "/addons/acme.links/player/youtube/dQw4w9WgXcQ")
            .await
            .status,
        404
    );
    // Its files wear `media-src` for its host alone.
    let csp = get_open(&socket, "/addons/acme.links/files/index.html")
        .await
        .header("content-security-policy")
        .expect("a policy");
    assert!(
        csp.contains("media-src 'self' data: blob: https://cdn.example;"),
        "{csp}"
    );

    // An addon nobody installed.
    assert_eq!(
        get_open(&socket, "/addons/acme.nope/player/youtube/dQw4w9WgXcQ")
            .await
            .status,
        404
    );

    // Switched off: closed; on again: open.
    let off = call(
        &socket,
        "PATCH",
        "/addons/acme.player",
        json!({ "enabled": false }),
    )
    .await;
    assert_eq!(off.status, 200, "{}", String::from_utf8_lossy(&off.body));
    assert_eq!(get_open(&socket, page_path).await.status, 404);
    let on = call(
        &socket,
        "PATCH",
        "/addons/acme.player",
        json!({ "enabled": true }),
    )
    .await;
    assert_eq!(on.status, 200, "{}", String::from_utf8_lossy(&on.body));
    assert_eq!(get_open(&socket, page_path).await.status, 200);

    // The machine's switch closes the page as it closes the files.
    let machine_off = call(
        &socket,
        "PUT",
        "/settings/machine",
        json!({ "values": { "addons.enabled": false } }),
    )
    .await;
    assert!(
        machine_off.status == 200 || machine_off.status == 204,
        "{}: {}",
        machine_off.status,
        String::from_utf8_lossy(&machine_off.body)
    );
    assert_eq!(get_open(&socket, page_path).await.status, 404);
    drop(dir);
}

/// An addon imported and switched on, for the tests of what serving it answers.
async fn byte_on(socket: &std::path::Path, src: &std::path::Path) {
    let from = folder(src, "acme.byte");
    std::fs::write(from.join("img/a.png"), b"0123456789").unwrap();
    let imported = call(
        socket,
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
}

#[tokio::test]
async fn a_bundles_files_are_read_across_origins_by_its_own_frame_alone() {
    // The files route answers without the token, and the node's CORS is open to every origin: a
    // page on any site that found the port could read an active addon's files. The sandboxed
    // frame that runs a bundle has an opaque origin — it says `null` — and needs the answer to
    // load the bundle's own fonts; nothing with a real origin does.
    let (_dir, socket, _stop) = boot().await;
    let src = tempfile::tempdir().unwrap();
    byte_on(&socket, src.path()).await;
    let file = "/addons/acme.byte/files/main.js";
    let get_as = |origin: Option<&'static str>| {
        let socket = socket.clone();
        async move {
            let headers: Vec<(&str, &str)> = origin.iter().map(|o| ("origin", *o)).collect();
            send_with(&socket, "GET", file, None, false, &headers).await
        }
    };
    let frame = get_as(Some("null")).await;
    assert_eq!(frame.status, 200);
    assert_eq!(
        frame.header("access-control-allow-origin").as_deref(),
        Some("null")
    );
    assert!(frame
        .header("vary")
        .unwrap_or_default()
        .to_lowercase()
        .contains("origin"));
    for site in [
        "https://example.com",
        "http://localhost:5173",
        "tauri://localhost",
    ] {
        let a = get_as(Some(site)).await;
        assert_eq!(a.status, 200, "{site}: the file is still a file");
        assert_eq!(
            a.header("access-control-allow-origin"),
            None,
            "{site} may not read it"
        );
    }
    assert_eq!(
        get_as(None).await.header("access-control-allow-origin"),
        None
    );
    // The player's page is a document a view loads, never read across origins.
    let player = send_with(
        &socket,
        "GET",
        "/addons/acme.byte/player/youtube/dQw4w9WgXcQ",
        None,
        false,
        &[("origin", "null")],
    )
    .await;
    assert_eq!(player.header("access-control-allow-origin"), None);
    // The rest of the API keeps its open CORS: the token is what guards it.
    let api = send_with(
        &socket,
        "GET",
        "/addons",
        None,
        true,
        &[("origin", "http://localhost:5173")],
    )
    .await;
    assert_eq!(
        api.header("access-control-allow-origin").as_deref(),
        Some("*")
    );
}

#[tokio::test]
async fn a_bundle_file_answers_head_and_a_range_of_its_bytes() {
    let (_dir, socket, _stop) = boot().await;
    let src = tempfile::tempdir().unwrap();
    byte_on(&socket, src.path()).await;
    let png = "/addons/acme.byte/files/img/a.png";
    // HEAD is GET without the body — the same walls, the same open door, nothing else.
    let head = send(&socket, "HEAD", png, None, false).await;
    assert_eq!(head.status, 200);
    assert!(head.body.is_empty());
    assert_eq!(head.header("content-type").as_deref(), Some("image/png"));
    assert!(head.header("content-security-policy").is_some());
    assert_eq!(
        send(&socket, "HEAD", "/addons", None, false).await.status,
        401
    );
    // A whole file says it can be asked for in parts.
    let whole = get_open(&socket, png).await;
    assert_eq!(whole.body, b"0123456789");
    assert_eq!(whole.header("accept-ranges").as_deref(), Some("bytes"));
    // A media element asks for a range, and seeks by asking for another.
    for (range, status, body, said) in [
        ("bytes=0-3", 206, &b"0123"[..], Some("bytes 0-3/10")),
        ("bytes=4-", 206, &b"456789"[..], Some("bytes 4-9/10")),
        ("bytes=-3", 206, &b"789"[..], Some("bytes 7-9/10")),
        ("bytes=8-99", 206, &b"89"[..], Some("bytes 8-9/10")),
        ("bytes=10-", 416, &b""[..], Some("bytes */10")),
        ("bytes=5-2", 416, &b""[..], Some("bytes */10")),
        ("bytes=-0", 416, &b""[..], Some("bytes */10")),
        // What is not one plain range is answered whole, as a server that knows no ranges would.
        ("bytes=0-1,4-5", 200, &b"0123456789"[..], None),
        ("lines=0-3", 200, &b"0123456789"[..], None),
        ("bytes=a-b", 200, &b"0123456789"[..], None),
    ] {
        let a = send_with(&socket, "GET", png, None, false, &[("range", range)]).await;
        assert_eq!(a.status, status, "{range}");
        assert_eq!(a.body, body, "{range}");
        assert_eq!(a.header("content-range").as_deref(), said, "{range}");
        assert!(
            a.header("content-security-policy").is_some(),
            "{range}: the walls stand on every answer"
        );
    }
}

#[tokio::test]
async fn grants_are_given_and_taken_by_word_over_the_wire() {
    let (_dir, socket, _stop) = boot().await;
    let src = tempfile::tempdir().unwrap();
    byte_on(&socket, src.path()).await;
    let patch = |body: Value| {
        let socket = socket.clone();
        async move { call(&socket, "PATCH", "/addons/acme.byte", body).await }
    };
    let a = patch(json!({ "grant": ["storage"] })).await;
    assert_eq!(a.status, 200, "{}", String::from_utf8_lossy(&a.body));
    assert_eq!(a.json()["addon"]["granted"], json!(["storage"]));
    let a = patch(json!({ "grant": ["network"], "enabled": false })).await;
    assert_eq!(
        a.json()["addon"]["granted"],
        json!(["storage", { "network": { "hosts": ["api.example.com"] } }]),
        "beside what was granted, never over it"
    );
    assert_eq!(a.json()["addon"]["enabled"], false);
    let a = patch(json!({ "revoke": ["storage"] })).await;
    assert_eq!(
        a.json()["addon"]["granted"],
        json!([{ "network": { "hosts": ["api.example.com"] } }])
    );
    // A word never declared refuses the patch; the whole list and words together are two ways
    // of saying one thing, and are refused.
    assert_eq!(patch(json!({ "grant": ["notify"] })).await.status, 400);
    let both = patch(json!({ "granted": [], "grant": ["storage"] })).await;
    assert_eq!(both.status, 400, "{}", String::from_utf8_lossy(&both.body));
    assert_eq!(
        get(&socket, "/addons/acme.byte").await.json()["addon"]["granted"],
        json!([{ "network": { "hosts": ["api.example.com"] } }])
    );
    // Unknown ids answer not found on every verb.
    assert_eq!(
        call(
            &socket,
            "PATCH",
            "/addons/acme.nope",
            json!({ "enabled": true })
        )
        .await
        .status,
        404
    );
    assert_eq!(
        send(&socket, "DELETE", "/addons/acme.nope", None, true)
            .await
            .status,
        404
    );
}
