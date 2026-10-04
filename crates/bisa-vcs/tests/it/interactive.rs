//! The consented tier (ide/04): the three source guards, and every
//! verb exercised against a temporary repository with its recovery ref
//! asserted — never by destroying a fixture.
//!
//! Every repository here is created by the test inside a `tempfile`
//! directory; "origin" is a local bare repository.

use std::path::{Path, PathBuf};
use std::process::Command;

use bisa_vcs::git;
use bisa_vcs::interactive::{
    self, HumanConsent, InProgress, MergeMode, PickRequest, PullMode, RebaseAction, RebasePlan,
    RebaseRequest, RebaseStep, RevertRequest,
};
use bisa_vcs::VcsError;
use bisa_vcs::{ConflictKind, ConflictSide, Resolution};

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

fn set_identity(repo: &Path) {
    raw_git(repo, &["config", "user.name", "Bisa Test"]);
    raw_git(repo, &["config", "user.email", "test@example.invalid"]);
}

struct Fixture {
    _root: tempfile::TempDir,
    repo: PathBuf,
    branch: String,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("tempdir");
        let repo = root.path().join("project");
        let origin = root.path().join("origin.git");
        git::init(&repo).expect("init");
        set_identity(&repo);
        std::fs::write(repo.join("README.md"), "hello\nworld\n").unwrap();
        git::add_all(&repo).expect("add");
        git::commit(&repo, "baseline", false).expect("commit");
        std::fs::create_dir_all(&origin).unwrap();
        raw_git(&origin, &["init", "--bare", "--quiet"]);
        git::remote_add(&repo, "origin", origin.to_str().unwrap()).expect("remote add");
        let branch = git::default_branch(&repo).expect("default branch");
        // The bare origin's HEAD names the branch this repository has, not
        // the machine's `init.defaultBranch`: a clone of it then checks a
        // real branch out, whatever git this test runs under is configured to.
        raw_git(
            &origin,
            &["symbolic-ref", "HEAD", &format!("refs/heads/{branch}")],
        );
        Self {
            _root: root,
            repo,
            branch,
        }
    }
}

/// The tests are the one other place allowed to mint: they stand in for the
/// node, and the layering test in `bisa-core` names this file.
fn consent() -> HumanConsent {
    HumanConsent::mint()
}

fn read(repo: &Path, rel: &str) -> String {
    std::fs::read_to_string(repo.join(rel)).unwrap()
}

// ---------------------------------------------------------------------------
// Source guards
// ---------------------------------------------------------------------------

fn interactive_source() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/interactive.rs");
    std::fs::read_to_string(path).expect("read interactive.rs")
}

/// The top-level verb functions: every `pub fn` at column zero of the
/// module, minus the two that are not verbs.
fn verb_fns(src: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(at) = src[i..].find("\npub fn ") {
        let start = i + at + 1;
        let name_start = start + "pub fn ".len();
        let name_end = src[name_start..]
            .find(['(', '<'])
            .map(|n| name_start + n)
            .unwrap_or(name_start);
        let name = src[name_start..name_end].to_string();
        // The body runs to the next top-level fn or module.
        let end = src[start..]
            .find("\npub fn ")
            .or_else(|| src[start..].find("\npub mod "))
            .or_else(|| src[start..].find("\nfn "))
            .map(|n| start + n)
            .unwrap_or(src.len());
        out.push((name, src[start..end].to_string()));
        i = start;
    }
    out.retain(|(n, _)| n != "capture" && n != "mint");
    assert!(out.len() >= 12, "expected the verbs, found {out:?}");
    out
}

#[test]
fn every_interactive_fn_takes_consent() {
    let src = interactive_source();
    for (name, body) in verb_fns(&src) {
        let sig_end = body.find('{').unwrap_or(body.len());
        assert!(
            body[..sig_end].contains("&HumanConsent"),
            "{name} does not take &HumanConsent"
        );
    }
    // The free-function forms too.
    let ops = &src[src.find("pub mod ops").expect("ops module")..];
    for line in ops
        .lines()
        .filter(|l| l.trim_start().starts_with("pub fn "))
    {
        let sig: String = ops[ops.find(line).unwrap()..]
            .chars()
            .take_while(|c| *c != '{')
            .collect();
        assert!(
            sig.contains("&HumanConsent"),
            "ops::{} lacks consent",
            line.trim()
        );
    }
}

#[test]
fn every_interactive_fn_records_recovery_before_it_runs_anything() {
    let src = interactive_source();
    for (name, body) in verb_fns(&src) {
        let recovery = body
            .find("capture(git, path,")
            .unwrap_or_else(|| panic!("{name} never captures a recovery"));
        // `apply_stash` is the one helper that runs git for a verb — its
        // first line is a `git.capture(` — so a verb that ends in it has run.
        let runner = [
            "run(git, path",
            "git.run(",
            "git.run_input(",
            "git.write(",
            "git.write_input(",
            "git.write_capture(",
            "git.capture(",
            "apply_stash(git, path",
        ]
        .iter()
        .filter_map(|needle| body.find(needle))
        .min()
        .unwrap_or_else(|| panic!("{name} runs nothing"));
        assert!(
            recovery < runner,
            "{name} runs git before it captures a recovery"
        );
    }
}

#[test]
fn interactive_never_plain_force() {
    let src = interactive_source();
    // Spelled in two halves so this file never contains the plain flag itself.
    let plain = ["--", "force"].concat();
    for (i, _) in src.match_indices(plain.as_str()) {
        assert!(
            src[i..].starts_with("--force-with-lease"),
            "a plain {plain} in interactive.rs at byte {i}"
        );
    }
    assert!(src.contains("--force-with-lease"), "the lease form exists");
}

#[test]
fn consent_has_no_default_clone_or_deserialize() {
    let src = interactive_source();
    let decl = src[src.find("pub struct HumanConsent").unwrap()..].to_string();
    let above = &src[..src.find("pub struct HumanConsent").unwrap()];
    let derive_line = above.lines().last().unwrap_or("");
    assert!(
        !derive_line.contains("derive"),
        "HumanConsent must not derive anything: {derive_line}"
    );
    assert!(decl.starts_with("pub struct HumanConsent(());"));
    assert!(!src.contains("impl Default for HumanConsent"));
    assert!(!src.contains("impl Clone for HumanConsent"));
}

// ---------------------------------------------------------------------------
// Behaviour
// ---------------------------------------------------------------------------

#[test]
fn checkout_saves_a_dirty_tree_first_and_restore_brings_it_back() {
    let fx = Fixture::new();
    let c = consent();
    git::branch_create(&fx.repo, "feature", None, false).unwrap();
    // Dirty the tree: one staged change, one unstaged.
    std::fs::write(fx.repo.join("README.md"), "hello\nworld\nstaged\n").unwrap();
    git::stage(&fx.repo, &["README.md"]).unwrap();
    std::fs::write(fx.repo.join("notes.txt"), "unstaged new file\n").unwrap();
    git::stage(&fx.repo, &["notes.txt"]).unwrap();
    std::fs::write(fx.repo.join("notes.txt"), "unstaged new file\nand more\n").unwrap();

    let rec =
        interactive::checkout(&git::Git::default(), &fx.repo, "feature", &c).expect("checkout");
    assert!(!rec.was_clean, "{rec:?}");
    assert!(rec.ref_name.ends_with("-checkout.wip"), "{}", rec.ref_name);
    assert_eq!(rec.branch.as_deref(), Some(fx.branch.as_str()));
    // The tree came along (git carries compatible changes across a checkout),
    // and the recovery ref exists and points at a stash-shaped commit.
    let listed = git::recovery_list(&fx.repo).unwrap();
    assert_eq!(listed.len(), 1, "{listed:#?}");
    assert_eq!(listed[0].op, "checkout");
    assert_eq!(listed[0].kind, git::RecoveryKind::Tree);
    assert_eq!(listed[0].branch.as_deref(), Some(fx.branch.as_str()));
    assert_eq!(listed[0].commit, rec.commit);
    let parents = raw_git(
        &fx.repo,
        &["rev-list", "--parents", "-1", rec.commit.as_str()],
    );
    assert!(
        parents.split_whitespace().count() >= 3,
        "a stash commit has the index as a parent: {parents}"
    );
    assert_eq!(
        raw_git(&fx.repo, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "feature"
    );
    assert_eq!(
        raw_git(&fx.repo, &["stash", "list"]),
        "",
        "stash create pushes nothing onto the list"
    );

    // Commit on feature, go back, then restore the recovery: HEAD returns to
    // the original branch with the saved work in place.
    git::add_all(&fx.repo).unwrap();
    git::commit(&fx.repo, "feature work", false).unwrap();
    let back = interactive::ops::checkout(&fx.repo, &fx.branch, &c).unwrap();
    assert!(back.was_clean);
    assert_eq!(read(&fx.repo, "README.md"), "hello\nworld\n");
    let restored = interactive::ops::restore(&fx.repo, &rec.ref_name, &c).expect("restore");
    assert!(restored.was_clean, "restoring itself records where we were");
    assert_eq!(
        raw_git(&fx.repo, &["rev-parse", "--abbrev-ref", "HEAD"]),
        fx.branch
    );
    assert_eq!(read(&fx.repo, "README.md"), "hello\nworld\nstaged\n");
    assert_eq!(read(&fx.repo, "notes.txt"), "unstaged new file\nand more\n");
    let files = git::status_files(&fx.repo).unwrap();
    let readme = files
        .iter()
        .find(|f| f.path == Path::new("README.md"))
        .unwrap();
    assert!(
        readme.is_staged(),
        "the staged half came back staged: {files:#?}"
    );
}

#[test]
fn checkout_refuses_to_clobber_and_a_clean_recovery_pins_head() {
    let fx = Fixture::new();
    let c = consent();
    let head = git::head(&fx.repo).unwrap();
    // A branch whose README differs, so checking it out with a dirty README
    // would overwrite it — git refuses, and the file is untouched.
    let wt = fx._root.path().join("wt");
    git::worktree_add(&fx.repo, &wt, "other", &fx.branch).unwrap();
    std::fs::write(wt.join("README.md"), "other\n").unwrap();
    git::add_all(&wt).unwrap();
    git::commit(&wt, "other readme", false).unwrap();
    git::worktree_remove(&wt, false).unwrap();

    std::fs::write(fx.repo.join("README.md"), "mine\n").unwrap();
    let err = interactive::ops::checkout(&fx.repo, "other", &c).expect_err("must refuse");
    assert!(matches!(err, VcsError::Dirty { .. }), "{err}");
    assert_eq!(read(&fx.repo, "README.md"), "mine\n");
    // The recovery was still written before the refusal — that is the order.
    let listed = git::recovery_list(&fx.repo).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].kind, git::RecoveryKind::Tree);

    // A clean checkout to a commit detaches and pins HEAD.
    std::fs::write(fx.repo.join("README.md"), "hello\nworld\n").unwrap();
    let rec = interactive::ops::checkout(&fx.repo, head.as_str(), &c).unwrap();
    assert!(rec.was_clean);
    assert_eq!(rec.commit, head);
    assert!(!rec.ref_name.ends_with(".wip"));
    // Detached: no symbolic ref to print, and git says so on stderr, not stdout.
    let out = Command::new("git")
        .arg("-C")
        .arg(&fx.repo)
        .args(["symbolic-ref", "-q", "HEAD"])
        .output()
        .expect("git");
    assert!(
        !out.status.success(),
        "HEAD is detached after checking out a commit"
    );
}

