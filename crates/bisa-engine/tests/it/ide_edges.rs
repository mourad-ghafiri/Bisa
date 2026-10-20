//! The IDE's file, search, watch, index and layout modules at their edges:
//! ids that are none, a run of the workspace writing in its scratch, a
//! save over a file that went, a symlink copied as a symlink, a delete the
//! disk refuses, the root named by a dotted path, a root with no folder
//! yet, globs that are no globs, CRLF lines and a last line without an
//! end, a file nobody can read and one that is no text, a rename and a
//! removal under the watch and a root that goes under it, a saved layout
//! that is no layout and one over the cap.

use crate::common::{armed, new_workflow, step, workspace, workspace_run};
use bisa_core::{StepKind, WaitFor};
use bisa_engine::events::FileChangeKind;
use bisa_engine::ide::files::{self, Disposal, ToDelete};
use bisa_engine::ide::search::{
    apply_replace, preview_replace, search, CaseMode, ReplaceOutcome, SearchHit, SearchQuery,
};
use bisa_engine::ide::{index, layout, watch};
use bisa_engine::{Engine, EngineError, EnginePayload};
use bisa_harness::HarnessCatalog;
use bisa_store::{content_hash, FileScope, NewProject, Workspace};
use std::time::Duration;

/// An engine over a managed project whose root is on disk: the engine,
/// the project's id (its primary workstream's) and the root.
fn engine(dir: &tempfile::TempDir) -> (Engine, String, std::path::PathBuf) {
    let ws = workspace(dir);
    let (pid, root) = project_of(&ws, "web", true);
    let engine = Engine::start(
        ws,
        HarnessCatalog::new(),
        bisa_engine::EngineConfig {
            design_enabled: false,
            events_enabled: false,
            ..Default::default()
        },
    )
    .unwrap();
    (engine, pid, root)
}

/// A managed project, its folder made or not.
fn project_of(ws: &Workspace, slug: &str, on_disk: bool) -> (String, std::path::PathBuf) {
    let project = ws
        .create_project(NewProject::managed(slug).unwrap())
        .unwrap();
    let root = ws.project_root_path(&project);
    if on_disk {
        std::fs::create_dir_all(&root).unwrap();
    }
    (project.id.to_string(), root)
}

fn query(q: &str) -> SearchQuery {
    SearchQuery {
        q: q.into(),
        regex: false,
        case: CaseMode::Smart,
        word: false,
        include: vec![],
        exclude: vec![],
        limit: None,
    }
}

fn hits_of(engine: &Engine, scope: FileScope, id: &str, q: &SearchQuery) -> Vec<SearchHit> {
    let mut out = Vec::new();
    search(engine.inner(), scope, id, q, |hit| {
        out.push(hit);
        true
    })
    .unwrap();
    out
}

/// A write to a goal or a run that does not exist is refused by the id, and
/// a run of the workspace writes under its own scratch folder.
#[tokio::test(flavor = "multi_thread")]
async fn writable_roots_refuse_ids_that_are_none_and_a_run_of_the_workspace_writes_in_its_scratch()
{
    let dir = tempfile::tempdir().unwrap();
    let (engine, _pid, _root) = engine(&dir);
    let inner = engine.inner();
    for scope in [FileScope::Goal, FileScope::Run] {
        let refused = files::write_file(inner, scope, "not-an-id", "a.txt", "x", None);
        assert!(
            matches!(refused, Err(EngineError::Invalid(_))),
            "{scope:?}: {refused:?}"
        );
    }
    let (_wf, run) = workspace_run(
        &engine,
        new_workflow(
            "hold",
            vec![step(
                "hold",
                StepKind::Wait {
                    until: WaitFor::Release,
                },
            )],
        ),
    );
    let rid = run.id.to_string();
    let written = files::write_file(
        inner,
        FileScope::Run,
        &rid,
        "notes/plan.md",
        "# plan\n",
        None,
    )
    .unwrap();
    assert_eq!(written.path, "notes/plan.md");
    assert!(written.created);
    let scratch = inner
        .ws
        .paths()
        .home(&bisa_core::Home::Run { run: run.id })
        .scratch();
    assert_eq!(
        std::fs::read_to_string(scratch.join("notes/plan.md")).unwrap(),
        "# plan\n",
        "under the run's scratch, never beside its journal"
    );
    engine.shutdown().await;
}

