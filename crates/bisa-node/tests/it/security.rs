//! The security routes over the node's socket: the status names rules and
//! never a value, a preview redacts on a scratch vault, the guard preview
//! names the rule, and the terminal guard route answers for a session alone.
//! Fakes only — a GitHub-shaped token that is not one, and commands that are
//! matched, never run.

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
const FAKE_TOKEN: &str = "ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";

async fn boot() -> (tempfile::TempDir, PathBuf, tokio::sync::oneshot::Sender<()>) {
    boot_with(|_| {}).await
}

/// Boot with the workspace seeded first — a row the routes will find.
async fn boot_with(
    seed: impl FnOnce(&Workspace),
) -> (tempfile::TempDir, PathBuf, tokio::sync::oneshot::Sender<()>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let data = dir.path().to_path_buf();
    let ws = Workspace::open_with_keystore(
        &data,
        Box::new(FileKeyStore::new(Paths::new(&data).identity_dir())),
    )
    .expect("workspace");
    seed(&ws);
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
        token: Some(TOKEN.to_string()),
        #[cfg(feature = "a2a")]
        a2a: None,
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
    bearer: &str,
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
        .header(hyper::header::CONTENT_TYPE, "application/json")
        .header(hyper::header::AUTHORIZATION, format!("Bearer {bearer}"))
        .body(Full::new(Bytes::from(
            body.map(|b| b.to_string()).unwrap_or_default(),
        )))
        .unwrap();
    let resp = sender.send_request(request).await.expect("request");
    let status = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    };
    (status, value)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_status_lists_the_rules_and_carries_no_secret() {
    let (_dir, socket, stop) = boot().await;
    let (code, status) = request(&socket, "GET", "/security/status", None, TOKEN).await;
    assert_eq!(code, 200, "{status}");
    assert_eq!(status["redactor_enabled"], true);
    assert_eq!(status["guard_enabled"], true);
    let redact = status["redact_rules"].as_array().unwrap();
    assert!(redact
        .iter()
        .any(|r| r["id"] == "github_token" && r["origin"] == "builtin" && r["enabled"] == true));
    let guard = status["guard_rules"].as_array().unwrap();
    assert!(guard
        .iter()
        .any(|r| r["id"] == "privilege_escalation" && r["action"] == "deny"));
    assert!(guard.iter().any(|r| r["action"] == "classify"));
    for id in ["harness_fetch", "harness_web_search"] {
        assert!(
            guard
                .iter()
                .any(|r| r["id"] == id && r["action"] == "ask" && r["origin"] == "builtin"),
            "the harness's own web tools are asked, since what they fetch is not screened: {id}"
        );
    }
    // The four rules that steer an agent to the platform's own tools say so:
    // they apply to the platform's sessions alone, and a terminal harness is
    // passed over. Every other rule carries no scope and applies everywhere.
    for id in [
        "machine_browser",
        "browser_test_runner",
        "harness_fetch",
        "harness_web_search",
    ] {
        assert!(
            guard
                .iter()
                .any(|r| r["id"] == id && r["applies_to"] == "platform"),
            "{id} applies to the platform's sessions alone: {guard:?}"
        );
    }
    assert!(
        guard
            .iter()
            .any(|r| r["id"] == "privilege_escalation" && r.get("applies_to").is_none()),
        "a rule that protects the machine names no scope: it applies everywhere"
    );
    assert_eq!(
        status["content"]["screen"],
        json!(true),
        "the content screen is on by default"
    );
    assert_eq!(status["classifier"]["agent"], "general-agent");
    assert_eq!(
        status["classifier"]["provider"], "agent",
        "the classifier's own agent reads, until somebody picks another reader"
    );
    assert_eq!(status["classifier"]["harness"], "claude-code");
    assert_eq!(status["classifier"]["model"], "claude-sonnet-5-5[1m]");
    assert_eq!(status["classifier"]["effort"], "high");
    assert_eq!(status["classifier"]["on_harmful"], "ask");
    assert_eq!(
        status["classifier_ready"], false,
        "no harness is registered on this node"
    );
    assert_eq!(status["problems"], json!([]));
    assert_eq!(status["recent"], json!([]));
    assert_eq!(status["vault_size"], 0);
    let _server_gone = stop.send(());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_redact_preview_answers_a_placeholder_and_teaches_the_vault_nothing() {
    let (_dir, socket, stop) = boot().await;
    let text = format!("deploy with {FAKE_TOKEN} tonight");
    let (code, first) = request(
        &socket,
        "POST",
        "/security/redact-preview",
        Some(json!({ "text": text })),
        TOKEN,
    )
    .await;
    assert_eq!(code, 200, "{first}");
    assert_eq!(first["count"], 1);
    assert_eq!(first["kinds"], json!(["github_token"]));
    let redacted = first["text"].as_str().unwrap();
    assert!(
        !redacted.contains(FAKE_TOKEN) && redacted.contains("«secret:github_token:"),
        "{redacted}"
    );
    let (_, second) = request(
        &socket,
        "POST",
        "/security/redact-preview",
        Some(json!({ "text": text })),
        TOKEN,
    )
    .await;
    assert_ne!(
        second["text"], first["text"],
        "each preview runs on its own scratch vault"
    );
    let (_, status) = request(&socket, "GET", "/security/status", None, TOKEN).await;
    assert_eq!(status["vault_size"], 0, "the real vault learned nothing");
    let (code, _) = request(
        &socket,
        "POST",
        "/security/redact-preview",
        Some(json!({})),
        TOKEN,
    )
    .await;
    assert_eq!(code, 400, "a body without text is refused, not ignored");
    let _server_gone = stop.send(());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_guard_preview_names_the_rule_and_records_nothing() {
    let (_dir, socket, stop) = boot().await;
    let (code, deny) = request(
        &socket,
        "POST",
        "/security/guard-preview",
        Some(json!({ "tool": "Bash", "input": { "command": "sudo make install" } })),
        TOKEN,
    )
    .await;
    assert_eq!(code, 200, "{deny}");
    assert_eq!(deny["verdict"], "deny");
    assert_eq!(deny["rule"], "privilege_escalation");
    assert_eq!(deny["label"], "sudo, doas, su");
    let (_, read) = request(
        &socket,
        "POST",
        "/security/guard-preview",
        Some(json!({ "tool": "Read", "input": { "file_path": "/proj/.env.local" } })),
        TOKEN,
    )
    .await;
    assert_eq!(read["verdict"], "deny");
    assert_eq!(read["rule"], "dotenv");
    assert_eq!(read["paths"], json!(["/proj/.env.local"]));
    // The preview judges as a session the platform drives would be judged:
    // the machine's browser is refused with the tools named, and the
    // harness's own page fetch is asked under its harness-neutral name.
    let (_, opened) = request(
        &socket,
        "POST",
        "/security/guard-preview",
        Some(json!({ "tool": "Bash", "input": { "command": "open https://example.com" } })),
        TOKEN,
    )
    .await;
    assert_eq!(opened["verdict"], "deny", "{opened}");
    assert_eq!(opened["rule"], "machine_browser");
    assert!(
        opened["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("browser_open"),
        "{opened}"
    );
    let (_, fetch) = request(
        &socket,
        "POST",
        "/security/guard-preview",
        Some(json!({ "tool": "WebFetch", "input": { "url": "https://example.com" } })),
        TOKEN,
    )
    .await;
    assert_eq!(fetch["verdict"], "ask", "{fetch}");
    assert_eq!(fetch["rule"], "harness_fetch", "WebFetch is read as fetch");
    let (_, quiet) = request(
        &socket,
        "POST",
        "/security/guard-preview",
        Some(json!({ "tool": "Bash", "input": { "command": "cargo test" } })),
        TOKEN,
    )
    .await;
    assert_eq!(quiet["verdict"], "fallthrough");
    let (code, _) = request(
        &socket,
        "POST",
        "/security/guard-preview",
        Some(json!({ "tool": "  " })),
        TOKEN,
    )
    .await;
    assert_eq!(code, 400);
    let (_, status) = request(&socket, "GET", "/security/status", None, TOKEN).await;
    assert_eq!(status["recent"], json!([]), "a preview is not a decision");
    let _server_gone = stop.send(());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_terminal_guard_route_wants_a_sessions_secret() {
    let (_dir, socket, stop) = boot().await;
    let payload = json!({ "payload": { "hook_event_name": "PreToolUse", "tool_name": "Bash", "tool_input": { "command": "ls" } } });
    let (code, body) = request(
        &socket,
        "POST",
        "/sessions/01J8ZQ0000000000000000SESS/guard",
        Some(payload),
        "not-a-secret",
    )
    .await;
    assert_eq!(
        code, 404,
        "the door is the session's: a session nobody opened is not found — {body}"
    );
    assert_eq!(
        body["text"]["id"],
        json!("error-engine-interactive-unknown-session"),
        "the handler's refusal, in the platform's words: {body}"
    );
    let _server_gone = stop.send(());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_status_counts_the_environment_detectors_and_says_whether_they_are_on() {
    let (_dir, socket, stop) = boot().await;
    let (code, status) = request(&socket, "GET", "/security/status", None, TOKEN).await;
    assert_eq!(code, 200, "{status}");
    assert_eq!(status["env_auto"], true);
    assert!(
        status["env_detectors"].is_u64(),
        "a count, never a name or a value: {}",
        status["env_detectors"]
    );
    let text = status.to_string();
    assert!(
        !text.contains(TOKEN),
        "the status carried the node's own token"
    );
    let _server_gone = stop.send(());
}

/// A transcript is the harness's own file, written in its own words: the node
/// redacts what it serves of it.
#[tokio::test(flavor = "multi_thread")]
async fn the_transcript_route_redacts_what_the_harness_wrote() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("fake-transcript.log");
    std::fs::write(
        &path,
        format!("tool: Bash\ncommand: gh auth login --with-token {FAKE_TOKEN}\n"),
    )
    .unwrap();
    let transcript = path.display().to_string();
    let (_dir, socket, stop) = boot_with(move |ws| {
        ws.record_session(&bisa_store::SessionRow {
            id: "sess-redacted".into(),
            adapter: "mock".into(),
            transcript_path: Some(transcript),
            status: bisa_store::SessionStatus::Parked,
            ..Default::default()
        })
        .unwrap();
    })
    .await;
    let (code, body) = request(
        &socket,
        "GET",
        "/sessions/sess-redacted/transcript",
        None,
        TOKEN,
    )
    .await;
    assert_eq!(code, 200, "{body}");
    let text = body["text"].as_str().unwrap_or_default();
    assert!(
        !text.contains(FAKE_TOKEN),
        "the route served the token: {text}"
    );
    assert!(text.contains("«secret:github_token:"), "{text}");
    assert!(text.contains("gh auth login"));
    assert!(body["next_byte"].as_u64().unwrap() > 0);
    let _server_gone = stop.send(());
}

/// A transcript is paged a mebibyte at a time, of bytes; a page that ends
/// inside a multibyte character keeps that character for the next page, and
/// the cursor always moves — a reader never meets a 500 at an offset it can
/// only ask for again.
#[tokio::test(flavor = "multi_thread")]
async fn the_transcript_route_pages_on_character_boundaries_and_never_sticks() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("wide-transcript.log");
    let page = 1024 * 1024;
    // ASCII up to one byte short of the page, then `é` (two bytes) straddles
    // the cut, then a tail.
    let mut whole = "a".repeat(page - 1);
    whole.push_str("é tail\n");
    std::fs::write(&path, &whole).unwrap();
    let transcript = path.display().to_string();
    let (_dir, socket, stop) = boot_with(move |ws| {
        ws.record_session(&bisa_store::SessionRow {
            id: "sess-wide".into(),
            adapter: "mock".into(),
            transcript_path: Some(transcript),
            status: bisa_store::SessionStatus::Parked,
            ..Default::default()
        })
        .unwrap();
    })
    .await;
    let (code, first) = request(
        &socket,
        "GET",
        "/sessions/sess-wide/transcript",
        None,
        TOKEN,
    )
    .await;
    assert_eq!(code, 200, "{first}");
    let next = first["next_byte"].as_u64().unwrap();
    assert_eq!(
        next as usize,
        page - 1,
        "the straddling character waits for the next page"
    );
    assert_eq!(first["text"].as_str().unwrap().len(), page - 1);
    let (code, second) = request(
        &socket,
        "GET",
        &format!("/sessions/sess-wide/transcript?from_byte={next}"),
        None,
        TOKEN,
    )
    .await;
    assert_eq!(code, 200, "{second}");
    assert_eq!(second["text"].as_str().unwrap(), "é tail\n");
    assert_eq!(second["next_byte"].as_u64().unwrap() as usize, whole.len());
    // An offset the client made up, inside the character: read lossily, and
    // the cursor still moves past it.
    let (code, torn) = request(
        &socket,
        "GET",
        &format!("/sessions/sess-wide/transcript?from_byte={}", next + 1),
        None,
        TOKEN,
    )
    .await;
    assert_eq!(code, 200, "{torn}");
    assert_eq!(torn["next_byte"].as_u64().unwrap() as usize, whole.len());
    assert!(torn["text"].as_str().unwrap().ends_with(" tail\n"));
    let _server_gone = stop.send(());
}
