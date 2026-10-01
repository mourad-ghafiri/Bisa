//! The supervisor against processes that exist on every machine — a command
//! that is not there fails typed, a process that exits before answering
//! `initialize` is `Failed` with the reason, never a hang — and against the
//! scripted server built beside these tests (`tests/scripted_lsp.rs`): the
//! handshake, a request answered, a frame that is not one, a crash.

use bisa_lsp::catalog::ServerDescriptor;
use bisa_lsp::server::{Server, ServerState};
use bisa_lsp::LspError;

fn desc(command: &str, args: &[&str]) -> ServerDescriptor {
    ServerDescriptor {
        language: "test".into(),
        command: command.into(),
        args: args.iter().map(|s| s.to_string()).collect(),
        initialization_options: None,
        install_hint: None,
    }
}

#[tokio::test]
async fn a_server_that_exits_at_once_is_a_typed_failure_not_a_hang() {
    let dir = tempfile::tempdir().unwrap();
    // `true` exits 0 without a word: the reader sees EOF, initialize is Closed.
    let err = Server::start(dir.path(), desc("true", &[]))
        .await
        .err()
        .expect("must fail");
    assert!(
        matches!(
            err,
            LspError::Closed | LspError::Spawn(_) | LspError::Timeout(_)
        ),
        "{err}"
    );
}

#[tokio::test]
async fn a_missing_command_fails_typed() {
    let dir = tempfile::tempdir().unwrap();
    let err = Server::start(
        dir.path(),
        desc("definitely-not-a-language-server-9f3a", &[]),
    )
    .await
    .err()
    .expect("must fail");
    assert!(
        matches!(
            err,
            LspError::Closed | LspError::Spawn(_) | LspError::Timeout(_)
        ),
        "{err}"
    );
}

/// The scripted server of this package, by the path cargo built it at.
fn scripted() -> ServerDescriptor {
    desc(env!("CARGO_BIN_EXE_scripted-lsp"), &[])
}

async fn running(dir: &tempfile::TempDir) -> std::sync::Arc<Server> {
    let server = Server::start(dir.path(), scripted())
        .await
        .expect("handshake");
    assert_eq!(server.state(), ServerState::Running);
    server
}

#[tokio::test]
async fn the_handshake_runs_a_request_is_answered_and_a_diagnostic_arrives() {
    let dir = tempfile::tempdir().unwrap();
    let server = running(&dir).await;
    assert_eq!(
        server.capabilities()["hoverProvider"],
        true,
        "the capabilities are the server's"
    );
    let uri = format!("{}/main.rs", bisa_lsp::uri::root_uri(dir.path()));
    let mut notes = server.take_notifications().expect("the listener's end");
    server
        .did_open(&uri, "rust", 1, "fn main() {}\n")
        .await
        .unwrap();
    let note = tokio::time::timeout(std::time::Duration::from_secs(5), notes.recv())
        .await
        .expect("a diagnostic in time")
        .expect("the channel is open");
    assert_eq!(note.method, "textDocument/publishDiagnostics");
    assert_eq!(note.params["uri"], uri);
    let hover = server
        .request(
            "textDocument/hover",
            serde_json::json!({"textDocument": {"uri": uri}, "position": {"line": 0, "character": 3}}),
        )
        .await
        .unwrap();
    assert_eq!(hover["contents"]["value"], format!("hovering {uri}"));
    let refused = server
        .request("textDocument/rename", serde_json::json!({}))
        .await
        .unwrap_err();
    assert!(
        refused.to_string().contains("not scripted"),
        "a server's own error is the answer: {refused}"
    );
    server.shutdown().await;
    assert_eq!(server.state(), ServerState::Stopped);
}

#[tokio::test]
async fn a_frame_that_is_not_one_fails_the_server_with_the_reason() {
    let dir = tempfile::tempdir().unwrap();
    let server = running(&dir).await;
    let uri = format!("{}/malformed.txt", bisa_lsp::uri::root_uri(dir.path()));
    server.did_open(&uri, "text", 1, "").await.unwrap();
    let state = wait_until_not_running(&server).await;
    match state {
        ServerState::Failed { reason } => {
            assert!(reason.contains("protocol"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
    assert!(
        matches!(
            server
                .request("textDocument/hover", serde_json::json!({}))
                .await,
            Err(LspError::Closed)
        ),
        "a failed server takes no request"
    );
}

#[tokio::test]
async fn a_server_that_crashes_mid_session_is_failed_with_what_it_said_last() {
    let dir = tempfile::tempdir().unwrap();
    let server = running(&dir).await;
    let uri = format!("{}/crash.txt", bisa_lsp::uri::root_uri(dir.path()));
    server.did_open(&uri, "text", 1, "").await.unwrap();
    match wait_until_not_running(&server).await {
        ServerState::Failed { reason } => {
            assert!(reason.contains("exited"), "{reason}");
            assert!(
                reason.contains("told to crash"),
                "its last words are the reason: {reason}"
            );
        }
        other => panic!("{other:?}"),
    }
}

async fn wait_until_not_running(server: &Server) -> ServerState {
    for _ in 0..100 {
        let state = server.state();
        if state != ServerState::Running {
            return state;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("the server is still running");
}
