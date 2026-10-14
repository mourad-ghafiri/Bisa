//! Language servers under the engine's settings: a server that cannot start
//! is an error each time and, after `MAX_STARTS_PER_MINUTE`, a verdict the
//! engine keeps until a person asks again — or until the `lsp.*` settings
//! change, which is the ask (`lsp::refresh_for`, through
//! `settings::refresh`, behind the one settings door). Every "server" there is a program that does
//! not exist: nothing is spawned but the attempt to.
//!
//! And one server per root and language, however many documents open at
//! once: the "server" of those tests is a few lines of shell written by the
//! test — it says it started, answers `initialize`, and reads until it is
//! told to go. No language server of this machine is ever started.

use crate::common::engine_with;
use bisa_core::SettingScope;
use bisa_core::WorkstreamId;
use bisa_engine::lsp;
use bisa_store::{FileScope, NewProject};
use serde_json::json;

const NOWHERE: &str = "bisa-no-such-language-server";

#[tokio::test(flavor = "multi_thread")]
async fn a_change_to_the_lsp_settings_forgets_the_crash_loop_verdict_and_disabling_stops_every_start(
) {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let ws = engine.workspace();
    let project = ws
        .create_project(NewProject::managed("lab").unwrap())
        .unwrap();
    let root = ws.project_root_path(&project);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
    // A project's root is its primary workstream: the scope the editor uses.
    let wid = WorkstreamId::primary_of(project.id).to_string();
    engine
        .set_setting(
            SettingScope::Machine,
            None,
            "lsp.servers",
            json!([{"language": "rust", "command": NOWHERE}]),
        )
        .unwrap();
    let inner = engine.inner();
    let open = || {
        lsp::open(
            inner,
            FileScope::Workstream,
            &wid,
            "main.rs",
            "fn main() {}\n",
        )
    };

    // A program that does not exist fails to start: an error, each time it
    // is tried — three tries a minute.
    for attempt in 1..=lsp::MAX_STARTS_PER_MINUTE {
        let r = open().await;
        assert!(r.is_err(), "attempt {attempt} tried to start: {r:?}");
    }
    // The fourth is the verdict: given up, no start, no error.
    let given_up = open().await.unwrap();
    assert_eq!(
        given_up,
        lsp::DocumentOpened {
            language: Some("rust".into()),
            following: false
        },
        "given up after the crash loop: the language is still named, nothing follows"
    );
    assert_eq!(open().await.unwrap(), given_up, "and it stays given up");
    let row = lsp::status(inner, FileScope::Workstream, &wid)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.language == "rust")
        .expect("the rust row");
    assert!(
        matches!(&row.state, bisa_lsp::server::ServerState::Failed { reason } if reason.contains(NOWHERE)),
        "the status says why it is given up: {row:?}"
    );

    // The setting changed: the verdict is forgotten and the next open tries
    // again — and fails again, since the program still does not exist.
    engine
        .set_setting(
            SettingScope::Machine,
            None,
            "lsp.servers",
            json!([{"language": "rust", "command": NOWHERE, "args": ["--stdio"]}]),
        )
        .unwrap();
    assert!(
        open().await.is_err(),
        "a start is tried again after the change"
    );

    // Servers off: nothing is tried, nothing fails — and a request has no
    // server to go to.
    engine
        .set_setting(SettingScope::Machine, None, "lsp.enabled", json!(false))
        .unwrap();
    assert!(
        !open().await.unwrap().following,
        "disabled: no start, no error"
    );
    let asked = lsp::request(
        inner,
        FileScope::Workstream,
        &wid,
        "main.rs",
        "textDocument/hover",
        json!({}),
    )
    .await;
    assert!(
        matches!(asked, Err(bisa_engine::EngineError::Invalid(_))),
        "no server configured for the language: {asked:?}"
    );
    engine.shutdown().await;
}