/// A save guarded by the hash of a file that has since gone is a conflict
/// whose current text is nothing — the client learns the file left, not
/// that its text changed.
#[tokio::test(flavor = "multi_thread")]
async fn a_save_over_a_file_that_went_is_a_conflict_with_nothing_current() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, root) = engine(&dir);
    let inner = engine.inner();
    let first =
        files::write_file(inner, FileScope::Workstream, &pid, "gone.md", "was\n", None).unwrap();
    std::fs::remove_file(root.join("gone.md")).unwrap();
    let conflict = files::write_file(
        inner,
        FileScope::Workstream,
        &pid,
        "gone.md",
        "now\n",
        Some(&first.hash),
    );
    match conflict {
        Err(EngineError::FileConflict {
            path,
            current_hash,
            current_text,
        }) => {
            assert_eq!(path, "gone.md");
            assert_eq!(current_hash, "");
            assert_eq!(current_text, "");
        }
        other => panic!("{other:?}"),
    }
    assert!(!root.join("gone.md").exists(), "nothing was made");
    engine.shutdown().await;
}

/// A folder copied as a duplicate keeps a symlink in it as a symlink, never
/// followed: the copy points where the original pointed.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_copied_folder_keeps_a_symlink_as_a_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, root) = engine(&dir);
    std::fs::create_dir_all(root.join("kit")).unwrap();
    std::fs::write(root.join("kit/real.txt"), "real\n").unwrap();
    std::os::unix::fs::symlink("real.txt", root.join("kit/alias.txt")).unwrap();
    let copied = files::copy_entry(
        engine.inner(),
        FileScope::Workstream,
        &pid,
        "kit",
        "kit-copy",
    )
    .unwrap();
    assert_eq!(copied, "kit-copy");
    let alias = root.join("kit-copy/alias.txt");
    assert!(
        alias.symlink_metadata().unwrap().is_symlink(),
        "a symlink, not its bytes"
    );
    assert_eq!(
        std::fs::read_link(&alias).unwrap(),
        std::path::PathBuf::from("real.txt")
    );
    assert_eq!(
        std::fs::read_to_string(root.join("kit-copy/real.txt")).unwrap(),
        "real\n"
    );
    engine.shutdown().await;
}

/// The root named by a dotted path (`./.`) is refused as the root itself;
/// a delete the disk refuses — the folder above the file cannot be written
/// — is the refusal, and the batch of one says nothing went.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn the_root_named_by_a_dotted_path_and_a_delete_the_disk_refuses_are_said() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, root) = engine(&dir);
    let inner = engine.inner();
    std::fs::write(root.join("keep.txt"), "x").unwrap();
    let asked = [ToDelete {
        path: "./.".into(),
        recursive: true,
    }];
    let refused =
        files::delete_entries(inner, FileScope::Workstream, &pid, &asked, Disposal::Unlink);
    assert!(
        matches!(refused, Err(EngineError::Invalid(_))),
        "{refused:?}"
    );
    assert!(root.join("keep.txt").is_file(), "nothing went");

    std::fs::create_dir_all(root.join("locked")).unwrap();
    std::fs::write(root.join("locked/held.txt"), "x").unwrap();
    std::fs::set_permissions(root.join("locked"), std::fs::Permissions::from_mode(0o555)).unwrap();
    let halted = files::delete_entry(
        inner,
        FileScope::Workstream,
        &pid,
        "locked/held.txt",
        false,
        Disposal::Unlink,
    );
    std::fs::set_permissions(root.join("locked"), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(halted, Err(EngineError::Io(_))), "{halted:?}");
    assert!(root.join("locked/held.txt").is_file(), "the file stayed");
    engine.shutdown().await;
}