#[test]
fn a_deleted_branch_is_pinned_and_can_be_recreated_from_safety() {
    let fx = Fixture::new();
    let c = consent();
    let wt = fx._root.path().join("wt");
    git::worktree_add(&fx.repo, &wt, "doomed", &fx.branch).unwrap();
    std::fs::write(wt.join("x.txt"), "x\n").unwrap();
    git::add_all(&wt).unwrap();
    let tip = git::commit(&wt, "on doomed", false).unwrap();
    git::worktree_remove(&wt, false).unwrap();

    let rec = interactive::ops::branch_delete(&fx.repo, "doomed", &c).expect("delete");
    assert!(rec.was_clean);
    assert_eq!(
        rec.commit, tip,
        "a clean delete pins the branch tip, not HEAD"
    );
    assert!(git::branch_list(&fx.repo, None)
        .unwrap()
        .iter()
        .all(|b| b.name != "doomed"));
    let listed = git::recovery_list(&fx.repo).unwrap();
    assert_eq!(listed[0].op, "branch_delete");
    // Recreate from the recovery ref: the safe tier can make a branch.
    git::branch_create(&fx.repo, "doomed", Some(listed[0].commit.as_str()), false).unwrap();
    let back = git::branch_list(&fx.repo, None)
        .unwrap()
        .into_iter()
        .find(|b| b.name == "doomed")
        .unwrap();
    assert_eq!(back.head, tip);
}

/// A recovery point is Safety's alone: the history walks the person's refs
/// and decorates with the person's names, so a pinned branch tip that no
/// branch names any more is not a commit of the history, and a recovery ref
/// on a commit the history does show is no chip on it.
#[test]
fn a_recovery_point_is_in_safety_and_nowhere_in_the_history() {
    let fx = Fixture::new();
    let c = consent();
    let wt = fx._root.path().join("wt");
    git::worktree_add(&fx.repo, &wt, "doomed", &fx.branch).unwrap();
    std::fs::write(wt.join("x.txt"), "x\n").unwrap();
    git::add_all(&wt).unwrap();
    let tip = git::commit(&wt, "on doomed", false).unwrap();
    git::worktree_remove(&wt, false).unwrap();
    // The delete pins the tip under `refs/bisa/safety/`; a checkout of the
    // branch we are on pins where HEAD stands too.
    interactive::ops::branch_delete(&fx.repo, "doomed", &c).expect("delete");
    std::fs::write(fx.repo.join("notes.txt"), "dirty\n").unwrap();
    interactive::checkout(&git::Git::default(), &fx.repo, &fx.branch, &c).expect("checkout");
    let safety = git::recovery_list(&fx.repo).unwrap();
    assert_eq!(safety.len(), 2, "{safety:#?}");
    assert_eq!(
        raw_git(
            &fx.repo,
            &["for-each-ref", "--format=%(refname)", "refs/bisa/"]
        )
        .lines()
        .count(),
        2
    );

    for scope in git::RefScope::ALL {
        let rows = git::graph_log(&fx.repo, None, scope).unwrap();
        assert!(
            rows.iter().all(|row| row.id != tip),
            "{scope:?}: the pinned tip nobody's branch names is not in the history: {rows:#?}"
        );
        for row in &rows {
            for r in &row.refs {
                assert!(
                    !r.name.contains("bisa/") && !r.name.contains("safety"),
                    "{scope:?}: a recovery ref is no chip: {r:?}"
                );
                assert!(
                    matches!(r.kind.as_str(), "head" | "branch" | "remote" | "tag"),
                    "{scope:?}: {r:?}"
                );
            }
        }
        assert!(
            rows.iter()
                .any(|row| row.refs.iter().any(|r| r.kind == "branch")),
            "{scope:?}: the person's branch is still a chip"
        );
    }
}

#[test]
fn discard_hunk_puts_the_index_version_back_and_saves_what_was_typed() {
    let fx = Fixture::new();
    let c = consent();
    std::fs::write(fx.repo.join("README.md"), "hello\nWORLD\n").unwrap();
    let diff = git::diff_file(&fx.repo, "README.md", false).unwrap();
    let rec = interactive::ops::discard_hunk(&fx.repo, &diff, &c).expect("discard");
    assert!(!rec.was_clean);
    assert_eq!(read(&fx.repo, "README.md"), "hello\nworld\n");
    assert_eq!(git::diff(&fx.repo, false).unwrap().trim(), "");
    // What was typed is in the recovery commit.
    let saved = raw_git(
        &fx.repo,
        &["show", &format!("{}:README.md", rec.commit.as_str())],
    );
    assert_eq!(saved, "hello\nWORLD");
    assert!(matches!(
        interactive::ops::discard_hunk(&fx.repo, "  ", &c),
        Err(VcsError::InvalidArg { .. })
    ));

    // discard_paths: the same, by path; untracked files are left alone.
    std::fs::write(fx.repo.join("README.md"), "typed again\n").unwrap();
    std::fs::write(fx.repo.join("new.txt"), "untracked\n").unwrap();
    interactive::ops::discard_paths(&fx.repo, &["README.md"], &c).unwrap();
    assert_eq!(read(&fx.repo, "README.md"), "hello\nworld\n");
    assert_eq!(read(&fx.repo, "new.txt"), "untracked\n");
}

#[test]
fn discard_paths_reads_a_path_the_way_staging_does() {
    let fx = Fixture::new();
    let c = consent();
    // A name with a space and a glob character: stageable, so discardable.
    std::fs::create_dir_all(fx.repo.join("sub dir")).unwrap();
    std::fs::write(fx.repo.join("sub dir/note [1].txt"), "one\n").unwrap();
    git::add_all(&fx.repo).unwrap();
    git::commit(&fx.repo, "a spaced name", false).unwrap();
    std::fs::write(fx.repo.join("sub dir/note [1].txt"), "two\n").unwrap();
    let rec =
        interactive::ops::discard_paths(&fx.repo, &["sub dir/note [1].txt"], &c).expect("discard");
    assert!(!rec.was_clean, "a recovery ref saved what was typed");
    assert_eq!(read(&fx.repo, "sub dir/note [1].txt"), "one\n");
    // The reach rules are staging's too.
    assert!(matches!(
        interactive::ops::discard_paths(&fx.repo, &["../outside.txt"], &c),
        Err(VcsError::InvalidArg { .. })
    ));
    assert!(matches!(
        interactive::ops::discard_paths(&fx.repo, &["-x"], &c),
        Err(VcsError::InvalidArg { .. })
    ));
    assert!(matches!(
        interactive::ops::discard_paths::<&str>(&fx.repo, &[], &c),
        Err(VcsError::InvalidArg { .. })
    ));
}

