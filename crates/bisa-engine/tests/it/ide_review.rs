//! Review notes (ide/04 §6): identity from the hunk, the sent
//! mark, resolution, and hunk staging through the engine.
use bisa_core::WorkstreamId;

use bisa_core::DiffScope;
use bisa_engine::ide::{git as ide_git, review};
use bisa_engine::{Engine, EngineConfig, EngineError};
use bisa_harness::HarnessCatalog;
use bisa_store::{content_hash, MemoryKeyStore, NewProject, Workspace};
use bisa_vcs::git;
use std::path::Path;
use std::process::Command;

fn set_identity(repo: &Path) {
    for (k, v) in [
        ("user.name", "Bisa Test"),
        ("user.email", "test@example.invalid"),
    ] {
        let ok = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["config", k, v])
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(ok, "git config {k}");
    }
}

fn engine(dir: &tempfile::TempDir) -> (Engine, bisa_core::ProjectId, std::path::PathBuf) {
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    let project = ws
        .create_project(NewProject::managed("web").unwrap())
        .unwrap();
    let root = ws.project_root_path(&project);
    std::fs::create_dir_all(&root).unwrap();
    let engine = Engine::start(
        ws,
        HarnessCatalog::new(),
        EngineConfig {
            design_enabled: false,
            events_enabled: false,
            ..Default::default()
        },
    )
    .unwrap();
    (engine, project.id, root)
}

fn new_note(project: bisa_core::ProjectId, hunk: &str, body: &str) -> review::NewNote {
    review::NewNote {
        project,
        workstream: None,
        path: "src/main.rs".into(),
        start: 10,
        end: 12,
        scope: DiffScope::Unstaged,
        hunk: hunk.into(),
        body: body.into(),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_note_is_pinned_to_its_hunk_and_forgets_being_sent_when_edited() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, _root) = engine(&dir);
    let inner = engine.inner();
    let hunk = "@@ -10,3 +10,3 @@\n-let x = 1;\n+let count = 1;\n";

    let note = review::create(inner, new_note(pid, hunk, "name it `total`")).unwrap();
    assert_eq!(note.diff_identity.as_str(), content_hash(hunk.as_bytes()));
    assert_eq!(note.hunk, hunk);
    assert_eq!(note.sent_at, None);

    // Refusals are typed, not panics.
    assert!(matches!(
        review::create(inner, new_note(pid, hunk, "   ")),
        Err(EngineError::Invalid(_))
    ));
    let mut bad = new_note(pid, hunk, "ok");
    bad.end = 3;
    assert!(matches!(
        review::create(inner, bad),
        Err(EngineError::Invalid(_))
    ));
    let mut escape = new_note(pid, hunk, "ok");
    escape.path = "../secrets".into();
    assert!(matches!(
        review::create(inner, escape),
        Err(EngineError::Invalid(_))
    ));

    // Send: a standalone project posts nowhere, but the note is marked sent.
    let sent = review::send(inner, pid, vec![]).unwrap();
    assert_eq!(sent.notes.len(), 1);
    assert!(sent.notes[0].sent_at.is_some());
    assert!(sent.posted_to.is_empty());
    assert!(
        matches!(
            review::send(inner, pid, vec![]),
            Err(EngineError::Invalid(_))
        ),
        "nothing left to send"
    );

    // An edit clears the mark; a resolve hides it from the default listing.
    let edited = review::edit(inner, pid, note.id, "name it `total`, and type it".into()).unwrap();
    assert_eq!(edited.sent_at, None);
    assert_eq!(review::list(inner, pid, None, false).unwrap().len(), 1);
    let resolved = review::resolve(inner, pid, note.id).unwrap();
    assert!(resolved.is_resolved());
    assert!(review::list(inner, pid, None, false).unwrap().is_empty());
    assert_eq!(review::list(inner, pid, None, true).unwrap().len(), 1);

    review::delete(inner, pid, note.id).unwrap();
    assert!(review::list(inner, pid, None, true).unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_long_hunk_is_bounded_before_it_becomes_the_identity() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, _root) = engine(&dir);
    let big = ("+".repeat(2000) + "\n").repeat(40);
    let note = review::create(engine.inner(), new_note(pid, &big, "too much")).unwrap();
    assert!(note.hunk.len() <= review::MAX_HUNK_BYTES + 32);
    assert!(note.hunk.ends_with("… (truncated)\n"));
    assert_eq!(
        note.diff_identity.as_str(),
        content_hash(note.hunk.as_bytes())
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn hunk_staging_blame_and_history_run_through_the_engine() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, root) = engine(&dir);
    let inner = engine.inner();
    git::init(&root).unwrap();
    set_identity(&root);
    std::fs::write(root.join("a.txt"), "one\ntwo\nthree\n").unwrap();
    git::add_all(&root).unwrap();
    git::commit(&root, "first", false).unwrap();
    std::fs::write(root.join("a.txt"), "one\nTWO\nthree\n").unwrap();

    let patch = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,3 +1,3 @@\n one\n-two\n+TWO\n three\n";
    let files = ide_git::stage_hunk(
        inner,
        WorkstreamId::primary_of(pid),
        patch.to_string(),
        false,
    )
    .await
    .unwrap();
    let a = files.iter().find(|f| f.path == Path::new("a.txt")).unwrap();
    assert!(a.is_staged(), "{files:#?}");
    assert!(!a.is_unstaged(), "{files:#?}");
    assert!(matches!(
        ide_git::stage_hunk(inner, WorkstreamId::primary_of(pid), String::new(), false).await,
        Err(EngineError::Invalid(_))
    ));
    let too_big = "x".repeat(ide_git::MAX_PATCH_BYTES + 1);
    assert!(matches!(
        ide_git::stage_hunk(inner, WorkstreamId::primary_of(pid), too_big, false).await,
        Err(EngineError::Invalid(_))
    ));

    let blame = ide_git::blame(inner, WorkstreamId::primary_of(pid), "a.txt".into(), None)
        .await
        .unwrap();
    assert_eq!(blame.len(), 3);
    assert_eq!(blame[0].summary, "first");
    assert!(blame[1].uncommitted, "the staged-but-uncommitted line");

    let history = ide_git::history(inner, WorkstreamId::primary_of(pid), "a.txt".into(), 10_000)
        .await
        .unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].subject, "first");
}

/// Before the first commit a file has no history and no line to blame: both
/// are empty answers through the engine, never git's refusal.
#[tokio::test]
async fn history_and_blame_before_the_first_commit_are_empty_not_errors() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, root) = engine(&dir);
    let inner = engine.inner();
    git::init(&root).unwrap();
    set_identity(&root);
    std::fs::write(root.join("index.html"), "<h1>hi</h1>\n").unwrap();
    git::add_all(&root).unwrap();
    let history = ide_git::history(
        inner,
        WorkstreamId::primary_of(pid),
        "index.html".into(),
        50,
    )
    .await
    .unwrap();
    assert!(history.is_empty(), "{history:?}");
    let blame = ide_git::blame(
        inner,
        WorkstreamId::primary_of(pid),
        "index.html".into(),
        None,
    )
    .await
    .unwrap();
    assert!(blame.is_empty(), "{blame:?}");
}
