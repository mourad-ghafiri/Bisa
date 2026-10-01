//! Mobile development over HTTP (ide/19): every mobile route is 503 on a
//! node started without the tools; the switch refuses the person's routes
//! with 409 until it is on; the status, the devices, a boot and a capture
//! read the fake the test hands in; a frame is bytes that are never cached;
//! a Flutter checkout is recognised; the run line names the resolved
//! Flutter in the checkout.

use crate::node::Node;
use bisa_core::settings::Scope as SettingScope;
use bisa_engine::EngineConfig;
use bisa_mobile_development::fake::{emulator, png_fixture, simulator};
use bisa_mobile_development::{DeviceState, FakeMobileDevelopment, FlutterInfo, Toolchain};
use http_body_util::{BodyExt as _, Full};
use hyper::body::Bytes;
use hyper::Request;
use hyper_util::rt::TokioIo;
use serde_json::json;
use std::sync::Arc;
use tokio::net::UnixStream;

const TOKEN: &str = "test-token-0123456789abcdef";

fn fake() -> Arc<FakeMobileDevelopment> {
    Arc::new(
        FakeMobileDevelopment::new()
            .with_toolchain(Toolchain {
                flutter: FlutterInfo {
                    installed: true,
                    path: Some("/opt/flutter/bin/flutter".into()),
                    version: Some("3.24.3".into()),
                    channel: Some("stable".into()),
                    dart: Some("3.5.3".into()),
                },
                ..Toolchain::default()
            })
            .with_devices(vec![
                simulator("AAAA-1", "iPhone 16", DeviceState::Shutdown),
                emulator("Pixel_8", "Pixel 8", DeviceState::Shutdown),
            ])
            .with_screenshot(png_fixture(1170, 2532)),
    )
}

async fn node_with(tools: Option<Arc<FakeMobileDevelopment>>) -> Node {
    let node = Node::start_in(
        tempfile::tempdir().expect("tempdir"),
        EngineConfig {
            design_enabled: false,
            mobile_development: tools
                .map(|t| t as Arc<dyn bisa_mobile_development::MobileDevelopmentTools>),
            ..Default::default()
        },
    )
    .await;
    node
}

fn switch_on(node: &Node) {
    node.ws
        .set_setting(
            SettingScope::Machine,
            None,
            "mobile_development.enabled",
            json!(true),
        )
        .unwrap();
}

/// One GET answering the status, the content type, the cache header and
/// the bytes — for the routes that answer an image.
async fn get_bytes(
    socket: &std::path::Path,
    path: &str,
) -> (u16, Option<String>, Option<String>, Vec<u8>) {
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
    let header = |name: hyper::header::HeaderName| {
        resp.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };
    let content_type = header(hyper::header::CONTENT_TYPE);
    let cache = header(hyper::header::CACHE_CONTROL);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, content_type, cache, bytes.to_vec())
}

