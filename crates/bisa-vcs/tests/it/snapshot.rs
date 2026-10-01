//! A snapshot of a checkout (ide/20): staged through a private index, so
//! nothing a person staged, and nothing in the tree, moves.
//!
//! Every repository here is a temporary directory; git runs with no global
//! and no system config.

use std::path::{Path, PathBuf};
use std::process::Command;

use bisa_vcs::snapshot;

fn raw_git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?} in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

struct Checkout {
    _dir: tempfile::TempDir,
    root: PathBuf,
    index: PathBuf,
}

/// A repository with one commit (`kept.txt`, `.gitignore` naming `build/`),
/// and a private index file outside it.
fn checkout() -> Checkout {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("repo");
    std::fs::create_dir_all(&root).unwrap();
    raw_git(&root, &["init", "--quiet"]);
    raw_git(&root, &["config", "user.name", "Bisa Test"]);
    raw_git(&root, &["config", "user.email", "test@example.invalid"]);
    std::fs::write(root.join("kept.txt"), "one\n").unwrap();
    std::fs::write(root.join(".gitignore"), "build/\n").unwrap();
    raw_git(&root, &["add", "-A"]);
    raw_git(&root, &["commit", "--quiet", "-m", "first"]);
    let index = dir.path().join("private").join("index");
    Checkout {
        _dir: dir,
        root,
        index,
    }
}

fn names(paths: Vec<PathBuf>) -> Vec<String> {
    let mut names: Vec<String> = paths
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_snapshot_sees_what_changed_was_made_and_was_removed_and_nothing_ignored() {
    let c = checkout();
    let before = snapshot::snapshot_tree(&c.root, &c.index).unwrap();
    assert!(snapshot::changed_between(&c.root, &before, &before)
        .unwrap()
        .is_empty());

    std::fs::write(c.root.join("kept.txt"), "one\ntwo\n").unwrap();
    std::fs::create_dir_all(c.root.join("src")).unwrap();
    std::fs::write(c.root.join("src/new file.rs"), "fn main() {}\n").unwrap();
    std::fs::create_dir_all(c.root.join("build")).unwrap();
    std::fs::write(c.root.join("build/out.o"), "binary").unwrap();
    let after = snapshot::snapshot_tree(&c.root, &c.index).unwrap();

    assert_eq!(
        names(snapshot::changed_between(&c.root, &before, &after).unwrap()),
        vec!["kept.txt", "src/new file.rs"]
    );
    assert_eq!(
        snapshot::blob_at(&c.root, &before, Path::new("kept.txt")).unwrap(),
        Some(b"one\n".to_vec())
    );
    assert_eq!(
        snapshot::blob_at(&c.root, &before, Path::new("src/new file.rs")).unwrap(),
        None,
        "it did not stand before"
    );

    std::fs::remove_file(c.root.join("kept.txt")).unwrap();
    let gone = snapshot::snapshot_tree(&c.root, &c.index).unwrap();
    assert_eq!(
        names(snapshot::changed_between(&c.root, &after, &gone).unwrap()),
        vec!["kept.txt"]
    );
    assert_eq!(
        snapshot::blob_at(&c.root, &gone, Path::new("kept.txt")).unwrap(),
        None
    );
}

#[test]
fn a_snapshot_moves_neither_the_persons_index_nor_the_tree() {
    let c = checkout();
    // The person has one change staged and one not.
    std::fs::write(c.root.join("staged.txt"), "staged\n").unwrap();
    raw_git(&c.root, &["add", "staged.txt"]);
    std::fs::write(c.root.join("kept.txt"), "one\nedited\n").unwrap();
    let status = raw_git(&c.root, &["status", "--porcelain=v1"]);
    let refs = raw_git(&c.root, &["for-each-ref"]);

    snapshot::snapshot_tree(&c.root, &c.index).unwrap();
    snapshot::snapshot_tree(&c.root, &c.index).unwrap();

    assert_eq!(raw_git(&c.root, &["status", "--porcelain=v1"]), status);
    assert_eq!(
        raw_git(&c.root, &["for-each-ref"]),
        refs,
        "no ref is written"
    );
    assert_eq!(
        std::fs::read_to_string(c.root.join("kept.txt")).unwrap(),
        "one\nedited\n"
    );
    assert!(c.index.is_file(), "the private index is the caller's file");
}

#[test]
fn a_repository_with_no_commit_yet_snapshots_from_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("fresh");
    std::fs::create_dir_all(&root).unwrap();
    raw_git(&root, &["init", "--quiet"]);
    let index = dir.path().join("index");
    let empty = snapshot::snapshot_tree(&root, &index).unwrap();
    std::fs::write(root.join("a.txt"), "a\n").unwrap();
    let one = snapshot::snapshot_tree(&root, &index).unwrap();
    assert_eq!(
        names(snapshot::changed_between(&root, &empty, &one).unwrap()),
        vec!["a.txt"]
    );
}
