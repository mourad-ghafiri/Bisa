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

/// A document opened on the running scripted server, by name — the name
/// is what asks the server for one of its moods.
async fn open_named(server: &Server, dir: &tempfile::TempDir, name: &str) -> String {
    let uri = format!("{}/{name}", bisa_lsp::uri::root_uri(dir.path()));
    server.did_open(&uri, "text", 1, "").await.unwrap();
    uri
}

/// A server that asks its client things is answered — a configuration
/// item per item asked, the root as its one folder, nothing for anything
/// else — so it never stalls on a question; a frame that is neither a
/// request nor a notification is read past; and a document's change and
/// close reach it as the notifications they are.
#[tokio::test]
async fn the_servers_own_questions_are_answered_and_a_frame_that_is_nothing_is_read_past() {
    let dir = tempfile::tempdir().unwrap();
    let server = running(&dir).await;
    let mut notes = server.take_notifications().expect("the listener's end");
    let uri = open_named(&server, &dir, "chatty.txt").await;
    let heard = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let note = notes.recv().await.expect("the channel is open");
            if note.method == "scripted/answered" {
                return note.params;
            }
        }
    })
    .await
    .expect("the server heard its three answers in time");
    assert_eq!(
        heard["ask-configuration"],
        serde_json::json!([null, null]),
        "one null per item asked: {heard}"
    );
    assert_eq!(
        heard["ask-folders"],
        serde_json::json!([{"uri": bisa_lsp::uri::root_uri(dir.path()), "name": "root"}]),
        "{heard}"
    );
    assert_eq!(heard["ask-oddity"], serde_json::Value::Null, "{heard}");
    assert_eq!(
        server.state(),
        ServerState::Running,
        "a frame that is nothing fails nothing"
    );
    server.did_change(&uri, 2, "changed").await.unwrap();
    server.did_close(&uri).await.unwrap();
    let hover = server
        .request(
            "textDocument/hover",
            serde_json::json!({"textDocument": {"uri": uri}, "position": {"line": 0, "character": 0}}),
        )
        .await
        .unwrap();
    assert_eq!(hover["contents"]["value"], format!("hovering {uri}"));
    server.shutdown().await;
}

/// Nobody listening for notifications is the engine letting the server go:
/// the diagnostic it sends is dropped, and the server is none the worse.
#[tokio::test]
async fn a_notification_nobody_listens_for_is_let_go_and_the_server_still_answers() {
    let dir = tempfile::tempdir().unwrap();
    let server = running(&dir).await;
    drop(
        server
            .take_notifications()
            .expect("the listener's end, dropped"),
    );
    assert!(server.take_notifications().is_none(), "taken once");
    let uri = open_named(&server, &dir, "quiet.txt").await;
    // The diagnostic went nowhere; the server answers as before.
    let hover = server
        .request(
            "textDocument/hover",
            serde_json::json!({"textDocument": {"uri": uri}, "position": {"line": 0, "character": 0}}),
        )
        .await
        .unwrap();
    assert_eq!(hover["contents"]["value"], format!("hovering {uri}"));
    assert_eq!(server.state(), ServerState::Running);
    server.shutdown().await;
}

/// A server that wrote more to stderr than the supervisor keeps is failed
/// with its last words, not its first.
#[tokio::test]
async fn a_server_that_exits_after_a_long_stderr_is_failed_with_its_last_words_only() {
    let dir = tempfile::tempdir().unwrap();
    let server = running(&dir).await;
    open_named(&server, &dir, "loud.txt").await;
    match wait_until_not_running(&server).await {
        ServerState::Failed { reason } => {
            assert!(reason.contains("the last words"), "{reason}");
            assert!(
                reason.len() < 2048 + 64,
                "cut to the tail: {}",
                reason.len()
            );
            assert!(
                !reason.contains(&"x".repeat(2500)),
                "the beginning is gone: {}",
                reason.len()
            );
        }
        other => panic!("{other:?}"),
    }
}

/// A shutdown the server refuses, and one it never answers — nor the exit
/// after it — both end with the process gone and the server `Stopped`; a
/// second shutdown finds no process to end; and a document opened after is
/// refused as closed.
#[tokio::test]
async fn a_shutdown_refused_or_ignored_still_ends_the_process() {
    let dir = tempfile::tempdir().unwrap();
    let refusing = running(&dir).await;
    open_named(&refusing, &dir, "refuse-shutdown.txt").await;
    refusing.shutdown().await;
    assert_eq!(refusing.state(), ServerState::Stopped);
    // The reader hears the end of the process after the stop and leaves the
    // word as it is.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert_eq!(
        refusing.state(),
        ServerState::Stopped,
        "stopped, never failed"
    );

    let dir = tempfile::tempdir().unwrap();
    let deaf = running(&dir).await;
    let uri = open_named(&deaf, &dir, "slow-shutdown.txt").await;
    let began = std::time::Instant::now();
    deaf.shutdown().await;
    let took = began.elapsed();
    assert!(
        took >= std::time::Duration::from_secs(4),
        "three seconds for the answer and two for the exit: {took:?}"
    );
    assert_eq!(deaf.state(), ServerState::Stopped);
    deaf.shutdown().await;
    assert_eq!(
        deaf.state(),
        ServerState::Stopped,
        "a second shutdown finds nothing to end"
    );
    assert!(
        matches!(
            deaf.did_open(&uri, "text", 2, "").await,
            Err(LspError::Closed)
        ),
        "the pipe to the server is gone"
    );
}

/// A request still waiting when the server is shut down, or when it
/// crashes, hears the close now rather than never.
#[tokio::test]
async fn a_request_still_waiting_hears_the_close_at_a_shutdown_and_at_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    let server = running(&dir).await;
    let uri = open_named(&server, &dir, "mute.txt").await;
    let hover = |server: std::sync::Arc<Server>, uri: String| async move {
        server
            .request(
                "textDocument/hover",
                serde_json::json!({"textDocument": {"uri": uri}, "position": {"line": 0, "character": 0}}),
            )
            .await
    };
    let waiting = tokio::spawn(hover(std::sync::Arc::clone(&server), uri.clone()));
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    server.shutdown().await;
    let heard = tokio::time::timeout(std::time::Duration::from_secs(5), waiting)
        .await
        .expect("the waiter hears the close at the shutdown")
        .unwrap();
    assert!(matches!(heard, Err(LspError::Closed)), "{heard:?}");

    let dir = tempfile::tempdir().unwrap();
    let server = running(&dir).await;
    let uri = open_named(&server, &dir, "mute.txt").await;
    let waiting = tokio::spawn(hover(std::sync::Arc::clone(&server), uri));
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    open_named(&server, &dir, "crash.txt").await;
    let heard = tokio::time::timeout(std::time::Duration::from_secs(5), waiting)
        .await
        .expect("the waiter hears the close at the crash")
        .unwrap();
    assert!(matches!(heard, Err(LspError::Closed)), "{heard:?}");
    assert!(matches!(
        wait_until_not_running(&server).await,
        ServerState::Failed { .. }
    ));
}