#[tokio::test(flavor = "multi_thread")]
async fn every_mobile_route_is_503_without_the_tools_and_409_while_off() {
    let node = node_with(None).await;
    let (code, v) = node.req("GET", "/mobile-development/status", None).await;
    assert_eq!(code, 503, "{v}");
    assert!(
        v["error"].as_str().unwrap().contains("not available"),
        "{v}"
    );
    // The switch is read before the tools: off says off.
    let (code, v) = node.req("GET", "/mobile-development/devices", None).await;
    assert_eq!(code, 409, "{v}");
    assert!(v["error"].as_str().unwrap().contains("turned off"), "{v}");
    switch_on(&node);
    for (method, path) in [
        ("GET", "/mobile-development/devices"),
        ("POST", "/mobile-development/devices/AAAA-1/boot"),
        ("POST", "/mobile-development/devices/AAAA-1/screenshot"),
    ] {
        let (code, v) = node.req(method, path, Some(json!({}))).await;
        assert_eq!(code, 503, "{method} {path}: {v}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_status_the_devices_a_boot_and_a_capture_read_the_machine() {
    let tools = fake();
    let node = node_with(Some(Arc::clone(&tools))).await;
    // The status is read before the switch is on: what to install comes first.
    let status = node.get("/mobile-development/status").await;
    assert_eq!(status["enabled"], json!(false));
    assert_eq!(status["platforms"], json!("both"));
    assert_eq!(status["toolchain"]["flutter"]["version"], json!("3.24.3"));
    assert!(status["checked_at"].as_u64().unwrap() > 0);
    let again = node
        .post("/mobile-development/status/check", json!({}))
        .await;
    assert!(again["checked_at"].as_u64().unwrap() >= status["checked_at"].as_u64().unwrap());
    assert_eq!(
        tools.calls().iter().filter(|c| *c == "toolchain").count(),
        2,
        "once at first read, once on check again"
    );
    switch_on(&node);
    let listed = node.get("/mobile-development/devices").await;
    let devices = listed["devices"].as_array().unwrap();
    assert_eq!(devices.len(), 2, "{listed}");
    assert_eq!(devices[0]["id"], json!("AAAA-1"));
    assert_eq!(devices[0]["state"], json!("shutdown"));
    let booted = node
        .post("/mobile-development/devices/AAAA-1/boot", json!({}))
        .await;
    assert_eq!(booted["state"], json!("booted"), "{booted}");
    let (code, v) = node
        .req(
            "POST",
            "/mobile-development/devices/ZZZZ/boot",
            Some(json!({})),
        )
        .await;
    assert_eq!(code, 404, "{v}");
    // A capture is an attachment the node then serves as an image.
    let shot = node
        .post("/mobile-development/devices/AAAA-1/screenshot", json!({}))
        .await;
    assert_eq!(shot["width"], json!(1170));
    let path = std::path::PathBuf::from(shot["path"].as_str().unwrap());
    assert!(path.is_file(), "{}", path.display());
    let sha = shot["attachment"]["sha256"].as_str().unwrap().to_string();
    let (code, content_type, _, bytes) =
        get_bytes(node.socket(), &format!("/attachments/{sha}?as=image")).await;
    assert_eq!(code, 200);
    assert_eq!(content_type.as_deref(), Some("image/png"));
    assert_eq!(bytes, png_fixture(1170, 2532));
    // A simulator made answers 201 and is then listed.
    let (code, made) = node
        .req(
            "POST",
            "/mobile-development/simulators",
            Some(json!({"name": "Test iPhone", "devicetype": "com.apple.CoreSimulator.SimDeviceType.iPhone-16", "runtime": "com.apple.CoreSimulator.SimRuntime.iOS-18-2"})),
        )
        .await;
    assert_eq!(code, 201, "{made}");
    assert_eq!(made["name"], json!("Test iPhone"));
    assert_eq!(
        node.get("/mobile-development/devices").await["devices"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    // A platform that is off hides its devices and refuses its boot.
    node.ws
        .set_setting(
            SettingScope::Machine,
            None,
            "mobile_development.platforms",
            json!("ios"),
        )
        .unwrap();
    let listed = node.get("/mobile-development/devices").await;
    assert!(listed["devices"]
        .as_array()
        .unwrap()
        .iter()
        .all(|d| d["platform"] == json!("ios")));
    let (code, v) = node
        .req(
            "POST",
            "/mobile-development/devices/Pixel_8/boot",
            Some(json!({})),
        )
        .await;
    assert_eq!(code, 409, "{v}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_frame_is_image_bytes_that_are_never_cached() {
    let node = node_with(Some(fake())).await;
    switch_on(&node);
    node.post("/mobile-development/devices/AAAA-1/boot", json!({}))
        .await;
    let (code, content_type, cache, bytes) = get_bytes(
        node.socket(),
        "/mobile-development/devices/AAAA-1/frame?format=png",
    )
    .await;
    assert_eq!(code, 200);
    assert_eq!(
        content_type.as_deref(),
        Some("image/png"),
        "the type the bytes say"
    );
    assert_eq!(cache.as_deref(), Some("no-store"));
    assert_eq!(bytes, png_fixture(1170, 2532));
    let (code, _, _, _) = get_bytes(
        node.socket(),
        "/mobile-development/devices/AAAA-1/frame?format=gif",
    )
    .await;
    assert_eq!(code, 400, "png or jpeg");
    let (code, v) = node
        .req(
            "POST",
            "/mobile-development/devices/AAAA-1/show",
            Some(json!({})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_flutter_checkout_is_recognised_and_its_run_line_is_the_flutter_in_the_checkout() {
    let node = node_with(Some(fake())).await;
    let made = node
        .post("/projects", json!({"kind": "new", "slug": "shop"}))
        .await;
    let pid = made["project"]["id"].as_str().unwrap().to_string();
    let project = node.ws.get_project(pid.parse().unwrap()).unwrap();
    let root = node.ws.project_root_path(&project);
    let facts = node
        .get(&format!("/workstreams/{pid}/mobile-development"))
        .await;
    assert_eq!(
        facts,
        json!({"flutter": false, "ios": false, "android": false})
    );
    std::fs::write(
        root.join("pubspec.yaml"),
        "name: shop\ndependencies:\n  flutter:\n    sdk: flutter\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("ios")).unwrap();
    let facts = node
        .get(&format!("/workstreams/{pid}/mobile-development"))
        .await;
    assert_eq!(
        facts,
        json!({"flutter": true, "ios": true, "android": false})
    );
    // Off: no run line.
    let (code, v) = node
        .req(
            "GET",
            &format!("/workstreams/{pid}/mobile-development/run-command?device=AAAA-1"),
            None,
        )
        .await;
    assert_eq!(code, 409, "{v}");
    switch_on(&node);
    let run = node
        .get(&format!(
            "/workstreams/{pid}/mobile-development/run-command?device=AAAA-1"
        ))
        .await;
    assert_eq!(
        run["command"],
        json!("/opt/flutter/bin/flutter run -d AAAA-1")
    );
    assert_eq!(run["cwd"], json!(root.display().to_string()));
    assert_eq!(run["device"], json!("AAAA-1"));
    let (code, v) = node
        .req(
            "GET",
            &format!("/workstreams/{pid}/mobile-development/run-command?device="),
            None,
        )
        .await;
    assert_eq!(code, 400, "{v}");
}