/// A project whose folder is not on disk yet: its path index is empty, a
/// search in it is refused in words, and nothing can be watched there.
#[tokio::test(flavor = "multi_thread")]
async fn the_index_the_search_and_the_watch_say_when_a_root_has_no_folder_yet() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    let (pid, root) = project_of(&ws, "unborn", false);
    let engine = Engine::start(
        ws,
        HarnessCatalog::new(),
        bisa_engine::EngineConfig {
            design_enabled: false,
            events_enabled: false,
            ..Default::default()
        },
    )
    .unwrap();
    let inner = engine.inner();
    assert!(!root.exists());
    let idx = index::paths(inner, FileScope::Workstream, &pid, None).unwrap();
    assert!(idx.paths.is_empty() && !idx.truncated, "{idx:?}");
    let refused = search(inner, FileScope::Workstream, &pid, &query("x"), |_| true);
    assert!(
        matches!(refused, Err(EngineError::Invalid(_))),
        "{refused:?}"
    );
    let refused = watch::watch(inner, FileScope::Workstream, &pid);
    assert!(
        matches!(refused, Err(EngineError::Invalid(_))),
        "{refused:?}"
    );
    assert!(inner.ide_watch.watched().is_empty());
    engine.shutdown().await;
}

/// An include or exclude glob that is no glob is refused by its name
/// before anything is walked.
#[tokio::test(flavor = "multi_thread")]
async fn search_globs_that_are_no_globs_are_refused_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, root) = engine(&dir);
    std::fs::write(root.join("a.txt"), "needle\n").unwrap();
    let mut bad_include = query("needle");
    bad_include.include = vec!["src/[".into()];
    let said = search(
        engine.inner(),
        FileScope::Workstream,
        &pid,
        &bad_include,
        |_| true,
    )
    .unwrap_err()
    .to_string();
    assert!(said.contains("src/["), "the glob is named: {said}");
    let mut bad_exclude = query("needle");
    bad_exclude.exclude = vec!["target/[".into()];
    let said = search(
        engine.inner(),
        FileScope::Workstream,
        &pid,
        &bad_exclude,
        |_| true,
    )
    .unwrap_err()
    .to_string();
    assert!(said.contains("target/["), "the glob is named: {said}");
    engine.shutdown().await;
}

/// A replacement keeps a line's own ending — CRLF, or none on the last
/// line — and a whole-word, case-insensitive query reaches the same lines
/// the search does; a replacement that would change nothing previews
/// nothing and applies as nothing changed.
#[tokio::test(flavor = "multi_thread")]
async fn a_replace_keeps_each_lines_own_ending_and_a_change_to_nothing_is_said_so() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, root) = engine(&dir);
    let inner = engine.inner();
    std::fs::write(
        root.join("win.txt"),
        "first\r\nNeedle here\r\nneedles\r\nlast NEEDLE",
    )
    .unwrap();
    let mut q = query("needle");
    q.word = true;
    q.case = CaseMode::Insensitive;
    let previews = preview_replace(inner, FileScope::Workstream, &pid, &q, "pin").unwrap();
    assert_eq!(previews.len(), 1, "{previews:?}");
    let lines: Vec<(u64, &str)> = previews[0]
        .changes
        .iter()
        .map(|c| (c.line, c.after.as_str()))
        .collect();
    assert_eq!(
        lines,
        vec![(2, "pin here"), (4, "last pin")],
        "whole words, any case; `needles` is another word"
    );
    let files = vec![(previews[0].path.clone(), previews[0].base_hash.clone())];
    let outcomes = apply_replace(inner, FileScope::Workstream, &pid, &q, "pin", &files).unwrap();
    assert!(
        matches!(&outcomes[0], ReplaceOutcome::Applied { changes: 2, .. }),
        "{outcomes:?}"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("win.txt")).unwrap(),
        "first\r\npin here\r\nneedles\r\nlast pin",
        "CRLF stays CRLF and the last line still ends in nothing"
    );

    // The same word for itself: nothing would change.
    let same = query("pin");
    assert!(
        preview_replace(inner, FileScope::Workstream, &pid, &same, "pin")
            .unwrap()
            .is_empty()
    );
    let hash = content_hash(&std::fs::read(root.join("win.txt")).unwrap());
    let outcomes = apply_replace(
        inner,
        FileScope::Workstream,
        &pid,
        &same,
        "pin",
        &[("win.txt".to_string(), hash.clone())],
    )
    .unwrap();
    assert!(
        matches!(&outcomes[0], ReplaceOutcome::Applied { changes: 0, hash: h, .. } if *h == hash),
        "{outcomes:?}"
    );
    engine.shutdown().await;
}

