//! Backend integration tests. Git fixtures are ephemeral repositories created
//! by the tests inside `tempfile` dirs and destroyed with them — they never
//! touch user repositories.

use std::fs;
use std::path::Path;
use std::process::Command;

use bisa_iso::{backend, resolve, BackendKind, ChangeKind};

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A throwaway git repo with one committed file.
fn git_fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = dir.path();
    git(repo, &["init", "--quiet", "--initial-branch=main"]);
    fs::write(repo.join("hello.txt"), "hello\nworld\n").unwrap();
    fs::write(repo.join("keep.txt"), "keep\n").unwrap();
    git(repo, &["add", "."]);
    git(repo, &["commit", "--quiet", "-m", "baseline"]);
    dir
}

#[test]
fn worktree_start_edit_diff_stop() {
    let fixture = git_fixture();
    let lower = fixture.path().join(".");
    let parent = tempfile::tempdir().unwrap();
    let merged = parent.path().join("wt");

    let be = backend(BackendKind::GitWorktree);
    assert!(be.probe().available, "git must be on PATH for this test");
    be.start(&lower, &merged).expect("worktree add");
    assert!(merged.join("hello.txt").exists());

    // Modify tracked, add untracked, remove tracked.
    fs::write(merged.join("hello.txt"), "hello\nbrave\nworld\n").unwrap();
    fs::write(merged.join("new.txt"), "fresh\n").unwrap();
    fs::remove_file(merged.join("keep.txt")).unwrap();

    let diff = be.diff(&lower, &merged).expect("diff");
    let by_path = |p: &str| diff.files.iter().find(|f| f.path == Path::new(p));
    assert_eq!(
        by_path("hello.txt").expect("hello change").op,
        ChangeKind::Modified
    );
    assert_eq!(
        by_path("new.txt").expect("new change").op,
        ChangeKind::Added
    );
    assert_eq!(
        by_path("keep.txt").expect("keep change").op,
        ChangeKind::Removed
    );

    let text = diff.unified_text();
    assert!(text.contains("+brave"), "modified hunk present: {text}");
    assert!(
        text.contains("+fresh"),
        "untracked file rendered as added patch: {text}"
    );

    be.stop(&merged).expect("worktree remove");
    assert!(!merged.exists());
    // The original repo is untouched.
    assert_eq!(
        fs::read_to_string(fixture.path().join("hello.txt")).unwrap(),
        "hello\nworld\n"
    );
}

#[test]
fn worktree_start_unavailable_outside_git() {
    let plain = tempfile::tempdir().unwrap();
    let parent = tempfile::tempdir().unwrap();
    let merged = parent.path().join("wt");
    let err = backend(BackendKind::GitWorktree)
        .start(plain.path(), &merged)
        .expect_err("non-git lower must be rejected");
    assert!(err.is_unavailable(), "expected Unavailable, got: {err}");
}

#[test]
fn copy_roundtrip_and_diff() {
    let lower = tempfile::tempdir().unwrap();
    fs::create_dir(lower.path().join("sub")).unwrap();
    fs::write(lower.path().join("a.txt"), "alpha\n").unwrap();
    fs::write(lower.path().join("sub/b.txt"), "beta\n").unwrap();
    // A .git dir that must not be copied.
    fs::create_dir(lower.path().join(".git")).unwrap();
    fs::write(lower.path().join(".git/config"), "x").unwrap();

    let parent = tempfile::tempdir().unwrap();
    let merged = parent.path().join("copy");
    let be = backend(BackendKind::Copy);
    be.start(lower.path(), &merged).expect("copy");
    assert!(merged.join("sub/b.txt").exists());
    assert!(!merged.join(".git").exists(), ".git skipped");

    fs::write(merged.join("a.txt"), "alpha\nomega\n").unwrap();
    fs::write(merged.join("c.txt"), "gamma\n").unwrap();
    fs::remove_file(merged.join("sub/b.txt")).unwrap();

    let diff = be.diff(lower.path(), &merged).expect("diff");
    let ops: Vec<(String, ChangeKind)> = diff
        .files
        .iter()
        .map(|f| (f.path.display().to_string(), f.op))
        .collect();
    assert!(ops.contains(&("a.txt".into(), ChangeKind::Modified)));
    assert!(ops.contains(&("c.txt".into(), ChangeKind::Added)));
    assert!(ops.contains(&("sub/b.txt".into(), ChangeKind::Removed)));
    assert!(diff.unified_text().contains("+omega"));

    be.stop(&merged).expect("cleanup");
    assert!(!merged.exists());
}

#[test]
fn resolve_prefers_worktree_over_copy_when_git_present() {
    let r = resolve(None);
    let wt = r
        .candidates
        .iter()
        .position(|k| *k == BackendKind::GitWorktree);
    let cp = r.candidates.iter().position(|k| *k == BackendKind::Copy);
    assert!(cp.is_some(), "copy floor always present");
    if backend(BackendKind::GitWorktree).probe().available {
        assert!(
            wt.expect("worktree candidate") < cp.unwrap(),
            "worktree before copy"
        );
    }
}