#[test]
fn a_conflicting_rebase_is_typed_and_abort_returns_the_tree() {
    let fx = Fixture::new();
    let c = consent();
    let wt = fx._root.path().join("wt");
    git::worktree_add(&fx.repo, &wt, "theirs", &fx.branch).unwrap();
    std::fs::write(wt.join("README.md"), "theirs\nworld\n").unwrap();
    git::add_all(&wt).unwrap();
    git::commit(&wt, "theirs", false).unwrap();
    git::worktree_remove(&wt, false).unwrap();
    std::fs::write(fx.repo.join("README.md"), "mine\nworld\n").unwrap();
    git::add_all(&fx.repo).unwrap();
    let mine = git::commit(&fx.repo, "mine", false).unwrap();

    let err = interactive::ops::rebase(
        &fx.repo,
        &RebaseRequest {
            upstream: "theirs".into(),
            onto: None,
            autostash: false,
        },
        &c,
    )
    .expect_err("conflict");
    assert!(
        matches!(&err, VcsError::Conflict { paths, in_progress: Some(InProgress::Rebase), .. } if paths == &[PathBuf::from("README.md")]),
        "a conflict names its paths and the operation left in progress: {err}"
    );
    assert_eq!(
        git::in_progress(&fx.repo).unwrap(),
        Some(InProgress::Rebase)
    );
    assert!(
        fx.repo.join(".git/rebase-merge").exists() || fx.repo.join(".git/rebase-apply").exists()
    );
    let rec = interactive::ops::abort(&fx.repo, InProgress::Rebase, &c).expect("abort");
    assert!(rec.ref_name.contains("-abort"));
    assert_eq!(git::head(&fx.repo).unwrap(), mine);
    assert_eq!(read(&fx.repo, "README.md"), "mine\nworld\n");

    // Merge conflicts the same way; cherry-pick of a clean commit works.
    let err = interactive::ops::merge(&fx.repo, "theirs", MergeMode::Ff, None, &c)
        .expect_err("merge conflict");
    assert!(
        matches!(
            &err,
            VcsError::Conflict {
                in_progress: Some(InProgress::Merge),
                ..
            }
        ),
        "{err}"
    );
    // Another operation while one is in progress is refused by name.
    let busy = interactive::ops::pull(&fx.repo, "origin", PullMode::Merge, &c).expect_err("busy");
    assert!(
        matches!(busy, VcsError::InProgress(InProgress::Merge)),
        "{busy}"
    );
    interactive::ops::abort(&fx.repo, InProgress::Merge, &c).unwrap();
    assert_eq!(
        git::in_progress(&fx.repo).unwrap(),
        None,
        "abort clears the marker"
    );
    assert_eq!(git::head(&fx.repo).unwrap(), mine);
    let wt2 = fx._root.path().join("wt2");
    git::worktree_add(&fx.repo, &wt2, "pickme", &fx.branch).unwrap();
    std::fs::write(wt2.join("pick.txt"), "picked\n").unwrap();
    git::add_all(&wt2).unwrap();
    let pick = git::commit(&wt2, "pickable", false).unwrap();
    git::worktree_remove(&wt2, false).unwrap();
    interactive::ops::cherry_pick(
        &fx.repo,
        &PickRequest {
            commits: vec![pick.as_str().to_string()],
            record_origin: false,
            no_commit: false,
            mainline: None,
        },
        &c,
    )
    .unwrap();
    assert_eq!(read(&fx.repo, "pick.txt"), "picked\n");
    // A new commit on our branch. Not `!= pick`: when the pick's parent is
    // HEAD and both land in the same second, git reproduces the very same id.
    assert_ne!(
        git::head(&fx.repo).unwrap(),
        mine,
        "the pick landed as a commit on our branch"
    );
}

#[test]
fn tags_and_remotes_are_consented_and_recoverable() {
    let fx = Fixture::new();
    let c = consent();
    let head = git::head(&fx.repo).unwrap();
    interactive::ops::tag_create(&fx.repo, "v1", None, Some("first release"), &c).unwrap();
    interactive::ops::tag_create(&fx.repo, "light", Some(head.as_str()), None, &c).unwrap();
    let tags = git::tag_list(&fx.repo).unwrap();
    assert_eq!(tags.len(), 2, "{tags:#?}");
    assert!(
        tags.iter().all(|t| t.target == head),
        "annotated tags peel to the commit: {tags:#?}"
    );
    let rec = interactive::ops::tag_delete(&fx.repo, "v1", &c).unwrap();
    assert_eq!(rec.commit, head, "the deleted tag's target is pinned");
    assert_eq!(git::tag_list(&fx.repo).unwrap().len(), 1);
    assert!(matches!(
        interactive::ops::tag_create(&fx.repo, "bad..name", None, None, &c),
        Err(VcsError::InvalidArg { .. })
    ));

    assert_eq!(git::remote_list(&fx.repo).unwrap()[0].name, "origin");
    interactive::ops::remote_remove(&fx.repo, "origin", &c).unwrap();
    assert!(git::remote_list(&fx.repo).unwrap().is_empty());
    assert!(matches!(
        interactive::ops::remote_remove(&fx.repo, "-x", &c),
        Err(VcsError::InvalidArg { .. })
    ));
}

#[test]
fn a_rewritten_branch_pushes_with_a_lease_where_a_plain_push_cannot() {
    let fx = Fixture::new();
    let c = consent();
    std::fs::write(fx.repo.join("a.txt"), "a\n").unwrap();
    git::add_all(&fx.repo).unwrap();
    git::commit(&fx.repo, "a", false).unwrap();
    git::push(&fx.repo, "origin", &fx.branch, true).unwrap();
    // Rewrite the tip; the ordinary push is a non-fast-forward and fails.
    raw_git(
        &fx.repo,
        &["commit", "--amend", "-m", "a, reworded", "--quiet"],
    );
    assert!(git::push(&fx.repo, "origin", &fx.branch, false).is_err());
    let rec =
        interactive::ops::push_with_lease(&fx.repo, "origin", &fx.branch, &c).expect("lease push");
    assert!(rec.ref_name.contains("push_with_lease"));
    assert_eq!(
        raw_git(&fx.repo, &["rev-parse", &format!("origin/{}", fx.branch)]),
        git::head(&fx.repo).unwrap().as_str()
    );
}

#[test]
fn recovery_names_are_unique_within_a_second_and_the_list_is_newest_first() {
    let fx = Fixture::new();
    let c = consent();
    let head = git::head(&fx.repo).unwrap();
    let a = interactive::ops::checkout(&fx.repo, head.as_str(), &c).unwrap();
    let b = interactive::ops::checkout(&fx.repo, &fx.branch, &c).unwrap();
    assert_ne!(a.ref_name, b.ref_name);
    let listed = git::recovery_list(&fx.repo).unwrap();
    assert_eq!(listed.len(), 2);
    assert!(listed[0].at >= listed[1].at);
    assert!(listed.iter().all(|r| r.op == "checkout"));
    assert!(matches!(
        interactive::ops::restore(&fx.repo, "refs/heads/main", &c),
        Err(VcsError::InvalidArg { .. })
    ));
}

// ---------------------------------------------------------------------------
// Amend — the one rewrite of HEAD, pinned first
// ---------------------------------------------------------------------------

#[test]
fn an_amend_rewrites_head_with_what_is_staged_and_pins_the_old_commit() {
    let fx = Fixture::new();
    let c = consent();
    std::fs::write(fx.repo.join("a.txt"), "a\n").unwrap();
    git::add_all(&fx.repo).unwrap();
    let old = git::commit(&fx.repo, "a", false).unwrap();
    std::fs::write(fx.repo.join("b.txt"), "b\n").unwrap();
    git::stage(&fx.repo, &["b.txt"]).unwrap();
    let (rec, new) = interactive::ops::amend(&fx.repo, "a, amended", &c).expect("amend");
    assert_ne!(new, old, "HEAD is a new commit");
    assert_eq!(git::head(&fx.repo).unwrap(), new);
    assert_eq!(
        raw_git(&fx.repo, &["rev-list", "--count", "HEAD"]),
        "2",
        "baseline and the amended one: no commit was added"
    );
    assert_eq!(
        raw_git(&fx.repo, &["log", "-1", "--format=%s"]),
        "a, amended"
    );
    let shown = raw_git(&fx.repo, &["show", "--stat", "--format=", "HEAD"]);
    assert!(
        shown.contains("b.txt"),
        "what was staged folded in: {shown}"
    );
    assert!(
        shown.contains("a.txt"),
        "and the commit's own files stay: {shown}"
    );
    assert!(rec.ref_name.contains("amend"), "{}", rec.ref_name);
    assert!(
        rec.was_clean,
        "the tree was clean apart from the index git folded in"
    );
    assert_eq!(rec.commit, old, "the old commit is the recovery itself");
    assert_eq!(
        raw_git(&fx.repo, &["rev-parse", &rec.ref_name]),
        old.as_str()
    );
    assert_eq!(rec.branch.as_deref(), Some(fx.branch.as_str()));
}

#[test]
fn amend_is_refused_by_name_before_any_ref_is_written() {
    let fx = Fixture::new();
    let c = consent();
    assert!(matches!(
        interactive::ops::amend(&fx.repo, "   ", &c),
        Err(VcsError::InvalidArg { .. })
    ));
    assert_eq!(
        raw_git(&fx.repo, &["for-each-ref", "refs/bisa/safety"]),
        "",
        "nothing saved for a refusal"
    );
    // No commit yet: nothing to amend, and still nothing saved.
    let root = tempfile::tempdir().expect("tempdir");
    let empty = root.path().join("empty");
    git::init(&empty).expect("init");
    set_identity(&empty);
    assert!(interactive::ops::amend(&empty, "first", &c).is_err());
    assert_eq!(raw_git(&empty, &["for-each-ref", "refs/bisa/safety"]), "");
}

// ---------------------------------------------------------------------------
// Pull, revert — the sync verbs
// ---------------------------------------------------------------------------

/// A second clone of the fixture's origin, standing in for a colleague's
/// machine: what it pushes is what a pull has to bring home.
fn other_side(fx: &Fixture) -> PathBuf {
    let origin = fx._root.path().join("origin.git");
    let other = fx._root.path().join("other");
    raw_git(
        fx._root.path(),
        &[
            "clone",
            "--quiet",
            origin.to_str().unwrap(),
            other.to_str().unwrap(),
        ],
    );
    set_identity(&other);
    other
}

/// Whether HEAD is a merge commit — asked of git directly, since a missing
/// second parent is an exit code and not an error of ours.
fn has_second_parent(repo: &Path) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--verify", "-q", "HEAD^2"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn commit_file(repo: &Path, rel: &str, text: &str, message: &str) -> git::CommitId {
    std::fs::write(repo.join(rel), text).unwrap();
    git::add_all(repo).unwrap();
    git::commit(repo, message, false).unwrap()
}

