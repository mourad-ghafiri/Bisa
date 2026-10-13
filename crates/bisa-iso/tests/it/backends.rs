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

/// Every kind has a word and reads back from it; the copy-on-write stubs
/// are dispatchable everywhere and unavailable everywhere.
#[test]
fn every_kind_has_a_word_and_the_cow_stubs_are_unavailable_everywhere() {
    for kind in [
        BackendKind::Apfs,
        BackendKind::Overlayfs,
        BackendKind::GitWorktree,
        BackendKind::Copy,
    ] {
        assert_eq!(kind.to_string(), kind.as_str());
        assert_eq!(BackendKind::from_str(kind.as_str()), Some(kind));
        assert_eq!(backend(kind).kind(), kind);
    }
    assert_eq!(
        BackendKind::from_str("worktree"),
        Some(BackendKind::GitWorktree)
    );
    assert_eq!(BackendKind::from_str("rcopy"), Some(BackendKind::Copy));
    assert_eq!(BackendKind::from_str("toaster"), None);
    let lower = tempfile::tempdir().unwrap();
    let merged = lower.path().join("merged");
    for kind in [BackendKind::Apfs, BackendKind::Overlayfs] {
        let be = backend(kind);
        let probe = be.probe();
        assert!(!probe.available);
        assert!(probe.reason.as_deref().unwrap_or("").contains("v1"));
        assert!(be
            .start(lower.path(), &merged)
            .unwrap_err()
            .is_unavailable());
        assert!(be.stop(&merged).unwrap_err().is_unavailable());
    }
}

/// A vcs error becomes the isolation's own: unavailable stays unavailable,
/// anything else is a failure carrying git's words.
#[test]
fn a_vcs_error_becomes_the_isolations_own_unavailable_or_other() {
    let gone: bisa_iso::IsoError = bisa_vcs::VcsError::NotAvailable("no git here".into()).into();
    assert!(gone.is_unavailable());
    assert!(gone.to_string().contains("no git here"), "{gone}");
    let other: bisa_iso::IsoError = bisa_vcs::VcsError::other("boom").into();
    assert!(!other.is_unavailable());
    assert_eq!(other.to_string(), "boom");
}

/// A copy refuses a merged path already there, skips links, and stops
/// quietly when nothing is there to remove.
#[test]
fn a_copy_refuses_a_merged_path_already_there_skips_links_and_stops_quietly_on_nothing() {
    let lower = tempfile::tempdir().unwrap();
    fs::write(lower.path().join("a.txt"), "alpha\n").unwrap();
    std::os::unix::fs::symlink(lower.path().join("a.txt"), lower.path().join("link.txt")).unwrap();
    let parent = tempfile::tempdir().unwrap();
    let merged = parent.path().join("copy");
    let be = backend(BackendKind::Copy);
    be.start(lower.path(), &merged).expect("copy");
    assert!(merged.join("a.txt").exists());
    assert!(!merged.join("link.txt").exists(), "a link is skipped");
    let err = be.start(lower.path(), &merged).expect_err("already there");
    assert!(err.to_string().contains("already exists"), "{err}");
    be.stop(&merged).expect("cleanup");
    assert!(!merged.exists());
    be.stop(&merged).expect("nothing there is nothing to do");
}

/// A worktree refuses a merged path already there, and a folder git no
/// longer counts as a worktree still disappears when it is stopped.
#[test]
fn a_worktree_refuses_a_merged_path_already_there_and_stops_a_folder_git_lost() {
    let fixture = git_fixture();
    let parent = tempfile::tempdir().unwrap();
    let merged = parent.path().join("wt");
    let be = backend(BackendKind::GitWorktree);
    assert!(be.probe().available, "git must be on PATH for this test");
    fs::create_dir_all(&merged).unwrap();
    let err = be
        .start(fixture.path(), &merged)
        .expect_err("already there");
    assert!(err.to_string().contains("already exists"), "{err}");
    // A repository of its own standing where a worktree was: not git's to
    // remove as a worktree, removed all the same.
    git(&merged, &["init", "--quiet"]);
    fs::write(merged.join("stray.txt"), "x").unwrap();
    be.stop(&merged).expect("removed all the same");
    assert!(!merged.exists());
}