/// A program that stands in for a language server: it writes a line beside
/// itself each time it starts, waits for the client's first word, answers
/// `initialize` (the client's first request, id 1) and then reads until its
/// input closes.
#[cfg(unix)]
fn a_server_that_counts_its_starts(dir: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("counting-server");
    std::fs::write(
        &path,
        r#"#!/bin/sh
echo started >> "$0.starts"
read -r _first
body='{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}'
printf 'Content-Length: %s\r\n\r\n%s' "${#body}" "$body"
cat > /dev/null
"#,
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(unix)]
fn starts_of(server: &std::path::Path) -> usize {
    std::fs::read_to_string(format!("{}.starts", server.display()))
        .map(|said| said.lines().count())
        .unwrap_or(0)
}

/// A workbench that restores its tabs opens every document at once. One
/// server is started for them — not one each, of which the fourth would be
/// taken for a crash loop and the language given up on — and every document
/// is open on it. Another root gets a server of its own; servers switched
/// off at this machine start none.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn documents_opened_at_once_share_one_server_and_each_root_has_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let tools = tempfile::tempdir().unwrap();
    let server = a_server_that_counts_its_starts(tools.path());
    let engine = engine_with(&dir, vec![]);
    let ws = engine.workspace();
    let mut roots = Vec::new();
    for slug in ["lab", "shed"] {
        let project = ws
            .create_project(NewProject::managed(slug).unwrap())
            .unwrap();
        let root = ws.project_root_path(&project);
        std::fs::create_dir_all(&root).unwrap();
        for n in 0..6 {
            std::fs::write(root.join(format!("f{n}.rs")), "fn main() {}\n").unwrap();
        }
        roots.push(WorkstreamId::primary_of(project.id).to_string());
    }
    engine
        .set_setting(
            SettingScope::Machine,
            None,
            "lsp.servers",
            json!([{"language": "rust", "command": server.display().to_string()}]),
        )
        .unwrap();
    let inner = engine.inner();
    let open = |wid: &str, n: usize| {
        let (wid, path) = (wid.to_string(), format!("f{n}.rs"));
        async move { lsp::open(inner, FileScope::Workstream, &wid, &path, "fn main() {}\n").await }
    };

    // Six at once in one root: more than a crash loop's worth of starts.
    let opened = futures::future::join_all((0..6).map(|n| open(&roots[0], n))).await;
    for (n, answer) in opened.iter().enumerate() {
        assert_eq!(
            answer.as_ref().ok(),
            Some(&lsp::DocumentOpened {
                language: Some("rust".into()),
                following: true
            }),
            "document {n} is followed by the server: {answer:?}"
        );
    }
    assert_eq!(starts_of(&server), 1, "one server for the six");
    let rust = |rows: Vec<lsp::ServerStatusRow>| {
        rows.into_iter()
            .find(|row| row.language == "rust")
            .expect("the rust row")
    };
    let row = rust(
        lsp::status(inner, FileScope::Workstream, &roots[0])
            .await
            .unwrap(),
    );
    assert_eq!(row.documents, 6, "every document is open on it: {row:?}");
    assert!(
        matches!(row.state, bisa_lsp::server::ServerState::Running),
        "{row:?}"
    );

    // Another root is another server, and the first keeps its documents.
    assert!(open(&roots[1], 0).await.unwrap().following);
    assert_eq!(starts_of(&server), 2, "a server per root");
    let other = rust(
        lsp::status(inner, FileScope::Workstream, &roots[1])
            .await
            .unwrap(),
    );
    assert_eq!(other.documents, 1, "{other:?}");

    // What the editor may ask is a list; a path that leaves the root is
    // refused before a server reads it.
    let refused = lsp::request(
        inner,
        FileScope::Workstream,
        &roots[0],
        "f0.rs",
        "textDocument/rename",
        json!({}),
    )
    .await;
    assert!(refused.is_err(), "not a request the editor may proxy");
    let outside = lsp::open(
        inner,
        FileScope::Workstream,
        &roots[0],
        "../shed/f0.rs",
        "fn main() {}\n",
    )
    .await;
    assert!(outside.is_err(), "a document outside its root: {outside:?}");

    // Switched off at this machine: every server stops, and none starts.
    engine
        .set_setting(SettingScope::Machine, None, "lsp.enabled", json!(false))
        .unwrap();
    assert_eq!(
        open(&roots[0], 0).await.unwrap(),
        lsp::DocumentOpened {
            language: Some("rust".into()),
            following: false
        },
        "off: the language is named, nothing follows"
    );
    assert_eq!(
        starts_of(&server),
        2,
        "nothing starts while servers are off"
    );
    let off = rust(
        lsp::status(inner, FileScope::Workstream, &roots[0])
            .await
            .unwrap(),
    );
    assert_eq!(off.documents, 0, "{off:?}");
    engine.shutdown().await;
}