#[test]
fn pull_fast_forwards_when_it_can_and_names_the_counts_when_it_cannot() {
    let fx = Fixture::new();
    let c = consent();
    git::push(&fx.repo, "origin", &fx.branch, true).unwrap();
    assert_eq!(
        git::upstream_of(&fx.repo).unwrap().as_deref(),
        Some(format!("origin/{}", fx.branch).as_str())
    );
    let other = other_side(&fx);
    let theirs = commit_file(&other, "theirs.txt", "from the other side\n", "theirs");
    git::push(&other, "origin", &fx.branch, false).unwrap();

    // Nothing local: a fast-forward brings their commit home, and says so.
    let before = git::head(&fx.repo).unwrap();
    let (rec, out) = interactive::ops::pull(&fx.repo, "origin", PullMode::FfOnly, &c).expect("ff");
    assert!(rec.ref_name.contains("-pull"), "{rec:?}");
    assert_eq!(out.mode, PullMode::FfOnly);
    assert_eq!(out.upstream, format!("origin/{}", fx.branch));
    assert_eq!(out.from, before);
    assert_eq!(out.to, theirs);
    assert!(out.moved);
    assert_eq!(read(&fx.repo, "theirs.txt"), "from the other side\n");

    // Nothing new: a pull is a no-op that still answers honestly.
    let (_, quiet) = interactive::ops::pull(&fx.repo, "origin", PullMode::FfOnly, &c).unwrap();
    assert!(!quiet.moved);
    assert_eq!(quiet.from, quiet.to);

    // Both sides moved: fast-forward-only refuses with the counts, nothing changes.
    let mine = commit_file(&fx.repo, "mine.txt", "mine\n", "mine");
    commit_file(&other, "theirs2.txt", "more\n", "theirs 2");
    raw_git(
        &other,
        &["pull", "--quiet", "--rebase", "origin", &fx.branch],
    );
    git::push(&other, "origin", &fx.branch, false).unwrap();
    let err =
        interactive::ops::pull(&fx.repo, "origin", PullMode::FfOnly, &c).expect_err("diverged");
    assert!(
        matches!(
            err,
            VcsError::NotFastForward {
                ahead: 1,
                behind: 1
            }
        ),
        "{err}"
    );
    assert_eq!(
        git::head(&fx.repo).unwrap(),
        mine,
        "a refusal moves nothing"
    );
    assert!(!git::can_fast_forward(&fx.repo, &format!("origin/{}", fx.branch)).unwrap());

    // Rebase replays the local commit on top: linear history, their file present.
    let (_, rebased) =
        interactive::ops::pull(&fx.repo, "origin", PullMode::Rebase, &c).expect("rebase");
    assert!(rebased.moved);
    assert_eq!(read(&fx.repo, "theirs2.txt"), "more\n");
    assert_eq!(read(&fx.repo, "mine.txt"), "mine\n");
    assert!(
        !has_second_parent(&fx.repo),
        "a rebase makes no merge commit"
    );
    assert_eq!(git::in_progress(&fx.repo).unwrap(), None);

    // Merge, on the next divergence, makes a merge commit with two parents.
    commit_file(&fx.repo, "mine2.txt", "mine 2\n", "mine 2");
    commit_file(&other, "theirs3.txt", "three\n", "theirs 3");
    raw_git(
        &other,
        &["pull", "--quiet", "--rebase", "origin", &fx.branch],
    );
    git::push(&other, "origin", &fx.branch, false).unwrap();
    let (_, merged) =
        interactive::ops::pull(&fx.repo, "origin", PullMode::Merge, &c).expect("merge");
    assert!(merged.moved);
    assert!(has_second_parent(&fx.repo), "a merge commit");
    assert_eq!(read(&fx.repo, "theirs3.txt"), "three\n");
}

#[test]
fn pull_refuses_a_branch_with_no_upstream_and_never_touches_the_tree() {
    let fx = Fixture::new();
    let c = consent();
    let head = git::head(&fx.repo).unwrap();
    assert_eq!(git::upstream_of(&fx.repo).unwrap(), None);
    let err =
        interactive::ops::pull(&fx.repo, "origin", PullMode::FfOnly, &c).expect_err("no upstream");
    assert!(matches!(err, VcsError::NoRemote(_)), "{err}");
    assert_eq!(git::head(&fx.repo).unwrap(), head);
    let err = interactive::ops::pull(&fx.repo, "nowhere", PullMode::FfOnly, &c)
        .expect_err("no such remote");
    assert!(matches!(err, VcsError::NoRemote(_)), "{err}");
}

#[test]
fn a_conflicting_pull_names_its_paths_and_what_to_abort() {
    let fx = Fixture::new();
    let c = consent();
    git::push(&fx.repo, "origin", &fx.branch, true).unwrap();
    let other = other_side(&fx);
    commit_file(&other, "README.md", "theirs\nworld\n", "theirs");
    git::push(&other, "origin", &fx.branch, false).unwrap();
    let mine = commit_file(&fx.repo, "README.md", "mine\nworld\n", "mine");

    let err =
        interactive::ops::pull(&fx.repo, "origin", PullMode::Merge, &c).expect_err("conflict");
    match &err {
        VcsError::Conflict {
            paths,
            in_progress,
            message,
        } => {
            assert_eq!(paths, &[PathBuf::from("README.md")]);
            assert_eq!(*in_progress, Some(InProgress::Merge));
            assert!(
                message.to_ascii_lowercase().contains("conflict"),
                "{message}"
            );
        }
        other => panic!("expected a conflict, got {other}"),
    }
    assert_eq!(
        git::conflicted_paths(&fx.repo).unwrap(),
        vec![PathBuf::from("README.md")]
    );
    interactive::ops::abort(&fx.repo, InProgress::Merge, &c).unwrap();
    assert_eq!(git::head(&fx.repo).unwrap(), mine);
    assert_eq!(read(&fx.repo, "README.md"), "mine\nworld\n");

    // The rebase form conflicts the same way and is aborted the same way.
    let err =
        interactive::ops::pull(&fx.repo, "origin", PullMode::Rebase, &c).expect_err("conflict");
    assert!(
        matches!(&err, VcsError::Conflict { in_progress: Some(InProgress::Rebase), paths, .. } if paths.len() == 1),
        "{err}"
    );
    interactive::ops::abort(&fx.repo, InProgress::Rebase, &c).unwrap();
    assert_eq!(git::head(&fx.repo).unwrap(), mine);
}

#[test]
fn revert_undoes_a_commit_with_a_new_one_and_a_conflicting_revert_is_typed() {
    let fx = Fixture::new();
    let c = consent();
    let added = commit_file(&fx.repo, "feature.txt", "feature\n", "add feature");
    let rec = interactive::ops::revert(
        &fx.repo,
        &RevertRequest {
            commits: vec![added.as_str().to_string()],
            no_commit: false,
            mainline: None,
        },
        &c,
    )
    .expect("revert");
    assert!(rec.ref_name.contains("-revert"), "{rec:?}");
    assert!(
        !fx.repo.join("feature.txt").exists(),
        "the revert removed what the commit added"
    );
    assert_ne!(
        git::head(&fx.repo).unwrap(),
        added,
        "a revert is a new commit"
    );
    assert_eq!(git::in_progress(&fx.repo).unwrap(), None);
    assert_eq!(
        raw_git(&fx.repo, &["log", "-1", "--format=%s"]),
        "Revert \"add feature\""
    );

    // Reverting a commit whose lines were changed since conflicts, names the
    // path, and leaves a revert in progress that abort clears.
    let first = commit_file(&fx.repo, "notes.txt", "one\n", "notes one");
    commit_file(&fx.repo, "notes.txt", "two\n", "notes two");
    let head = git::head(&fx.repo).unwrap();
    let err = interactive::ops::revert(
        &fx.repo,
        &RevertRequest {
            commits: vec![first.as_str().to_string()],
            no_commit: false,
            mainline: None,
        },
        &c,
    )
    .expect_err("conflict");
    assert!(
        matches!(&err, VcsError::Conflict { in_progress: Some(InProgress::Revert), paths, .. } if paths == &[PathBuf::from("notes.txt")]),
        "{err}"
    );
    interactive::ops::abort(&fx.repo, InProgress::Revert, &c).unwrap();
    assert_eq!(git::head(&fx.repo).unwrap(), head);
    assert_eq!(read(&fx.repo, "notes.txt"), "two\n");
}

// ---------------------------------------------------------------------------
// Stash (ide/04 §Stash)
// ---------------------------------------------------------------------------

use bisa_vcs::git::RecoveryKind;
use bisa_vcs::interactive::{StashPush, StashTarget};

fn target(entry: &git::StashEntry) -> StashTarget {
    StashTarget {
        index: entry.index,
        commit: entry.commit.as_str().to_string(),
    }
}

#[test]
fn stash_push_saves_the_tree_captures_first_and_lists_the_entry_with_its_message_and_branch() {
    let fx = Fixture::new();
    let c = consent();
    let g = git::Git::default();
    std::fs::write(fx.repo.join("README.md"), "hello\nworld\nstaged\n").unwrap();
    git::stage(&fx.repo, &["README.md"]).unwrap();
    std::fs::write(fx.repo.join("notes.txt"), "brand new\n").unwrap();

    // Untracked files stay behind unless asked for; the staged half stays
    // staged with `keep_index`.
    let (rec, entry) = interactive::stash_push(
        &g,
        &fx.repo,
        &StashPush {
            message: Some("  half a change ".into()),
            include_untracked: false,
            keep_index: true,
            paths: vec![],
        },
        &c,
    )
    .expect("stash push");
    assert!(!rec.was_clean, "the tree was captured first: {rec:?}");
    assert!(
        rec.ref_name.ends_with("-stash_push.wip"),
        "{}",
        rec.ref_name
    );
    assert_eq!(entry.index, 0);
    assert_eq!(entry.branch.as_deref(), Some(fx.branch.as_str()));
    assert_eq!(entry.message.as_deref(), Some("half a change"));
    assert_eq!(entry.subject, format!("On {}: half a change", fx.branch));
    assert!(!entry.untracked);
    assert!(
        fx.repo.join("notes.txt").exists(),
        "untracked is not stashed without -u"
    );
    let readme = git::status_files(&fx.repo)
        .unwrap()
        .into_iter()
        .find(|f| f.path == Path::new("README.md"))
        .expect("kept in the index");
    assert!(readme.is_staged() && !readme.is_unstaged(), "{readme:?}");
    let listed = git::stash_list(&fx.repo).unwrap();
    assert_eq!(listed, vec![entry.clone()]);
    assert_eq!(raw_git(&fx.repo, &["stash", "list"]).lines().count(), 1);

    // A second push with untracked files and no message: git's own subject.
    git::unstage(&fx.repo, &["README.md"]).unwrap();
    let (_, second) = interactive::ops::stash_push(
        &fx.repo,
        &StashPush {
            include_untracked: true,
            ..StashPush::default()
        },
        &c,
    )
    .expect("second push");
    assert_eq!(second.index, 0);
    assert_eq!(second.message, None);
    assert!(
        second
            .subject
            .starts_with(&format!("WIP on {}: ", fx.branch)),
        "{}",
        second.subject
    );
    assert!(second.untracked);
    assert!(!fx.repo.join("notes.txt").exists());
    assert_eq!(read(&fx.repo, "README.md"), "hello\nworld\n");
    let listed = git::stash_list(&fx.repo).unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(
        listed[1].commit, entry.commit,
        "the first entry moved to index 1"
    );
    assert_eq!(listed[1].index, 1);
    let patch = git::stash_diff(&fx.repo, second.commit.as_str()).unwrap();
    assert!(patch.contains("+staged"), "the tracked half: {patch}");
    assert!(
        patch.contains("notes.txt") && patch.contains("+brand new"),
        "the untracked half: {patch}"
    );
}

