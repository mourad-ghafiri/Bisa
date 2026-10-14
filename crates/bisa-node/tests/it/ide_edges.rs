//! The IDE's file, search and served-folder routes at their edges over
//! HTTP: a case word that is none, globs that narrow a stream, a search
//! whose root has no folder, a reader that leaves mid-stream, a scope that
//! is none, a file too large to render and one nobody can open, a batch
//! delete the disk halts, a served folder whose checkout is not on disk or
//! is no folder, and what a server resolves for a stranger, an artifact
//! and the checkout root itself.

use crate::ide::{boot, http_get, raw_body, raw_bytes, request, TOKEN};
use http_body_util::{BodyExt as _, Full};
use hyper::body::Bytes;
use hyper::Request;
use hyper_util::rt::TokioIo;
use serde_json::{json, Value};
use tokio::net::UnixStream;

/// A language server from a few lines of shell: `initialize`, a hover and
/// the shutdown answered, every other frame read and let be.
#[cfg(unix)]
fn a_server_that_answers(dir: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("answering-server");
    std::fs::write(
        &path,
        r#"#!/bin/sh
send() {
  printf 'Content-Length: %s\r\n\r\n%s' "${#1}" "$1"
}
while IFS= read -r line; do
  case "$line" in
    Content-Length:*) ;;
    *) continue ;;
  esac
  len=$(printf '%s' "${line#Content-Length: }" | tr -d '\r')
  IFS= read -r _blank
  body=$(dd bs=1 count="$len" 2>/dev/null)
  id=${body#*\"id\":}
  id=${id%%,*}
  case "$body" in
    *'"method":"initialize"'*)
      send '{"jsonrpc":"2.0","id":'"$id"',"result":{"capabilities":{"hoverProvider":true}}}' ;;
    *'"method":"textDocument/hover"'*)
      uri=${body#*\"uri\":\"}
      uri=${uri%%\"*}
      send '{"jsonrpc":"2.0","id":'"$id"',"result":{"contents":{"kind":"plaintext","value":"hovered"},"uri":"'"$uri"'"}}' ;;
    *'"method":"shutdown"'*)
      send '{"jsonrpc":"2.0","id":'"$id"',"result":null}' ;;
    *'"method":"exit"'*) exit 0 ;;
  esac
