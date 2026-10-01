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

    // Servers off: nothing is tried, nothing fails.
    engine
        .set_setting(SettingScope::Machine, None, "lsp.enabled", json!(false))
        .unwrap();
    assert!(
        !open().await.unwrap().following,
        "disabled: no start, no error"
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