#[test]
fn stash_push_with_nothing_to_save_is_refused_by_name_and_writes_no_stash() {
    let fx = Fixture::new();
    let c = consent();
    let g = git::Git::default();
    let clean = interactive::stash_push(&g, &fx.repo, &StashPush::default(), &c);
    assert!(matches!(clean, Err(VcsError::NothingToStash)), "{clean:?}");
    // Only untracked files, and untracked not asked for.
    std::fs::write(fx.repo.join("notes.txt"), "new\n").unwrap();
    let untracked_only = interactive::stash_push(&g, &fx.repo, &StashPush::default(), &c);
    assert!(
        matches!(untracked_only, Err(VcsError::NothingToStash)),
        "{untracked_only:?}"
    );
    // A path with no change in it.
    std::fs::write(fx.repo.join("README.md"), "hello\nworld\nmore\n").unwrap();
    let elsewhere = interactive::stash_push(
        &g,
        &fx.repo,
        &StashPush {
            paths: vec!["notes.txt".into()],
            ..StashPush::default()
        },
        &c,
    );
    assert!(
        matches!(elsewhere, Err(VcsError::NothingToStash)),
        "{elsewhere:?}"
    );
    assert!(git::stash_list(&fx.repo).unwrap().is_empty());
    assert!(
        git::recovery_list(&fx.repo).unwrap().is_empty(),
        "a refusal writes no recovery ref"
    );
    // A path-scoped push takes only that path.
    let (_, entry) = interactive::stash_push(
        &g,
        &fx.repo,
        &StashPush {
            paths: vec!["README.md".into()],
            ..StashPush::default()
        },
        &c,
    )
    .unwrap();
    assert_eq!(entry.index, 0);
    assert_eq!(read(&fx.repo, "README.md"), "hello\nworld\n");
    assert!(fx.repo.join("notes.txt").exists());
}

#[test]
fn stash_apply_keeps_the_entry_and_pop_drops_it_only_when_the_apply_succeeded() {
    let fx = Fixture::new();
    let c = consent();
    let g = git::Git::default();
    std::fs::write(fx.repo.join("README.md"), "hello\nworld\nstashed\n").unwrap();
    let (_, entry) = interactive::stash_push(&g, &fx.repo, &StashPush::default(), &c).unwrap();
    assert_eq!(read(&fx.repo, "README.md"), "hello\nworld\n");

    let applied = interactive::stash_apply(&g, &fx.repo, &target(&entry), &c).expect("apply");
    assert!(applied.was_clean, "the clean tree was pinned first");
    assert!(
        applied.ref_name.ends_with("-stash_apply"),
        "{}",
        applied.ref_name
    );
    assert_eq!(read(&fx.repo, "README.md"), "hello\nworld\nstashed\n");
    assert_eq!(
        git::stash_list(&fx.repo).unwrap().len(),
        1,
        "apply keeps the entry"
    );

    // Back to clean, then a conflicting commit on the same lines.
    interactive::discard_paths(&g, &fx.repo, &["README.md"], &c).unwrap();
    std::fs::write(
        fx.repo.join("README.md"),
        "hello\nworld\ncommitted instead\n",
    )
    .unwrap();
    git::add_all(&fx.repo).unwrap();
    git::commit(&fx.repo, "conflicting", false).unwrap();
    let popped = interactive::stash_pop(&g, &fx.repo, &target(&entry), &c);
    match popped {
        Err(VcsError::Conflict {
            paths, in_progress, ..
        }) => {
            assert_eq!(paths, vec![PathBuf::from("README.md")]);
            assert_eq!(
                in_progress, None,
                "a stash conflict leaves no operation to abort"
            );
        }
        other => panic!("expected a conflict, got {other:?}"),
    }
    assert_eq!(
        git::stash_list(&fx.repo).unwrap().len(),
        1,
        "a conflicting pop keeps the entry"
    );
    assert!(
        read(&fx.repo, "README.md").contains("<<<<<<<"),
        "the markers are in the tree"
    );
    // The stash pin was written before anything moved, and the entry is
    // still listed, so both doors to the work are open.
    let pins: Vec<_> = git::recovery_list(&fx.repo)
        .unwrap()
        .into_iter()
        .filter(|r| r.op == "stash_pop" && r.kind == RecoveryKind::Stash)
        .collect();
    assert_eq!(pins.len(), 1, "{pins:#?}");
    assert_eq!(pins[0].commit, entry.commit);

    // Settle the conflict by discarding, then pop cleanly on a matching base.
    interactive::discard_paths(&g, &fx.repo, &["README.md"], &c).unwrap();
    raw_git(&fx.repo, &["reset", "--hard", "HEAD~1"]);
    let entry = git::stash_list(&fx.repo).unwrap().remove(0);
    let popped = interactive::ops::stash_pop(&fx.repo, &target(&entry), &c).expect("clean pop");
    assert!(
        popped.ref_name.contains("-stash_pop") && !popped.ref_name.ends_with(".wip"),
        "a clean tree pinned as a commit (bumped when both pops land in one second): {}",
        popped.ref_name
    );
    assert_eq!(read(&fx.repo, "README.md"), "hello\nworld\nstashed\n");
    assert!(
        git::stash_list(&fx.repo).unwrap().is_empty(),
        "a clean pop drops the entry"
    );
}

#[test]
fn a_dropped_or_popped_stash_is_pinned_as_a_stash_recovery_and_restore_puts_the_entry_back() {
    let fx = Fixture::new();
    let c = consent();
    let g = git::Git::default();
    std::fs::write(fx.repo.join("README.md"), "hello\nworld\nparked\n").unwrap();
    let (_, entry) = interactive::stash_push(
        &g,
        &fx.repo,
        &StashPush {
            message: Some("parked".into()),
            ..StashPush::default()
        },
        &c,
    )
    .unwrap();
    // Dirty the tree so the drop's recovery has both a tree and a pin.
    std::fs::write(fx.repo.join("other.txt"), "meanwhile\n").unwrap();
    git::stage(&fx.repo, &["other.txt"]).unwrap();

    let dropped = interactive::stash_drop(&g, &fx.repo, &target(&entry), &c).expect("drop");
    assert!(!dropped.was_clean);
    assert!(
        dropped.ref_name.ends_with("-stash_drop.wip"),
        "{}",
        dropped.ref_name
    );
    assert!(git::stash_list(&fx.repo).unwrap().is_empty());
    let listed = git::recovery_list(&fx.repo).unwrap();
    let pin = listed
        .iter()
        .find(|r| r.kind == RecoveryKind::Stash)
        .expect("the dropped stash is pinned");
    assert_eq!(pin.op, "stash_drop");
    assert_eq!(pin.commit, entry.commit);
    assert!(
        pin.ref_name.ends_with("-stash_drop.stash"),
        "{}",
        pin.ref_name
    );
    assert_eq!(pin.branch.as_deref(), Some(fx.branch.as_str()));
    assert!(
        listed
            .iter()
            .any(|r| r.kind == RecoveryKind::Tree && r.op == "stash_drop"),
        "the dirty tree was saved beside it: {listed:#?}"
    );

    // Restore re-lists the entry with its own subject and moves nothing.
    let restored = interactive::ops::restore(&fx.repo, &pin.ref_name, &c).expect("restore");
    assert!(
        !restored.was_clean,
        "restoring recorded the dirty tree first"
    );
    let back = git::stash_list(&fx.repo).unwrap();
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].commit, entry.commit);
    assert_eq!(back[0].subject, entry.subject);
    assert_eq!(back[0].message.as_deref(), Some("parked"));
    assert_eq!(
        read(&fx.repo, "README.md"),
        "hello\nworld\n",
        "the tree was not touched"
    );
    assert!(fx.repo.join("other.txt").exists());
}

#[test]
fn a_stash_target_whose_index_moved_is_refused_before_anything_is_captured() {
    let fx = Fixture::new();
    let c = consent();
    let g = git::Git::default();
    std::fs::write(fx.repo.join("README.md"), "hello\nworld\none\n").unwrap();
    let (_, first) = interactive::stash_push(&g, &fx.repo, &StashPush::default(), &c).unwrap();
    std::fs::write(fx.repo.join("README.md"), "hello\nworld\ntwo\n").unwrap();
    let (_, second) = interactive::stash_push(&g, &fx.repo, &StashPush::default(), &c).unwrap();
    assert_eq!(second.index, 0);
    let before = git::recovery_list(&fx.repo).unwrap().len();
    // The caller still holds `first` at index 0 — but that is `second` now.
    let stale = StashTarget {
        index: 0,
        commit: first.commit.as_str().to_string(),
    };
    for result in [
        interactive::stash_apply(&g, &fx.repo, &stale, &c),
        interactive::stash_pop(&g, &fx.repo, &stale, &c),
        interactive::stash_drop(&g, &fx.repo, &stale, &c),
    ] {
        match result {
            Err(VcsError::StashMoved { index, commit, now }) => {
                assert_eq!(index, 0);
                assert_eq!(commit, first.commit.as_str());
                assert_eq!(now, Some(second.commit.clone()));
            }
            other => panic!("expected StashMoved, got {other:?}"),
        }
    }
    assert_eq!(git::stash_list(&fx.repo).unwrap().len(), 2, "nothing moved");
    assert_eq!(
        git::recovery_list(&fx.repo).unwrap().len(),
        before,
        "a refusal writes no recovery ref"
    );
    // An index past the end reads `now: None`.
    let gone = interactive::stash_drop(
        &g,
        &fx.repo,
        &StashTarget {
            index: 7,
            commit: first.commit.as_str().to_string(),
        },
        &c,
    );
    assert!(
        matches!(gone, Err(VcsError::StashMoved { now: None, .. })),
        "{gone:?}"
    );
}