/// A plain diff tells bytes apart, not moments: a file rewritten with the
/// same bytes is no change, a binary that changed or went has no text, and
/// the unified text joins what has text.
#[test]
fn a_plain_diff_reads_unchanged_bytes_and_binaries() {
    let lower = tempfile::tempdir().unwrap();
    let merged = tempfile::tempdir().unwrap();
    fs::write(lower.path().join("same.txt"), "same\n").unwrap();
    fs::write(merged.path().join("same.txt"), "same\n").unwrap();
    let older = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
    fs::File::options()
        .write(true)
        .open(lower.path().join("same.txt"))
        .unwrap()
        .set_modified(older)
        .unwrap();
    fs::write(lower.path().join("pic.bin"), [0u8, 1, 2]).unwrap();
    fs::write(merged.path().join("pic.bin"), [0u8, 1, 9]).unwrap();
    fs::write(lower.path().join("gone.bin"), [0u8, 9]).unwrap();
    fs::write(merged.path().join("new.bin"), [0u8, 4]).unwrap();
    fs::write(merged.path().join("new.txt"), "fresh\n").unwrap();

    let diff = backend(BackendKind::Copy)
        .diff(lower.path(), merged.path())
        .expect("diff");
    let of = |name: &str| {
        diff.files
            .iter()
            .find(|f| f.path.display().to_string() == name)
            .unwrap_or_else(|| panic!("{name} in {:?}", diff.files))
    };
    assert!(
        !diff
            .files
            .iter()
            .any(|f| f.path.display().to_string() == "same.txt"),
        "same bytes, another moment: no change"
    );
    assert_eq!(of("pic.bin").op, ChangeKind::Modified);
    assert_eq!(of("pic.bin").diff, None, "a binary has no text");
    assert_eq!(of("gone.bin").op, ChangeKind::Removed);
    assert_eq!(of("gone.bin").diff, None);
    assert_eq!(of("new.bin").op, ChangeKind::Added);
    assert_eq!(of("new.bin").diff, None);
    assert!(of("new.txt")
        .diff
        .as_deref()
        .unwrap_or("")
        .contains("+fresh"));
    assert!(diff.unified_text().contains("+fresh"));

    let joined = bisa_iso::Diff {
        files: vec![
            bisa_iso::FileChange {
                path: "a".into(),
                op: ChangeKind::Modified,
                diff: Some(String::new()),
            },
            bisa_iso::FileChange {
                path: "b".into(),
                op: ChangeKind::Modified,
                diff: Some("--- b\n+++ b\n-x".into()),
            },
            bisa_iso::FileChange {
                path: "c".into(),
                op: ChangeKind::Modified,
                diff: Some("--- c\n".into()),
            },
        ],
    };
    assert_eq!(joined.unified_text(), "--- b\n+++ b\n-x\n--- c\n");
}

/// A worktree's diff reads what git says: a staged new file, a deleted one,
/// a binary that changed, and an untracked binary with no text.
#[test]
fn a_worktree_diff_reads_staged_adds_deletes_and_binaries() {
    let fixture = git_fixture();
    fs::write(fixture.path().join("pic.bin"), [0u8, 1, 2]).unwrap();
    git(fixture.path(), &["add", "pic.bin"]);
    git(fixture.path(), &["commit", "--quiet", "-m", "a picture"]);
    let parent = tempfile::tempdir().unwrap();
    let merged = parent.path().join("wt");
    let be = backend(BackendKind::GitWorktree);
    assert!(be.probe().available, "git must be on PATH for this test");
    be.start(fixture.path(), &merged).expect("worktree add");

    fs::write(merged.join("pic.bin"), [0u8, 1, 9]).unwrap();
    fs::remove_file(merged.join("keep.txt")).unwrap();
    fs::write(merged.join("fresh.txt"), "new\n").unwrap();
    git(&merged, &["add", "fresh.txt"]);
    fs::write(merged.join("blob.bin"), [0u8, 7]).unwrap();

    let diff = be.diff(fixture.path(), &merged).expect("diff");
    let of = |name: &str| {
        diff.files
            .iter()
            .find(|f| f.path.display().to_string() == name)
            .unwrap_or_else(|| panic!("{name} in {:?}", diff.files))
    };
    assert_eq!(of("pic.bin").op, ChangeKind::Modified);
    assert_eq!(of("pic.bin").diff, None, "binaries differ, no text");
    assert_eq!(of("keep.txt").op, ChangeKind::Removed);
    assert_eq!(of("fresh.txt").op, ChangeKind::Added);
    assert!(of("fresh.txt")
        .diff
        .as_deref()
        .unwrap_or("")
        .contains("+new"));
    assert_eq!(of("blob.bin").op, ChangeKind::Added);
    assert_eq!(of("blob.bin").diff, None, "an untracked binary has no text");
    be.stop(&merged).expect("cleanup");
}