/// A file nobody can read is skipped by the search; one that is no text —
/// bytes that are not UTF-8, with no NUL to call it binary — is found by
/// the search, left out of the preview and failed by the apply, as is a
/// path that is not there.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_file_nobody_can_read_is_skipped_and_one_that_is_no_text_is_found_but_never_rewritten() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, root) = engine(&dir);
    let inner = engine.inner();
    std::fs::write(root.join("plain.txt"), "needle\n").unwrap();
    std::fs::write(root.join("sealed.txt"), "needle\n").unwrap();
    std::fs::set_permissions(
        root.join("sealed.txt"),
        std::fs::Permissions::from_mode(0o000),
    )
    .unwrap();
    std::fs::write(root.join("latin.txt"), b"needle caf\xe9\n").unwrap();
    let q = query("needle");
    let mut found: Vec<String> = hits_of(&engine, FileScope::Workstream, &pid, &q)
        .into_iter()
        .map(|h| h.path)
        .collect();
    found.sort();
    assert_eq!(
        found,
        vec!["latin.txt", "plain.txt"],
        "the sealed file is skipped"
    );

    let previews = preview_replace(inner, FileScope::Workstream, &pid, &q, "pin").unwrap();
    let previewed: Vec<&str> = previews.iter().map(|p| p.path.as_str()).collect();
    assert_eq!(previewed, vec!["plain.txt"], "no text, no preview");

    let latin_hash = content_hash(&std::fs::read(root.join("latin.txt")).unwrap());
    let outcomes = apply_replace(
        inner,
        FileScope::Workstream,
        &pid,
        &q,
        "pin",
        &[
            ("latin.txt".to_string(), latin_hash),
            ("nowhere.txt".to_string(), "0".repeat(64)),
        ],
    )
    .unwrap();
    assert!(
        matches!(&outcomes[0], ReplaceOutcome::Failed { path, error } if path == "latin.txt" && error.contains("UTF-8")),
        "{outcomes:?}"
    );
    assert!(
        matches!(&outcomes[1], ReplaceOutcome::Failed { path, .. } if path == "nowhere.txt"),
        "{outcomes:?}"
    );
    std::fs::set_permissions(
        root.join("sealed.txt"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    engine.shutdown().await;
}

/// A replacement in a goal's folder writes where the goal can be written —
/// under its `scratch/` — and names a file outside it as read-only rather
/// than claiming it changed since the preview.
#[tokio::test(flavor = "multi_thread")]
async fn a_replace_in_a_goals_folder_writes_under_work_and_names_the_rest_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, _pid, _root) = engine(&dir);
    let inner = engine.inner();
    let goal = engine
        .submit_goal(bisa_engine::SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..bisa_engine::SubmitRequest::captured("replace in my folder")
        })
        .unwrap();
    let gid = goal.id.to_string();
    let goal_dir = inner.ws.paths().goal(goal.id).dir().to_path_buf();
    let work = inner.ws.paths().goal(goal.id).scratch();
    std::fs::create_dir_all(&work).unwrap();
    std::fs::write(work.join("draft.md"), "a needle in the work\n").unwrap();
    std::fs::write(goal_dir.join("readme.md"), "a needle beside the journal\n").unwrap();
    let q = query("needle");
    let mut found: Vec<String> = hits_of(&engine, FileScope::Goal, &gid, &q)
        .into_iter()
        .map(|h| h.path)
        .collect();
    found.sort();
    assert_eq!(
        found,
        vec!["readme.md", "scratch/draft.md"],
        "the search reads the whole folder"
    );

    let previews = preview_replace(inner, FileScope::Goal, &gid, &q, "pin").unwrap();
    let previewed: Vec<&str> = previews.iter().map(|p| p.path.as_str()).collect();
    assert_eq!(
        previewed,
        vec!["scratch/draft.md"],
        "only what can be written is previewed"
    );
    let readme_hash = content_hash(&std::fs::read(goal_dir.join("readme.md")).unwrap());
    let outcomes = apply_replace(
        inner,
        FileScope::Goal,
        &gid,
        &q,
        "pin",
        &[
            (previews[0].path.clone(), previews[0].base_hash.clone()),
            ("readme.md".to_string(), readme_hash),
        ],
    )
    .unwrap();
    assert!(
        matches!(&outcomes[0], ReplaceOutcome::Applied { path, changes: 1, .. } if path == "scratch/draft.md"),
        "{outcomes:?}"
    );
    assert_eq!(
        std::fs::read_to_string(work.join("draft.md")).unwrap(),
        "a pin in the work\n"
    );
    assert!(
        matches!(&outcomes[1], ReplaceOutcome::Failed { path, .. } if path == "readme.md"),
        "outside the writable root: a refusal, not a change since the preview: {outcomes:?}"
    );
    assert_eq!(
        std::fs::read_to_string(goal_dir.join("readme.md")).unwrap(),
        "a needle beside the journal\n",
        "never written"
    );
    engine.shutdown().await;
}