/// A language server from a few lines of shell, reading every frame whole
/// by its `Content-Length`: it answers `initialize`, a hover and the
/// shutdown, says a document it is given to *diagnose* has a diagnostic —
/// after a log message and a progress the engine keeps to itself — exits
/// with a word on stderr when told to *crash*, and writes a line beside
/// itself each time it starts.
#[cfg(unix)]
fn a_server_that_speaks(dir: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("speaking-server");
    std::fs::write(
        &path,
        r#"#!/bin/sh
echo started >> "$0.starts"
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
    *'"method":"textDocument/didOpen"'*)
      uri=${body#*\"uri\":\"}
      uri=${uri%%\"*}
      case "$body" in
        *crash*) echo "told to crash" >&2; exit 3 ;;
        *diagnose*)
          send '{"jsonrpc":"2.0","method":"window/logMessage","params":{"type":3,"message":"kept to the engine"}}'
          send '{"jsonrpc":"2.0","method":"$/progress","params":{"token":1,"value":{}}}'
          send '{"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{"uri":"'"$uri"'","diagnostics":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"message":"spoken","severity":2}]}}' ;;
      esac ;;
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

/// A project with a source file, as the editor's workstream scope.
#[cfg(unix)]
fn a_rust_project(engine: &bisa_engine::Engine, slug: &str) -> String {
    let ws = engine.workspace();
    let project = ws
        .create_project(NewProject::managed(slug).unwrap())
        .unwrap();
    let root = ws.project_root_path(&project);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
    WorkstreamId::primary_of(project.id).to_string()
}

/// The next `lsp` frame of this method on the bus, in time.
#[cfg(unix)]
async fn lsp_frame(
    bus: &mut tokio::sync::broadcast::Receiver<bisa_engine::EngineEvent>,
    method: &str,
) -> serde_json::Value {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let e = bus.recv().await.unwrap();
            if let bisa_engine::EnginePayload::Lsp {
                method: m, params, ..
            } = e.payload
            {
                if m == method {
                    return params;
                }
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the {method} frame in time"))
}

/// A server's notifications ride the engine bus with their URIs rewritten
/// to root-relative paths — the log messages and progress it sends are
/// kept to the engine — a change and a hover reach it, a change on a root
/// with no server yet starts one, and a restart on request stops it, which
/// the bus says.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_servers_notifications_ride_the_bus_with_their_uris_rewritten_and_a_restart_says_it_stopped(
) {
    let dir = tempfile::tempdir().unwrap();
    let tools = tempfile::tempdir().unwrap();
    let server = a_server_that_speaks(tools.path());
    let engine = engine_with(&dir, vec![]);
    let lab = a_rust_project(&engine, "lab");
    let shed = a_rust_project(&engine, "shed");
    engine
        .set_setting(
            SettingScope::Machine,
            None,
            "lsp.servers",
            json!([{"language": "rust", "command": server.display().to_string()}]),
        )
        .unwrap();
    let inner = engine.inner();
    let mut bus = engine.events();
    let opened = lsp::open(inner, FileScope::Workstream, &lab, "main.rs", "diagnose me")
        .await
        .unwrap();
    assert!(opened.following);
    let started = lsp_frame(&mut bus, "bisa/serverStarted").await;
    assert_eq!(started["command"], json!(server.display().to_string()));
    let diagnostics = lsp_frame(&mut bus, "textDocument/publishDiagnostics").await;
    assert_eq!(
        diagnostics["uri"],
        json!("main.rs"),
        "the server's file:// URI is the editor's path: {diagnostics}"
    );
    assert_eq!(diagnostics["diagnostics"][0]["message"], json!("spoken"));

    lsp::change(
        inner,
        FileScope::Workstream,
        &lab,
        "main.rs",
        "fn main() { let x = 1; }\n",
    )
    .await
    .unwrap();
    let hover = lsp::request(
        inner,
        FileScope::Workstream,
        &lab,
        "main.rs",
        "textDocument/hover",
        json!({"textDocument": {"uri": "main.rs"}, "position": {"line": 0, "character": 0}}),
    )
    .await
    .unwrap();
    assert_eq!(hover["contents"]["value"], json!("hovered"));
    assert_eq!(
        hover["uri"],
        json!("main.rs"),
        "what the server answered is rewritten back: {hover}"
    );
    assert_eq!(starts_of(&server), 1);

    // A change on a root nobody opened a document in yet is an open.
    lsp::change(
        inner,
        FileScope::Workstream,
        &shed,
        "main.rs",
        "fn main() {}\n",
    )
    .await
    .unwrap();
    assert_eq!(starts_of(&server), 2, "a server of the shed's own");
    let shed_row = lsp::status(inner, FileScope::Workstream, &shed)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.language == "rust")
        .unwrap();
    assert_eq!(shed_row.documents, 1);

    // Restarted on request: the server stops, and the pump says so.
    lsp::restart(inner, FileScope::Workstream, &lab, "rust")
        .await
        .unwrap();
    lsp_frame(&mut bus, "bisa/serverStopped").await;
    let lab_row = lsp::status(inner, FileScope::Workstream, &lab)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.language == "rust")
        .unwrap();
    assert!(
        matches!(lab_row.state, bisa_lsp::server::ServerState::Stopped),
        "{lab_row:?}"
    );
    // A close with no server running for the language is nothing to do.
    lsp::close(inner, FileScope::Workstream, &lab, "main.rs")
        .await
        .unwrap();
    engine.shutdown().await;
}

