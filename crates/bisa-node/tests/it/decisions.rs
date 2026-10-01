//! The Decision-Making Agent over the node's socket: the status names who
//! answers and never a key, a key is kept and not read back, *try it* holds
//! both ways to the contract and records nothing, and the judgements come back
//! newest first.
//! Fakes only — a scripted provider stands in for the one the settings name,
//! and the key is a fixture that opens nothing.

use bisa_decision::ScriptedProvider;
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
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UnixStream;

pub(crate) const TOKEN: &str = "test-token-0123456789abcdef";
/// A fixture: it opens nothing.
const FIXTURE_KEY: &str = "fixture-key-opens-nothing";

pub(crate) async fn boot_with(
    prepare: impl FnOnce(&Engine),
) -> (tempfile::TempDir, PathBuf, tokio::sync::oneshot::Sender<()>) {
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
            ..Default::default()
        },
    )
    .expect("engine");
    prepare(&engine);
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

pub(crate) async fn request(
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
async fn the_status_says_who_answers_and_that_it_is_off() {
    let (_dir, socket, stop) = boot_with(|_| {}).await;
    let (code, status) = request(&socket, "GET", "/decisions/status", None, TOKEN).await;
    assert_eq!(code, 200, "{status}");
    assert_eq!(status["agent"]["id"], "decision-making-agent");
    assert_eq!(status["agent"]["name"], "Decision-Making Agent");
    assert!(
        status.get("model").is_none(),
        "the status names the agent once, under `agent`: {status}"
    );
    assert_eq!(status["enabled"], false);
    assert_eq!(status["provider"], "harness");
    assert_eq!(status["answers_as"], "claude-code/claude-sonnet-5-5[1m]");
    assert_eq!(
        status["agent"]["default_effort"], "high",
        "what answers out of the box says how hard it works"
    );
    assert!(
        status.get("effort").is_none(),
        "no harness here says what the model takes: {status}"
    );
    assert_eq!(status["calibrated"], false);
    assert_eq!(
        status["ready"], false,
        "no harness is registered on this node"
    );
    assert!(status.get("key_stored").is_none(), "a harness takes no key");
    let points = status["points"].as_array().unwrap();
    assert_eq!(points.len(), 11);
    assert!(points.iter().all(|p| p["on"] == p["selected_explicitly"]));
    // The effort point is the second, and is on where it is selected.
    assert_eq!(points[0]["point"], "model.route");
    assert_eq!(points[1]["point"], "model.effort");
    assert_eq!(points[1]["selected_explicitly"], true);
    let _server_gone = stop.send(());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_key_is_kept_and_never_read_back() {
    let (_dir, socket, stop) = boot_with(|_| {}).await;
    let body = json!({ "key": FIXTURE_KEY });
    let (code, said) = request(
        &socket,
        "PUT",
        "/decisions/key/jev",
        Some(body.clone()),
        TOKEN,
    )
    .await;
    assert_eq!(code, 200, "{said}");
    assert_eq!(said, json!({ "key_stored": true }));

    let (_, status) = request(&socket, "GET", "/decisions/status", None, TOKEN).await;
    assert!(!status.to_string().contains(FIXTURE_KEY), "{status}");

    // A provider that takes no key, and a word that is no provider.
    for provider in ["harness", "agent", "oracle"] {
        let (code, _) = request(
            &socket,
            "PUT",
            &format!("/decisions/key/{provider}"),
            Some(body.clone()),
            TOKEN,
        )
        .await;
        assert_eq!(code, 400, "{provider}");
    }
    let (code, said) = request(&socket, "DELETE", "/decisions/key/jev", None, TOKEN).await;
    assert_eq!((code, said), (200, json!({ "key_stored": false })));
    let _server_gone = stop.send(());
}

#[tokio::test(flavor = "multi_thread")]
async fn try_it_answers_in_the_contract_and_records_nothing() {
    let (_dir, socket, stop) = boot_with(|engine| {
        let answer =
            ScriptedProvider::answers("q", ScriptedProvider::choice("b", &["a", "b"], 0.8));
        engine.stand_in_decision_provider(Some(Arc::new(ScriptedProvider::new(vec![answer]))));
    })
    .await;
    let ask = json!({
        "state": "the build is red",
        "questions": { "q": {
            "type": "choice", "instructions": "Which?",
            "criteria": { "a": "first", "b": "second" }
        } }
    });
    let (code, response) = request(&socket, "POST", "/decisions/try", Some(ask), TOKEN).await;
    assert_eq!(code, 200, "{response}");
    assert_eq!(response["model"], "scripted");
    assert_eq!(response["answers"]["q"]["choice"], "b");
    assert_eq!(response["usage"]["input_tokens"], 0);

    // A request that breaks the contract is refused, saying how.
    let broken = json!({ "state": "s", "questions": { "q": {
        "type": "choice", "instructions": "Which?", "criteria": { "only": "one" }
    } } });
    let (code, said) = request(&socket, "POST", "/decisions/try", Some(broken), TOKEN).await;
    assert_eq!(code, 400, "{said}");
    assert!(said.to_string().contains("options"), "{said}");
    // … and so is a key the contract does not name.
    let stray = json!({ "state": "s", "questions": {}, "model": "mine" });
    let (code, _) = request(&socket, "POST", "/decisions/try", Some(stray), TOKEN).await;
    assert!((400..500).contains(&code), "{code}");

    let (code, recent) = request(&socket, "GET", "/decisions?limit=5", None, TOKEN).await;
    assert_eq!((code, recent), (200, json!([])));
    let _server_gone = stop.send(());
}

/// What the provider did is what the status says: a contract break is the
/// 400 of a bad request, a refusal or an unreadable answer the 502 of a bad
/// gateway, a provider not set up or busy or out of reach a 503, one that ran
/// out of time a 504 — and a request that is a document is refused before any
/// provider is asked.
#[tokio::test(flavor = "multi_thread")]
async fn try_it_answers_with_the_status_of_what_the_provider_did() {
    use bisa_decision::ProviderError as PE;
    let (_dir, socket, stop) = boot_with(|engine| {
        let script = vec![
            Err(PE::TimedOut),
            Err(PE::Busy {
                status: 429,
                retry_after: None,
            }),
            Err(PE::Unreachable("connection refused".into())),
            Err(PE::Misconfigured("no API key is stored".into())),
            Err(PE::Refused {
                status: 401,
                message: "bad key".into(),
            }),
            Err(PE::Unreadable("<html>".into())),
        ];
        engine.stand_in_decision_provider(Some(Arc::new(ScriptedProvider::new(script))));
    })
    .await;
    let ask = || {
        json!({
            "state": "the build is red",
            "questions": { "q": {
                "type": "choice", "instructions": "Which?",
                "criteria": { "a": "first", "b": "second" }
            } }
        })
    };
    for (expected, word) in [
        (504, "in time"),
        (503, "busy"),
        (503, "cannot be reached"),
        (503, "not set up"),
        (502, "refused"),
        (502, "cannot be read"),
    ] {
        let (code, said) = request(&socket, "POST", "/decisions/try", Some(ask()), TOKEN).await;
        assert_eq!(code, expected, "{said}");
        assert!(said.to_string().contains(word), "{said}");
    }
    // The script is spent; a document is refused before it would be asked.
    let mut big = ask();
    big["state"] = json!("x".repeat(bisa_core::MAX_REQUEST_BYTES + 1));
    let (code, said) = request(&socket, "POST", "/decisions/try", Some(big), TOKEN).await;
    assert_eq!(code, 400, "{said}");
    assert!(said.to_string().contains("too large"), "{said}");
    let (code, recent) = request(&socket, "GET", "/decisions?limit=5", None, TOKEN).await;
    assert_eq!(
        (code, recent),
        (200, json!([])),
        "try records nothing, however it went"
    );
    let _server_gone = stop.send(());
}