#[test]
fn apply_refuses_untracked_collisions_first_lands_the_whole_stash_and_types_a_conflict() {
    let fx = Fixture::new();
    let c = consent();
    let g = git::Git::default();
    // A stash with an untracked file; then the same path appears in the tree.
    std::fs::write(fx.repo.join("README.md"), "hello\nworld\nchange\n").unwrap();
    std::fs::write(fx.repo.join("scratch.txt"), "from the stash\n").unwrap();
    let (_, entry) = interactive::stash_push(
        &g,
        &fx.repo,
        &StashPush {
            include_untracked: true,
            ..StashPush::default()
        },
        &c,
    )
    .unwrap();
    assert!(entry.untracked);
    std::fs::write(fx.repo.join("scratch.txt"), "already here\n").unwrap();
    let before = git::recovery_list(&fx.repo).unwrap().len();
    let collided = interactive::stash_apply(&g, &fx.repo, &target(&entry), &c);
    match collided {
        Err(VcsError::Dirty { details, .. }) => {
            assert!(details.contains("scratch.txt"), "{details}")
        }
        other => panic!("expected Dirty naming the path, got {other:?}"),
    }
    assert_eq!(
        read(&fx.repo, "scratch.txt"),
        "already here\n",
        "nothing was applied"
    );
    assert_eq!(
        git::recovery_list(&fx.repo).unwrap().len(),
        before,
        "refused before capture"
    );
    std::fs::remove_file(fx.repo.join("scratch.txt")).unwrap();

    // Onto a clean tree the whole stash lands: the tracked change and the
    // untracked file alike, and the entry stays on the list.
    std::fs::write(fx.repo.join("README.md"), "hello\nworld\n").unwrap();
    interactive::stash_apply(&g, &fx.repo, &target(&entry), &c).expect("applies");
    assert_eq!(read(&fx.repo, "README.md"), "hello\nworld\nchange\n");
    assert_eq!(read(&fx.repo, "scratch.txt"), "from the stash\n");

    // A real conflict is typed with its paths, and the tree carries markers.
    interactive::discard_paths(&g, &fx.repo, &["README.md"], &c).unwrap();
    std::fs::remove_file(fx.repo.join("scratch.txt")).unwrap();
    std::fs::write(fx.repo.join("README.md"), "hello\nworld\nother change\n").unwrap();
    git::add_all(&fx.repo).unwrap();
    git::commit(&fx.repo, "diverge", false).unwrap();
    let conflict = interactive::stash_apply(&g, &fx.repo, &target(&entry), &c);
    match conflict {
        Err(VcsError::Conflict { paths, .. }) => {
            assert_eq!(paths, vec![PathBuf::from("README.md")])
        }
        other => panic!("expected Conflict, got {other:?}"),
    }
    assert!(read(&fx.repo, "README.md").contains("<<<<<<<"));
    assert_eq!(
        git::stash_list(&fx.repo).unwrap().len(),
        1,
        "apply never drops"
    );
}

#[test]
fn the_pin_ref_is_bumped_with_its_main_name_and_parses_as_its_op() {
    let fx = Fixture::new();
    let c = consent();
    let g = git::Git::default();
    git::branch_create(&fx.repo, "one", None, false).unwrap();
    git::branch_create(&fx.repo, "two", None, false).unwrap();
    std::fs::write(fx.repo.join("README.md"), "hello\nworld\ndirty\n").unwrap();
    // Two deletes of tips beside a dirty tree, within one second more often
    // than not: the pin refs must both survive.
    let a = interactive::branch_delete(&g, &fx.repo, "one", &c).unwrap();
    let b = interactive::branch_delete(&g, &fx.repo, "two", &c).unwrap();
    assert!(!a.was_clean && !b.was_clean);
    let listed = git::recovery_list(&fx.repo).unwrap();
    let pins: Vec<_> = listed
        .iter()
        .filter(|r| r.kind == RecoveryKind::Commit && r.op == "branch_delete")
        .collect();
    assert_eq!(pins.len(), 2, "{listed:#?}");
    assert!(
        pins.iter().all(|r| r.ref_name.contains("_pin")),
        "{pins:#?}"
    );
    assert!(
        listed.iter().all(|r| !r.op.contains("_pin")),
        "the pin suffix is not part of the op: {listed:#?}"
    );
    let trees = listed
        .iter()
        .filter(|r| r.kind == RecoveryKind::Tree)
        .count();
    assert_eq!(trees, 2);
}

// ---------------------------------------------------------------------------
// Conflicts settled and continued; the operations' options
// ---------------------------------------------------------------------------

/// A branch off the fixture's, with one commit that changes `rel` to `text`.
/// Made in a throwaway worktree so the fixture's checkout never moves.
fn branch_with(fx: &Fixture, name: &str, rel: &str, text: &str, message: &str) -> git::CommitId {
    let wt = fx
        ._root
        .path()
        .join(format!("wt-{}", name.replace('/', "-")));
    git::worktree_add(&fx.repo, &wt, name, &fx.branch).unwrap();
    std::fs::write(wt.join(rel), text).unwrap();
    git::add_all(&wt).unwrap();
    let id = git::commit(&wt, message, false).unwrap();
    git::worktree_remove(&wt, false).unwrap();
    id
}