/// A server that died is forgotten and started again by the next open; a
/// change to an `lsp.*` setting stops every running server, which the bus
/// says, and the next document starts one afresh.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_server_that_died_is_started_again_and_a_settings_change_stops_every_server() {
    let dir = tempfile::tempdir().unwrap();
    let tools = tempfile::tempdir().unwrap();
    let server = a_server_that_speaks(tools.path());
    let engine = engine_with(&dir, vec![]);
    let lab = a_rust_project(&engine, "lab");
    engine
        .set_setting(
            SettingScope::Machine,
            None,
            "lsp.servers",
            json!([{"language": "rust", "command": server.display().to_string()}]),
        )
        .unwrap();
    let inner = engine.inner();
    let mut bus = engine.events();
    assert!(
        lsp::open(
            inner,
            FileScope::Workstream,
            &lab,
            "boom.rs",
            "please crash"
        )
        .await
        .unwrap()
        .following
    );
    lsp_frame(&mut bus, "bisa/serverStopped").await;
    let failed = lsp::status(inner, FileScope::Workstream, &lab)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.language == "rust")
        .unwrap();
    assert!(
        matches!(&failed.state, bisa_lsp::server::ServerState::Failed { reason } if reason.contains("told to crash")),
        "{failed:?}"
    );
    // The next open finds the dead server, forgets it and starts another.
    assert!(
        lsp::open(
            inner,
            FileScope::Workstream,
            &lab,
            "main.rs",
            "fn main() {}\n"
        )
        .await
        .unwrap()
        .following
    );
    assert_eq!(starts_of(&server), 2);

    engine
        .set_setting(SettingScope::Machine, None, "lsp.idle_ttl_secs", json!(120))
        .unwrap();
    lsp_frame(&mut bus, "bisa/serverStopped").await;
    let stopped = lsp::status(inner, FileScope::Workstream, &lab)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.language == "rust")
        .unwrap();
    assert!(
        matches!(stopped.state, bisa_lsp::server::ServerState::Stopped),
        "{stopped:?}"
    );
    assert_eq!(stopped.documents, 0);
    engine.shutdown().await;
}