done
"#,
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// A document opened over HTTP is followed by the server this machine
/// names for its language, a change reaches it, a proxied request is
/// answered with its URIs rewritten, and the close is taken.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_proxied_request_is_answered_over_http_by_the_server_this_machine_names() {
    let (dir, socket, pid, root, _stop) = boot().await;
    let server = a_server_that_answers(dir.path());
    std::fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
    let (status, v) = request(
        &socket,
        "PUT",
        "/settings/machine",
        Some(json!({"values": {"lsp.servers": [{"language": "rust", "command": server.display().to_string()}]}})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let lsp = format!("/ide/lsp/workstream/{pid}");
    let (status, v) = request(
        &socket,
        "POST",
        &format!("{lsp}/open"),
        Some(json!({"path": "main.rs", "text": "fn main() {}\n"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["following"], json!(true), "{v}");
    let (status, v) = request(
        &socket,
        "POST",
        &format!("{lsp}/change"),
        Some(json!({"path": "main.rs", "text": "fn main() { let x = 1; }\n"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (status, v) = request(
        &socket,
        "POST",
        &format!("{lsp}/request"),
        Some(json!({"path": "main.rs", "method": "textDocument/hover", "params": {"textDocument": {"uri": "main.rs"}, "position": {"line": 0, "character": 0}}})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["method"], json!("textDocument/hover"));
    assert_eq!(v["result"]["contents"]["value"], json!("hovered"), "{v}");
    assert_eq!(v["result"]["uri"], json!("main.rs"), "rewritten back: {v}");
    let (status, v) = request(
        &socket,
        "POST",
        &format!("{lsp}/close"),
        Some(json!({"path": "main.rs"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (status, v) = request(&socket, "GET", &format!("{lsp}/status"), None).await;
    assert_eq!(status, 200, "{v}");
    let rust = v["servers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["language"] == "rust")
        .cloned()
        .unwrap();
    assert_eq!(rust["state"]["state"], json!("running"), "{rust}");
    assert_eq!(rust["documents"], json!(0), "{rust}");
}

/// The frames of a search stream, as JSON.
fn frames_of(body: &str) -> Vec<Value> {
    body.lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .map(|d| serde_json::from_str(d.trim()).unwrap())
        .collect()
}

/// A case word the route does not know is refused before the stream opens;
/// include and exclude globs narrow what is walked.
#[tokio::test(flavor = "multi_thread")]
async fn a_case_word_that_is_none_is_refused_and_globs_narrow_the_stream() {
    let (_dir, socket, pid, root, _stop) = boot().await;
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/a.rs"), "needle\n").unwrap();
    std::fs::write(root.join("src/b.rs"), "needle\n").unwrap();
    std::fs::write(root.join("notes.md"), "needle\n").unwrap();
    let (status, body) = raw_body(
        &socket,
        "GET",
        &format!("/ide/search/workstream/{pid}?q=needle&case=loud"),
    )
    .await;
    assert_eq!(status, 400, "{body}");
    assert!(body.contains("loud"), "the word is named: {body}");

    let (status, body) = raw_body(
        &socket,
        "GET",
        &format!("/ide/search/workstream/{pid}?q=needle&include=src/*.rs,%20&exclude=src/b.rs"),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let frames = frames_of(&body);
    let hits: Vec<&str> = frames
        .iter()
        .filter(|f| f["type"] == json!("hit"))
        .map(|f| f["hit"]["path"].as_str().unwrap())
        .collect();
    assert_eq!(hits, vec!["src/a.rs"], "{body}");
}

/// A search whose root has no folder ends the stream with an error frame;
/// a reader that leaves while the walk is still going ends the walk, and
/// the node answers the next search as before.
#[tokio::test(flavor = "multi_thread")]
async fn a_search_without_a_folder_says_so_on_the_stream_and_a_reader_that_leaves_ends_the_walk() {
    let (_dir, socket, pid, root, _stop) = boot().await;
    std::fs::rename(&root, root.with_extension("aside")).unwrap();
    let (status, body) = raw_body(
        &socket,
        "GET",
        &format!("/ide/search/workstream/{pid}?q=needle"),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let frames = frames_of(&body);
    assert_eq!(frames.len(), 1, "{body}");
    assert_eq!(frames[0]["type"], json!("error"), "{body}");
    assert!(
        frames[0]["error"]
            .as_str()
            .unwrap_or_default()
            .contains(&pid),
        "the root is named: {body}"
    );

    // Thousands of hits, and a reader that reads one chunk and leaves.
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("many.txt"), "needle here\n".repeat(20_000)).unwrap();
    let stream = UnixStream::connect(&socket).await.expect("connect");
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .expect("handshake");
    let conn = tokio::spawn(conn);
    let req = Request::builder()
        .method("GET")
        .uri(format!("/ide/search/workstream/{pid}?q=needle&limit=20000"))
        .header(hyper::header::HOST, "localhost")
        .header(hyper::header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .body(Full::new(Bytes::new()))
        .unwrap();
    let mut resp = sender.send_request(req).await.expect("request");
    assert_eq!(resp.status().as_u16(), 200);
    let first = resp.body_mut().frame().await.expect("a first frame");
    assert!(first.is_ok());
    drop(resp);
    drop(sender);
    conn.abort();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let (status, body) = raw_body(
        &socket,
        "GET",
        &format!("/ide/search/workstream/{pid}?q=needle&limit=3"),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let frames = frames_of(&body);
    assert_eq!(
        frames.last().map(|f| f["type"].clone()),
        Some(json!("done")),
        "the node searches as before: {body}"
    );
}

/// A scope the file routes do not know is refused by name, with the ones
/// that work.
#[tokio::test(flavor = "multi_thread")]
async fn an_ide_route_with_a_scope_that_is_none_is_refused_by_name() {
    let (_dir, socket, _pid, _root, _stop) = boot().await;
    let (status, v) = request(&socket, "GET", "/ide/file/agent/x?path=a.txt", None).await;
    assert_eq!(status, 400, "{v}");
    let said = v["error"].as_str().unwrap_or_default();
    assert!(said.contains("agent") && said.contains("workstream"), "{v}");
    let (status, v) = request(&socket, "GET", "/ide/lsp/agent/x/status", None).await;
    assert_eq!(status, 400, "{v}");
}

/// A file over the render bound is refused with its size and the bound;
/// one nobody can open is said as not found.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_file_too_large_to_render_is_refused_with_its_size_and_one_nobody_can_open_is_said() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, socket, pid, root, _stop) = boot().await;
    // A sparse file: the size without the bytes.
    let huge = std::fs::File::create(root.join("huge.bin")).unwrap();
    let size = bisa_engine::ide::files::RAW_REFUSE_BYTES + 1;
    huge.set_len(size).unwrap();
    drop(huge);
    let (status, _, bytes) =
        raw_bytes(&socket, &format!("/ide/raw/workstream/{pid}?path=huge.bin")).await;
    assert_eq!(status, 413);
    let said: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(said["size"], json!(size), "{said}");
    assert_eq!(
        said["limit"],
        json!(bisa_engine::ide::files::RAW_REFUSE_BYTES),
        "{said}"
    );

    std::fs::write(root.join("sealed.png"), b"\x89PNG\r\n\x1a\n").unwrap();
    std::fs::set_permissions(
        root.join("sealed.png"),
        std::fs::Permissions::from_mode(0o000),
    )
    .unwrap();
    let (status, _, _) = raw_bytes(
        &socket,
        &format!("/ide/raw/workstream/{pid}?path=sealed.png"),
    )
    .await;
    std::fs::set_permissions(
        root.join("sealed.png"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert_eq!(status, 404, "a file that cannot be opened");
}

/// A batch delete the disk halts names the entry that did not go and lists
/// what went before it; the answer is not `ok`.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_batch_delete_halted_by_the_disk_names_the_entry_and_what_went_before_it() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, socket, pid, root, _stop) = boot().await;
    let (status, v) = request(
        &socket,
        "PUT",
        "/settings/machine",
        Some(json!({"values": {"editor.delete.trash": false}})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    std::fs::write(root.join("first.txt"), "x").unwrap();
    std::fs::create_dir_all(root.join("locked")).unwrap();
    std::fs::write(root.join("locked/held.txt"), "x").unwrap();
    std::fs::set_permissions(root.join("locked"), std::fs::Permissions::from_mode(0o555)).unwrap();
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/ide/files/workstream/{pid}/delete"),
        Some(json!({"entries": [{"path": "first.txt"}, {"path": "locked/held.txt"}]})),
    )
    .await;
    std::fs::set_permissions(root.join("locked"), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["ok"], json!(false), "{v}");
    assert_eq!(v["failed"]["path"], json!("locked/held.txt"), "{v}");
    assert!(
        !v["failed"]["reason"]
            .as_str()
            .unwrap_or_default()
            .is_empty(),
        "{v}"
    );
    assert_eq!(v["deleted"].as_array().map(|d| d.len()), Some(1), "{v}");
    assert!(!root.join("first.txt").exists());
    assert!(root.join("locked/held.txt").is_file(), "it stayed");
}

/// A served folder needs its checkout on disk, as a folder; a server
/// nobody started resolves to a refusal, an artifact's page resolves to
/// nothing — it is nobody's file to edit — and the checkout served whole
/// resolves a page to its plain path.
#[tokio::test(flavor = "multi_thread")]
async fn a_served_folder_needs_a_checkout_on_disk_and_resolves_by_its_owner() {
    let (_dir, socket, pid, root, _stop) = boot().await;
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/servers/nobody/resolve?path=/"),
        None,
    )
    .await;
    assert_eq!(status, 404, "{v}");

    // The checkout itself, served whole: a page resolves to its plain path.
    std::fs::write(root.join("index.html"), "<h1>root</h1>\n").unwrap();
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/servers"),
        Some(json!({"folder": ""})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let whole = v["id"].as_str().unwrap().to_string();
    let port = v["port"].as_u64().unwrap() as u16;
    let (code, body) = http_get(port, "/").await;
    assert_eq!(code, 200, "{body}");
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/servers/{whole}/resolve?path=/"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["path"], json!("index.html"), "{v}");

    // An artifact's page: served on a port of its own, resolving to no file.
    let (status, file) = request(
        &socket,
        "POST",
        "/attachments?name=report.html&mime=text/html",
        Some(json!("<h1>a report</h1>")),
    )
    .await;
    assert_eq!(status, 201, "{file}");
    let sha = file["sha256"].as_str().expect("the digest").to_string();
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/artifacts/{sha}/serve"),
        Some(json!({"name": "report.html"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let artifact = v["id"].as_str().unwrap().to_string();
    assert_eq!(v["owner"]["kind"], json!("artifact"), "{v}");
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/servers/{artifact}/resolve?path=/report.html"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["path"], Value::Null, "nobody's file to edit: {v}");

    // A project whose folder is not on disk cannot serve; one whose folder
    // is a file is no folder.
    let (status, made) = request(
        &socket,
        "POST",
        "/projects",
        Some(json!({"kind": "new", "slug": "shed"})),
    )
    .await;
    assert_eq!(status, 200, "{made}");
    let shed = made["project"]["id"]
        .as_str()
        .or(made["id"].as_str())
        .expect("the project")
        .to_string();
    let shed_root = made["path"]
        .as_str()
        .map(std::path::PathBuf::from)
        .expect("its tree");
    std::fs::rename(&shed_root, shed_root.with_extension("aside")).unwrap();
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{shed}/servers"),
        Some(json!({"folder": ""})),
    )
    .await;
    assert_eq!(status, 400, "not on disk: {v}");
    std::fs::write(&shed_root, "not a folder").unwrap();
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{shed}/servers"),
        Some(json!({"folder": ""})),
    )
    .await;
    assert_eq!(status, 400, "no folder: {v}");
}