fn subjects(repo: &Path, range: &str) -> Vec<String> {
    raw_git(repo, &["log", "--format=%s", range])
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn a_conflicting_merge_is_settled_by_side_and_continued_and_continue_refuses_while_paths_are_unmerged(
) {
    let fx = Fixture::new();
    let c = consent();
    branch_with(&fx, "theirs", "README.md", "theirs\nworld\n", "theirs");
    commit_file(&fx.repo, "README.md", "mine\nworld\n", "mine");

    let err =
        interactive::ops::merge(&fx.repo, "theirs", MergeMode::Ff, None, &c).expect_err("conflict");
    assert!(
        matches!(
            &err,
            VcsError::Conflict {
                in_progress: Some(InProgress::Merge),
                ..
            }
        ),
        "{err}"
    );

    // Continue refuses by name while a path is unmerged, and writes no ref.
    let refs_before = git::recovery_list(&fx.repo).unwrap().len();
    let err = interactive::ops::continue_op(&fx.repo, InProgress::Merge, &c).expect_err("unmerged");
    assert!(
        matches!(&err, VcsError::Conflict { paths, in_progress: Some(InProgress::Merge), .. } if paths == &[PathBuf::from("README.md")]),
        "{err}"
    );
    assert_eq!(
        git::recovery_list(&fx.repo).unwrap().len(),
        refs_before,
        "a refusal writes nothing"
    );
    assert!(
        matches!(
            interactive::ops::skip_op(&fx.repo, InProgress::Merge, &c),
            Err(VcsError::InvalidArg { .. })
        ),
        "a merge has nothing to skip"
    );
    assert!(
        matches!(
            interactive::ops::continue_op(&fx.repo, InProgress::Rebase, &c),
            Err(VcsError::InvalidArg { .. })
        ),
        "the operation is named, and a merge is not a rebase"
    );

    // Take their side whole, then continue: git's prepared message stands, no editor asked.
    let rec = interactive::ops::resolve(&fx.repo, "README.md", ConflictSide::Theirs.into(), &c)
        .expect("resolve");
    assert!(rec.ref_name.contains("-resolve"));
    assert_eq!(read(&fx.repo, "README.md"), "theirs\nworld\n");
    assert!(git::conflicted_paths(&fx.repo).unwrap().is_empty());
    let rec = interactive::ops::continue_op(&fx.repo, InProgress::Merge, &c).expect("continue");
    assert!(rec.ref_name.contains("-continue"));
    assert_eq!(git::in_progress(&fx.repo).unwrap(), None);
    assert_eq!(
        raw_git(&fx.repo, &["log", "-1", "--format=%s"]),
        "Merge branch 'theirs'"
    );
    assert_eq!(
        raw_git(&fx.repo, &["log", "-1", "--format=%P"])
            .split_whitespace()
            .count(),
        2,
        "a merge commit"
    );
    assert!(
        matches!(
            interactive::ops::continue_op(&fx.repo, InProgress::Merge, &c),
            Err(VcsError::InvalidArg { .. })
        ),
        "nothing is in progress any more"
    );
}

/// A path one side deleted and the other changed has no side to check out:
/// it is settled by keeping the file (the side that has it) or removing it
/// (`Resolution::Delete`, git's own answer), and the merge goes on.
#[test]
fn a_path_deleted_by_one_side_is_settled_by_removing_it_or_keeping_it() {
    let fx = Fixture::new();
    let c = consent();
    let wt = fx.repo.parent().unwrap().join("wt-deleted");
    git::worktree_add(&fx.repo, &wt, "deleter", &fx.branch).unwrap();
    std::fs::remove_file(wt.join("README.md")).unwrap();
    git::add_all(&wt).unwrap();
    git::commit(&wt, "theirs drops it", false).unwrap();
    git::worktree_remove(&wt, false).unwrap();
    commit_file(&fx.repo, "README.md", "hello\nours\n", "ours changes it");

    let err = interactive::ops::merge(&fx.repo, "deleter", MergeMode::Ff, None, &c).unwrap_err();
    assert!(matches!(err, VcsError::Conflict { .. }), "{err:?}");
    let rows = git::status_files(&fx.repo).unwrap();
    assert_eq!(rows[0].conflict, Some(ConflictKind::DeletedByThem));

    // Taking the side that deleted it is not a checkout: it is a removal.
    let rec = interactive::ops::resolve(&fx.repo, "README.md", Resolution::Delete, &c).unwrap();
    assert!(rec.ref_name.contains("-resolve"));
    assert!(
        !fx.repo.join("README.md").exists(),
        "the path is gone from the tree"
    );
    assert!(
        git::conflicted_paths(&fx.repo).unwrap().is_empty(),
        "and settled in the index"
    );
    interactive::ops::continue_op(&fx.repo, InProgress::Merge, &c).unwrap();
    assert!(git::in_progress(&fx.repo).unwrap().is_none());
    assert!(!fx.repo.join("README.md").exists());
}

#[test]
fn merge_modes_land_as_asked() {
    let fx = Fixture::new();
    let c = consent();
    let tip = branch_with(&fx, "topic", "topic.txt", "t\n", "topic");
    interactive::ops::merge(&fx.repo, "topic", MergeMode::FfOnly, None, &c).expect("fast-forward");
    assert_eq!(
        git::head(&fx.repo).unwrap(),
        tip,
        "a fast-forward moves to the tip"
    );

    branch_with(&fx, "topic2", "t2.txt", "t\n", "topic2");
    interactive::ops::merge(
        &fx.repo,
        "topic2",
        MergeMode::NoFf,
        Some("merged topic2"),
        &c,
    )
    .expect("merge commit");
    assert_eq!(
        raw_git(&fx.repo, &["log", "-1", "--format=%P"])
            .split_whitespace()
            .count(),
        2,
        "always a merge commit"
    );
    assert_eq!(
        raw_git(&fx.repo, &["log", "-1", "--format=%s"]),
        "merged topic2"
    );

    branch_with(&fx, "topic3", "t3.txt", "t\n", "topic3");
    let before = git::head(&fx.repo).unwrap();
    interactive::ops::merge(&fx.repo, "topic3", MergeMode::Squash, None, &c).expect("squash");
    assert_eq!(
        git::head(&fx.repo).unwrap(),
        before,
        "a squash commits nothing"
    );
    assert_eq!(
        git::status(&fx.repo).unwrap().staged,
        1,
        "the change is staged as one"
    );
    git::commit(&fx.repo, "squashed topic3", false).unwrap();

    // Diverged: a fast-forward-only merge is git's refusal, nothing half-done.
    let base = raw_git(&fx.repo, &["rev-parse", "HEAD~2"]);
    let wt = fx._root.path().join("wt-topic4");
    git::worktree_add(&fx.repo, &wt, "topic4", &base).unwrap();
    std::fs::write(wt.join("t4.txt"), "t\n").unwrap();
    git::add_all(&wt).unwrap();
    git::commit(&wt, "topic4", false).unwrap();
    git::worktree_remove(&wt, false).unwrap();
    assert!(interactive::ops::merge(&fx.repo, "topic4", MergeMode::FfOnly, None, &c).is_err());
    assert_eq!(git::in_progress(&fx.repo).unwrap(), None);
    assert!(
        matches!(
            interactive::ops::merge(&fx.repo, "topic4", MergeMode::NoFf, Some("  "), &c),
            Err(VcsError::InvalidArg { .. })
        ),
        "an empty message is refused before anything moves"
    );
}

#[test]
fn a_conflicting_cherry_pick_of_two_commits_is_skipped_and_the_rest_lands_with_its_origin() {
    let fx = Fixture::new();
    let c = consent();
    commit_file(&fx.repo, "notes.txt", "one\n", "notes one");
    let wt = fx._root.path().join("wt-picks");
    git::worktree_add(&fx.repo, &wt, "picks", &fx.branch).unwrap();
    std::fs::write(wt.join("notes.txt"), "theirs\n").unwrap();
    git::add_all(&wt).unwrap();
    let a = git::commit(&wt, "A", false).unwrap();
    std::fs::write(wt.join("pick.txt"), "picked\n").unwrap();
    git::add_all(&wt).unwrap();
    let b = git::commit(&wt, "B", false).unwrap();
    git::worktree_remove(&wt, false).unwrap();
    commit_file(&fx.repo, "notes.txt", "two\n", "notes two");

    let req = PickRequest {
        commits: vec![a.as_str().to_string(), b.as_str().to_string()],
        record_origin: true,
        no_commit: false,
        mainline: None,
    };
    let err = interactive::ops::cherry_pick(&fx.repo, &req, &c).expect_err("A conflicts");
    assert!(
        matches!(&err, VcsError::Conflict { in_progress: Some(InProgress::CherryPick), paths, .. } if paths == &[PathBuf::from("notes.txt")]),
        "{err}"
    );
    let rec = interactive::ops::skip_op(&fx.repo, InProgress::CherryPick, &c).expect("skip A");
    assert!(rec.ref_name.contains("-skip"));
    assert_eq!(
        git::in_progress(&fx.repo).unwrap(),
        None,
        "B landed and the sequence ended"
    );
    assert_eq!(read(&fx.repo, "notes.txt"), "two\n", "A was left out");
    assert_eq!(read(&fx.repo, "pick.txt"), "picked\n");
    assert!(
        raw_git(&fx.repo, &["log", "-1", "--format=%B"]).contains("cherry picked from commit"),
        "the origin line was recorded"
    );
    assert!(
        matches!(
            interactive::ops::cherry_pick(
                &fx.repo,
                &PickRequest {
                    commits: vec![],
                    record_origin: false,
                    no_commit: false,
                    mainline: None
                },
                &c
            ),
            Err(VcsError::InvalidArg { .. })
        ),
        "no commits, no pick"
    );
}

#[test]
fn rebase_carries_a_dirty_tree_with_autostash_and_onto_replays_only_the_commits_since_upstream() {
    let fx = Fixture::new();
    let c = consent();
    branch_with(&fx, "topic", "t1.txt", "1\n", "t1");
    interactive::ops::checkout(&fx.repo, "topic", &c).unwrap();
    commit_file(&fx.repo, "t2.txt", "2\n", "t2");
    // main moves on while topic is checked out here.
    let wt = fx._root.path().join("wt-main");
    git::worktree_add_existing(&fx.repo, &wt, &fx.branch).unwrap();
    std::fs::write(wt.join("m1.txt"), "m\n").unwrap();
    git::add_all(&wt).unwrap();
    let m1 = git::commit(&wt, "m1", false).unwrap();
    git::worktree_remove(&wt, false).unwrap();

    std::fs::write(fx.repo.join("README.md"), "hello\nworld\nunsaved\n").unwrap();
    let dirty = RebaseRequest {
        upstream: fx.branch.clone(),
        onto: None,
        autostash: false,
    };
    assert!(
        interactive::ops::rebase(&fx.repo, &dirty, &c).is_err(),
        "a dirty tree refuses a plain rebase"
    );
    let stashed = RebaseRequest {
        upstream: fx.branch.clone(),
        onto: None,
        autostash: true,
    };
    interactive::ops::rebase(&fx.repo, &stashed, &c).expect("rebase with autostash");
    assert_eq!(
        read(&fx.repo, "README.md"),
        "hello\nworld\nunsaved\n",
        "the edit came back"
    );
    assert_eq!(
        raw_git(&fx.repo, &["merge-base", "topic", &fx.branch]),
        m1.as_str(),
        "topic now stands on m1"
    );
    assert_eq!(
        subjects(&fx.repo, &format!("{}..topic", fx.branch)),
        vec!["t2", "t1"]
    );

    // --onto: the two commits since main replayed on another branch, m1 left behind.
    std::fs::write(fx.repo.join("README.md"), "hello\nworld\n").unwrap();
    let wt = fx._root.path().join("wt-other");
    let base = raw_git(&fx.repo, &["rev-parse", &format!("{}~1", fx.branch)]);
    git::worktree_add(&fx.repo, &wt, "other", &base).unwrap();
    std::fs::write(wt.join("o1.txt"), "o\n").unwrap();
    git::add_all(&wt).unwrap();
    git::commit(&wt, "o1", false).unwrap();
    git::worktree_remove(&wt, false).unwrap();
    let onto = RebaseRequest {
        upstream: fx.branch.clone(),
        onto: Some("other".into()),
        autostash: false,
    };
    interactive::ops::rebase(&fx.repo, &onto, &c).expect("rebase --onto");
    assert_eq!(subjects(&fx.repo, "other..topic"), vec!["t2", "t1"]);
    assert!(
        !subjects(&fx.repo, "topic").contains(&"m1".to_string()),
        "m1 is not on topic any more"
    );
    let between = git::commits_between(&fx.repo, "topic", "other", 10).unwrap();
    assert_eq!(
        between
            .iter()
            .map(|c| c.subject.as_str())
            .collect::<Vec<_>>(),
        vec!["t2", "t1"],
        "newest first"
    );
}

#[test]
fn an_interactive_plan_reorders_rewords_squashes_and_drops_and_a_bad_plan_is_refused_before_any_ref(
) {
    let fx = Fixture::new();
    let c = consent();
    raw_git(&fx.repo, &["tag", "base"]);
    let ids: Vec<String> = (1..=5)
        .map(|n| {
            commit_file(
                &fx.repo,
                &format!("f{n}.txt"),
                &format!("{n}\n"),
                ["one", "two", "three", "four", "five"][n - 1],
            )
            .as_str()
            .to_string()
        })
        .collect();
    let step = |action: RebaseAction, n: usize, message: Option<&str>| RebaseStep {
        action,
        commit: ids[n - 1].clone(),
        message: message.map(str::to_string),
    };
    let plan = |steps: Vec<RebaseStep>| RebasePlan {
        upstream: "base".into(),
        onto: None,
        steps,
    };

    let refs_before = git::recovery_list(&fx.repo).unwrap().len();
    for bad in [
        plan(vec![step(RebaseAction::Pick, 1, None)]),
        plan(vec![
            step(RebaseAction::Squash, 1, None),
            step(RebaseAction::Pick, 2, None),
            step(RebaseAction::Pick, 3, None),
            step(RebaseAction::Pick, 4, None),
            step(RebaseAction::Pick, 5, None),
        ]),
        plan(vec![
            step(RebaseAction::Reword, 1, None),
            step(RebaseAction::Pick, 2, None),
            step(RebaseAction::Pick, 3, None),
            step(RebaseAction::Pick, 4, None),
            step(RebaseAction::Pick, 5, None),
        ]),
        RebasePlan {
            upstream: "base".into(),
            onto: None,
            steps: vec![RebaseStep {
                action: RebaseAction::Pick,
                commit: "deadbeef".repeat(5),
                message: None,
            }],
        },
    ] {
        assert!(
            matches!(
                interactive::ops::rebase_plan(&fx.repo, &bad, &c),
                Err(VcsError::InvalidArg { .. })
            ),
            "{bad:?}"
        );
    }
    assert_eq!(
        git::recovery_list(&fx.repo).unwrap().len(),
        refs_before,
        "a refused plan writes nothing"
    );
    assert_eq!(git::in_progress(&fx.repo).unwrap(), None);

    let good = plan(vec![
        step(RebaseAction::Pick, 2, None),
        step(RebaseAction::Reword, 1, Some("one, reworded")),
        step(RebaseAction::Drop, 3, None),
        step(RebaseAction::Pick, 4, None),
        step(RebaseAction::Squash, 5, Some("four and five")),
    ]);
    let rec = interactive::ops::rebase_plan(&fx.repo, &good, &c).expect("plan");
    assert!(rec.ref_name.contains("rebase_plan"));
    assert_eq!(git::in_progress(&fx.repo).unwrap(), None);
    assert_eq!(
        subjects(&fx.repo, "base..HEAD"),
        vec!["four and five", "one, reworded", "two"]
    );
    assert!(!fx.repo.join("f3.txt").exists(), "dropped");
    assert!(
        fx.repo.join("f4.txt").exists() && fx.repo.join("f5.txt").exists(),
        "squashed together"
    );
    assert!(
        fx.repo.join(".git/bisa/rebase").is_dir(),
        "the messages lived under the git directory"
    );
}

#[test]
fn a_remote_branch_is_taken_up_as_a_tracking_branch_its_standing_is_read_and_its_delete_on_the_remote_pins_the_tip(
) {
    let fx = Fixture::new();
    let c = consent();
    git::push(&fx.repo, "origin", &fx.branch, true).unwrap();
    let theirs = branch_with(&fx, "feature/theirs", "theirs.txt", "t\n", "theirs");
    git::push(&fx.repo, "origin", "feature/theirs", false).unwrap();
    interactive::ops::branch_delete(&fx.repo, "feature/theirs", &c).unwrap();
    assert!(
        matches!(
            git::branch_create(&fx.repo, "feature/theirs", None, true),
            Err(VcsError::InvalidArg { .. })
        ),
        "a tracking branch needs the branch it follows"
    );
    git::branch_create(
        &fx.repo,
        "feature/theirs",
        Some("origin/feature/theirs"),
        true,
    )
    .expect("tracking branch");

    let list = git::branch_list(&fx.repo, Some(&fx.branch)).unwrap();
    let mine = list
        .iter()
        .find(|b| b.name == "feature/theirs")
        .expect("listed");
    assert_eq!(mine.upstream.as_deref(), Some("origin/feature/theirs"));
    assert_eq!((mine.ahead, mine.behind, mine.merged), (0, 0, false));
    let main = list.iter().find(|b| b.name == fx.branch).unwrap();
    assert!(main.merged, "a branch is merged into itself");

    commit_file(&fx.repo, "m.txt", "m\n", "ahead of origin");
    let main = git::branch_list(&fx.repo, Some(&fx.branch))
        .unwrap()
        .into_iter()
        .find(|b| b.current)
        .unwrap();
    assert_eq!(
        (main.ahead, main.behind),
        (1, 0),
        "one commit origin has not got"
    );
    interactive::ops::merge(&fx.repo, "feature/theirs", MergeMode::Ff, None, &c).unwrap();
    let merged = git::branch_list(&fx.repo, Some(&fx.branch))
        .unwrap()
        .into_iter()
        .find(|b| b.name == "feature/theirs")
        .unwrap();
    assert!(merged.merged, "reachable from the default branch now");

    git::branch_set_upstream(&fx.repo, "feature/theirs", None).unwrap();
    assert_eq!(
        git::branch_list(&fx.repo, None)
            .unwrap()
            .into_iter()
            .find(|b| b.name == "feature/theirs")
            .unwrap()
            .upstream,
        None
    );
    git::branch_set_upstream(&fx.repo, "feature/theirs", Some("origin/feature/theirs")).unwrap();

    let rec = interactive::ops::push_delete(&fx.repo, "origin", "feature/theirs", &c)
        .expect("delete on the remote");
    assert_eq!(rec.commit, theirs, "the remote branch's tip is pinned");
    assert!(rec.was_clean);
    assert!(
        git::remote_branch_list(&fx.repo)
            .unwrap()
            .iter()
            .all(|b| b.name != "feature/theirs"),
        "the tracking ref went with it"
    );
    assert!(
        interactive::ops::push_delete(&fx.repo, "origin", "never-fetched", &c).is_err(),
        "no tip to pin, nothing asked of the remote"
    );
}

// ---------------------------------------------------------------------------
// A selection read a moment ago
// ---------------------------------------------------------------------------

/// A discard over the Changes list as it was read, one file of which an agent
/// has since taken out of the index and deleted: the rest is discarded, never
/// the whole `checkout` refused for the one git no longer knows.
#[test]
fn a_discard_with_a_path_gone_since_it_was_listed_discards_the_rest() {
    let fx = Fixture::new();
    let c = consent();
    std::fs::write(fx.repo.join("gone.txt"), "tracked\n").unwrap();
    git::add_all(&fx.repo).unwrap();
    git::commit(&fx.repo, "a file the agent will remove", false).unwrap();
    std::fs::write(fx.repo.join("README.md"), "typed\n").unwrap();
    std::fs::write(fx.repo.join("gone.txt"), "typed too\n").unwrap();
    // The list is read; then the agent removes the file from git and disk.
    raw_git(&fx.repo, &["rm", "--quiet", "-f", "--", "gone.txt"]);

    interactive::ops::discard_paths(&fx.repo, &["README.md", "gone.txt", "never-there.txt"], &c)
        .expect("the rest is discarded");
    assert_eq!(read(&fx.repo, "README.md"), "hello\nworld\n");
    assert!(
        !fx.repo.join("gone.txt").exists(),
        "nothing brought back that git no longer holds"
    );
}

/// Nothing selected has a change git can put back — gone, or never tracked:
/// a typed refusal, before any recovery ref is written.
#[test]
fn a_discard_of_paths_all_gone_or_untracked_is_nothing_to_discard_and_saves_nothing() {
    let fx = Fixture::new();
    let c = consent();
    std::fs::write(fx.repo.join("untracked.txt"), "never added\n").unwrap();
    let refused = interactive::ops::discard_paths(&fx.repo, &["untracked.txt", "vanished.txt"], &c);
    assert!(
        matches!(refused, Err(VcsError::NothingToDiscard)),
        "{refused:?}"
    );
    assert_eq!(
        read(&fx.repo, "untracked.txt"),
        "never added\n",
        "an untracked file is never touched"
    );
    assert_eq!(
        raw_git(&fx.repo, &["for-each-ref", "refs/bisa/safety"]),
        "",
        "no recovery ref left behind"
    );
    // An empty request is still the caller's mistake, said as before.
    assert!(matches!(
        interactive::ops::discard_paths::<&str>(&fx.repo, &[], &c),
        Err(VcsError::InvalidArg { .. })
    ));
}

/// A stash over a selection one path of which is gone stashes the rest; a
/// selection that is all gone is nothing to stash, as an empty one is.
#[test]
fn a_stash_with_a_path_gone_since_it_was_listed_stashes_the_rest() {
    use bisa_vcs::interactive::StashPush;
    let fx = Fixture::new();
    let c = consent();
    let g = git::Git::default();
    std::fs::write(fx.repo.join("README.md"), "hello\nworld\nchanged\n").unwrap();

    let (_, entry) = interactive::stash_push(
        &g,
        &fx.repo,
        &StashPush {
            message: Some("the rest".into()),
            include_untracked: false,
            keep_index: false,
            paths: vec!["README.md".into(), "gone.txt".into()],
        },
        &c,
    )
    .expect("the rest is stashed");
    assert_eq!(entry.message.as_deref(), Some("the rest"), "{entry:?}");
    assert_eq!(
        read(&fx.repo, "README.md"),
        "hello\nworld\n",
        "the change went to the stash"
    );

    // With untracked files asked for, a vanished one is left out too.
    std::fs::write(fx.repo.join("new.txt"), "fresh\n").unwrap();
    interactive::stash_push(
        &g,
        &fx.repo,
        &StashPush {
            message: None,
            include_untracked: true,
            keep_index: false,
            paths: vec!["new.txt".into(), "vanished.txt".into()],
        },
        &c,
    )
    .expect("the untracked file is stashed");
    assert!(!fx.repo.join("new.txt").exists());

    let refused = interactive::stash_push(
        &g,
        &fx.repo,
        &StashPush {
            message: None,
            include_untracked: false,
            keep_index: false,
            paths: vec!["vanished.txt".into()],
        },
        &c,
    );
    assert!(
        matches!(refused, Err(VcsError::NothingToStash)),
        "{refused:?}"
    );
}