/// The idle time to live: the last document closed arms it; a document
/// opened and closed again since makes the earlier sweep find the server
/// not yet due; the later sweep stops it, and the bus says so. Thirty
/// seconds is the least the setting allows, so this test takes that long.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn the_idle_time_to_live_stops_a_server_nobody_uses_and_spares_one_opened_again() {
    let dir = tempfile::tempdir().unwrap();
    let tools = tempfile::tempdir().unwrap();
    let server = a_server_that_speaks(tools.path());
    let engine = engine_with(&dir, vec![]);
    let lab = a_rust_project(&engine, "lab");
    engine
        .set_setting(
            SettingScope::Machine,
            None,
            "lsp.servers",
            json!([{"language": "rust", "command": server.display().to_string()}]),
        )
        .unwrap();
    engine
        .set_setting(SettingScope::Machine, None, "lsp.idle_ttl_secs", json!(30))
        .unwrap();
    let inner = engine.inner();
    let mut bus = engine.events();
    let lab = lab.as_str();
    let open = |path: &str| {
        let path = path.to_string();
        async move { lsp::open(inner, FileScope::Workstream, lab, &path, "fn main() {}\n").await }
    };
    let close = |path: &str| {
        let path = path.to_string();
        async move { lsp::close(inner, FileScope::Workstream, lab, &path).await }
    };
    open("a.rs").await.unwrap();
    open("b.rs").await.unwrap();
    close("a.rs").await.unwrap();
    close("nothing-open.md").await.unwrap();
    close("b.rs").await.unwrap();
    let first_closed = std::time::Instant::now();
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    open("a.rs").await.unwrap();
    close("a.rs").await.unwrap();
    // The first sweep, at thirty seconds: a document was closed since, so
    // the server is not yet due and stays.
    tokio::time::sleep(std::time::Duration::from_secs(29)).await;
    let row = lsp::status(inner, FileScope::Workstream, lab)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.language == "rust")
        .unwrap();
    assert!(
        matches!(row.state, bisa_lsp::server::ServerState::Running),
        "not yet due at {:?}: {row:?}",
        first_closed.elapsed()
    );
    lsp_frame(&mut bus, "bisa/serverStopped").await;
    let row = lsp::status(inner, FileScope::Workstream, lab)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.language == "rust")
        .unwrap();
    assert!(
        matches!(row.state, bisa_lsp::server::ServerState::Stopped),
        "stopped after idling: {row:?}"
    );
    assert_eq!(starts_of(&server), 1);
    engine.shutdown().await;
}

/// A sweep whose server was restarted away in the meantime finds nothing
/// and does nothing. Thirty seconds is the least the setting allows.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_sweep_whose_server_went_in_the_meantime_finds_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let tools = tempfile::tempdir().unwrap();
    let server = a_server_that_speaks(tools.path());
    let engine = engine_with(&dir, vec![]);
    let lab = a_rust_project(&engine, "lab");
    engine
        .set_setting(
            SettingScope::Machine,
            None,
            "lsp.servers",
            json!([{"language": "rust", "command": server.display().to_string()}]),
        )
        .unwrap();
    engine
        .set_setting(SettingScope::Machine, None, "lsp.idle_ttl_secs", json!(30))
        .unwrap();
    let inner = engine.inner();
    lsp::open(inner, FileScope::Workstream, &lab, "a.rs", "fn main() {}\n")
        .await
        .unwrap();
    lsp::close(inner, FileScope::Workstream, &lab, "a.rs")
        .await
        .unwrap();
    lsp::restart(inner, FileScope::Workstream, &lab, "rust")
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_secs(31)).await;
    let row = lsp::status(inner, FileScope::Workstream, &lab)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.language == "rust")
        .unwrap();
    assert!(
        matches!(row.state, bisa_lsp::server::ServerState::Stopped),
        "{row:?}"
    );
    assert_eq!(starts_of(&server), 1, "nothing was started by the sweep");
    engine.shutdown().await;
}