/// Every frame for one root, until `until` says it is the one.
async fn frames_until(
    bus: &mut tokio::sync::broadcast::Receiver<bisa_engine::EngineEvent>,
    mut until: impl FnMut(&str, &FileChangeKind, &Option<String>) -> bool,
) -> Vec<(String, FileChangeKind, Option<String>)> {
    let mut seen = Vec::new();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let e = bus.recv().await.unwrap();
            if let EnginePayload::FileChanged {
                path, kind, from, ..
            } = e.payload
            {
                let done = until(&path, &kind, &from);
                seen.push((path, kind, from));
                if done {
                    return;
                }
            }
        }
    })
    .await
    .expect("the frame in time");
    seen
}

/// The watcher reports a rename and a removal made outside the platform,
/// says nothing of git's object store, and ends with a rescan when the
/// root itself goes.
#[tokio::test(flavor = "multi_thread")]
async fn the_watcher_reports_renames_and_removals_ignores_the_object_store_and_ends_when_the_root_goes(
) {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, root) = engine(&dir);
    let inner = engine.inner();
    let mut bus = engine.events();
    std::fs::create_dir_all(root.join(".git/objects/aa")).unwrap();
    std::fs::write(root.join("old.txt"), "x").unwrap();
    watch::watch(inner, FileScope::Workstream, &pid).unwrap();
    armed(&mut bus, &root).await;

    // A rename: reported under its new name, as a rename or as a removal
    // and a creation, whichever way this machine's watcher tells it.
    std::fs::rename(root.join("old.txt"), root.join("new.txt")).unwrap();
    let seen = frames_until(&mut bus, |path, _, _| path == "new.txt").await;
    assert!(
        seen.iter().any(|(p, k, _)| p == "new.txt"
            && matches!(k, FileChangeKind::Renamed | FileChangeKind::Created)),
        "{seen:?}"
    );

    // Something under `.git/objects/` is nothing the panel shows: a sentinel
    // written after it is reported — the OS tells changes in order — and
    // nothing of the object store before it; then the next removal is
    // reported and nothing of the object store with it either.
    std::fs::write(root.join(".git/objects/aa/bb"), "blob").unwrap();
    std::fs::write(root.join("after-the-object.txt"), "x").unwrap();
    let seen = frames_until(&mut bus, |path, _, _| path == "after-the-object.txt").await;
    assert!(
        seen.iter().all(|(p, _, _)| !p.starts_with(".git/objects")),
        "{seen:?}"
    );
    std::fs::remove_file(root.join("new.txt")).unwrap();
    let seen = frames_until(&mut bus, |path, kind, _| {
        path == "new.txt" && *kind == FileChangeKind::Removed
    })
    .await;
    assert!(
        seen.iter().all(|(p, _, _)| !p.starts_with(".git/objects")),
        "{seen:?}"
    );

    // Renamed and removed within one window: the OS tells both in one
    // breath, in no order; what the disk says at the flush is the word, so
    // a file that is gone is reported gone, never as renamed into being.
    std::fs::write(root.join("blink.txt"), "x").unwrap();
    frames_until(&mut bus, |path, _, _| path == "blink.txt").await;
    std::fs::rename(root.join("blink.txt"), root.join("blinked.txt")).unwrap();
    std::fs::remove_file(root.join("blinked.txt")).unwrap();
    let seen = frames_until(&mut bus, |path, kind, _| {
        path == "blinked.txt" && *kind == FileChangeKind::Removed
    })
    .await;
    assert!(
        !seen
            .iter()
            .any(|(p, k, _)| p == "blinked.txt" && *k != FileChangeKind::Removed),
        "a path that is gone is never said to be there: {seen:?}"
    );

    // The root goes under the watch — moved aside — one rescan, and the
    // watch ends.
    std::fs::rename(&root, root.with_extension("aside")).unwrap();
    let seen = frames_until(&mut bus, |path, kind, _| {
        path.is_empty() && *kind == FileChangeKind::Rescan
    })
    .await;
    assert!(!seen.is_empty());
    let gone = tokio::time::timeout(Duration::from_secs(5), async {
        while !inner.ide_watch.watched().is_empty() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    assert!(gone.is_ok(), "the watch ended with its root");

    // A goal's folder is watched like a checkout; its frames name the goal.
    let goal = engine
        .submit_goal(bisa_engine::SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..bisa_engine::SubmitRequest::captured("watch my folder")
        })
        .unwrap();
    let gid = goal.id.to_string();
    let goal_dir = watch::watch(inner, FileScope::Goal, &gid).unwrap();
    armed(&mut bus, &goal_dir).await;
    std::fs::write(goal_dir.join("note.md"), "a note").unwrap();
    let seen = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let e = bus.recv().await.unwrap();
            if let EnginePayload::FileChanged {
                path, scope, id, ..
            } = e.payload
            {
                if path == "note.md" {
                    return (scope, id);
                }
            }
        }
    })
    .await
    .expect("the goal's frame in time");
    assert_eq!(seen, ("goal".to_string(), gid.clone()));
    assert!(watch::unwatch(inner, FileScope::Goal, &gid));
    engine.shutdown().await;
}

/// A saved layout that is not JSON is said as such rather than read as
/// "none saved"; a layout over the cap is refused with the cap.
#[tokio::test(flavor = "multi_thread")]
async fn a_layout_that_is_no_json_is_said_and_one_over_the_cap_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, pid, _root) = engine(&dir);
    let inner = engine.inner();
    assert_eq!(
        layout::read(inner, FileScope::Workstream, &pid).unwrap(),
        None
    );
    let path = inner.ws.paths().ide_layout(FileScope::Workstream, &pid);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "not json").unwrap();
    let said = layout::read(inner, FileScope::Workstream, &pid)
        .unwrap_err()
        .to_string();
    assert!(!said.trim().is_empty(), "{said}");
    let huge = serde_json::json!({"tabs": "x".repeat(layout::MAX_LAYOUT_BYTES + 1)});
    let refused = layout::write(inner, FileScope::Workstream, &pid, &huge)
        .unwrap_err()
        .to_string();
    assert!(
        refused.contains(&layout::MAX_LAYOUT_BYTES.to_string()),
        "the cap is named: {refused}"
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "not json",
        "a refused write leaves what was there"
    );
    engine.shutdown().await;
}
