//! The IDE's file routes over HTTP: the read cap, the 409 that carries the
//! current text, and the explorer mutations.

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
    PathBuf,
    tokio::sync::oneshot::Sender<()>,
) {
    boot_with(None).await
}

/// A `git` that cannot see the developer's global or system config, so the
/// node under test observes the repository and nothing else. Per handle,
/// never the process environment: tests run in parallel.
fn isolated_git(dir: &std::path::Path) -> bisa_vcs::Git {
    bisa_vcs::Git::new()
        .with_env("GIT_CONFIG_GLOBAL", dir.join("no-global.gitconfig"))
        .with_env("GIT_CONFIG_NOSYSTEM", "1")
}

/// [`boot`], with the engine's `git` chosen — `None` is the process's.
async fn boot_with(
    git: Option<fn(&std::path::Path) -> bisa_vcs::Git>,
) -> (
    tempfile::TempDir,
    PathBuf,
    String,
    PathBuf,
    tokio::sync::oneshot::Sender<()>,
) {
    let dir = tempfile::tempdir().expect("tempdir");
    let git = git.map(|make| make(dir.path()));
    let data = dir.path().to_path_buf();
    let ws = Workspace::open_with_keystore(
        &data,
        Box::new(FileKeyStore::new(Paths::new(&data).identity_dir())),
    )
    .expect("workspace");
    let project = ws
        .create_project(NewProject::managed("web").unwrap())
        .unwrap();
    // The tests that need a repository `git init -b main` the root; the record
    // says so too, since the record — not the folder — decides how a
    // workstream isolates (a worktree, not a copy) and which branch is trunk.
    let project = ws
        .update_project(bisa_core::Project {
            vcs: bisa_core::Vcs::Git {
                default_branch: "main".into(),
                remote: None,
                code_host: None,
            },
            ..project
        })
        .unwrap();
    let root = ws.project_root_path(&project);
    std::fs::create_dir_all(&root).unwrap();
    let pid = project.id.to_string();
    let engine = Engine::start(
        ws,
        HarnessCatalog::new(),
        EngineConfig {
            design_enabled: false,
            git,
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
    (dir, actual, pid, root, stop)
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

#[tokio::test(flavor = "multi_thread")]
async fn read_write_and_the_conflict_body() {
    let (_dir, socket, pid, root, _stop) = boot().await;
    let file = format!("/ide/file/workstream/{pid}?path=README.md");

    // Create, then read back with the hash a save needs.
    let (status, v) = request(&socket, "PUT", &file, Some(json!({"text": "# Web\n"}))).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["created"], json!(true));
    let hash = v["hash"].as_str().unwrap().to_string();

    let (status, v) = request(&socket, "GET", &file, None).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["text"], json!("# Web\n"));
    assert_eq!(v["hash"], json!(hash));
    assert_eq!(v["editable"], json!(true));
    assert_eq!(v["binary"], json!(false));

    // Somebody else saved: the stale hash is a 409 that carries what is there now.
    std::fs::write(root.join("README.md"), "# Web — by an agent\n").unwrap();
    let (status, v) = request(
        &socket,
        "PUT",
        &file,
        Some(json!({"text": "# mine\n", "base_hash": hash})),
    )
    .await;
    assert_eq!(status, 409, "{v}");
    assert_eq!(v["current_text"], json!("# Web — by an agent\n"));
    assert!(v["current_hash"].as_str().unwrap().len() == 64);
    assert!(v["error"]
        .as_str()
        .unwrap()
        .contains("changed since you read it"));

    // Merge, resend with the current hash: it lands.
    let current = v["current_hash"].as_str().unwrap().to_string();
    let (status, v) = request(
        &socket,
        "PUT",
        &file,
        Some(json!({"text": "# Web — merged\n", "base_hash": current})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(
        std::fs::read_to_string(root.join("README.md")).unwrap(),
        "# Web — merged\n"
    );

    // A create over an existing path, and a path that leaves the root, are 400s.
    let (status, _) = request(&socket, "PUT", &file, Some(json!({"text": "x"}))).await;
    assert_eq!(status, 400);
    let (status, _) = request(
        &socket,
        "PUT",
        &format!("/ide/file/workstream/{pid}?path=../x"),
        Some(json!({"text": "x"})),
    )
    .await;
    assert_eq!(status, 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_editor_cap_is_stated_not_silent() {
    let (_dir, socket, pid, root, _stop) = boot().await;
    // Above the editable size, below the refusal: served, read-only.
    std::fs::write(root.join("big.txt"), "a".repeat(3 * 1024 * 1024)).unwrap();
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/ide/file/workstream/{pid}?path=big.txt"),
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(v["editable"], json!(false));
    assert_eq!(v["truncated"], json!(false));
    assert!(v["hash"].is_string());
    // Above the refusal: 413 with the size, so the client can offer reveal.
    std::fs::write(root.join("huge.txt"), "b".repeat(21 * 1024 * 1024)).unwrap();
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/ide/file/workstream/{pid}?path=huge.txt"),
        None,
    )
    .await;
    assert_eq!(status, 413, "{v}");
    assert_eq!(v["size"], json!(21 * 1024 * 1024));
    // Binary is never text, whatever the name says.
    std::fs::write(root.join("image.txt"), [0u8, 159, 146, 150]).unwrap();
    let (_, v) = request(
        &socket,
        "GET",
        &format!("/ide/file/workstream/{pid}?path=image.txt"),
        None,
    )
    .await;
    assert_eq!(v["binary"], json!(true));
    assert_eq!(v["editable"], json!(false));
}

/// The caps at their edges: a file of exactly the editable size is edited and
/// one byte more is read-only; exactly the refusal size is served and one
/// byte more is `413` with its size; text that is not UTF-8, a mark at its
/// head and nothing at all are each said as what they are.
#[tokio::test(flavor = "multi_thread")]
async fn the_editors_caps_hold_at_their_edges() {
    use bisa_node::ide::{EDITABLE_BYTES, REFUSE_BYTES};
    let (_dir, socket, pid, root, _stop) = boot().await;
    let sized = |name: &str, bytes: u64| {
        std::fs::write(root.join(name), vec![b'a'; bytes as usize]).unwrap();
    };
    let read = |name: &'static str| {
        let (socket, pid) = (socket.clone(), pid.clone());
        async move {
            request(
                &socket,
                "GET",
                &format!("/ide/file/workstream/{pid}?path={name}"),
                None,
            )
            .await
        }
    };

    sized("at-editable.txt", EDITABLE_BYTES);
    sized("over-editable.txt", EDITABLE_BYTES + 1);
    // The two large ones are sparse: a size with nothing written, since only
    // the size is what these two are asked about.
    let sparse = |name: &str, bytes: u64| {
        std::fs::File::create(root.join(name))
            .unwrap()
            .set_len(bytes)
            .unwrap();
    };
    sparse("at-refuse.txt", REFUSE_BYTES);
    sparse("over-refuse.txt", REFUSE_BYTES + 1);
    let (status, v) = read("at-editable.txt").await;
    assert_eq!(
        (status, &v["editable"], &v["size"]),
        (200, &json!(true), &json!(EDITABLE_BYTES))
    );
    let (status, v) = read("over-editable.txt").await;
    assert_eq!(
        (status, &v["editable"]),
        (200, &json!(false)),
        "one byte more is read-only"
    );
    assert!(v["text"].is_string(), "and still read");
    let (status, v) = read("at-refuse.txt").await;
    assert_eq!(
        (status, &v["truncated"]),
        (200, &json!(false)),
        "exactly the cap is served whole: {}",
        v["size"]
    );
    let (status, v) = read("over-refuse.txt").await;
    assert_eq!(status, 413, "one byte more is refused");
    assert_eq!(
        (&v["size"], &v["limit"]),
        (&json!(REFUSE_BYTES + 1), &json!(REFUSE_BYTES))
    );

    // Nothing at all is a file, and editable.
    std::fs::write(root.join("empty.txt"), b"").unwrap();
    let (status, v) = read("empty.txt").await;
    assert_eq!(
        (status, &v["size"], &v["editable"], &v["text"]),
        (200, &json!(0), &json!(true), &json!(""))
    );
    // Bytes that are no UTF-8 are binary: never text the editor would save back changed.
    std::fs::write(root.join("latin1.txt"), [b'c', b'a', b'f', 0xE9, b'\n']).unwrap();
    let (status, v) = read("latin1.txt").await;
    assert_eq!(status, 200);
    assert_eq!(
        v["editable"],
        json!(false),
        "what cannot be read as text is not edited as text: {v}"
    );
    // A mark at the head, and CRLF, survive a read and a save with the hash it was read at.
    let marked = "\u{feff}one\r\ntwo\r\n";
    std::fs::write(root.join("marked.txt"), marked).unwrap();
    let (_, v) = read("marked.txt").await;
    assert_eq!(v["text"], json!(marked), "read as it is on disk");
    let (status, saved) = request(
        &socket,
        "PUT",
        &format!("/ide/file/workstream/{pid}?path=marked.txt"),
        Some(json!({"text": format!("{marked}three\r\n"), "base_hash": v["hash"]})),
    )
    .await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(
        std::fs::read(root.join("marked.txt")).unwrap(),
        format!("{marked}three\r\n").into_bytes(),
        "saved as it was typed: the mark and the line ends kept"
    );
}

/// A request whose answer is bytes: the status, the three typing headers, the body.
async fn raw_bytes(socket: &std::path::Path, path: &str) -> (u16, Vec<(String, String)>, Vec<u8>) {
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
    let headers = [
        "content-type",
        "x-content-type-options",
        "content-disposition",
    ]
    .iter()
    .filter_map(|h| {
        resp.headers()
            .get(*h)
            .map(|v| (h.to_string(), v.to_str().unwrap_or("").to_string()))
    })
    .collect();
    let bytes = resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (status, headers, bytes)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_files_bytes_are_served_typed_by_their_header_never_their_name() {
    let (_dir, socket, pid, root, _stop) = boot().await;
    let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    // A PNG called .txt is an image: the header decides, not the name.
    std::fs::write(root.join("photo.txt"), &png).unwrap();
    let (status, headers, bytes) = raw_bytes(
        &socket,
        &format!("/ide/raw/workstream/{pid}?path=photo.txt"),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(bytes, png, "the bytes come back as they are");
    let of = |name: &str| {
        headers
            .iter()
            .find(|(h, _)| h == name)
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(of("content-type"), Some("image/png"));
    assert_eq!(of("x-content-type-options"), Some("nosniff"));
    assert_eq!(of("content-disposition"), Some("inline"));
    // Anything else — a PDF, a page called .png, plain text — is a download, never a page.
    std::fs::write(root.join("page.png"), "<script>alert(1)</script>").unwrap();
    let (status, headers, bytes) =
        raw_bytes(&socket, &format!("/ide/raw/workstream/{pid}?path=page.png")).await;
    assert_eq!(status, 200);
    assert_eq!(bytes, b"<script>alert(1)</script>");
    let of = |name: &str| {
        headers
            .iter()
            .find(|(h, _)| h == name)
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(of("content-type"), Some("application/octet-stream"));
    assert_eq!(of("x-content-type-options"), Some("nosniff"));
    assert_eq!(of("content-disposition"), Some("attachment"));
    // The containment check is every file route's: outside is refused, missing is refused, a folder is refused.
    let (status, _, _) = raw_bytes(&socket, &format!("/ide/raw/workstream/{pid}?path=../x")).await;
    assert_eq!(status, 400);
    let (status, _, _) = raw_bytes(
        &socket,
        &format!("/ide/raw/workstream/{pid}?path=nothing.pdf"),
    )
    .await;
    assert_eq!(status, 400);
    std::fs::create_dir_all(root.join("dir")).unwrap();
    let (status, _, _) = raw_bytes(&socket, &format!("/ide/raw/workstream/{pid}?path=dir")).await;
    assert_eq!(status, 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn explorer_mutations_and_the_watch_lease() {
    let (_dir, socket, pid, root, _stop) = boot().await;
    let files = format!("/ide/files/workstream/{pid}");
    let (status, _) = request(
        &socket,
        "POST",
        &files,
        Some(json!({"path": "src", "kind": "dir"})),
    )
    .await;
    assert_eq!(status, 200);
    let (status, _) = request(
        &socket,
        "POST",
        &files,
        Some(json!({"path": "src/a.rs", "kind": "file"})),
    )
    .await;
    assert_eq!(status, 200);
    let (status, _) = request(
        &socket,
        "POST",
        &files,
        Some(json!({"path": "src/a.rs", "kind": "sock"})),
    )
    .await;
    assert_eq!(status, 400);
    let (status, v) = request(
        &socket,
        "POST",
        &format!("{files}/move"),
        Some(json!({"from": "src/a.rs", "to": "src/b.rs"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert!(root.join("src/b.rs").exists());
    let (status, v) = request(&socket, "DELETE", &format!("{files}?path=src"), None).await;
    assert_eq!(status, 400, "a directory needs the confirmation: {v}");
    // Unlinked, not the OS Trash: a fixture never leaves its tempdir.
    let (status, v) = request(
        &socket,
        "PUT",
        "/settings/machine",
        Some(json!({"values": {"editor.delete.trash": false}})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (status, _) = request(
        &socket,
        "DELETE",
        &format!("{files}?path=src&recursive=true"),
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert!(!root.join("src").exists());
    assert!(root.is_dir());

    // Several entries as one act: checked whole, answered whole.
    for path in ["one.txt", "two/x.txt", "three.txt"] {
        std::fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
        std::fs::write(root.join(path), "x").unwrap();
    }
    let batch = format!("{files}/delete");
    let (status, v) = request(
        &socket,
        "POST",
        &batch,
        Some(json!({"entries": [{"path": "one.txt"}, {"path": "two", "recursive": true}, {"path": "three.txt"}]})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["ok"], json!(true));
    assert_eq!(v["failed"], json!(null));
    assert_eq!(v["disposal"], json!("unlink"));
    let gone: Vec<&str> = v["deleted"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["path"].as_str().unwrap())
        .collect();
    assert_eq!(gone, vec!["one.txt", "two", "three.txt"]);
    assert!(!root.join("one.txt").exists() && !root.join("two").exists());
    assert!(!root.join("three.txt").exists());
    std::fs::write(root.join("four.txt"), "x").unwrap();
    let (status, v) = request(
        &socket,
        "POST",
        &batch,
        Some(json!({"entries": [{"path": "four.txt"}, {"path": "nope.txt"}]})),
    )
    .await;
    assert_eq!(status, 400, "a missing entry refuses the batch whole: {v}");
    assert!(root.join("four.txt").is_file(), "nothing went");
    let (status, _) = request(&socket, "POST", &batch, Some(json!({"entries": []}))).await;
    assert_eq!(status, 400, "nothing named");
    let (status, _) = request(
        &socket,
        "POST",
        &batch,
        Some(json!({"entries": [{"path": "four.txt"}], "recursive": true})),
    )
    .await;
    assert_eq!(status, 400, "a key the body does not declare");
    assert!(root.join("four.txt").is_file());

    let watch = format!("/ide/watch/workstream/{pid}");
    let (status, v) = request(&socket, "POST", &watch, None).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["watching"], json!(true));
    let (status, v) = request(&socket, "DELETE", &watch, None).await;
    assert_eq!(status, 200);
    assert_eq!(v["was_watching"], json!(true));
}

// ---------------------------------------------------------------------------
// Search and replace (ide/12)
// ---------------------------------------------------------------------------

async fn raw_body(socket: &std::path::Path, method: &str, path: &str) -> (u16, String) {
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
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test(flavor = "multi_thread")]
async fn search_streams_hits_then_a_summary_and_replace_is_a_preview_then_a_guarded_write() {
    let (_dir, socket, pid, root, _stop) = boot().await;
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/a.rs"), "let x = needle;\nlet y = 2;\n").unwrap();
    std::fs::write(root.join("src/b.rs"), "needle\nneedle\n").unwrap();

    let (status, body) = raw_body(
        &socket,
        "GET",
        &format!("/ide/search/workstream/{pid}?q=needle&case=sensitive"),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let frames: Vec<Value> = body
        .lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .map(|d| serde_json::from_str(d.trim()).unwrap())
        .collect();
    let hits: Vec<&Value> = frames
        .iter()
        .filter(|f| f["type"] == json!("hit"))
        .collect();
    assert_eq!(hits.len(), 3, "{body}");
    let done = frames
        .iter()
        .find(|f| f["type"] == json!("done"))
        .expect("a done frame ends the stream");
    assert_eq!(done["matches"], json!(3));
    assert_eq!(done["files_with_matches"], json!(2));
    assert_eq!(done["truncated"], json!(false));
    assert_eq!(frames.last().unwrap()["type"], json!("done"));

    // A pattern that does not compile is refused before the stream opens — a
    // 400, never an error frame inside a stream that already said 200.
    let (status, body) = raw_body(
        &socket,
        "GET",
        &format!("/ide/search/workstream/{pid}?q=%5B&regex=true"),
    )
    .await;
    assert_eq!(status, 400, "{body}");
    assert!(body.contains("regex"), "{body}");

    // Preview, then apply with the previews' hashes.
    let (status, preview) = request(
        &socket,
        "POST",
        &format!("/ide/replace/workstream/{pid}"),
        Some(json!({"q": "needle", "case": "sensitive", "replacement": "pin"})),
    )
    .await;
    assert_eq!(status, 200, "{preview}");
    assert_eq!(preview["applied"], json!(false));
    let files = preview["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    assert!(
        std::fs::read_to_string(root.join("src/b.rs"))
            .unwrap()
            .contains("needle"),
        "a preview writes nothing"
    );
    let apply_files: Vec<Value> = files
        .iter()
        .map(|f| json!({"path": f["path"], "base_hash": f["base_hash"]}))
        .collect();
    let (status, applied) = request(
        &socket,
        "POST",
        &format!("/ide/replace/workstream/{pid}"),
        Some(json!({"q": "needle", "case": "sensitive", "replacement": "pin", "apply": true, "files": apply_files})),
    )
    .await;
    assert_eq!(status, 200, "{applied}");
    assert_eq!(applied["applied"], json!(true));
    assert!(
        applied["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|o| o["outcome"] == json!("applied")),
        "{applied}"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("src/b.rs")).unwrap(),
        "pin\npin\n"
    );
    // Apply without the preview's files is refused.
    let (status, _) = request(
        &socket,
        "POST",
        &format!("/ide/replace/workstream/{pid}"),
        Some(json!({"q": "pin", "replacement": "x", "apply": true})),
    )
    .await;
    assert_eq!(status, 400);
}

/// What git prints for a read — a commit id, a subject, a count.
fn git_text(dir: &std::path::Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn raw_git(dir: &std::path::Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Hunk staging, blame, history, and the review-note loop over HTTP.
/// A pull request's draft is asked as a commit message's is: always 200, and a
/// checkout with no branch of its own — the project's primary — says why
/// rather than drafting anything.
#[tokio::test(flavor = "multi_thread")]
async fn a_pull_request_draft_is_always_answered_and_says_why_when_there_is_none() {
    let (_dir, socket, pid, root, _stop) = boot().await;
    raw_git(&root, &["init", "--quiet", "-b", "main"]);
    raw_git(&root, &["config", "user.name", "Bisa Test"]);
    raw_git(&root, &["config", "user.email", "test@example.invalid"]);
    std::fs::write(root.join("a.txt"), "one\n").unwrap();
    raw_git(&root, &["add", "-A"]);
    raw_git(&root, &["commit", "-m", "first", "--quiet"]);

    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/pr/suggest"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["suggested"], json!(false), "{v}");
    assert_eq!(
        (v["title"].clone(), v["body"].clone()),
        (json!(""), json!("")),
        "nothing drafted: {v}"
    );
    assert!(
        v["error"].as_str().is_some_and(|e| !e.is_empty()),
        "the reason, said: {v}"
    );

    let (status, _) = request(&socket, "POST", "/workstreams/not-an-id/pr/suggest", None).await;
    assert_eq!(status, 400, "an id that is not one is refused");
}

#[tokio::test(flavor = "multi_thread")]
async fn hunks_blame_history_and_review_notes() {
    let (_dir, socket, pid, root, _stop) = boot().await;
    raw_git(&root, &["init", "--quiet", "-b", "main"]);
    raw_git(&root, &["config", "user.name", "Bisa Test"]);
    raw_git(&root, &["config", "user.email", "test@example.invalid"]);
    std::fs::write(root.join("a.txt"), "one\ntwo\nthree\n").unwrap();
    raw_git(&root, &["add", "-A"]);
    raw_git(&root, &["commit", "-m", "first", "--quiet"]);
    std::fs::write(root.join("a.txt"), "one\nTWO\nthree\n").unwrap();

    // Stage one hunk; the row comes back staged and the tree is untouched.
    let patch = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,3 +1,3 @@\n one\n-two\n+TWO\n three\n";
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/hunk"),
        Some(json!({"patch": patch})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let row = v["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == "a.txt")
        .unwrap();
    assert_eq!(row["index"], json!("M"), "{row}");
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).unwrap(),
        "one\nTWO\nthree\n"
    );
    // Reverse it.
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/hunk"),
        Some(json!({"patch": patch, "reverse": true})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let row = v["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == "a.txt")
        .unwrap();
    assert_eq!(row["index"], json!("."), "{row}");
    // An empty patch is the caller's mistake.
    let (status, _) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/hunk"),
        Some(json!({"patch": ""})),
    )
    .await;
    assert_eq!(status, 400);

    // Blame and history.
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/blame?path=a.txt"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let lines = v["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0]["summary"], json!("first"));
    assert_eq!(lines[1]["uncommitted"], json!(true));
    let (status, _) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/blame?path=a.txt&end=2"),
        None,
    )
    .await;
    assert_eq!(status, 400, "end without start");
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/history?path=a.txt"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["commits"][0]["subject"], json!("first"));

    // Review notes: create, list, edit clears sent, send, resolve hides, delete.
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/projects/{pid}/review"),
        Some(json!({
            "path": "a.txt", "start": 2, "end": 2,
            "scope": {"scope": "unstaged"},
            "hunk": "@@ -2 +2 @@\n-two\n+TWO\n",
            "body": "keep it lowercase"
        })),
    )
    .await;
    assert_eq!(status, 201, "{v}");
    let id = v["note"]["id"].as_str().unwrap().to_string();
    assert_eq!(v["note"]["diff_identity"].as_str().unwrap().len(), 64);
    assert!(v["note"].get("sent_at").is_none());

    let (status, v) = request(&socket, "GET", &format!("/projects/{pid}/review"), None).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["notes"].as_array().unwrap().len(), 1);

    let (status, v) = request(
        &socket,
        "POST",
        &format!("/projects/{pid}/review/send"),
        Some(json!({"ids": []})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert!(v["notes"][0]["sent_at"].is_u64(), "{v}");
    assert_eq!(
        v["posted_to"],
        json!([]),
        "a standalone project has no goal thread"
    );

    let (status, v) = request(
        &socket,
        "PATCH",
        &format!("/projects/{pid}/review/{id}"),
        Some(json!({"body": "keep it lowercase, please"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert!(
        v["note"].get("sent_at").is_none(),
        "an edit clears the sent mark: {v}"
    );

    let (status, v) = request(
        &socket,
        "POST",
        &format!("/projects/{pid}/review/{id}/resolve"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert!(v["note"]["resolved_at"].is_u64());
    let (_, v) = request(&socket, "GET", &format!("/projects/{pid}/review"), None).await;
    assert!(
        v["notes"].as_array().unwrap().is_empty(),
        "resolved notes are hidden by default"
    );
    let (_, v) = request(
        &socket,
        "GET",
        &format!("/projects/{pid}/review?resolved=true"),
        None,
    )
    .await;
    assert_eq!(v["notes"].as_array().unwrap().len(), 1);

    let (status, _) = request(
        &socket,
        "DELETE",
        &format!("/projects/{pid}/review/{id}"),
        None,
    )
    .await;
    assert_eq!(status, 204);
    // Gone, it is not found — edited or resolved — never a request that was
    // wrong; an id that is no id is.
    let (status, v) = request(
        &socket,
        "PATCH",
        &format!("/projects/{pid}/review/{id}"),
        Some(json!({"body": "too late"})),
    )
    .await;
    assert_eq!(status, 404, "{v}");
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/projects/{pid}/review/{id}/resolve"),
        None,
    )
    .await;
    assert_eq!(status, 404, "{v}");
    let (status, _) = request(
        &socket,
        "PATCH",
        &format!("/projects/{pid}/review/not-an-id"),
        Some(json!({"body": "x"})),
    )
    .await;
    assert_eq!(status, 400);

    // The commit graph and the inspector over the same repository.
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/ide/graph/workstream/{pid}?from=0&count=50"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["total"], json!(1));
    assert_eq!(v["done"], json!(true));
    assert_eq!(v["rows"][0]["lane"], json!(0));
    assert_eq!(v["rows"][0]["subject"], json!("first"));
    assert!(
        v["rows"][0]["refs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["kind"] == "head"),
        "{v}"
    );
    let sha = v["rows"][0]["id"].as_str().unwrap().to_string();
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/commit/{sha}"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["commit"]["subject"], json!("first"));
    assert_eq!(v["commit"]["files"][0]["path"], json!("a.txt"));
    assert!(v["diff"].as_str().unwrap().contains("+one"));
    assert_eq!(v["truncated"], json!(false));
    let (status, _) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/commit/not%20a%20sha"),
        None,
    )
    .await;
    assert_eq!(status, 400);

    // One file of the commit, in the shapes the three views read (ide/05):
    // its own patch, and its two sides — a root commit has no left side.
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/commit/{sha}/diff?path=a.txt"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["sha"], json!(sha));
    assert_eq!(v["path"], json!("a.txt"));
    assert!(v["diff"].as_str().unwrap().contains("+one"), "{v}");
    assert_eq!(v["truncated"], json!(false));
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/commit/{sha}/sides?path=a.txt"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(
        v["original"],
        serde_json::Value::Null,
        "a root commit's left side"
    );
    assert_eq!(
        v["modified"],
        json!("one\ntwo\nthree\n"),
        "the file as the first commit wrote it"
    );
    assert_eq!(v["binary"], json!(false));
    assert_eq!(v["truncated"], json!(false));
    let (status, _) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/commit/{sha}/diff?path=missing.txt"),
        None,
    )
    .await;
    assert_eq!(status, 400, "a path the commit did not touch");
    let (status, _) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/commit/not%20a%20sha/sides?path=a.txt"),
        None,
    )
    .await;
    assert_eq!(status, 400);
}

/// The consented tier over HTTP: branches, checkout with a recovery ref,
/// discard, tags, restore — and the 409 when git will not clobber.
#[tokio::test(flavor = "multi_thread")]
async fn consented_git_writes_a_recovery_ref_first() {
    let (_dir, socket, pid, root, _stop) = boot().await;
    raw_git(&root, &["init", "--quiet", "-b", "main"]);
    raw_git(&root, &["config", "user.name", "Bisa Test"]);
    raw_git(&root, &["config", "user.email", "test@example.invalid"]);
    std::fs::write(root.join("a.txt"), "one\n").unwrap();
    raw_git(&root, &["add", "-A"]);
    raw_git(&root, &["commit", "-m", "first", "--quiet"]);

    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/branches"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let branches = v["branches"].as_array().unwrap();
    assert_eq!(branches.len(), 1);
    assert_eq!(branches[0]["current"], json!(true));
    let main = branches[0]["name"].as_str().unwrap().to_string();

    // Create and switch: the switch half is consented and answers with a recovery.
    std::fs::write(root.join("a.txt"), "one\ntyped\n").unwrap();
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/branches"),
        Some(json!({"name": "feature", "switch": true})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["branch"], json!("feature"));
    let rec = v["recovery"]["ref_name"].as_str().unwrap().to_string();
    assert!(
        rec.starts_with("refs/bisa/safety/") && rec.ends_with("checkout.wip"),
        "{rec}"
    );
    assert_eq!(v["recovery"]["was_clean"], json!(false));
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).unwrap(),
        "one\ntyped\n",
        "the edit came along"
    );

    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/recovery"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["recovery"][0]["op"], json!("checkout"));
    assert_eq!(v["recovery"][0]["kind"], json!("tree"));
    assert_eq!(v["recovery"][0]["branch"], json!(main));

    // Discard the hunk: the file goes back to the index version; the recovery keeps what was typed.
    let (_, d) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/diff?path=a.txt"),
        None,
    )
    .await;
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/discard"),
        Some(json!({"patch": d["diff"]})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).unwrap(),
        "one\n"
    );
    let (status, _) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/discard"),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, 400, "neither patch nor paths");
    // Paths the index no longer holds — gone since the list was read, or
    // never tracked: nothing to discard, the tree's state and not a bad
    // request, and nothing written.
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/discard"),
        Some(json!({"paths": ["vanished.txt"]})),
    )
    .await;
    assert_eq!(status, 409, "{v}");
    assert!(
        v["error"]
            .as_str()
            .is_some_and(|e| e.contains("nothing to discard")),
        "{v}"
    );

    // Restore the checkout's recovery: back on main with the typed line.
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/recovery/restore"),
        Some(json!({"ref": rec})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["branch"], json!(main));
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).unwrap(),
        "one\ntyped\n"
    );

    // Tags: create, list, delete.
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/tags"),
        Some(json!({"name": "v1", "message": "first"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (_, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/tags"),
        None,
    )
    .await;
    assert_eq!(v["tags"][0]["name"], json!("v1"));
    let (status, _) = request(
        &socket,
        "DELETE",
        &format!("/workstreams/{pid}/git/tags/v1"),
        None,
    )
    .await;
    assert_eq!(status, 200);

    // A checkout git refuses is a 409, and the file is untouched.
    std::fs::write(root.join("a.txt"), "one\n").unwrap();
    std::fs::write(root.join("b.txt"), "b\n").unwrap();
    raw_git(&root, &["add", "-A"]);
    raw_git(&root, &["commit", "-m", "b on main", "--quiet"]);
    raw_git(&root, &["branch", "-f", "feature", "HEAD~1"]);
    std::fs::write(root.join("b.txt"), "mine\n").unwrap();
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/checkout"),
        Some(json!({"target": "feature"})),
    )
    .await;
    assert_eq!(status, 409, "{v}");
    assert_eq!(
        std::fs::read_to_string(root.join("b.txt")).unwrap(),
        "mine\n"
    );

    // A branch delete pins the tip.
    let (status, v) = request(
        &socket,
        "DELETE",
        &format!("/workstreams/{pid}/git/branches/feature"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(
        v["recovery"]["was_clean"],
        json!(false),
        "the tree was dirty, so the tree is saved too"
    );
    let (_, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/branches"),
        None,
    )
    .await;
    assert_eq!(v["branches"].as_array().unwrap().len(), 1);
}

/// Workstreams from the IDE (ide/07): a typed branch name, live status rows,
/// and closing with the checkout removed — which saves a recovery ref first.
#[tokio::test(flavor = "multi_thread")]
async fn workstreams_have_live_status_and_close_saving_a_recovery() {
    let (_dir, socket, pid, root, _stop) = boot().await;
    // Unborn HEAD: a workstream cannot open, and the answer says so.
    raw_git(&root, &["init", "--quiet", "-b", "main"]);
    raw_git(&root, &["config", "user.name", "Bisa Test"]);
    raw_git(&root, &["config", "user.email", "test@example.invalid"]);
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/projects/{pid}/workstreams"),
        Some(json!({"source": {"source": "new_branch", "name": "feature/dark mode"}})),
    )
    .await;
    assert_eq!(status, 409, "{v}");

    std::fs::write(root.join("a.txt"), "one\n").unwrap();
    raw_git(&root, &["add", "-A"]);
    raw_git(&root, &["commit", "-m", "first", "--quiet"]);

    // A typed name is made safe, never refused.
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/projects/{pid}/workstreams"),
        Some(json!({"source": {"source": "new_branch", "name": "feature/dark mode"}})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let wid = v["workstream"]["id"].as_str().unwrap().to_string();
    assert_eq!(
        v["workstream"]["kind"]["branch"],
        json!("feature/dark-mode"),
        "{v}"
    );
    let wt = std::path::PathBuf::from(v["path"].as_str().unwrap());
    assert!(wt.is_dir());

    // Status: clean, on the branch, level with its base, nobody running.
    let (status, v) = request(&socket, "GET", &format!("/workstreams/{wid}/status"), None).await;
    assert_eq!(status, 200, "{v}");
    let s = &v["status"];
    assert_eq!(s["branch"], json!("feature/dark-mode"));
    assert_eq!(s["clean"], json!(true));
    assert_eq!(s["ahead_of_base"], json!(0));
    assert_eq!(s["running_agents"], json!(0));
    assert_eq!(s["exists"], json!(true));

    // Dirty the checkout; the cache is two seconds, so wait it out.
    std::fs::write(wt.join("b.txt"), "two\n").unwrap();
    // A tracked change too: it is what a recovery ref saves when the tree
    // goes (an untracked file is stashed only when asked — ide/04).
    std::fs::write(wt.join("a.txt"), "changed\n").unwrap();
    tokio::time::sleep(Duration::from_millis(2_100)).await;
    let (_, v) = request(
        &socket,
        "GET",
        &format!("/projects/{pid}/workstreams/status"),
        None,
    )
    .await;
    let rows = v["statuses"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "the primary checkout and the worktree: {v}");
    let row = rows
        .iter()
        .find(|r| r["workstream"] == json!(wid))
        .expect("the worktree's row");
    assert_eq!(row["untracked"], json!(1));
    assert_eq!(row["clean"], json!(false));
    let primary = rows
        .iter()
        .find(|r| r["workstream"] == json!(pid))
        .expect("the primary's row");
    assert_eq!(
        primary["clean"],
        json!(true),
        "nothing was written in the primary"
    );
    let (_, v) = request(&socket, "GET", "/workstreams/status", None).await;
    assert_eq!(v["statuses"].as_array().unwrap().len(), 2);

    // Closing with the tree removed is consented and saves what was there.
    let (status, v) = request(
        &socket,
        "DELETE",
        &format!("/workstreams/{wid}?tree=true"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["removed_tree"], json!(true));
    assert_eq!(
        v["stopped_sessions"],
        json!(0),
        "nothing stood in the checkout: {v}"
    );
    assert!(!wt.exists(), "the checkout is gone");
    let rec = v["recovery"]["ref_name"]
        .as_str()
        .expect("a recovery ref was written");
    assert!(
        rec.starts_with("refs/bisa/safety/") && rec.contains("workstream_close"),
        "{rec}"
    );
    assert_eq!(
        v["recovery"]["was_clean"],
        json!(false),
        "the modified file was saved"
    );
    // The ref lives in the shared repository and outlives the worktree.
    let (_, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/recovery"),
        None,
    )
    .await;
    assert!(
        v["recovery"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["ref_name"] == rec),
        "{v}"
    );
    // A closed workstream drops out of the status rows; the primary checkout stays.
    let (_, v) = request(
        &socket,
        "GET",
        &format!("/projects/{pid}/workstreams/status"),
        None,
    )
    .await;
    let rows = v["statuses"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{v}");
    assert_eq!(
        rows[0]["workstream"],
        json!(pid),
        "only the primary is left"
    );
}

/// The code host routes without a code host: a project with no remote has no
/// capabilities, a workstream that opened no pull request has `pr: null`, and a
/// conflicted path shows its three sides and can be marked resolved.
#[tokio::test(flavor = "multi_thread")]
async fn code_host_routes_are_honest_without_a_remote_and_conflicts_have_three_sides() {
    let (_dir, socket, pid, root, _stop) = boot().await;
    raw_git(&root, &["init", "--quiet", "-b", "main"]);
    raw_git(&root, &["config", "user.name", "Bisa Test"]);
    raw_git(&root, &["config", "user.email", "test@example.invalid"]);
    std::fs::write(root.join("a.txt"), "base\n").unwrap();
    raw_git(&root, &["add", "-A"]);
    raw_git(&root, &["commit", "-m", "first", "--quiet"]);

    let (status, v) = request(
        &socket,
        "GET",
        &format!("/codehost/capabilities/{pid}"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert!(v["code_host"].is_null(), "no origin, no code host: {v}");

    // A workstream with no pull request.
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/projects/{pid}/workstreams"),
        Some(json!({"source": {"source": "new_branch", "name": "feature/x"}})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let wid = v["workstream"]["id"].as_str().unwrap().to_string();
    let (status, v) = request(&socket, "GET", &format!("/workstreams/{wid}/pr"), None).await;
    assert_eq!(status, 200, "{v}");
    assert!(v["pr"].is_null());
    // Checks and merge need a pull request first — a refusal, not a code host call.
    let (status, _) = request(
        &socket,
        "GET",
        &format!("/workstreams/{wid}/pr/checks"),
        None,
    )
    .await;
    assert_eq!(status, 400);
    let (status, _) = request(
        &socket,
        "POST",
        &format!("/workstreams/{wid}/pr/merge"),
        Some(json!({"strategy": "squash"})),
    )
    .await;
    assert_eq!(status, 400, "no pull request to merge");
    // A reply on a thread: empty words are a 400 before anything else, and
    // with words a workstream without a pull request is a refusal, not a
    // code host call.
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{wid}/pr/threads/T1/reply"),
        Some(json!({"body": "   ", "resolve": true})),
    )
    .await;
    assert_eq!(status, 400, "{v}");
    assert!(v.to_string().contains("needs words"), "{v}");
    let (status, _) = request(
        &socket,
        "POST",
        &format!("/workstreams/{wid}/pr/threads/T1/reply"),
        Some(json!({"body": "Fixed."})),
    )
    .await;
    assert_eq!(status, 400, "no PR to reply on");
    // The accounts route never returns a token, and an empty one is refused
    // before GitHub is asked.
    let (status, v) = request(&socket, "GET", "/codehost/github/accounts", None).await;
    assert_eq!(status, 200);
    assert!(v.get("token").is_none());
    assert!(
        v["accounts"].is_array() && v["env_override"].is_boolean() && v.get("default").is_some(),
        "{v}"
    );
    let (status, _) = request(
        &socket,
        "PUT",
        "/codehost/github/accounts",
        Some(json!({"token": "   "})),
    )
    .await;
    assert_eq!(status, 400);
    // The connection card is honest about a checkout with no remote: no
    // remote, no code host, no transport, nothing to caution about.
    let (status, c) = request(
        &socket,
        "GET",
        &format!("/workstreams/{wid}/git/connection"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{c}");
    assert!(c["remote"].is_null(), "{c}");
    assert!(c["code_host"].is_null());
    assert_eq!(c["transport"]["kind"], json!("none"));
    assert_eq!(c["account"]["source"], json!("none"));
    assert!(
        c["cautions"].as_array().is_some_and(|c| c.is_empty()),
        "{c}"
    );
    // Without SSH configured the SSH routes say so, and spawn nothing.
    let (status, _) = request(&socket, "GET", "/git/ssh", None).await;
    assert_eq!(status, 503);
    // Profiles: none yet, and a bad spec is refused by name.
    let (status, p) = request(&socket, "GET", "/git/profiles", None).await;
    assert_eq!(status, 200, "{p}");
    assert!(p["profiles"].as_array().is_some_and(|p| p.is_empty()));
    let (status, _) = request(
        &socket,
        "PUT",
        "/git/profiles/acme",
        Some(json!({"label": "Acme", "host": "github.com", "owner": "acme/web", "name": "Ada", "email": "ada@acme.example"})),
    )
    .await;
    assert_eq!(status, 400, "an owner with a slash is not an owner");

    // A conflict: the same line changed on two branches, merged in the project tree.
    raw_git(&root, &["branch", "theirs"]);
    std::fs::write(root.join("a.txt"), "mine\n").unwrap();
    raw_git(&root, &["commit", "-am", "mine", "--quiet"]);
    let theirs_wt = root.parent().unwrap().join("theirs-wt");
    raw_git(
        &root,
        &[
            "worktree",
            "add",
            "--quiet",
            theirs_wt.to_str().unwrap(),
            "theirs",
        ],
    );
    std::fs::write(theirs_wt.join("a.txt"), "theirs\n").unwrap();
    raw_git(&theirs_wt, &["commit", "-am", "theirs", "--quiet"]);
    let merge = std::process::Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["merge", "theirs", "--no-edit"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(!merge.status.success(), "the merge conflicts");

    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/conflict?path=a.txt"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["base"], json!("base\n"));
    assert_eq!(v["ours"], json!("mine\n"));
    assert_eq!(v["theirs"], json!("theirs\n"));

    // Resolve: write the merged text, mark it resolved, and the row is staged.
    std::fs::write(root.join("a.txt"), "mine and theirs\n").unwrap();
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/resolve"),
        Some(json!({"path": "a.txt"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let row = v["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == "a.txt")
        .unwrap();
    assert_eq!(row["conflicted"], json!(false), "{row}");
    assert_eq!(row["staged"], json!(true), "{row}");
}

/// A conversation about the checkout (ide/09): a message with chips
/// round-trips with its context, and a context over the wire bound is
/// refused. The workstream itself is no message scope any more.
#[tokio::test(flavor = "multi_thread")]
async fn a_checkouts_conversation_carries_context_chips_and_bounds_them() {
    let (_dir, socket, pid, _root, _stop) = boot().await;
    let (status, _) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/messages"),
        None,
    )
    .await;
    assert_eq!(status, 404, "a workstream has no thread of its own");
    let (status, v) = request(
        &socket,
        "POST",
        "/conversations",
        Some(json!({"origin": {"kind": "workstream", "id": pid, "project": pid}})),
    )
    .await;
    assert_eq!(status, 201, "{v}");
    let cid = v["conversation"]["id"].as_str().unwrap().to_string();
    assert_eq!(v["conversation"]["origin"]["kind"], json!("workstream"));
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/conversations/{cid}/messages"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["messages"].as_array().unwrap().len(), 0);

    let (status, v) = request(
        &socket,
        "POST",
        &format!("/conversations/{cid}/messages"),
        Some(json!({
            "content": "have a look at this",
            "context": [
                {"kind": "file", "path": "src/main.rs"},
                {"kind": "selection", "path": "src/main.rs", "range": {"start": 3, "end": 4}, "text": "let x = 1;"},
                {"kind": "annotation", "page": {"kind": "file", "path": "www/index.html"}, "selector": "#save", "excerpt": "<button id=\"save\">Save</button>", "note": "make it blue"}
            ]
        })),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (_, v) = request(
        &socket,
        "GET",
        &format!("/conversations/{cid}/messages"),
        None,
    )
    .await;
    let m = &v["messages"][0];
    assert_eq!(m["content"], json!("have a look at this"));
    assert_eq!(m["context"].as_array().unwrap().len(), 3, "{m}");
    assert_eq!(m["context"][0]["kind"], json!("file"));
    assert_eq!(m["context"][1]["range"]["start"], json!(3));
    assert_eq!(m["context"][2]["kind"], json!("annotation"));
    assert_eq!(
        m["context"][2]["note"],
        json!("make it blue"),
        "an annotated element of a page comes back as it was posted"
    );

    // Over 64 KiB of chips is a refusal, not a truncation.
    let big = "x".repeat(70 * 1024);
    let (status, _) = request(
        &socket,
        "POST",
        &format!("/conversations/{cid}/messages"),
        Some(json!({"content": "too much", "context": [{"kind": "terminal", "session": "t1", "tail": big}]})),
    )
    .await;
    assert_eq!(status, 400);
    // A path that leaves the project is refused by the core type.
    let (status, _) = request(
        &socket,
        "POST",
        &format!("/conversations/{cid}/messages"),
        Some(json!({"content": "nope", "context": [{"kind": "file", "path": "../secrets"}]})),
    )
    .await;
    assert_eq!(status, 400, "refused in the platform's own shape");
}

/// The language-server proxy without a server (ide/10): status lists the
/// presets with availability, a Markdown file has no language, a Rust file
/// opened with servers off is named as Rust and followed by nothing — the
/// answer the editor opens it again on when the server starts — an unknown
/// method is refused, and nothing is spawned for an index that is open.
#[tokio::test(flavor = "multi_thread")]
async fn language_servers_are_reported_honestly_and_never_started_for_prose() {
    let (_dir, socket, pid, _root, _stop) = boot().await;
    // Off at this machine: no server of this machine's is ever started here.
    let (status, v) = request(
        &socket,
        "PUT",
        "/settings/machine",
        Some(json!({"values": {"lsp.enabled": false}})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/ide/lsp/workstream/{pid}/status"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let rows = v["servers"].as_array().unwrap();
    assert!(
        rows.iter()
            .any(|r| r["language"] == "rust" && r["command"] == "rust-analyzer"),
        "{v}"
    );
    assert!(
        rows.iter().all(|r| r["state"]["state"] == "stopped"),
        "nothing runs for an open index: {v}"
    );
    assert!(rows.iter().all(|r| r["available"].is_boolean()));

    let (status, v) = request(
        &socket,
        "POST",
        &format!("/ide/lsp/workstream/{pid}/open"),
        Some(json!({"path": "README.md", "text": "# hi"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert!(v["language"].is_null(), "prose has no server: {v}");
    assert_eq!(v["following"], json!(false), "{v}");

    let (status, v) = request(
        &socket,
        "POST",
        &format!("/ide/lsp/workstream/{pid}/open"),
        Some(json!({"path": "src/main.rs", "text": "fn main() {}"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["language"], json!("rust"), "the language is named: {v}");
    assert_eq!(
        v["following"],
        json!(false),
        "servers are off: nothing follows it: {v}"
    );

    let (status, _) = request(
        &socket,
        "POST",
        &format!("/ide/lsp/workstream/{pid}/request"),
        Some(json!({"path": "src/main.rs", "method": "workspace/executeCommand", "params": {}})),
    )
    .await;
    assert_eq!(
        status, 400,
        "a method off the allow list is refused before any server is asked"
    );
    let (status, _) = request(
        &socket,
        "POST",
        &format!("/ide/lsp/workstream/{pid}/request"),
        Some(json!({"path": "notes.txt", "method": "textDocument/hover", "params": {}})),
    )
    .await;
    assert_eq!(status, 400, "no language, no server");
    let (status, _) = request(
        &socket,
        "POST",
        &format!("/ide/lsp/workstream/{pid}/close"),
        Some(json!({"path": "README.md"})),
    )
    .await;
    assert_eq!(status, 200);
}

/// Who commits here, over HTTP: read, set into the repository's local config,
/// seen by a worktree of the same repository, and refused when half-given. The
/// engine runs on an isolated `git`, so the first read is `none` — the
/// developer's own global config cannot reach the assertion.
#[tokio::test(flavor = "multi_thread")]
async fn identity_is_read_and_set_per_repository_and_never_globally() {
    let (_dir, socket, pid, root, _stop) = boot_with(Some(isolated_git)).await;
    raw_git(&root, &["init", "--quiet", "-b", "main"]);

    let (status, before) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/identity"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{before}");
    assert_eq!(before["source"], json!("none"));
    assert_eq!(before["global"], Value::Null);
    assert_eq!(
        before["suggested"],
        Value::Null,
        "no code host, no account, nothing to suggest"
    );
    for key in ["name", "email", "source", "global"] {
        assert!(
            before.get(key).is_some(),
            "the view carries {key}: {before}"
        );
    }

    // The identity is written as local git config; a value the schema refuses is 400.
    let (status, half) = request(
        &socket,
        "PUT",
        &format!("/workstreams/{pid}/git/config"),
        Some(json!({"set": {"user.name": "Ada Lovelace", "user.email": "   "}})),
    )
    .await;
    assert_eq!(status, 400, "{half}");
    let (status, gone) = request(
        &socket,
        "PUT",
        &format!("/workstreams/{pid}/git/identity"),
        Some(json!({"name": "x", "email": "x@y.z"})),
    )
    .await;
    assert_eq!(
        status, 405,
        "identity is read here, written through config: {gone}"
    );

    let (status, written) = request(
        &socket,
        "PUT",
        &format!("/workstreams/{pid}/git/config"),
        Some(json!({"set": {"user.name": "Ada Lovelace", "user.email": "ada@example.invalid"}})),
    )
    .await;
    assert_eq!(status, 200, "{written}");
    let name = written["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["key"] == "user.name")
        .unwrap();
    assert_eq!(name["local"], json!("Ada Lovelace"));
    assert_eq!(name["effective"], json!("Ada Lovelace"));
    let (status, set) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/identity"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{set}");
    assert_eq!(set["source"], json!("local"));
    assert_eq!(set["name"], json!("Ada Lovelace"));
    assert_eq!(set["email"], json!("ada@example.invalid"));

    // Independently: the repository's own config, and nothing global.
    let local = std::process::Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["config", "--local", "--get", "user.name"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&local.stdout).trim(),
        "Ada Lovelace"
    );

    // A commit made through the platform is authored by it.
    std::fs::write(root.join("a.txt"), "one\n").unwrap();
    let (status, committed) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/commit"),
        Some(json!({"message": "first", "paths": ["a.txt"]})),
    )
    .await;
    assert_eq!(status, 200, "{committed}");
    let author = std::process::Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["log", "-1", "--format=%an <%ae>"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&author.stdout).trim(),
        "Ada Lovelace <ada@example.invalid>"
    );
}

/// An amend over HTTP: the last commit rewritten with what is staged and a
/// new message, the old commit pinned in Safety first; a blank message is
/// refused before anything is asked of git.
#[tokio::test(flavor = "multi_thread")]
async fn amending_rewrites_the_last_commit_and_pins_it() {
    let (_dir, socket, pid, root, _stop) = boot().await;
    raw_git(&root, &["init", "--quiet", "-b", "main"]);
    raw_git(&root, &["config", "user.name", "Bisa Test"]);
    raw_git(&root, &["config", "user.email", "test@example.invalid"]);
    std::fs::write(root.join("a.txt"), "one\n").unwrap();
    raw_git(&root, &["add", "-A"]);
    raw_git(&root, &["commit", "-m", "first", "--quiet"]);
    let before = git_text(&root, &["rev-parse", "HEAD"]);

    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/amend"),
        Some(json!({"message": "   "})),
    )
    .await;
    assert_eq!(status, 400, "{v}");
    assert_eq!(
        git_text(&root, &["for-each-ref", "refs/bisa/safety"]),
        "",
        "a refusal saves nothing"
    );

    std::fs::write(root.join("b.txt"), "two\n").unwrap();
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/amend"),
        Some(json!({"message": "first, amended", "paths": ["b.txt"]})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let ref_name = v["recovery"]["ref_name"].as_str().expect("recovery ref");
    assert!(ref_name.contains("amend"), "{ref_name}");
    assert_eq!(
        v["recovery"]["commit"],
        json!(before),
        "the old commit is the recovery"
    );
    assert_eq!(v["branch"], json!("main"));
    let after = v["commit"].as_str().expect("commit").to_string();
    assert_ne!(after, before);
    assert_eq!(
        v["short"].as_str().map(str::len),
        Some(7),
        "the short id, as a commit answers it"
    );
    assert_eq!(git_text(&root, &["rev-parse", "HEAD"]), after);
    assert_eq!(
        git_text(&root, &["log", "-1", "--format=%s"]),
        "first, amended"
    );
    assert_eq!(
        git_text(&root, &["rev-list", "--count", "HEAD"]),
        "1",
        "no commit was added"
    );
    assert_eq!(
        git_text(&root, &["rev-parse", ref_name]),
        before,
        "Safety holds the old commit"
    );
    assert!(
        v["files"].as_array().is_some_and(|f| f.is_empty()),
        "the tree is clean once the amend lands: {v}"
    );
}

/// Served folders (ide/18): a folder of the checkout on a loopback port,
/// listed, resolved back to files, refused twice, stopped; a dotfile never
/// served; the run command answered once the project sets one.
#[tokio::test(flavor = "multi_thread")]
async fn served_folders_come_and_go_and_resolve_to_files() {
    let (_dir, socket, pid, root, _stop) = boot().await;
    std::fs::create_dir_all(root.join("site/docs")).unwrap();
    std::fs::write(root.join("site/index.html"), "<h1>hi</h1>\n").unwrap();
    std::fs::write(root.join("site/docs/index.html"), "<h1>docs</h1>\n").unwrap();
    std::fs::write(root.join("site/.secret"), "no\n").unwrap();

    let (status, v) = request(&socket, "GET", &format!("/workstreams/{pid}/servers"), None).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["servers"], json!([]));

    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/servers"),
        Some(json!({"folder": "site"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let id = v["id"].as_str().unwrap().to_string();
    let url = v["url"].as_str().unwrap().to_string();
    let port = v["port"].as_u64().unwrap() as u16;
    assert_eq!(url, format!("http://127.0.0.1:{port}/"));
    assert_eq!(v["owner"]["kind"], json!("workstream"), "{v}");
    assert_eq!(v["owner"]["workstream"], json!(pid), "{v}");
    assert_eq!(v["owner"]["folder"], json!("site"), "{v}");

    // The page is served with its content type; a dotfile is not there.
    let (code, body) = http_get(port, "/").await;
    assert_eq!(code, 200, "{body}");
    assert!(body.contains("<h1>hi</h1>"));
    let (code, _) = http_get(port, "/docs/").await;
    assert_eq!(code, 200, "a directory serves its index.html");
    let (code, _) = http_get(port, "/.secret").await;
    assert_eq!(code, 404, "never a dotfile");
    let (code, _) = http_get(port, "/nothing.html").await;
    assert_eq!(code, 404);

    // A URL path resolves to the file of the checkout it lands on.
    let resolve = |path: &str| {
        let socket = socket.clone();
        let pid = pid.clone();
        let id = id.clone();
        let path = path.to_string();
        async move {
            let (status, v) = request(
                &socket,
                "GET",
                &format!("/workstreams/{pid}/servers/{id}/resolve?path={path}"),
                None,
            )
            .await;
            assert_eq!(status, 200, "{v}");
            v["path"].clone()
        }
    };
    assert_eq!(resolve("/").await, json!("site/index.html"));
    assert_eq!(resolve("/docs/").await, json!("site/docs/index.html"));
    assert_eq!(
        resolve("/docs/index.html?x=1").await,
        json!("site/docs/index.html")
    );
    assert_eq!(resolve("/missing.html").await, Value::Null);
    assert_eq!(resolve("/.secret").await, Value::Null);

    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/servers"),
        Some(json!({"folder": "site/"})),
    )
    .await;
    assert_eq!(status, 409, "the same folder twice: {v}");
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/servers"),
        Some(json!({"folder": "../elsewhere"})),
    )
    .await;
    assert_eq!(status, 400, "outside the checkout: {v}");

    let (status, v) = request(&socket, "GET", &format!("/workstreams/{pid}/servers"), None).await;
    assert_eq!(status, 200);
    assert_eq!(v["servers"].as_array().unwrap().len(), 1);
    let (status, v) = request(
        &socket,
        "DELETE",
        &format!("/workstreams/{pid}/servers/{id}"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (status, _) = request(
        &socket,
        "DELETE",
        &format!("/workstreams/{pid}/servers/{id}"),
        None,
    )
    .await;
    assert_eq!(status, 404, "stopped once");
    let (status, v) = request(&socket, "GET", &format!("/workstreams/{pid}/servers"), None).await;
    assert_eq!(status, 200);
    assert_eq!(v["servers"], json!([]));

    // The run command: none, then set and untrusted, then approved.
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/run-command"),
        None,
    )
    .await;
    assert_eq!(status, 404, "{v}");
    let (status, v) = request(
        &socket,
        "PUT",
        &format!("/settings/project?project={pid}"),
        Some(json!({ "values": { "workstreams.script.run": "python3 -m http.server 8123" } })),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/run-command"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["command"], json!("python3 -m http.server 8123"));
    assert_eq!(v["trusted"], json!(false));
    assert_eq!(v["cwd"], json!(root.display().to_string()));
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/projects/{pid}/workstream-scripts/approve"),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/run-command"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["trusted"], json!(true));
}

/// The browser bridge's door (ide/18): nothing parked reads as an empty
/// list, and an answer for nothing waiting is a 404.
#[tokio::test(flavor = "multi_thread")]
async fn the_browser_bridge_lists_nothing_and_refuses_an_answer_for_nobody() {
    let (_dir, socket, _pid, _root, _stop) = boot().await;
    let (status, v) = request(&socket, "GET", "/browser/requests", None).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["requests"], json!([]));
    let (status, v) = request(
        &socket,
        "POST",
        "/browser/requests/01NOBODY",
        Some(json!({"ok": true, "tabs": []})),
    )
    .await;
    assert_eq!(status, 404, "{v}");
}

/// One GET over loopback TCP to a served folder, whole response.
#[tokio::test(flavor = "multi_thread")]
async fn a_served_folder_hands_out_nothing_hidden_and_nothing_outside_however_the_url_spells_it() {
    let (dir, socket, pid, root, _stop) = boot().await;
    std::fs::create_dir_all(root.join("site/.cache")).unwrap();
    std::fs::write(root.join("site/index.html"), "<h1>hi</h1>\n").unwrap();
    std::fs::write(root.join("site/my page.html"), "<h1>spaced</h1>\n").unwrap();
    std::fs::write(root.join("site/.hidden.txt"), "hidden\n").unwrap();
    std::fs::write(root.join("site/.cache/inner.txt"), "hidden too\n").unwrap();
    // A file beside the checkout, and links in the served folder that leave it.
    let outside = dir.path().join("outside-the-checkout.txt");
    std::fs::write(&outside, "not the site's\n").unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, root.join("site/link.txt")).unwrap();
        std::os::unix::fs::symlink(dir.path(), root.join("site/up")).unwrap();
        std::os::unix::fs::symlink(root.join("site/index.html"), root.join("site/home.html"))
            .unwrap();
    }

    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/servers"),
        Some(json!({"folder": "site"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let port = v["port"].as_u64().unwrap() as u16;
    let id = v["id"].as_str().unwrap().to_string();

    let (code, body) = http_get(port, "/my%20page.html").await;
    assert_eq!(code, 200, "an encoded name is its file: {body}");
    for hidden in [
        "/.hidden.txt",
        "/%2Ehidden.txt",
        "/%2ehidden.txt",
        "/.cache/inner.txt",
        "/%2Ecache/inner.txt",
        "/docs/../.hidden.txt",
        "/%2E%2E/outside-the-checkout.txt",
        "/..%2Foutside-the-checkout.txt",
    ] {
        let (code, body) = http_get(port, hidden).await;
        assert_eq!(
            code, 404,
            "{hidden}: what is hidden is not there, however it is spelt — {body}"
        );
        assert!(
            !body.contains("hidden") && !body.contains("not the site's"),
            "{hidden}"
        );
    }
    #[cfg(unix)]
    {
        for leaving in ["/link.txt", "/up/outside-the-checkout.txt"] {
            let (code, body) = http_get(port, leaving).await;
            assert_eq!(code, 404, "{leaving} leaves the folder: {body}");
        }
        let (code, body) = http_get(port, "/home.html").await;
        assert_eq!(code, 200, "a link that stays inside is a file: {body}");
    }

    // The same names, asked which file they are.
    for (path, file) in [
        ("/my%20page.html", json!("site/my page.html")),
        ("/%2Ehidden.txt", Value::Null),
        ("/link.txt", Value::Null),
    ] {
        let (status, v) = request(
            &socket,
            "GET",
            &format!(
                "/workstreams/{pid}/servers/{id}/resolve?path={}",
                path.replace('%', "%25")
            ),
            None,
        )
        .await;
        assert_eq!(status, 200, "{v}");
        assert_eq!(v["path"], file, "{path}");
    }
}

async fn http_get(port: u16, path: &str) -> (u16, String) {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect the served folder");
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await
        .unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.unwrap();
    let text = String::from_utf8_lossy(&raw).to_string();
    let code = text
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let body = text
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_string())
        .unwrap_or_default();
    (code, body)
}

/// The global layer, from Settings → Git: `GET /git/config` lists
/// the schema and each key's global value; `PUT /git/config` is the one place
/// the platform writes the person's global file — here, at their request; a
/// global-only key is accepted there and refused locally.
#[tokio::test(flavor = "multi_thread")]
async fn the_global_git_config_is_read_and_written_from_settings_only() {
    let (dir, socket, pid, root, _stop) = boot_with(Some(isolated_git)).await;
    raw_git(&root, &["init", "--quiet", "-b", "main"]);
    let global_file = dir.path().join("no-global.gitconfig");

    let (status, empty) = request(&socket, "GET", "/git/config", None).await;
    assert_eq!(status, 200, "{empty}");
    let keys: Vec<&str> = empty["schema"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["key"].as_str().unwrap())
        .collect();
    assert_eq!(
        keys[..3],
        ["user.name", "user.email", "user.useConfigOnly"],
        "{keys:?}"
    );
    assert!(keys.contains(&"init.defaultBranch"));
    assert!(
        empty["entries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["global"].is_null() && e["local"].is_null()),
        "{empty}"
    );
    let crlf = empty["schema"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["key"] == "core.autocrlf")
        .unwrap();
    assert_eq!(crlf["kind"]["type"], json!("choice"));
    assert_eq!(crlf["kind"]["options"], json!(["true", "false", "input"]));

    let (status, written) = request(
        &socket,
        "PUT",
        "/git/config",
        Some(json!({"set": {"user.name": "Grace Hopper", "user.email": "grace@example.invalid", "init.defaultBranch": "main"}})),
    )
    .await;
    assert_eq!(status, 200, "{written}");
    let name = written["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["key"] == "user.name")
        .unwrap();
    assert_eq!(name["global"], json!("Grace Hopper"));
    let file = std::fs::read_to_string(&global_file).unwrap();
    assert!(
        file.contains("Grace Hopper") && file.contains("defaultBranch = main"),
        "the engine's global file: {file}"
    );

    // The repository now inherits it, and nothing landed locally.
    let (_, identity) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/identity"),
        None,
    )
    .await;
    assert_eq!(identity["source"], json!("global"));
    assert_eq!(
        identity["global"],
        json!({"name": "Grace Hopper", "email": "grace@example.invalid"})
    );

    // Refusals: a global-only key locally, a value that is not of its kind, an unknown key.
    for body in [
        json!({"set": {"init.defaultBranch": "main"}}),
        json!({"set": {"core.autocrlf": "maybe"}}),
        json!({"set": {"core.hooksPath": "x"}}),
    ] {
        let (status, v) = request(
            &socket,
            "PUT",
            &format!("/workstreams/{pid}/git/config"),
            Some(body.clone()),
        )
        .await;
        assert_eq!(status, 400, "{body}: {v}");
    }
    let (status, _) = request(
        &socket,
        "PUT",
        "/git/config",
        Some(json!({"set": {"core.hooksPath": "x"}})),
    )
    .await;
    assert_eq!(status, 400);

    // Unset falls through: the global name goes, the identity is gone with it.
    let (status, after) = request(
        &socket,
        "PUT",
        "/git/config",
        Some(json!({"unset": ["user.name", "init.defaultBranch"]})),
    )
    .await;
    assert_eq!(status, 200, "{after}");
    assert!(!std::fs::read_to_string(&global_file)
        .unwrap()
        .contains("defaultBranch"));
    let (_, identity) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/identity"),
        None,
    )
    .await;
    assert_eq!(identity["source"], json!("none"));

    // The ask dialog's seed: the global pair (now half, so none) and the open questions.
    std::fs::write(root.join("a.txt"), "one\n").unwrap();
    let (status, refused) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/commit"),
        Some(json!({"message": "first", "paths": ["a.txt"]})),
    )
    .await;
    assert_eq!(status, 400, "{refused}");
    let (_, overview) = request(&socket, "GET", "/git/committer", None).await;
    assert_eq!(overview["global"], Value::Null);
    assert_eq!(overview["pending"][0]["project"], json!(pid), "{overview}");
    assert_eq!(overview["pending"][0]["reason"], json!("commit_refused"));
    let (status, _) = request(
        &socket,
        "PUT",
        &format!("/workstreams/{pid}/git/config"),
        Some(json!({"set": {"user.name": "Ada", "user.email": "ada@example.invalid"}})),
    )
    .await;
    assert_eq!(status, 200);
    let (_, answered) = request(&socket, "GET", "/git/committer", None).await;
    assert_eq!(answered["pending"], json!([]), "{answered}");
}

/// A creation body may carry git config; it lands in the new repository's
/// local layer, and a project with nothing on the body inherits the global
/// layer — asking only when nothing resolves.
#[tokio::test(flavor = "multi_thread")]
async fn a_create_body_s_git_config_lands_locally_and_an_inheriting_project_never_asks() {
    let (_dir, socket, _pid, _root, _stop) = boot_with(Some(isolated_git)).await;
    let (status, made) = request(
        &socket,
        "POST",
        "/projects",
        Some(json!({
            "kind": "new",
            "slug": "seeded",
            "git_config": {"user.name": "Grace Hopper", "user.email": "grace@example.invalid", "user.useConfigOnly": "true"}
        })),
    )
    .await;
    assert_eq!(status, 200, "{made}");
    let pid = made["project"]["id"].as_str().unwrap().to_string();

    let (status, config) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/config"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{config}");
    let entries = config["entries"].as_array().unwrap();
    let get = |key: &str| entries.iter().find(|e| e["key"] == key).unwrap().clone();
    assert_eq!(get("user.name")["local"], json!("Grace Hopper"));
    assert_eq!(get("user.useConfigOnly")["local"], json!("true"));
    assert_eq!(get("pull.rebase")["effective"], Value::Null);
    let (_, identity) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/identity"),
        None,
    )
    .await;
    assert_eq!(identity["source"], json!("local"));
    let (_, overview) = request(&socket, "GET", "/git/committer", None).await;
    assert_eq!(overview["pending"], json!([]), "nothing to ask: {overview}");

    // Nothing on the body and nothing global: the new project asks.
    let (status, asked) = request(
        &socket,
        "POST",
        "/projects",
        Some(json!({"kind": "new", "slug": "unseeded"})),
    )
    .await;
    assert_eq!(status, 200, "{asked}");
    let (_, overview) = request(&socket, "GET", "/git/committer", None).await;
    assert_eq!(
        overview["pending"][0]["slug"],
        json!("unseeded"),
        "{overview}"
    );
    assert_eq!(overview["pending"][0]["reason"], json!("created"));

    // Set the global pair; the next project inherits and does not ask.
    let (status, _) = request(
        &socket,
        "PUT",
        "/git/config",
        Some(json!({"set": {"user.name": "Grace Hopper", "user.email": "grace@example.invalid"}})),
    )
    .await;
    assert_eq!(status, 200);
    // …and the project that was asking inherits it: the desk is settled
    // by the global write, not by a visit to its checkout.
    let (_, overview) = request(&socket, "GET", "/git/committer", None).await;
    assert_eq!(
        overview["pending"],
        json!([]),
        "the global identity answered the open question: {overview}"
    );
    let (status, made) = request(
        &socket,
        "POST",
        "/projects",
        Some(json!({"kind": "new", "slug": "inheriting"})),
    )
    .await;
    assert_eq!(status, 200, "{made}");
    let ipid = made["project"]["id"].as_str().unwrap().to_string();
    let (_, identity) = request(
        &socket,
        "GET",
        &format!("/workstreams/{ipid}/git/identity"),
        None,
    )
    .await;
    assert_eq!(identity["source"], json!("global"));
    let (_, overview) = request(&socket, "GET", "/git/committer", None).await;
    assert!(
        !overview["pending"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["slug"] == "inheriting"),
        "{overview}"
    );

    // A value git would refuse fails the creation, and no record is left.
    let (status, bad) = request(
        &socket,
        "POST",
        "/projects",
        Some(json!({"kind": "new", "slug": "bad", "git_config": {"user.name": "--global"}})),
    )
    .await;
    assert_eq!(status, 400, "{bad}");
    let (_, projects) = request(&socket, "GET", "/projects", None).await;
    assert!(!projects.to_string().contains("\"bad\""), "{projects}");
}

/// The workstream scripts over HTTP (ide/07 §Workstream scripts): the texts are
/// settings, the trust is this machine's — read as one view, approved by one
/// call — and a script that fails refuses the workstream it guards with a coded
/// `409` the dialog can show in place.
#[tokio::test(flavor = "multi_thread")]
async fn workstream_scripts_are_read_approved_and_refuse() {
    let (_dir, socket, pid, root, _stop) = boot().await;
    raw_git(&root, &["init", "--quiet", "-b", "main"]);
    raw_git(&root, &["config", "user.name", "Bisa Test"]);
    raw_git(&root, &["config", "user.email", "test@example.invalid"]);
    std::fs::write(root.join("a.txt"), "one\n").unwrap();
    raw_git(&root, &["add", "-A"]);
    raw_git(&root, &["commit", "-m", "first", "--quiet"]);

    let set = |key: &str, text: &str| {
        let socket = socket.clone();
        let pid = pid.clone();
        let body = json!({ "values": { key: text } });
        async move {
            let (status, v) = request(
                &socket,
                "PUT",
                &format!("/settings/project?project={pid}"),
                Some(body),
            )
            .await;
            assert_eq!(status, 200, "{v}");
        }
    };

    // Written as a setting, read back untrusted: nobody here approved it yet.
    set("workstreams.script.pre_create", "echo nope >&2; exit 3").await;
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/projects/{pid}/workstream-scripts"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["timeout_secs"], json!(300));
    let scripts = v["scripts"].as_array().unwrap();
    assert_eq!(
        scripts.len(),
        4,
        "the three lifecycle scripts and the run command"
    );
    assert_eq!(scripts[3]["phase"], json!("run"));
    assert_eq!(scripts[0]["phase"], json!("pre_create"));
    assert_eq!(scripts[0]["command"], json!("echo nope >&2; exit 3"));
    assert_eq!(scripts[0]["trusted"], json!(false));
    assert_eq!(
        scripts[1]["trusted"],
        json!(true),
        "an empty script needs no approval"
    );

    // Unapproved: the workstream is refused, and the code says why.
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/projects/{pid}/workstreams"),
        Some(json!({"source": {"source": "new_branch", "name": "a"}})),
    )
    .await;
    assert_eq!(status, 409, "{v}");
    assert_eq!(v["code"], json!("script_failed"));
    assert_eq!(v["detail"]["phase"], json!("pre_create"));
    assert!(
        v["error"]
            .as_str()
            .unwrap()
            .contains("not approved on this machine"),
        "{v}"
    );

    // Approved, it runs — and fails, with its words in the detail.
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/projects/{pid}/workstream-scripts/approve"),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["scripts"][0]["trusted"], json!(true));
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/projects/{pid}/workstreams"),
        Some(json!({"source": {"source": "new_branch", "name": "a"}})),
    )
    .await;
    assert_eq!(status, 409, "{v}");
    assert_eq!(v["code"], json!("script_failed"));
    assert_eq!(v["detail"]["output"], json!("nope"));
    assert!(
        v["error"]
            .as_str()
            .unwrap()
            .contains("exited with status 3"),
        "{v}"
    );
    assert_eq!(
        bisa_vcs::git::worktree_list(&root).unwrap().len(),
        1,
        "nothing was created"
    );

    // A clean script that fails keeps the checkout; emptied and approved, the delete goes through.
    set("workstreams.script.pre_create", "").await;
    set("workstreams.script.clean", "exit 1").await;
    request(
        &socket,
        "POST",
        &format!("/projects/{pid}/workstream-scripts/approve"),
        Some(json!({})),
    )
    .await;
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/projects/{pid}/workstreams"),
        Some(json!({"source": {"source": "new_branch", "name": "b"}})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let wid = v["workstream"]["id"].as_str().unwrap().to_string();
    let wt = std::path::PathBuf::from(v["path"].as_str().unwrap());
    let (status, v) = request(
        &socket,
        "DELETE",
        &format!("/workstreams/{wid}?tree=true"),
        None,
    )
    .await;
    assert_eq!(status, 409, "{v}");
    assert_eq!(v["code"], json!("script_failed"));
    assert_eq!(v["detail"]["phase"], json!("clean"));
    assert!(
        wt.is_dir(),
        "the checkout stays while the clean script fails"
    );
    set("workstreams.script.clean", "").await;
    let (status, v) = request(
        &socket,
        "DELETE",
        &format!("/workstreams/{wid}?tree=true"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert!(!wt.exists());
}

/// `workstreams.dirty_close`: with `refuse`, a checkout that holds work nobody
/// committed keeps its tree — said in words, before anything is stopped —
/// while closing the record alone, and a clean checkout, go through; with the
/// default, the tree goes behind a recovery ref.
#[tokio::test(flavor = "multi_thread")]
async fn a_dirty_checkout_keeps_its_tree_when_the_workspace_says_refuse() {
    let (_dir, socket, pid, root, _stop) = boot().await;
    raw_git(&root, &["init", "--quiet", "-b", "main"]);
    raw_git(&root, &["config", "user.name", "Bisa Test"]);
    raw_git(&root, &["config", "user.email", "test@example.invalid"]);
    std::fs::write(root.join("a.txt"), "one\n").unwrap();
    raw_git(&root, &["add", "-A"]);
    raw_git(&root, &["commit", "-m", "first", "--quiet"]);
    let open = |branch: &'static str| {
        let (socket, pid) = (socket.clone(), pid.clone());
        async move {
            let (status, v) = request(
                &socket,
                "POST",
                &format!("/projects/{pid}/workstreams"),
                Some(json!({ "source": { "source": "new_branch", "name": branch } })),
            )
            .await;
            assert_eq!(status, 200, "{v}");
            (
                v["workstream"]["id"].as_str().unwrap().to_string(),
                std::path::PathBuf::from(v["path"].as_str().unwrap()),
            )
        }
    };
    let dirty_close = |value: &'static str| {
        let socket = socket.clone();
        async move {
            let (status, v) = request(
                &socket,
                "PUT",
                "/settings/workspace",
                Some(json!({ "values": { "workstreams.dirty_close": value } })),
            )
            .await;
            assert_eq!(status, 200, "{v}");
        }
    };
    let remove = |wid: String| {
        let socket = socket.clone();
        async move {
            request(
                &socket,
                "DELETE",
                &format!("/workstreams/{wid}?tree=true"),
                None,
            )
            .await
        }
    };

    dirty_close("refuse").await;
    let (wid, wt) = open("dirty").await;
    std::fs::write(wt.join("unsaved.txt"), "the only copy\n").unwrap();
    std::fs::write(wt.join("a.txt"), "edited\n").unwrap();
    let (status, v) = remove(wid.clone()).await;
    assert_eq!(status, 409, "{v}");
    let said = v["error"].as_str().unwrap();
    assert!(
        said.contains("1 changed file") && said.contains("1 untracked file"),
        "the refusal names what would have been lost: {said}"
    );
    assert!(said.contains("workstreams.dirty_close"), "{said}");
    assert!(wt.join("unsaved.txt").is_file(), "the tree stays, whole");
    let (_, w) = request(&socket, "GET", &format!("/workstreams/{wid}"), None).await;
    assert_ne!(
        w["workstream"]["state"]["state"],
        json!("closed"),
        "and the record is as it was: {w}"
    );

    // A clean checkout goes, under the same setting.
    let (clean, clean_wt) = open("clean").await;
    let (status, v) = remove(clean).await;
    assert_eq!(status, 200, "{v}");
    assert!(!clean_wt.exists());

    // The default: the dirty tree goes, behind a recovery ref.
    dirty_close("confirm_with_recovery").await;
    let (status, v) = remove(wid).await;
    assert_eq!(status, 200, "{v}");
    assert!(
        v["recovery"].is_object(),
        "what was there can be had back: {v}"
    );
    assert!(!wt.exists());
}

/// Stash over HTTP (ide/04 §Stash): the list, a push with its recovery and
/// entry, the patch, apply keeping the entry, pop dropping it, a moved index
/// refused by code, a clean tree refused by code — and never by index alone.
#[tokio::test(flavor = "multi_thread")]
async fn stashes_are_pushed_listed_shown_applied_popped_and_dropped_over_http() {
    let (_dir, socket, pid, root, _stop) = boot().await;
    raw_git(&root, &["init", "--quiet", "-b", "main"]);
    raw_git(&root, &["config", "user.name", "Bisa Test"]);
    raw_git(&root, &["config", "user.email", "test@example.invalid"]);
    std::fs::write(root.join("a.txt"), "one\n").unwrap();
    raw_git(&root, &["add", "-A"]);
    raw_git(&root, &["commit", "-m", "first", "--quiet"]);

    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/stashes"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["stashes"], json!([]));

    // A clean tree has nothing to stash: refused by code, no recovery written.
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/stash"),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, 409, "{v}");
    assert_eq!(v["code"], json!("nothing_to_stash"));
    let (_, r) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/recovery"),
        None,
    )
    .await;
    assert_eq!(r["recovery"], json!([]), "a refusal writes no recovery ref");

    // Push with a message and an untracked file: the answer is a consented
    // one — recovery, branch, files — plus the entry.
    std::fs::write(root.join("a.txt"), "one\ntyped\n").unwrap();
    std::fs::write(root.join("new.txt"), "brand new\n").unwrap();
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/stash"),
        Some(json!({"message": "parked", "include_untracked": true})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["recovery"]["was_clean"], json!(false));
    assert!(v["recovery"]["ref_name"]
        .as_str()
        .unwrap()
        .ends_with("-stash_push.wip"));
    assert_eq!(v["stash"]["index"], json!(0));
    assert_eq!(v["stash"]["message"], json!("parked"));
    assert_eq!(v["stash"]["untracked"], json!(true));
    assert!(
        v["files"].as_array().unwrap().is_empty(),
        "the tree is clean: {v}"
    );
    let sha = v["stash"]["commit"].as_str().unwrap().to_string();
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).unwrap(),
        "one\n"
    );
    assert!(!root.join("new.txt").exists());
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/stash"),
        Some(json!({"unknown": 1})),
    )
    .await;
    assert_eq!(status, 400, "an unknown key is refused: {v}");

    // The list and the patch.
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/stashes"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["stashes"].as_array().unwrap().len(), 1);
    assert_eq!(v["stashes"][0]["commit"], json!(sha));
    assert!(
        v["stashes"][0]["branch"]
            .as_str()
            .is_some_and(|b| !b.is_empty()),
        "the branch it was made on: {v}"
    );
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/stashes/{sha}/diff"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let diff = v["diff"].as_str().unwrap();
    assert!(
        diff.contains("+typed") && diff.contains("+brand new"),
        "{diff}"
    );
    assert_eq!(v["truncated"], json!(false));

    // Apply keeps the entry; pop drops it and pins it as a stash recovery.
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/stashes/{sha}/apply"),
        Some(json!({"index": 0})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).unwrap(),
        "one\ntyped\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("new.txt")).unwrap(),
        "brand new\n"
    );
    let (_, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/stashes"),
        None,
    )
    .await;
    assert_eq!(
        v["stashes"].as_array().unwrap().len(),
        1,
        "apply keeps the entry"
    );
    // Back to clean so the pop has room; discard the tracked change and drop the new file.
    let (status, _) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/discard"),
        Some(json!({"paths": ["a.txt"]})),
    )
    .await;
    assert_eq!(status, 200);
    std::fs::remove_file(root.join("new.txt")).unwrap();

    // A moved index is refused by code, with what sits there now.
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/stashes/{sha}/pop"),
        Some(json!({"index": 3})),
    )
    .await;
    assert_eq!(status, 409, "{v}");
    assert_eq!(v["code"], json!("stash_moved"));
    assert_eq!(v["detail"]["index"], json!(3));
    assert_eq!(v["detail"]["commit"], json!(sha));
    assert_eq!(v["detail"]["now"], json!(null));

    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/stashes/{sha}/pop"),
        Some(json!({"index": 0})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).unwrap(),
        "one\ntyped\n"
    );
    let (_, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/stashes"),
        None,
    )
    .await;
    assert_eq!(v["stashes"], json!([]), "a clean pop drops the entry");
    let (_, r) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/recovery"),
        None,
    )
    .await;
    let pinned = r["recovery"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["kind"] == json!("stash"))
        .expect("the popped stash is pinned");
    assert_eq!(pinned["op"], json!("stash_pop"));
    assert_eq!(pinned["commit"], json!(sha));

    // Restore the pin: the entry is back on the list and the tree untouched.
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/recovery/restore"),
        Some(json!({"ref": pinned["ref_name"]})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (_, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/stashes"),
        None,
    )
    .await;
    assert_eq!(v["stashes"][0]["commit"], json!(sha));
    assert_eq!(v["stashes"][0]["message"], json!("parked"));
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).unwrap(),
        "one\ntyped\n"
    );

    // Drop pins it too; a drop by a wrong sha at a right index is refused.
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/stashes/{}/drop", "b".repeat(40)),
        Some(json!({"index": 0})),
    )
    .await;
    assert_eq!(status, 409, "{v}");
    assert_eq!(v["code"], json!("stash_moved"));
    assert_eq!(v["detail"]["now"], json!(sha));
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/stashes/{sha}/drop"),
        Some(json!({"index": 0})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (_, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/stashes"),
        None,
    )
    .await;
    assert_eq!(v["stashes"], json!([]));
}

/// A repository with an identity and one commit on the project's root, and a
/// bare origin beside it: what every branch test starts from.
fn seed_repo(dir: &std::path::Path, root: &std::path::Path) -> std::path::PathBuf {
    raw_git(root, &["init", "--quiet", "-b", "main"]);
    raw_git(root, &["config", "user.name", "Bisa Test"]);
    raw_git(root, &["config", "user.email", "test@example.invalid"]);
    std::fs::write(root.join("a.txt"), "one\n").unwrap();
    raw_git(root, &["add", "-A"]);
    raw_git(root, &["commit", "-m", "first", "--quiet"]);
    let origin = dir.join("origin.git");
    std::fs::create_dir_all(&origin).unwrap();
    raw_git(&origin, &["init", "--bare", "--quiet"]);
    raw_git(root, &["remote", "add", "origin", origin.to_str().unwrap()]);
    origin
}

fn git_out(dir: &std::path::Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A merge that conflicts over HTTP, settled by side and continued in place;
/// continue refuses by name while a path is unmerged (ide/04 §Conflicts).
/// A conflict is described whole over HTTP — its kind, its three sides, the
/// file with its markers — and the operation's facts name the sides by
/// branch, whether it was started here or in a terminal; a merge is
/// foreseen before it runs, and nothing moves for the preview.
#[tokio::test(flavor = "multi_thread")]
async fn a_conflict_is_described_whole_and_a_merge_is_foreseen_over_http() {
    let (dir, socket, pid, root, _stop) = boot().await;
    seed_repo(dir.path(), &root);
    let main = git_out(&root, &["rev-parse", "--abbrev-ref", "HEAD"]);
    // `gone.txt` is on both sides before they part: theirs deletes it,
    // ours changes it — the deleted-by-them kind below.
    std::fs::write(root.join("gone.txt"), "kept by us\n").unwrap();
    raw_git(&root, &["add", "."]);
    raw_git(&root, &["commit", "-m", "shared", "--quiet"]);
    raw_git(&root, &["branch", "feature"]);
    std::fs::write(root.join("a.txt"), "mine\n").unwrap();
    raw_git(&root, &["add", "."]);
    raw_git(&root, &["commit", "-m", "mine", "--quiet"]);
    raw_git(&root, &["checkout", "--quiet", "feature"]);
    std::fs::write(root.join("a.txt"), "theirs\n").unwrap();
    raw_git(&root, &["commit", "-am", "theirs", "--quiet"]);
    raw_git(&root, &["checkout", "--quiet", &main]);

    // Nothing is half-done: no facts, and the preview foresees the conflict.
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/operation"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v, json!(null));
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/merge-preview?source=feature"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(
        v["supported"],
        json!(true),
        "git 2.38 or later runs the tests"
    );
    assert_eq!(v["clean"], json!(false));
    assert_eq!(v["paths"], json!(["a.txt"]));
    let (_, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/status"),
        None,
    )
    .await;
    assert_eq!(v["status"]["clean"], json!(true), "a preview moves nothing");
    assert_eq!(v["status"]["in_progress"], json!(null));

    // A merge started in a terminal: the facts read the same.
    let merged = std::process::Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["merge", "feature"])
        .output()
        .unwrap();
    assert!(!merged.status.success(), "the merge must conflict");
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/operation"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["kind"], json!("merge"));
    assert_eq!(v["branch"], json!(main));
    assert_eq!(v["ours"]["role"], json!("branch"));
    assert_eq!(v["ours"]["name"], json!(main));
    assert_eq!(v["theirs"]["role"], json!("branch"));
    assert_eq!(
        v["theirs"]["name"],
        json!("feature"),
        "named from the prepared message"
    );
    assert_eq!(v["theirs"]["subject"], json!("theirs"));
    assert_eq!(v["step"], json!(null));

    // The conflicted path, whole: kind, sides, the marker-laden text, a hash.
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/conflict?path=a.txt"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["kind"], json!("both_modified"));
    assert_eq!(v["ours"], json!("mine\n"));
    assert_eq!(v["theirs"], json!("theirs\n"));
    assert_eq!(v["binary"], json!(false));
    let text = v["text"].as_str().unwrap();
    assert!(
        text.starts_with("<<<<<<<"),
        "the file as git wrote it: {text}"
    );
    assert!(text.contains("=======") && text.contains(">>>>>>>"));
    assert!(v["hash"].as_str().is_some_and(|h| !h.is_empty()));
    let (_, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/files"),
        None,
    )
    .await;
    let row = v["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == json!("a.txt"))
        .unwrap();
    assert_eq!(row["conflicted"], json!(true));
    assert_eq!(
        row["conflict"],
        json!("both_modified"),
        "the row carries the kind"
    );
    let (status, _) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/abort"),
        Some(json!({"what": "merge"})),
    )
    .await;
    assert_eq!(status, 200);

    // A file deleted on one side and changed on the other is a kind of its
    // own, with one side missing.
    raw_git(&root, &["checkout", "--quiet", "feature"]);
    raw_git(&root, &["rm", "--quiet", "gone.txt"]);
    raw_git(&root, &["commit", "-m", "theirs drops it", "--quiet"]);
    raw_git(&root, &["checkout", "--quiet", &main]);
    std::fs::write(root.join("gone.txt"), "kept by us, changed\n").unwrap();
    raw_git(&root, &["commit", "-am", "ours changes it", "--quiet"]);
    let merged = std::process::Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["merge", "feature"])
        .output()
        .unwrap();
    assert!(!merged.status.success(), "the merge must conflict");
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/conflict?path=gone.txt"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["kind"], json!("deleted_by_them"));
    assert_eq!(
        v["theirs"],
        json!(null),
        "the side that deleted it has no stage"
    );
    assert_eq!(v["ours"], json!("kept by us, changed\n"));
    let (status, _) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/abort"),
        Some(json!({"what": "merge"})),
    )
    .await;
    assert_eq!(status, 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_conflicting_merge_is_settled_by_side_and_continued_over_http() {
    let (dir, socket, pid, root, _stop) = boot().await;
    seed_repo(dir.path(), &root);
    // The branch is read before the switch: a reflog entry is a machine's
    // setting, the name of the branch we stand on is not.
    let main = git_out(&root, &["rev-parse", "--abbrev-ref", "HEAD"]);
    raw_git(&root, &["branch", "theirs"]);
    std::fs::write(root.join("a.txt"), "mine\n").unwrap();
    raw_git(&root, &["commit", "-am", "mine", "--quiet"]);
    raw_git(&root, &["checkout", "--quiet", "theirs"]);
    std::fs::write(root.join("a.txt"), "theirs\n").unwrap();
    raw_git(&root, &["commit", "-am", "theirs", "--quiet"]);
    raw_git(&root, &["checkout", "--quiet", &main]);

    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/merge"),
        Some(json!({"source": "theirs", "mode": "no_ff"})),
    )
    .await;
    assert_eq!(status, 409, "{v}");
    assert_eq!(v["code"], json!("conflict"));
    assert_eq!(v["detail"]["paths"], json!(["a.txt"]));
    assert_eq!(v["detail"]["in_progress"], json!("merge"));

    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/status"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["status"]["in_progress"], json!("merge"));
    assert_eq!(v["status"]["conflicted"], json!(1));

    // Continue refuses while the path is unmerged, naming it.
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/continue"),
        Some(json!({"what": "merge"})),
    )
    .await;
    assert_eq!(status, 409, "{v}");
    assert_eq!(v["code"], json!("conflict"));
    assert_eq!(v["detail"]["paths"], json!(["a.txt"]));
    // A merge has nothing to skip.
    let (status, _) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/skip"),
        Some(json!({"what": "merge"})),
    )
    .await;
    assert_eq!(status, 400);

    // The three sides, then their side taken whole.
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/conflict?path=a.txt"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["ours"], json!("mine\n"));
    assert_eq!(v["theirs"], json!("theirs\n"));
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/resolve"),
        Some(json!({"path": "a.txt", "take": "theirs"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert!(
        v["recovery"]["ref_name"]
            .as_str()
            .unwrap()
            .contains("resolve"),
        "taking a side is consented and recorded"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).unwrap(),
        "theirs\n"
    );
    assert!(v["files"]
        .as_array()
        .unwrap()
        .iter()
        .all(|f| f["conflicted"] == json!(false)));

    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/continue"),
        Some(json!({"what": "merge"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert!(v["recovery"]["ref_name"]
        .as_str()
        .unwrap()
        .contains("continue"));
    assert_eq!(
        git_out(&root, &["log", "-1", "--format=%s"]),
        "Merge branch 'theirs'"
    );
    let (_, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/status"),
        None,
    )
    .await;
    assert_eq!(v["status"]["in_progress"], json!(null));
    assert_eq!(v["status"]["clean"], json!(true));
    let (status, _) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/continue"),
        Some(json!({"what": "merge"})),
    )
    .await;
    assert_eq!(status, 400, "nothing is in progress any more");
}

/// Two commits picked in one request; the one that conflicts is skipped and
/// the other lands with its origin recorded.
#[tokio::test(flavor = "multi_thread")]
async fn a_two_commit_cherry_pick_is_skipped_past_its_conflict_over_http() {
    let (dir, socket, pid, root, _stop) = boot().await;
    seed_repo(dir.path(), &root);
    let main = git_out(&root, &["rev-parse", "--abbrev-ref", "HEAD"]);
    raw_git(&root, &["checkout", "--quiet", "-b", "picks"]);
    std::fs::write(root.join("a.txt"), "theirs\n").unwrap();
    raw_git(&root, &["commit", "-am", "A", "--quiet"]);
    let a = git_out(&root, &["rev-parse", "HEAD"]);
    std::fs::write(root.join("b.txt"), "b\n").unwrap();
    raw_git(&root, &["add", "-A"]);
    raw_git(&root, &["commit", "-m", "B", "--quiet"]);
    let b = git_out(&root, &["rev-parse", "HEAD"]);
    raw_git(&root, &["checkout", "--quiet", &main]);
    std::fs::write(root.join("a.txt"), "two\n").unwrap();
    raw_git(&root, &["commit", "-am", "two", "--quiet"]);

    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/branches/picks/commits?against={main}"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(
        v["commits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["subject"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["B", "A"],
        "newest first"
    );

    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/cherry-pick"),
        Some(json!({"commits": [a, b], "record_origin": true})),
    )
    .await;
    assert_eq!(status, 409, "{v}");
    assert_eq!(v["detail"]["in_progress"], json!("cherry_pick"));
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/skip"),
        Some(json!({"what": "cherry_pick"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(
        v["files"],
        json!([]),
        "B landed clean and the sequence ended"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).unwrap(),
        "two\n"
    );
    assert_eq!(std::fs::read_to_string(root.join("b.txt")).unwrap(), "b\n");
    assert!(git_out(&root, &["log", "-1", "--format=%B"]).contains("cherry picked from commit"));
    let (status, _) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/cherry-pick"),
        Some(json!({"commits": []})),
    )
    .await;
    assert_eq!(status, 400, "no commits, no pick");
}

/// An interactive rebase planned over HTTP: a reorder, a reword, a squash and
/// a drop land; a plan outside the range is a 400 before any ref.
#[tokio::test(flavor = "multi_thread")]
async fn an_interactive_rebase_plan_lands_over_http() {
    let (dir, socket, pid, root, _stop) = boot().await;
    seed_repo(dir.path(), &root);
    raw_git(&root, &["tag", "base"]);
    let mut ids = Vec::new();
    for (n, subject) in ["one", "two", "three", "four"].iter().enumerate() {
        std::fs::write(root.join(format!("f{}.txt", n + 1)), format!("{n}\n")).unwrap();
        raw_git(&root, &["add", "-A"]);
        raw_git(&root, &["commit", "-m", subject, "--quiet"]);
        ids.push(git_out(&root, &["rev-parse", "HEAD"]));
    }
    let (status, v) = request(&socket, "POST", &format!("/workstreams/{pid}/git/rebase/plan"), Some(json!({"upstream": "base", "steps": [{"action": "pick", "commit": "0000000000000000000000000000000000000000"}]}))).await;
    assert_eq!(status, 400, "{v}");
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/rebase/plan"),
        Some(json!({"upstream": "base", "steps": [
            {"action": "pick", "commit": ids[1]},
            {"action": "reword", "commit": ids[0], "message": "one, reworded"},
            {"action": "drop", "commit": ids[2]},
            {"action": "pick", "commit": ids[3]},
        ]})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert!(v["recovery"]["ref_name"]
        .as_str()
        .unwrap()
        .contains("rebase_plan"));
    assert_eq!(
        git_out(&root, &["log", "--format=%s", "base..HEAD"]),
        "four\none, reworded\ntwo"
    );
    assert!(!root.join("f3.txt").exists());
}

/// A remote branch taken up as a tracking branch, its upstream moved, its
/// standing read, and its delete on the remote through the Publish gate.
#[tokio::test(flavor = "multi_thread")]
async fn a_remote_branch_is_checked_out_with_tracking_and_deleted_on_the_remote_through_the_gate() {
    let (dir, socket, pid, root, _stop) = boot().await;
    seed_repo(dir.path(), &root);
    let main = git_out(&root, &["rev-parse", "--abbrev-ref", "HEAD"]);
    raw_git(&root, &["push", "--quiet", "-u", "origin", &main]);
    raw_git(&root, &["branch", "feature/theirs"]);
    raw_git(&root, &["push", "--quiet", "origin", "feature/theirs"]);
    raw_git(&root, &["branch", "-D", "feature/theirs"]);

    let (status, v) = request(&socket, "POST", &format!("/workstreams/{pid}/git/branches"), Some(json!({"name": "feature/theirs", "start": "origin/feature/theirs", "track": true, "switch": true}))).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["branch"], json!("feature/theirs"));
    let (_, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/branches"),
        None,
    )
    .await;
    let row = v["branches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["name"] == json!("feature/theirs"))
        .unwrap()
        .clone();
    assert_eq!(row["upstream"], json!("origin/feature/theirs"));
    assert_eq!(
        (
            row["ahead"].as_u64(),
            row["behind"].as_u64(),
            row["merged"].as_bool()
        ),
        (Some(0), Some(0), Some(true)),
        "a branch at the default branch's tip is merged into it"
    );

    std::fs::write(root.join("t.txt"), "t\n").unwrap();
    raw_git(&root, &["add", "-A"]);
    raw_git(&root, &["commit", "-m", "theirs", "--quiet"]);
    let (_, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/branches"),
        None,
    )
    .await;
    let row = v["branches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["current"] == json!(true))
        .unwrap()
        .clone();
    assert_eq!(
        (row["ahead"].as_u64(), row["merged"].as_bool()),
        (Some(1), Some(false))
    );

    let (status, v) = request(
        &socket,
        "PUT",
        &format!("/workstreams/{pid}/git/branches/feature%2Ftheirs/upstream"),
        Some(json!({"upstream": null})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert!(v["branches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["name"] == json!("feature/theirs"))
        .unwrap()
        .get("upstream")
        .is_none());
    let (status, _) = request(
        &socket,
        "PUT",
        &format!("/workstreams/{pid}/git/branches/feature%2Ftheirs/upstream"),
        Some(json!({"upstream": "origin/feature/theirs"})),
    )
    .await;
    assert_eq!(status, 200);

    // The default branch is never deleted from here; a manual policy refuses; auto lands.
    let (status, _) = request(
        &socket,
        "DELETE",
        &format!("/workstreams/{pid}/git/remote-branches/origin/{main}"),
        None,
    )
    .await;
    assert_eq!(status, 400);
    let (status, v) = request(
        &socket,
        "PATCH",
        &format!("/projects/{pid}"),
        Some(json!({"publish": "manual"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (status, v) = request(
        &socket,
        "DELETE",
        &format!("/workstreams/{pid}/git/remote-branches/origin/feature%2Ftheirs"),
        None,
    )
    .await;
    assert_eq!(status, 409, "{v}");
    assert_eq!(v["code"], json!("publish_manual"));
    let (status, _) = request(
        &socket,
        "PATCH",
        &format!("/projects/{pid}"),
        Some(json!({"publish": "auto"})),
    )
    .await;
    assert_eq!(status, 200);
    let (status, v) = request(
        &socket,
        "DELETE",
        &format!("/workstreams/{pid}/git/remote-branches/origin/feature%2Ftheirs"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert!(v["recovery"]["ref_name"]
        .as_str()
        .unwrap()
        .contains("push_delete"));
    let origin = dir.path().join("origin.git");
    assert_eq!(
        git_out(&origin, &["branch", "--list", "feature/theirs"]),
        "",
        "gone from the remote"
    );
    let (_, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/remote-branches"),
        None,
    )
    .await;
    assert!(v["remote_branches"]
        .as_array()
        .unwrap()
        .iter()
        .all(|b| b["name"] != json!("feature/theirs")));
}

/// The same request without the `Authorization` header — the token in the
/// query alone, the way an `EventSource` presents it.
async fn request_query_token(
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
    let joined = if path.contains('?') { "&" } else { "?" };
    let request = Request::builder()
        .method(method)
        .uri(format!("{path}{joined}token={TOKEN}"))
        .header(hyper::header::HOST, "localhost")
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

/// A consented verb is minted from the `Authorization` header alone: the
/// token in the query lets a read through the middleware — an `EventSource`
/// has no header — and proves no click.
#[tokio::test(flavor = "multi_thread")]
async fn a_consented_verb_needs_the_token_in_the_header_not_the_query() {
    let (dir, socket, pid, root, _stop) = boot().await;
    seed_repo(dir.path(), &root);
    raw_git(&root, &["branch", "topic"]);
    // A read passes with the query token, as every read does.
    let (status, v) = request_query_token(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/branches"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    // A consented verb does not.
    let (status, v) = request_query_token(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/checkout"),
        Some(json!({"target": "topic"})),
    )
    .await;
    assert_eq!(status, 401, "{v}");
    assert!(
        v["error"].as_str().unwrap_or("").contains("Authorization"),
        "{v}"
    );
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/checkout"),
        Some(json!({"target": "topic"})),
    )
    .await;
    assert_eq!(status, 200, "the header is the proof: {v}");
}

/// The routes the desktop reads for its furniture and its explorer answer:
/// a layout saved and read back whole, the path index, a copy in place, the
/// disposal a delete here would take, and a branch renamed with consent.
#[tokio::test(flavor = "multi_thread")]
async fn the_layout_index_copy_disposal_and_rename_routes_answer() {
    let (dir, socket, pid, root, _stop) = boot().await;
    seed_repo(dir.path(), &root);
    let layout = json!({"version": 5, "tabs": [{"kind": "file", "path": "README.md"}], "active": "file:README.md", "panes": {"kind": "leaf", "id": "d1", "tabs": ["file:README.md"], "active": "file:README.md"}, "pinned": [], "strip": ["file:README.md"]});
    let (status, v) = request(
        &socket,
        "PUT",
        &format!("/ide/layout/workstream/{pid}"),
        Some(layout.clone()),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/ide/layout/workstream/{pid}"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["layout"], layout, "what was saved is what is read");
    let (status, v) = request(
        &socket,
        "GET",
        "/ide/layout/workstream/01J0000000000000000000NOPE",
        None,
    )
    .await;
    assert_eq!(status, 400, "an id that is no id: {v}");
    let (status, v) = request(
        &socket,
        "GET",
        "/ide/layout/workstream/01J00000000000000000000000",
        None,
    )
    .await;
    assert_eq!(status, 404, "an id nothing has: {v}");

    let (status, v) = request(
        &socket,
        "GET",
        &format!("/ide/index/workstream/{pid}"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert!(
        v["paths"].as_array().unwrap().iter().any(|p| p == "a.txt"),
        "the seeded file is indexed: {v}"
    );

    let (status, v) = request(
        &socket,
        "POST",
        &format!("/ide/files/workstream/{pid}/copy"),
        Some(json!({"from": "a.txt", "to": "docs/COPY.md"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["path"], json!("docs/COPY.md"));
    assert!(root.join("docs/COPY.md").is_file());
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/ide/files/workstream/{pid}/copy"),
        Some(json!({"from": "a.txt", "to": "../COPY.md"})),
    )
    .await;
    assert_eq!(status, 400, "a copy out of the root is refused: {v}");

    let (status, v) = request(
        &socket,
        "GET",
        &format!("/ide/files/workstream/{pid}/disposal"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert!(
        matches!(v["disposal"].as_str(), Some("trash") | Some("unlink")),
        "{v}"
    );

    raw_git(&root, &["branch", "feature/old"]);
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/branches/feature%2Fold/rename"),
        Some(json!({"to": "feature/new"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let branches = git_out(&root, &["branch", "--format=%(refname:short)"]);
    assert!(
        branches.contains("feature/new") && !branches.contains("feature/old"),
        "{branches}"
    );
    let main = git_out(&root, &["rev-parse", "--abbrev-ref", "HEAD"]);
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/workstreams/{pid}/git/branches/{main}/rename"),
        Some(json!({"to": "elsewhere"})),
    )
    .await;
    assert_ne!(
        status, 200,
        "the default branch is never renamed from here: {v}"
    );
}

/// A change on a document no server follows is a no-op that answers, and a
/// restart on a language with no server running clears nothing and answers
/// — neither is a way to make the node hold a thread or a process.
#[tokio::test(flavor = "multi_thread")]
async fn an_lsp_change_and_a_restart_answer_without_a_server() {
    let (dir, socket, pid, root, _stop) = boot().await;
    seed_repo(dir.path(), &root);
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/ide/lsp/workstream/{pid}/change"),
        Some(json!({"path": "README.md", "text": "# changed\n"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/ide/lsp/workstream/{pid}/change"),
        Some(json!({"path": "../escape.rs", "text": ""})),
    )
    .await;
    assert_eq!(
        status, 400,
        "a path out of the root never becomes a URI: {v}"
    );
    let (status, v) = request(
        &socket,
        "POST",
        &format!("/ide/lsp/workstream/{pid}/restart"),
        Some(json!({"language": "rust"})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
}

/// A file's history before the first commit is `200` with no commits — the
/// Changes view's *History* shows its empty sentence, never git's refusal.
#[tokio::test(flavor = "multi_thread")]
async fn a_files_history_before_the_first_commit_is_empty_over_http() {
    let (_dir, socket, pid, root, _stop) = boot().await;
    raw_git(&root, &["init", "--quiet", "-b", "main"]);
    std::fs::write(root.join("index.html"), "<h1>hi</h1>\n").unwrap();
    raw_git(&root, &["add", "-A"]);
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/history?path=index.html"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["commits"], json!([]), "{v}");
    let (status, v) = request(
        &socket,
        "GET",
        &format!("/workstreams/{pid}/git/blame?path=index.html"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["lines"], json!([]), "{v}");
}

/// **The editor's two bounds are this machine's to set.** Settings › Editor
/// shows *Editable up to* and *Refuse above*, and nothing read either: a
/// person set them and the editor went on as before. The read side, the
/// write side and the refusal follow them, at once, within what the
/// registry lets them say.
#[tokio::test(flavor = "multi_thread")]
async fn the_editors_bounds_follow_what_this_machine_set() {
    const MIB: u64 = 1024 * 1024;
    let (_dir, socket, pid, root, _stop) = boot().await;
    let sparse = |name: &str, bytes: u64| {
        std::fs::File::create(root.join(name))
            .unwrap()
            .set_len(bytes)
            .unwrap();
    };
    let file = |name: &str| format!("/ide/file/workstream/{pid}?path={name}");
    let set = |values: Value| {
        let socket = socket.clone();
        async move {
            let (status, v) = request(
                &socket,
                "PUT",
                "/settings/machine",
                Some(json!({ "values": values })),
            )
            .await;
            assert!(status == 200 || status == 204, "{status}: {v}");
        }
    };
    std::fs::write(root.join("three.txt"), vec![b'a'; (3 * MIB) as usize]).unwrap();
    sparse("thirty.txt", 30 * MIB);
    sparse("nine.txt", 9 * MIB);

    // As nobody set them: editable to 2 MiB, refused above 20.
    let (status, v) = request(&socket, "GET", &file("three.txt"), None).await;
    assert_eq!(
        (status, &v["editable"]),
        (200, &json!(false)),
        "{}",
        v["size"]
    );
    let (status, v) = request(&socket, "GET", &file("thirty.txt"), None).await;
    assert_eq!((status, &v["limit"]), (413, &json!(20 * MIB)));

    // Editable up to 4, refused above 32: both files are read, one edited.
    set(json!({ "editor.large_file.editable_mib": 4, "editor.large_file.refuse_mib": 32 })).await;
    let (status, v) = request(&socket, "GET", &file("three.txt"), None).await;
    assert_eq!(
        (status, &v["editable"]),
        (200, &json!(true)),
        "{}",
        v["size"]
    );
    let (status, v) = request(&socket, "GET", &file("thirty.txt"), None).await;
    assert_eq!(
        (status, &v["editable"]),
        (200, &json!(false)),
        "{}",
        v["size"]
    );
    // And what the read side calls editable the write side takes whole.
    let text = "\"\n".repeat((2 * MIB) as usize);
    let (status, v) = request(
        &socket,
        "PUT",
        &file("dense.txt"),
        Some(json!({"text": text})),
    )
    .await;
    assert_eq!(status, 200, "a save of 4 MiB, every byte escaped: {v}");
    let over = "a".repeat((4 * MIB + 1) as usize);
    let (status, v) = request(
        &socket,
        "PUT",
        &file("over.txt"),
        Some(json!({"text": over})),
    )
    .await;
    assert_eq!((status, &v["limit"]), (413, &json!(4 * MIB)), "{v}");
    assert!(!root.join("over.txt").exists());

    // Down again: refused above 8, editable to 1.
    set(json!({ "editor.large_file.editable_mib": 1, "editor.large_file.refuse_mib": 8 })).await;
    let (status, v) = request(&socket, "GET", &file("nine.txt"), None).await;
    assert_eq!((status, &v["limit"]), (413, &json!(8 * MIB)));
    let (status, v) = request(&socket, "GET", &file("three.txt"), None).await;
    assert_eq!((status, &v["editable"]), (200, &json!(false)));
    let (status, v) = request(
        &socket,
        "PUT",
        &file("small.txt"),
        Some(json!({"text": "a".repeat((MIB + 1) as usize)})),
    )
    .await;
    assert_eq!((status, &v["limit"]), (413, &json!(MIB)), "{v}");

    // What the registry does not let them say is refused where it is set.
    let (status, v) = request(
        &socket,
        "PUT",
        "/settings/machine",
        Some(json!({ "values": { "editor.large_file.editable_mib": 9 } })),
    )
    .await;
    assert_eq!(status, 400, "{v}");
}

/// A save the read side calls editable is one the write side takes whole —
/// the route is sized from `EDITABLE_BYTES`, not the framework's default —
/// and a text over it is the same 413 the read side answers, with the limit.
#[tokio::test(flavor = "multi_thread")]
async fn a_save_at_the_editable_size_is_taken_whole_and_one_over_it_is_a_413_with_the_limit() {
    let (_dir, socket, pid, root, _stop) = boot().await;
    let cap = bisa_node::ide::EDITABLE_BYTES as usize;
    // Exactly the editable size, of the two characters JSON must escape: the
    // body on the wire is twice the text, and it lands whole.
    let text = "\"\n".repeat(cap / 2);
    let file = format!("/ide/file/workstream/{pid}?path=dense.txt");
    let (status, v) = request(&socket, "PUT", &file, Some(json!({"text": text}))).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(
        std::fs::read_to_string(root.join("dense.txt")).unwrap(),
        text
    );
    let (status, v) = request(&socket, "GET", &file, None).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["editable"], json!(true), "{v}");
    assert_eq!(v["size"], json!(cap));
    // One byte over: refused with the size and the limit, and nothing written.
    let over = "a".repeat(cap + 1);
    let (status, v) = request(
        &socket,
        "PUT",
        &format!("/ide/file/workstream/{pid}?path=over.txt"),
        Some(json!({"text": over})),
    )
    .await;
    assert_eq!(status, 413, "{v}");
    assert_eq!(v["limit"], json!(cap));
    assert_eq!(v["size"], json!(cap + 1));
    assert!(v["error"].as_str().unwrap().contains("over.txt"), "{v}");
    assert!(!root.join("over.txt").exists());
}
