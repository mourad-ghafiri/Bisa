//! Integration tests for the git layer.
//!
//! **Every repository here is created by the test, inside a `tempfile`
//! directory, and destroyed with it.** Nothing touches a user repository and
//! nothing touches the network: "origin" is a local bare repository addressed
//! by its path, which exercises the entire fetch/push code path — refspec
//! construction, upstream tracking, ahead/behind — with no remote in sight.
//!
//! The one thing these tests cannot prove is behaviour that only a real remote
//! has: credential prompts, rejected non-fast-forwards from a concurrent
//! writer, server-side hooks. Those are classified from stderr, and the
//! classifier is unit-tested against captured strings in `git.rs` instead.

use std::path::{Path, PathBuf};
use std::process::Command;

use bisa_vcs::git::{self, ChangeKind};
use bisa_vcs::{ConfigScope, ConflictKind, InProgress, SideRole, Step, VcsError};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Raw git, for the few fixture steps the crate deliberately does not expose
/// (`init --bare`) and for independent verification of what it did.
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

/// Raw git whose non-zero exit is an answer, not a failure — `config --get`
/// on a key that is not set exits 1 and says nothing.
fn raw_git_optional(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn git");
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Identity is set in the repository's *local* config, never globally and
/// never through the environment: a global write would be a side effect on the
/// developer's machine, and an env var would race across parallel tests.
fn set_identity(repo: &Path) {
    raw_git(repo, &["config", "user.name", "Bisa Test"]);
    raw_git(repo, &["config", "user.email", "test@example.invalid"]);
}

struct Fixture {
    _root: tempfile::TempDir,
    root: PathBuf,
    repo: PathBuf,
    origin: PathBuf,
    branch: String,
}

impl Fixture {
    /// A repository with one commit, and a bare repository next to it standing
    /// in for `origin`.
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

        let branch = git::default_branch(&repo).expect("default branch");
        git::remote_add(&repo, "origin", origin.to_str().unwrap()).expect("remote add");

        Self {
            root: root.path().to_path_buf(),
            _root: root,
            repo,
            origin,
            branch,
        }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    let c = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    c(a) == c(b)
}

// ---------------------------------------------------------------------------
// The happy path, end to end
// ---------------------------------------------------------------------------

#[test]
fn probe_reports_the_host_git() {
    let probe = git::probe();
    assert!(
        probe.available,
        "git must be on PATH to run this suite: {:?}",
        probe.reason
    );
}

#[test]
fn init_commit_worktrees_push_prune() {
    let fx = Fixture::new();

    // -- the seed commit ----------------------------------------------------
    let head = git::head(&fx.repo).expect("head");
    assert_eq!(head.as_str().len(), 40, "a full object id");
    let log = git::log(&fx.repo, None, 10).expect("log");
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].subject, "baseline");
    assert_eq!(log[0].email, "test@example.invalid");
    assert_eq!(log[0].id, head);

    assert_eq!(
        git::remote_get(&fx.repo, "origin").unwrap().as_deref(),
        Some(fx.origin.to_str().unwrap())
    );
    assert_eq!(git::remote_get(&fx.repo, "upstream").unwrap(), None);

    git::push(&fx.repo, "origin", &fx.branch, true).expect("push base");
    let st = git::status(&fx.repo).expect("status");
    assert_eq!(st.branch.as_deref(), Some(fx.branch.as_str()));
    assert_eq!(
        st.upstream.as_deref(),
        Some(&*format!("origin/{}", fx.branch))
    );
    assert_eq!((st.ahead, st.behind), (0, 0));
    assert!(st.is_clean && !st.detached);

    // -- two workstreams on one repository, concurrently -----------------------
    let a = fx.path("wt-checkout");
    let b = fx.path("wt-cart");
    git::worktree_add(&fx.repo, &a, "feature/checkout", &fx.branch).expect("worktree a");
    git::worktree_add(&fx.repo, &b, "fix/cart-total", &fx.branch).expect("worktree b");

    let listed = git::worktree_list(&fx.repo).expect("worktree list");
    assert_eq!(
        listed.len(),
        3,
        "main tree plus two workstreams: {listed:#?}"
    );
    let branches: Vec<&str> = listed.iter().filter_map(|w| w.branch.as_deref()).collect();
    assert!(branches.contains(&"feature/checkout"));
    assert!(branches.contains(&"fix/cart-total"));
    assert!(listed
        .iter()
        .all(|w| !w.detached && !w.locked && !w.prunable));
    assert!(listed.iter().any(|w| same_path(&w.path, &a)));

    // -- work happens in a workstream -----------------------------------------
    std::fs::write(a.join("checkout.rs"), "fn checkout() {}\n").unwrap();
    std::fs::write(a.join("README.md"), "hello\nchanged\nworld\n").unwrap();

    let dirty = git::status(&a).expect("status a");
    assert_eq!((dirty.staged, dirty.unstaged, dirty.untracked), (0, 1, 1));
    assert!(!dirty.is_clean);
    assert_eq!(
        git::list_untracked(&a).unwrap(),
        vec![PathBuf::from("checkout.rs")]
    );

    git::add_all(&a).expect("add a");
    let staged = git::status(&a).expect("status a staged");
    assert_eq!(
        (staged.staged, staged.unstaged, staged.untracked),
        (2, 0, 0)
    );
    assert!(git::diff(&a, true).unwrap().contains("+fn checkout()"));
    assert!(git::diff(&a, false).unwrap().is_empty(), "nothing unstaged");

    let stat = git::diff_stat(&a).expect("diff stat");
    let by = |p: &str| stat.iter().find(|f| f.path == Path::new(p)).unwrap();
    assert_eq!(stat.len(), 2, "{stat:#?}");
    assert_eq!(by("checkout.rs").kind, ChangeKind::Added);
    assert_eq!(by("checkout.rs").insertions, 1);
    assert_eq!(by("README.md").kind, ChangeKind::Modified);
    assert_eq!(
        (by("README.md").insertions, by("README.md").deletions),
        (1, 0)
    );

    let commit_a = git::commit(&a, "checkout skeleton", false).expect("commit a");
    assert!(git::status(&a).unwrap().is_clean);
    assert_eq!(git::ahead_behind(&a, &fx.branch).unwrap(), (1, 0));
    assert_eq!(
        git::ahead_behind(&fx.repo, "feature/checkout").unwrap(),
        (0, 1)
    );
    let on_branch = git::log(&a, Some(&fx.branch), 10).expect("log a");
    assert_eq!(on_branch.len(), 1);
    assert_eq!(on_branch[0].id, commit_a);

    std::fs::write(b.join("cart.rs"), "fn total() {}\n").unwrap();
    git::add_all(&b).expect("add b");
    git::commit(&b, "cart total", false).expect("commit b");

    // The two workstreams did not see each other's work.
    assert!(!a.join("cart.rs").exists());
    assert!(!b.join("checkout.rs").exists());
    assert!(!fx.repo.join("cart.rs").exists());

    // -- publishing ----------------------------------------------------------
    git::push(&a, "origin", "feature/checkout", true).expect("push a");
    git::push(&b, "origin", "fix/cart-total", true).expect("push b");

    let refs = raw_git(&fx.origin, &["show-ref"]);
    assert!(
        refs.contains("refs/heads/feature/checkout"),
        "origin has branch a: {refs}"
    );
    assert!(
        refs.contains("refs/heads/fix/cart-total"),
        "origin has branch b: {refs}"
    );

    let pushed = git::status(&a).expect("status after push");
    assert_eq!(pushed.upstream.as_deref(), Some("origin/feature/checkout"));
    assert_eq!((pushed.ahead, pushed.behind), (0, 0));

    // The main tree learns about the new refs through a plain fetch.
    git::fetch(&fx.repo, "origin").expect("fetch");
    assert_eq!(
        git::ahead_behind(&fx.repo, "origin/feature/checkout").unwrap(),
        (0, 1)
    );

    // -- teardown ------------------------------------------------------------
    git::worktree_remove(&a, false).expect("remove clean workstream");
    assert!(!a.exists());
    assert_eq!(git::worktree_list(&fx.repo).unwrap().len(), 2);

    // A workstream deleted from underneath git leaves a stale registration that
    // `prune` clears — the fallback path the isolation backend relies on.
    let common = git::common_dir(&b).expect("common dir");
    // The checkout vanishes from under git: moved aside, which is what a
    // deleted worktree looks like to `worktree list` and `prune`.
    std::fs::rename(&b, b.with_extension("gone")).unwrap();
    assert!(git::worktree_list(&fx.repo)
        .unwrap()
        .iter()
        .any(|w| w.prunable));
    git::worktree_prune(&common).expect("prune via the common git dir");
    assert_eq!(git::worktree_list(&fx.repo).unwrap().len(), 1);
}

// ---------------------------------------------------------------------------
// Failure modes
// ---------------------------------------------------------------------------

/// What a branch that moved changed: the paths between its old tip and its
/// new one, bounded, and a revision that looks like an option refused.
#[test]
fn the_paths_two_commits_differ_by_are_listed_and_bounded() {
    let fx = Fixture::new();
    let before = raw_git(&fx.repo, &["rev-parse", "HEAD"]);
    std::fs::write(fx.repo.join("a.txt"), "a\n").unwrap();
    std::fs::write(fx.repo.join("b.txt"), "b\n").unwrap();
    std::fs::write(fx.repo.join("README.md"), "hello\n").unwrap();
    git::add_all(&fx.repo).expect("add");
    git::commit(&fx.repo, "three files", false).expect("commit");
    let after = raw_git(&fx.repo, &["rev-parse", "HEAD"]);
    let mut paths = git::changed_paths(&fx.repo, &before, &after, 10).unwrap();
    paths.sort();
    assert_eq!(paths, vec!["README.md", "a.txt", "b.txt"]);
    assert_eq!(
        git::changed_paths(&fx.repo, &before, &after, 2)
            .unwrap()
            .len(),
        2,
        "bounded"
    );
    assert!(git::changed_paths(&fx.repo, &after, &after, 10)
        .unwrap()
        .is_empty());
    assert!(git::changed_paths(&fx.repo, "--output=x", &after, 10).is_err());
}

#[test]
fn a_plain_directory_is_not_a_repository() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();

    assert!(!git::is_repo(path));
    for err in [
        git::status(path).unwrap_err(),
        git::head(path).unwrap_err(),
        git::diff_stat(path).unwrap_err(),
        git::worktree_list(path).unwrap_err(),
        git::default_branch(path).unwrap_err(),
    ] {
        assert!(
            matches!(err, VcsError::NotARepository(_)),
            "expected NotARepository, got {err}"
        );
        assert!(err.is_unavailable(), "callers fall back on this");
    }
}

#[test]
fn removing_a_dirty_workstream_needs_force() {
    let fx = Fixture::new();
    let wt = fx.path("wt-dirty");
    git::worktree_add(&fx.repo, &wt, "wip", &fx.branch).expect("worktree");
    std::fs::write(wt.join("scratch.txt"), "unsaved\n").unwrap();

    let err = git::worktree_remove(&wt, false).expect_err("must refuse");
    match err {
        VcsError::Dirty { details, .. } => {
            assert!(details.contains("modified or untracked"), "{details}")
        }
        other => panic!("expected Dirty, got {other}"),
    }
    assert!(wt.exists(), "nothing was destroyed");

    git::worktree_remove(&wt, true).expect("force removes it");
    assert!(!wt.exists());
}

#[test]
fn a_branch_that_already_exists_is_a_plain_command_error() {
    let fx = Fixture::new();
    let first = fx.path("wt-1");
    let second = fx.path("wt-2");
    git::worktree_add(&fx.repo, &first, "taken", &fx.branch).expect("first");

    // Not one of the modelled situations, so it stays an honest Command error
    // with git's own words rather than being squeezed into a variant.
    match git::worktree_add(&fx.repo, &second, "taken", &fx.branch) {
        Err(VcsError::Command { stderr, .. }) => assert!(stderr.contains("already exists")),
        other => panic!("expected Command, got {other:?}"),
    }
    // Checking the existing branch out is the call that is meant to work.
    git::worktree_add_existing(&fx.repo, &second, "taken").expect_err("already checked out");
}

#[test]
fn pushing_to_a_remote_that_is_not_there() {
    let fx = Fixture::new();
    let missing = fx.path("gone.git");
    git::remote_add(&fx.repo, "nowhere", missing.to_str().unwrap()).expect("remote add");

    let err = git::push(&fx.repo, "nowhere", &fx.branch, false).expect_err("must fail");
    assert!(
        matches!(err, VcsError::NoRemote(_)),
        "expected NoRemote, got {err}"
    );
}

// ---------------------------------------------------------------------------
// Argument safety
// ---------------------------------------------------------------------------

#[test]
fn option_injection_never_reaches_git() {
    let fx = Fixture::new();
    for evil in ["--upload-pack=evil", "--exec=whatever", "-b"] {
        let err = git::worktree_add(&fx.repo, &fx.path("wt-evil"), evil, &fx.branch)
            .expect_err("must be refused before spawning git");
        assert!(
            matches!(err, VcsError::InvalidArg { .. }),
            "expected InvalidArg for {evil:?}, got {err}"
        );
        assert!(!fx.path("wt-evil").exists(), "nothing was created");
    }
    // The same guard on every argument position that git cannot `--` protect.
    assert!(matches!(
        git::push(&fx.repo, "--receive-pack=evil", &fx.branch, false),
        Err(VcsError::InvalidArg { .. })
    ));
    assert!(matches!(
        git::fetch(&fx.repo, "--upload-pack=evil"),
        Err(VcsError::InvalidArg { .. })
    ));
    assert!(matches!(
        git::clone("--upload-pack=evil", &fx.path("c"), None),
        Err(VcsError::InvalidArg { .. })
    ));
}

#[test]
fn shell_metacharacters_are_just_characters() {
    let fx = Fixture::new();
    let sentinel = fx.path("pwned.txt");
    assert!(!sentinel.exists());

    // Legal ref name, alarming to a shell. There is no shell: argv goes
    // straight to execve, so git creates a branch with this literal name.
    let branch = "evil;touch;pwned.txt";
    let wt = fx.path("wt-literal");
    git::worktree_add(&fx.repo, &wt, branch, &fx.branch).expect("branch created literally");

    let listed = git::worktree_list(&fx.repo).unwrap();
    assert!(
        listed.iter().any(|w| w.branch.as_deref() == Some(branch)),
        "the branch name survived verbatim: {listed:#?}"
    );
    assert!(!sentinel.exists(), "nothing was executed");
    assert!(!fx.repo.join("pwned.txt").exists());
    assert!(!wt.join("pwned.txt").exists());

    // And a commit message full of metacharacters is just a message.
    std::fs::write(wt.join("f.txt"), "x\n").unwrap();
    git::add_all(&wt).unwrap();
    let id = git::commit(&wt, "$(touch pwned.txt) && `touch pwned2.txt`", false).expect("commit");
    assert_eq!(
        git::log(&wt, None, 1).unwrap()[0].subject,
        "$(touch pwned.txt) && `touch pwned2.txt`"
    );
    assert!(!sentinel.exists());
    assert!(!wt.join("pwned.txt").exists());
    assert_eq!(git::head(&wt).unwrap(), id);
}

// ---------------------------------------------------------------------------
// Awkward inputs the parsers have to survive for real
// ---------------------------------------------------------------------------

#[test]
fn spaces_unicode_renames_and_binaries_round_trip() {
    let fx = Fixture::new();
    std::fs::write(fx.repo.join("a file with spaces.txt"), "one\n").unwrap();
    std::fs::write(fx.repo.join("ünïcode-fïle.txt"), "deux\n").unwrap();
    std::fs::write(fx.repo.join("logo.bin"), [0u8, 1, 2, 3, 0, 5]).unwrap();
    git::add_all(&fx.repo).unwrap();
    git::commit(&fx.repo, "awkward names", false).unwrap();

    std::fs::rename(
        fx.repo.join("a file with spaces.txt"),
        fx.repo.join("renamed with spaces.txt"),
    )
    .unwrap();
    std::fs::write(fx.repo.join("ünïcode-fïle.txt"), "deux\ntrois\n").unwrap();
    std::fs::write(fx.repo.join("logo.bin"), [9u8, 9, 0, 9]).unwrap();
    git::add_all(&fx.repo).unwrap();

    let st = git::status(&fx.repo).unwrap();
    assert_eq!(st.staged, 3, "rename, edit, binary: {st:?}");
    assert_eq!(st.untracked, 0);

    let stat = git::diff_stat(&fx.repo).unwrap();
    let find = |p: &str| stat.iter().find(|f| f.path == Path::new(p)).unwrap();

    let renamed = find("renamed with spaces.txt");
    assert_eq!(renamed.kind, ChangeKind::Renamed);
    assert_eq!(
        renamed.old_path.as_deref(),
        Some(Path::new("a file with spaces.txt"))
    );

    let unicode = find("ünïcode-fïle.txt");
    assert_eq!(unicode.kind, ChangeKind::Modified);
    assert_eq!((unicode.insertions, unicode.deletions), (1, 0));

    let binary = find("logo.bin");
    assert!(binary.binary, "line counts are meaningless for {binary:?}");
    assert_eq!((binary.insertions, binary.deletions), (0, 0));
}

#[test]
fn a_repository_with_no_commits_is_readable() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("fresh");
    git::init(&repo).expect("init");
    set_identity(&repo);

    assert!(git::is_repo(&repo));
    let st = git::status(&repo).unwrap();
    assert!(st.branch.is_some(), "HEAD names an unborn branch");
    assert_eq!(st.oid, None, "no commit yet");
    assert!(st.is_clean);
    // default_branch falls through origin/HEAD to the unborn branch's name.
    assert_eq!(git::default_branch(&repo).unwrap(), st.branch.unwrap());
    assert!(git::head(&repo).is_err());

    // Diffing an unborn HEAD works because the baseline is the empty tree,
    // which is what "the first commit would contain" actually means.
    std::fs::write(repo.join("first.txt"), "one\n").unwrap();
    git::add_all(&repo).unwrap();
    let stat = git::diff_stat(&repo).unwrap();
    assert_eq!(stat.len(), 1);
    assert_eq!(stat[0].kind, ChangeKind::Added);
    assert!(git::diff_head(&repo).unwrap().contains("+one"));
    // A file's history and blame before the first commit are empty answers,
    // never git's "does not have any commits yet" as an error.
    assert_eq!(git::file_history(&repo, "first.txt", 50).unwrap(), vec![]);
    assert_eq!(git::blame(&repo, "first.txt", None).unwrap(), vec![]);
    assert!(
        matches!(
            git::file_history(&dir.path().join("nowhere"), "x", 50),
            Err(VcsError::NotARepository(_))
        ),
        "a folder that is no repository is still refused by name"
    );

    git::commit(&repo, "first", false).unwrap();
    assert!(git::head(&repo).is_ok());
    assert!(git::status(&repo).unwrap().oid.is_some());
}

#[test]
fn detached_worktrees_report_no_branch() {
    let fx = Fixture::new();
    let wt = fx.path("wt-detached");
    git::worktree_add_detached(&fx.repo, &wt, None).expect("detached worktree");

    let st = git::status(&wt).unwrap();
    assert!(st.detached && st.branch.is_none() && st.upstream.is_none());
    assert!(st.is_clean);

    let entry = git::worktree_list(&fx.repo)
        .unwrap()
        .into_iter()
        .find(|w| same_path(&w.path, &wt))
        .expect("listed");
    assert!(entry.detached && entry.branch.is_none());
    assert!(entry.head.is_some());
}

#[test]
fn conflicted_paths_are_counted_as_conflicts() {
    let fx = Fixture::new();
    let wt = fx.path("wt-conflict");
    git::worktree_add(&fx.repo, &wt, "divergent", &fx.branch).unwrap();

    std::fs::write(wt.join("README.md"), "hello\ntheirs\n").unwrap();
    git::add_all(&wt).unwrap();
    git::commit(&wt, "theirs", false).unwrap();

    std::fs::write(fx.repo.join("README.md"), "hello\nours\n").unwrap();
    git::add_all(&fx.repo).unwrap();
    git::commit(&fx.repo, "ours", false).unwrap();

    // Raw git for the merge itself: this crate has no merge verb, and the
    // point of the test is what `status` reports once one is in progress.
    let merged = Command::new("git")
        .arg("-C")
        .arg(&fx.repo)
        .args(["merge", "divergent"])
        .output()
        .unwrap();
    assert!(!merged.status.success(), "the merge must conflict");

    let st = git::status(&fx.repo).unwrap();
    assert_eq!(st.conflicted, 1);
    assert!(!st.is_clean);
}

// ---------------------------------------------------------------------------
// Clone, from a local bare repository
// ---------------------------------------------------------------------------

#[test]
fn clone_from_a_local_bare_repository() {
    let fx = Fixture::new();
    git::push(&fx.repo, "origin", &fx.branch, true).expect("seed origin");
    // origin/HEAD only exists once the bare repository's HEAD resolves, which
    // it does after the first push of its default branch.
    let dest = fx.path("clone");

    git::clone(fx.origin.to_str().unwrap(), &dest, None).expect("clone");
    assert!(dest.join("README.md").exists());
    assert!(git::is_repo(&dest));
    assert_eq!(
        git::remote_get(&dest, "origin").unwrap().as_deref(),
        Some(fx.origin.to_str().unwrap())
    );
    // A clone records origin/HEAD, so default_branch answers from the remote
    // rather than from whatever happens to be checked out.
    assert_eq!(git::default_branch(&dest).unwrap(), fx.branch);
    assert_eq!(git::log(&dest, None, 10).unwrap().len(), 1);

    let err = git::clone("/definitely/not/a/repo", &fx.path("nope"), None).expect_err("must fail");
    assert!(
        matches!(err, VcsError::NoRemote(_) | VcsError::Command { .. }),
        "got {err}"
    );
}

// ---------------------------------------------------------------------------
// Per-file staging
// ---------------------------------------------------------------------------

/// Everything the whole per-file surface exists for, in one assertion: a user
/// touched three files, meant to commit one, and the other two must still be
/// sitting there afterwards exactly as they left them.
#[test]
fn staging_one_of_three_files_commits_only_that_one() {
    let fx = Fixture::new();
    for (name, body) in [
        ("a.txt", "alpha\n"),
        ("b.txt", "beta\n"),
        ("c.txt", "gamma\n"),
    ] {
        std::fs::write(fx.repo.join(name), body).unwrap();
    }
    git::add_all(&fx.repo).unwrap();
    git::commit(&fx.repo, "three files", false).unwrap();

    std::fs::write(fx.repo.join("a.txt"), "alpha edited\n").unwrap();
    std::fs::write(fx.repo.join("b.txt"), "beta edited\n").unwrap();
    std::fs::write(fx.repo.join("c.txt"), "gamma edited\n").unwrap();

    let before = git::status_files(&fx.repo).unwrap();
    assert_eq!(before.len(), 3, "{before:#?}");
    assert!(
        before.iter().all(|f| f.is_unstaged() && !f.is_staged()),
        "{before:#?}"
    );

    git::stage(&fx.repo, &["b.txt"]).expect("stage one");
    let staged = git::status_files(&fx.repo).unwrap();
    let row = |p: &str| staged.iter().find(|f| f.path == Path::new(p)).unwrap();
    assert!(row("b.txt").is_staged() && !row("b.txt").is_unstaged());
    assert!(!row("a.txt").is_staged() && !row("c.txt").is_staged());

    // One file's patch, not the tree's.
    let patch = git::diff_file(&fx.repo, "b.txt", true).unwrap();
    assert!(patch.contains("+beta edited"), "{patch}");
    assert!(!patch.contains("alpha"), "only b.txt is in it: {patch}");

    git::commit(&fx.repo, "just beta", false).expect("commit the selection");

    let after = git::status_files(&fx.repo).unwrap();
    assert_eq!(
        after.len(),
        2,
        "the other two survive the commit: {after:#?}"
    );
    for name in ["a.txt", "c.txt"] {
        let f = after.iter().find(|f| f.path == Path::new(name)).unwrap();
        assert!(
            f.is_unstaged() && !f.is_staged(),
            "{name} is still modified"
        );
    }
    assert_eq!(
        std::fs::read_to_string(fx.repo.join("a.txt")).unwrap(),
        "alpha edited\n",
        "a commit of somebody else's file must not touch this one"
    );
    assert_eq!(
        std::fs::read_to_string(fx.repo.join("c.txt")).unwrap(),
        "gamma edited\n"
    );

    // The commit recorded one file, and the log agrees.
    let head = raw_git(&fx.repo, &["show", "--name-only", "--format=", "HEAD"]);
    assert_eq!(head.trim(), "b.txt", "{head}");
}

/// Unstaging is an index operation. The word "restore" is one flag away from
/// the command that would overwrite the file, so the bytes are asserted.
#[test]
fn unstaging_returns_a_file_to_modified_and_leaves_its_bytes_alone() {
    let fx = Fixture::new();
    let edited = "hello\nedited by a human\nworld\n";
    std::fs::write(fx.repo.join("README.md"), edited).unwrap();
    std::fs::write(fx.repo.join("new.txt"), "brand new\n").unwrap();

    git::stage(&fx.repo, &["README.md", "new.txt"]).expect("stage both");
    let staged = git::status_files(&fx.repo).unwrap();
    assert!(staged.iter().all(|f| f.is_staged()), "{staged:#?}");

    git::unstage(&fx.repo, &["README.md"]).expect("unstage the tracked one");
    let after = git::status_files(&fx.repo).unwrap();
    let readme = after
        .iter()
        .find(|f| f.path == Path::new("README.md"))
        .unwrap();
    assert!(!readme.is_staged() && readme.is_unstaged());
    assert_eq!(
        std::fs::read_to_string(fx.repo.join("README.md")).unwrap(),
        edited,
        "unstaging must not write to the working tree"
    );

    // A file git had never seen goes back to being untracked, not deleted.
    git::unstage(&fx.repo, &["new.txt"]).expect("unstage the new one");
    let after = git::status_files(&fx.repo).unwrap();
    let new = after
        .iter()
        .find(|f| f.path == Path::new("new.txt"))
        .unwrap();
    assert!(new.untracked, "{after:#?}");
    assert_eq!(
        std::fs::read_to_string(fx.repo.join("new.txt")).unwrap(),
        "brand new\n",
        "the file is still there"
    );
}

/// A repository with no commits has no HEAD to unstage against, which is the
/// one place `restore --staged` needs to be told its source explicitly.
#[test]
fn unstaging_works_before_the_first_commit() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("fresh");
    git::init(&repo).unwrap();
    set_identity(&repo);
    std::fs::write(repo.join("first.txt"), "one\n").unwrap();

    git::stage(&repo, &["first.txt"]).expect("stage into an unborn HEAD");
    assert_eq!(git::status(&repo).unwrap().staged, 1);

    git::unstage(&repo, &["first.txt"]).expect("unstage against the empty tree");
    let files = git::status_files(&repo).unwrap();
    assert_eq!(files.len(), 1);
    assert!(files[0].untracked);
    assert_eq!(
        std::fs::read_to_string(repo.join("first.txt")).unwrap(),
        "one\n"
    );
}

/// A filename with a space and a `é` in it is not an edge case, it is
/// Tuesday — so it stages, diffs and commits like any other name.
#[test]
fn a_name_with_a_space_and_an_accent_stages_and_commits() {
    let fx = Fixture::new();
    let name = "notes de réunion.txt";
    std::fs::write(fx.repo.join(name), "première ligne\n").unwrap();
    std::fs::write(fx.repo.join("untouched.txt"), "leave me\n").unwrap();

    let listed = git::status_files(&fx.repo).unwrap();
    assert!(
        listed.iter().any(|f| f.path == Path::new(name)),
        "the name survives the porcelain round trip: {listed:#?}"
    );

    git::stage(&fx.repo, &[name]).expect("stage it");
    let patch = git::diff_file(&fx.repo, name, true).unwrap();
    assert!(patch.contains("+première ligne"), "{patch}");

    git::commit(&fx.repo, "add the notes", false).expect("commit it");
    let head = raw_git(&fx.repo, &["show", "--name-only", "--format=", "HEAD"]);
    assert!(
        head.contains("de r"),
        "committed under its real name: {head}"
    );
    assert!(fx.repo.join(name).exists());

    // And the other file was not swept in by a stray glob.
    let after = git::status_files(&fx.repo).unwrap();
    assert_eq!(after.len(), 1);
    assert!(after[0].untracked && after[0].path == Path::new("untouched.txt"));
}

/// A pathspec is not a ref: it must accept the names real files have and
/// refuse only what could mean something other than one file.
#[test]
fn pathspecs_accept_real_filenames_and_refuse_reach() {
    let fx = Fixture::new();
    for reach in [
        "",                      // selects nothing
        "-f",                    // read as an option
        "--cached",              //  "
        ":(exclude)*.rs",        // git's pathspec magic
        ":/",                    //  "
        "/etc/passwd",           // absolute: a different repository's business
        "../outside.txt",        // leaves the repository
        "src/../../outside.txt", //  "  the long way round
        "line\nbreak.txt",       // git's own porcelain cannot round-trip it
    ] {
        let err = git::stage(&fx.repo, &[reach]).expect_err("must be refused");
        assert!(
            matches!(err, VcsError::InvalidArg { .. }),
            "{reach:?} gave {err}"
        );
        assert!(git::unstage(&fx.repo, &[reach]).is_err(), "{reach:?}");
        assert!(git::diff_file(&fx.repo, reach, false).is_err(), "{reach:?}");
    }

    // Names that a `validate_ref`-style check would have refused, and that
    // people nonetheless have on disk.
    for ordinary in [
        "with space.txt",
        "accented-é.txt",
        "sub dir/nested file.txt",
        "star*in*name.txt",
        "brack[et].txt",
        "semi;colon.txt",
    ] {
        if let Some(parent) = std::path::Path::new(ordinary).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(fx.repo.join(parent)).unwrap();
            }
        }
        std::fs::write(fx.repo.join(ordinary), "x\n").unwrap();
        git::stage(&fx.repo, &[ordinary]).unwrap_or_else(|e| panic!("{ordinary:?}: {e}"));
    }
    // Six names in, six paths staged: a `*` in a name stayed a `*`, and did
    // not quietly become a glob that swept up the tree.
    assert_eq!(git::status(&fx.repo).unwrap().staged, 6);
}

/// The invariant, tested rather than trusted: **the safe tier can create,
/// stage, commit and push, and it cannot revert a file.**
///
/// Since ide/04 the crate has a second tier, `interactive.rs`, where the
/// banned verbs are allowed — behind `&HumanConsent` and a recovery ref, held
/// to that by `tests/interactive.rs`. This test covers every other file.
///
/// Now that `stage` and `unstage` sit right next to the calls that would
/// destroy somebody's uncommitted work, the distance between them is one
/// flag. A grep is a blunt instrument and that is the point — it fails on the
/// day someone reaches for the neighbouring command, and the fix is either to
/// not do that or to come here and argue for it in public.
#[test]
fn no_content_discarding_git_call_exists_in_this_crate() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offences = Vec::new();

    for entry in std::fs::read_dir(&src).expect("src/") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        // The consented tier is the one file allowed to hold these verbs; its
        // own tests hold it to consent-first and recovery-first.
        if path.file_name().and_then(|n| n.to_str()) == Some("interactive.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read source");
        // Only shipped code: a `#[cfg(test)]` fixture may legitimately hold
        // the word "clean" as a table-test field name, and does.
        let text = match text.find("#[cfg(test)]") {
            Some(at) => &text[..at],
            None => &text[..],
        };
        let lines: Vec<&str> = text.lines().collect();
        for (n, line) in lines.iter().enumerate() {
            // Only argv literals count: `s("restore")` is a git subcommand,
            // the word "restore" in a doc comment is a sentence about one.
            let calls = |verb: &str| {
                line.contains(&format!("s(\"{verb}\")"))
                    || line.contains(&format!("\"{verb}\","))
                    || line.contains(&format!("[\"{verb}\""))
            };
            let banned = ["checkout", "clean", "reset", "stash", "rm", "restore"];
            for verb in banned {
                if !calls(verb) {
                    continue;
                }
                // `restore --staged` is the one permitted neighbour: it
                // rewrites the index and never the working tree. The flag has
                // to be in the *same* argv literal, a couple of lines at
                // most — "somewhere else in the file" would exempt every
                // future `restore` the moment this one exists.
                let same_argv = lines[n..lines.len().min(n + 4)]
                    .iter()
                    .any(|l| l.contains("--staged"));
                if verb == "restore" && same_argv {
                    continue;
                }
                offences.push(format!(
                    "{}:{}: `git {verb}` — {}",
                    path.file_name().unwrap().to_string_lossy(),
                    n + 1,
                    line.trim()
                ));
            }
        }
    }

    assert!(
        offences.is_empty(),
        "bisa-vcs must not be able to discard a change:\n{}",
        offences.join("\n")
    );
}

// ---------------------------------------------------------------------------
// Hunk staging, blame, file history (ide/04 §6)
// ---------------------------------------------------------------------------

/// Split a unified diff into its file header and its `@@` hunks, each hunk
/// with its own header line — the cut the IDE makes before staging one.
fn split_hunks(diff: &str) -> (String, Vec<String>) {
    let mut header = String::new();
    let mut hunks: Vec<String> = Vec::new();
    for line in diff.split_inclusive('\n') {
        if line.starts_with("@@") {
            hunks.push(line.to_string());
        } else if let Some(last) = hunks.last_mut() {
            last.push_str(line);
        } else {
            header.push_str(line);
        }
    }
    (header, hunks)
}

fn numbered(n: usize) -> String {
    (1..=n).map(|i| format!("line {i}\n")).collect()
}

#[test]
fn a_hunk_can_be_staged_and_unstaged_without_touching_the_tree() {
    let fx = Fixture::new();
    std::fs::write(fx.repo.join("big.txt"), numbered(30)).unwrap();
    git::add_all(&fx.repo).unwrap();
    git::commit(&fx.repo, "thirty lines", false).unwrap();

    // Two edits far enough apart to be two hunks.
    let edited = numbered(30)
        .replace("line 2\n", "line 2 edited\n")
        .replace("line 28\n", "line 28 edited\n");
    std::fs::write(fx.repo.join("big.txt"), &edited).unwrap();
    let diff = git::diff_file(&fx.repo, "big.txt", false).unwrap();
    let (header, hunks) = split_hunks(&diff);
    assert_eq!(hunks.len(), 2, "{diff}");

    // Stage only the first hunk.
    let first = format!("{header}{}", hunks[0]);
    git::apply_cached(&fx.repo, &first, false).expect("stage hunk");
    let staged = git::diff_file(&fx.repo, "big.txt", true).unwrap();
    assert!(staged.contains("+line 2 edited"), "{staged}");
    assert!(!staged.contains("line 28 edited"), "{staged}");
    let unstaged = git::diff_file(&fx.repo, "big.txt", false).unwrap();
    assert!(unstaged.contains("+line 28 edited"), "{unstaged}");
    assert!(!unstaged.contains("+line 2 edited"), "{unstaged}");
    assert_eq!(
        std::fs::read_to_string(fx.repo.join("big.txt")).unwrap(),
        edited,
        "staging a hunk must not write to the working tree"
    );

    // Reverse takes it back out; the tree is still exactly what was typed.
    git::apply_cached(&fx.repo, &first, true).expect("unstage hunk");
    assert_eq!(
        git::diff_file(&fx.repo, "big.txt", true).unwrap().trim(),
        ""
    );
    assert_eq!(
        std::fs::read_to_string(fx.repo.join("big.txt")).unwrap(),
        edited
    );
}

#[test]
fn a_patch_of_selected_lines_stages_only_those_lines() {
    let fx = Fixture::new();
    std::fs::write(fx.repo.join("pair.txt"), "a\nb\nc\n").unwrap();
    git::add_all(&fx.repo).unwrap();
    git::commit(&fx.repo, "pair", false).unwrap();
    std::fs::write(fx.repo.join("pair.txt"), "a\nB\nC\n").unwrap();

    // The hunk changes b→B and c→C. Keep only the b→B change: the c lines
    // become context, exactly as the IDE's line picker builds it, with counts
    // left for `--recount` to fix.
    let patch = "diff --git a/pair.txt b/pair.txt\n--- a/pair.txt\n+++ b/pair.txt\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n";
    git::apply_cached(&fx.repo, patch, false).expect("stage the b line");
    let staged = git::diff_file(&fx.repo, "pair.txt", true).unwrap();
    assert!(staged.contains("+B") && !staged.contains("+C"), "{staged}");
    let unstaged = git::diff_file(&fx.repo, "pair.txt", false).unwrap();
    assert!(
        unstaged.contains("+C") && !unstaged.contains("+B"),
        "{unstaged}"
    );
}

#[test]
fn apply_cached_refuses_an_empty_patch_and_one_that_does_not_fit() {
    let fx = Fixture::new();
    assert!(matches!(
        git::apply_cached(&fx.repo, "   \n", false),
        Err(VcsError::InvalidArg { .. })
    ));
    let wrong = "diff --git a/README.md b/README.md\n--- a/README.md\n+++ b/README.md\n@@ -1,2 +1,2 @@\n-not what is there\n+something\n world\n";
    assert!(git::apply_cached(&fx.repo, wrong, false).is_err());
    assert_eq!(
        std::fs::read_to_string(fx.repo.join("README.md")).unwrap(),
        "hello\nworld\n",
        "a refused patch changes nothing"
    );
    assert_eq!(git::diff(&fx.repo, true).unwrap().trim(), "");
}

#[test]
fn a_remotes_credentials_are_never_in_an_error_this_crate_makes() {
    let fx = Fixture::new();
    // `origin` is there already: adding it again fails, and the failure
    // describes the invocation — its arguments, the URL among them. No
    // network: git refuses before it would dial anything.
    let url = "https://mona:ghp_0123456789abcdef@code.example.invalid/acme/shop.git";
    let err = git::remote_add(&fx.repo, "origin", url).unwrap_err();
    let said = format!("{err} {err:?}");
    assert!(
        !said.contains("ghp_0123456789abcdef") && !said.contains("mona:"),
        "{said}"
    );
    assert!(
        said.contains("code.example.invalid"),
        "the host is still named: {said}"
    );
    assert_eq!(
        bisa_vcs::scrub_userinfo("pushed to https://a:b@h.test/r.git"),
        "pushed to https://***@h.test/r.git",
        "the same scrubber is there for whoever logs a remote"
    );
}

#[test]
fn a_commit_message_of_nothing_is_refused_before_git_is_asked_and_an_odd_one_is_kept() {
    let fx = Fixture::new();
    std::fs::write(fx.repo.join("a.txt"), "a\n").unwrap();
    git::add_all(&fx.repo).unwrap();
    let head = git::head(&fx.repo).unwrap();
    for nothing in ["", "   ", "\n\t\n"] {
        assert!(
            matches!(
                git::commit(&fx.repo, nothing, false),
                Err(VcsError::InvalidArg { .. })
            ),
            "{nothing:?}"
        );
    }
    assert_eq!(git::head(&fx.repo).unwrap(), head, "nothing was committed");
    // A message that looks like an option, and a long one, are messages.
    let commit = git::commit(&fx.repo, "--amend -m pwned", false).unwrap();
    assert_ne!(commit, head);
    assert_eq!(
        raw_git(&fx.repo, &["log", "-1", "--format=%s"]).trim(),
        "--amend -m pwned"
    );
    assert_eq!(
        raw_git(&fx.repo, &["rev-list", "--count", "HEAD"]).trim(),
        "2",
        "a commit was added, none rewritten"
    );
    std::fs::write(fx.repo.join("b.txt"), "b\n").unwrap();
    git::add_all(&fx.repo).unwrap();
    let long = format!("subject\n\n{}", "word ".repeat(20_000));
    git::commit(&fx.repo, &long, false).unwrap();
    assert_eq!(
        raw_git(&fx.repo, &["log", "-1", "--format=%s"]).trim(),
        "subject"
    );
}

#[test]
fn another_git_holding_the_index_is_said_as_busy_and_nothing_is_written() {
    let fx = Fixture::new();
    std::fs::write(fx.repo.join("a.txt"), "a\n").unwrap();
    // What another git process leaves while it writes the index.
    let lock = fx.repo.join(".git").join("index.lock");
    std::fs::write(&lock, b"").unwrap();
    let err = git::stage(&fx.repo, &["a.txt".to_string()]).unwrap_err();
    assert!(matches!(err, VcsError::RepositoryBusy { .. }), "{err}");
    assert!(err.to_string().contains("try again"), "{err}");
    // Moved aside by the test that made it; then the same call goes through.
    std::fs::rename(&lock, fx.path("index.lock.aside")).unwrap();
    git::stage(&fx.repo, &["a.txt".to_string()]).expect("the repository is free again");
    assert!(git::diff(&fx.repo, true).unwrap().contains("a.txt"));
}

#[test]
fn a_hunk_is_staged_whatever_the_files_line_ends_and_a_link_is_a_link() {
    let fx = Fixture::new();
    // CRLF, no final newline, and a name with a space: committed, then changed.
    let cases = [
        ("crlf.txt", "one\r\ntwo\r\n", "one\r\nTWO\r\n"),
        ("no-newline.txt", "one\ntwo", "one\nTWO"),
        ("with space.txt", "one\ntwo\n", "one\nTWO\n"),
    ];
    for (name, before, _) in cases {
        std::fs::write(fx.repo.join(name), before).unwrap();
    }
    git::add_all(&fx.repo).unwrap();
    git::commit(&fx.repo, "lines", false).unwrap();
    for (name, _, after) in cases {
        std::fs::write(fx.repo.join(name), after).unwrap();
        let patch = git::diff_file(&fx.repo, name, false).unwrap();
        assert!(!patch.trim().is_empty(), "{name} has a diff");
        git::apply_cached(&fx.repo, &patch, false).unwrap_or_else(|e| panic!("stage {name}: {e}"));
        assert_eq!(
            git::diff_file(&fx.repo, name, false).unwrap().trim(),
            "",
            "{name}: nothing left unstaged"
        );
        assert_eq!(
            std::fs::read(fx.repo.join(name)).unwrap(),
            after.as_bytes(),
            "{name}: the tree is not touched"
        );
        // And out again, by the same patch reversed.
        git::apply_cached(&fx.repo, &patch, true).unwrap_or_else(|e| panic!("unstage {name}: {e}"));
        assert_eq!(
            git::diff_file(&fx.repo, name, true).unwrap().trim(),
            "",
            "{name}: nothing left staged"
        );
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("crlf.txt", fx.repo.join("link")).unwrap();
        git::stage(&fx.repo, &["link".to_string()]).unwrap();
        let rows = git::status_files(&fx.repo).unwrap();
        assert!(
            rows.iter().any(|r| r.path == Path::new("link")),
            "{rows:#?}"
        );
        let mode = raw_git(&fx.repo, &["ls-files", "--stage", "link"]);
        assert!(
            mode.starts_with("120000"),
            "staged as the link it is, not as what it points at: {mode}"
        );
    }
}

#[test]
fn two_writers_of_this_process_on_one_checkout_queue_and_both_land() {
    let fx = Fixture::new();
    let repo = std::sync::Arc::new(fx.repo.clone());
    for i in 0..8 {
        std::fs::write(fx.repo.join(format!("f{i}.txt")), format!("{i}\n")).unwrap();
    }
    let start = std::sync::Arc::new(std::sync::Barrier::new(8));
    let writers: Vec<_> = (0..8)
        .map(|i| {
            let (repo, start) = (std::sync::Arc::clone(&repo), std::sync::Arc::clone(&start));
            std::thread::spawn(move || {
                start.wait();
                git::stage(&repo, &[format!("f{i}.txt")])
            })
        })
        .collect();
    for w in writers {
        w.join()
            .unwrap()
            .expect("a writer of this process waits its turn; it never meets git's own lock");
    }
    let staged = git::diff(&fx.repo, true).unwrap();
    for i in 0..8 {
        assert!(staged.contains(&format!("f{i}.txt")), "f{i}.txt is staged");
    }
}

#[test]
fn blame_names_the_commit_for_each_line_and_marks_uncommitted_ones() {
    let fx = Fixture::new();
    let head = git::head(&fx.repo).unwrap();
    std::fs::write(fx.repo.join("README.md"), "hello\nworld\nnew line\n").unwrap();

    let rows = git::blame(&fx.repo, "README.md", None).expect("blame");
    assert_eq!(rows.len(), 3, "{rows:#?}");
    assert_eq!(rows[0].line, 1);
    assert_eq!(rows[0].commit, head);
    assert_eq!(rows[0].author, "Bisa Test");
    assert_eq!(rows[0].summary, "baseline");
    assert!(rows[0].timestamp > 0);
    assert!(!rows[0].uncommitted);
    assert_eq!(rows[1].commit, head);
    assert!(rows[2].uncommitted, "{:?}", rows[2]);
    assert_eq!(rows[2].line, 3);

    let range = git::blame(&fx.repo, "README.md", Some((2, 3))).unwrap();
    assert_eq!(range.iter().map(|r| r.line).collect::<Vec<_>>(), vec![2, 3]);
    assert!(matches!(
        git::blame(&fx.repo, "README.md", Some((3, 2))),
        Err(VcsError::InvalidArg { .. })
    ));
}

#[test]
fn file_history_is_newest_first_and_follows_a_rename() {
    let fx = Fixture::new();
    std::fs::write(fx.repo.join("old.txt"), "one\n").unwrap();
    git::add_all(&fx.repo).unwrap();
    git::commit(&fx.repo, "add old", false).unwrap();
    std::fs::write(fx.repo.join("old.txt"), "one\ntwo\n").unwrap();
    git::add_all(&fx.repo).unwrap();
    git::commit(&fx.repo, "grow old", false).unwrap();
    // A rename by the filesystem, recorded by the next commit: the tree still
    // has every byte, under the new name.
    std::fs::rename(fx.repo.join("old.txt"), fx.repo.join("new.txt")).unwrap();
    git::add_all(&fx.repo).unwrap();
    git::commit(&fx.repo, "rename to new", false).unwrap();

    let history = git::file_history(&fx.repo, "new.txt", 50).expect("history");
    let subjects: Vec<&str> = history.iter().map(|c| c.subject.as_str()).collect();
    assert_eq!(
        subjects,
        vec!["rename to new", "grow old", "add old"],
        "{history:#?}"
    );
    assert_eq!(git::file_history(&fx.repo, "new.txt", 1).unwrap().len(), 1);
    assert!(git::file_history(&fx.repo, "README.md", 50)
        .unwrap()
        .iter()
        .all(|c| c.subject == "baseline"));
}

// ---------------------------------------------------------------------------
// Who commits here — the repository's own identity
// ---------------------------------------------------------------------------

/// A `Git` that cannot see the developer's global or system config, so what
/// these tests observe is the repository and nothing else. Per handle, never
/// the process environment: the other tests in this binary run in parallel.
fn isolated_git(root: &Path) -> git::Git {
    git::Git::new()
        .with_env("GIT_CONFIG_GLOBAL", root.join("no-global.gitconfig"))
        .with_env("GIT_CONFIG_NOSYSTEM", "1")
}

#[test]
fn identity_is_none_in_a_fresh_repository_and_local_after_it_is_set() {
    let root = tempfile::tempdir().expect("tempdir");
    let repo = root.path().join("project");
    let g = isolated_git(root.path());
    g.init(&repo).expect("init");

    let before = g.identity(&repo).expect("identity");
    assert_eq!(before.source, git::IdentitySource::None);
    assert_eq!(before.name, None);
    assert_eq!(before.email, None);
    assert_eq!(
        before.global, None,
        "no global config is visible to this handle"
    );

    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    g.add_all(&repo).unwrap();
    let refused = g
        .commit(&repo, "first", false)
        .expect_err("nobody is set to commit");
    assert!(
        matches!(refused, VcsError::IdentityUnset(_)),
        "expected IdentityUnset, got {refused}"
    );

    let after = g
        .set_local_identity(&repo, "  Ada Lovelace ", "ada@example.invalid")
        .expect("set local identity");
    assert_eq!(after.source, git::IdentitySource::Local);
    assert_eq!(after.name.as_deref(), Some("Ada Lovelace"), "trimmed");
    assert_eq!(after.email.as_deref(), Some("ada@example.invalid"));
    // Verified independently: the write landed in the repository's own config.
    assert_eq!(
        raw_git(&repo, &["config", "--local", "--get", "user.name"]),
        "Ada Lovelace"
    );
    assert_eq!(
        raw_git(&repo, &["config", "--local", "--get", "user.email"]),
        "ada@example.invalid"
    );

    g.commit(&repo, "first", false)
        .expect("the same commit now succeeds");
    assert_eq!(
        raw_git(&repo, &["log", "-1", "--format=%an <%ae>"]),
        "Ada Lovelace <ada@example.invalid>"
    );

    // A worktree of the repository shares its config, so the same author
    // signs a commit made there — the property "every git operation the
    // platform runs in this repository" rests on.
    let wt = root.path().join("wt");
    g.worktree_add(&repo, &wt, "feature/x", &g.default_branch(&repo).unwrap())
        .expect("worktree");
    assert_eq!(g.identity(&wt).unwrap().source, git::IdentitySource::Local);
}

#[test]
fn a_global_identity_is_reported_as_global_and_pinnable() {
    let root = tempfile::tempdir().expect("tempdir");
    let global = root.path().join("global.gitconfig");
    std::fs::write(
        &global,
        "[user]\n\tname = Grace Hopper\n\temail = grace@example.invalid\n",
    )
    .unwrap();
    let g = git::Git::new()
        .with_env("GIT_CONFIG_GLOBAL", &global)
        .with_env("GIT_CONFIG_NOSYSTEM", "1");
    let repo = root.path().join("project");
    g.init(&repo).unwrap();

    let inherited = g.identity(&repo).unwrap();
    assert_eq!(inherited.source, git::IdentitySource::Global);
    assert_eq!(inherited.name.as_deref(), Some("Grace Hopper"));
    assert_eq!(
        inherited.global,
        Some(git::Ident {
            name: "Grace Hopper".into(),
            email: "grace@example.invalid".into()
        }),
        "the global pair is offered so a caller can pin it"
    );

    // Pinning writes the same pair locally; the source flips, the global file
    // is untouched.
    let pinned = g
        .set_local_identity(&repo, "Grace Hopper", "grace@example.invalid")
        .unwrap();
    assert_eq!(pinned.source, git::IdentitySource::Local);
    assert_eq!(
        std::fs::read_to_string(&global).unwrap(),
        "[user]\n\tname = Grace Hopper\n\temail = grace@example.invalid\n",
        "never a global write"
    );

    // A local name alone is not a local identity: both keys or it is inherited.
    let repo2 = root.path().join("half");
    g.init(&repo2).unwrap();
    raw_git(&repo2, &["config", "--local", "user.name", "Only Name"]);
    assert_eq!(
        g.identity(&repo2).unwrap().source,
        git::IdentitySource::Global
    );
}

#[test]
fn global_identity_is_none_under_an_isolated_handle_and_some_when_the_global_file_names_one() {
    let root = tempfile::tempdir().expect("tempdir");
    // No repository anywhere: the global read needs none.
    assert_eq!(isolated_git(root.path()).global_identity().unwrap(), None);

    let global = root.path().join("global.gitconfig");
    std::fs::write(&global, "[user]\n\tname = Grace Hopper\n").unwrap();
    let half = git::Git::new()
        .with_env("GIT_CONFIG_GLOBAL", &global)
        .with_env("GIT_CONFIG_NOSYSTEM", "1");
    assert_eq!(
        half.global_identity().unwrap(),
        None,
        "a name alone is not an identity"
    );

    std::fs::write(
        &global,
        "[user]\n\tname = Grace Hopper\n\temail = grace@example.invalid\n",
    )
    .unwrap();
    let pair = half.global_identity().unwrap().expect("both keys set");
    assert_eq!(pair.name, "Grace Hopper");
    assert_eq!(pair.email, "grace@example.invalid");
    assert_eq!(pair.to_string(), "Grace Hopper <grace@example.invalid>");
    assert_eq!(
        std::fs::read_to_string(&global).unwrap(),
        "[user]\n\tname = Grace Hopper\n\temail = grace@example.invalid\n",
        "a read leaves the global file as it was"
    );
}

#[test]
fn parse_ident_accepts_name_angle_email_and_refuses_the_rest() {
    let ada = git::Ident {
        name: "Ada Lovelace".into(),
        email: "ada@example.invalid".into(),
    };
    assert_eq!(
        git::parse_ident("Ada Lovelace <ada@example.invalid>").unwrap(),
        ada
    );
    assert_eq!(
        git::parse_ident("  Ada Lovelace  < ada@example.invalid >  ").unwrap(),
        ada,
        "whitespace around either part is trimmed"
    );
    assert_eq!(
        git::parse_ident(&ada.to_string()).unwrap(),
        ada,
        "Display round-trips"
    );
    for bad in [
        "",
        "Ada Lovelace",
        "ada@example.invalid",
        "<ada@example.invalid>",
        "Ada <ada@example.invalid",
        "Ada ada@example.invalid>",
        "Ada <<ada@example.invalid>>",
        "Ada <ada@@example.invalid>",
        "Ada <ada example.invalid>",
        "--global <ada@example.invalid>",
        "Ada <--local>",
        "Ada\nLovelace <ada@example.invalid>",
    ] {
        match git::parse_ident(bad) {
            Err(VcsError::InvalidArg { what, .. }) => assert_eq!(what, "committer", "{bad:?}"),
            other => panic!("{bad:?} should be refused, got {other:?}"),
        }
    }
}

#[test]
fn identity_validation_refuses_empty_dash_prefixed_and_non_email() {
    let root = tempfile::tempdir().expect("tempdir");
    let repo = root.path().join("project");
    let g = isolated_git(root.path());
    g.init(&repo).unwrap();
    for (name, email) in [
        ("", "a@b.c"),
        ("   ", "a@b.c"),
        ("--global", "a@b.c"),
        ("Ada\nLovelace", "a@b.c"),
        ("Ada", ""),
        ("Ada", "no-at-sign"),
        ("Ada", "two@at@signs"),
        ("Ada", "with space@b.c"),
        ("Ada", "@b.c"),
        ("Ada", "--local"),
    ] {
        let err = g
            .set_local_identity(&repo, name, email)
            .expect_err(&format!("{name:?} / {email:?} must be refused"));
        assert!(
            matches!(err, VcsError::InvalidArg { .. }),
            "{name:?}/{email:?}: {err}"
        );
    }
    // Refused before anything was spawned: the repository's config is still empty.
    let out = Command::new("git")
        .args(["-C"])
        .arg(&repo)
        .args(["config", "--local", "--get", "user.name"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "no user.name was written");
    assert!(git::validate_identity("Ada", "ada@example.invalid").is_ok());
}

/// The one invariant the identity feature adds (02-domain-model I45): the
/// person's global config is written by named functions only — `config_set`
/// for a schema key, `include_set` / `include_remove` for the platform's own
/// profile includes — each selecting the layer through `ConfigScope::Global`;
/// every other `--global` is a read (`ReadScope::Global`), and `config` is
/// spelled in `git.rs` alone.
#[test]
fn global_config_is_written_only_by_config_set() {
    // I45: `git config` is spelled in one module, and every
    // `--global` there is a layer selected through the two scope enums —
    // there is no other way to reach the person's global file.
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut config_files = Vec::new();
    for entry in std::fs::read_dir(&src_dir).expect("src/") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        if text.contains("s(\"config\")") {
            config_files.push(path.file_name().unwrap().to_string_lossy().to_string());
        }
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if !line.contains("\"--global\"") || line.trim_start().starts_with("//") {
                continue;
            }
            assert!(
                line.contains("ReadScope::Global") || line.contains("ConfigScope::Global"),
                "{}:{}: a `--global` outside a scope arm: {line}",
                path.display(),
                i + 1
            );
        }
        assert!(
            !text.contains("\"--global\", s(\"user"),
            "{}: writes a global user key by hand",
            path.display()
        );
    }
    assert_eq!(
        config_files,
        vec!["git.rs".to_string()],
        "`git config` is spelled in one module"
    );
}

#[test]
fn config_view_lists_every_schema_key_with_local_and_global_layers() {
    let root = tempfile::tempdir().expect("tempdir");
    let global = root.path().join("global.gitconfig");
    std::fs::write(
        &global,
        "[user]\n\tname = Grace Hopper\n\temail = grace@example.invalid\n[pull]\n\trebase = true\n",
    )
    .unwrap();
    let g = git::Git::new()
        .with_env("GIT_CONFIG_GLOBAL", &global)
        .with_env("GIT_CONFIG_NOSYSTEM", "1");
    let repo = root.path().join("project");
    g.init(&repo).unwrap();
    raw_git(
        &repo,
        &["config", "--local", "user.email", "ada@example.invalid"],
    );

    let keys: Vec<&str> = bisa_vcs::GIT_CONFIG_KEYS.iter().map(|d| d.key).collect();
    let view = g.config_view(Some(&repo)).unwrap();
    assert_eq!(
        view.entries
            .iter()
            .map(|e| e.key.as_str())
            .collect::<Vec<_>>(),
        keys,
        "every key, in schema order"
    );
    let name = view.get("user.name").unwrap();
    assert_eq!(
        (
            name.local.as_deref(),
            name.global.as_deref(),
            name.effective()
        ),
        (None, Some("Grace Hopper"), Some("Grace Hopper"))
    );
    let email = view.get("user.email").unwrap();
    assert_eq!(
        (
            email.local.as_deref(),
            email.global.as_deref(),
            email.effective()
        ),
        (
            Some("ada@example.invalid"),
            Some("grace@example.invalid"),
            Some("ada@example.invalid")
        ),
        "local wins"
    );
    assert_eq!(
        view.get("pull.rebase").unwrap().global.as_deref(),
        Some("true")
    );
    assert_eq!(view.get("core.autocrlf").unwrap().effective(), None);

    // No repository: the global layer alone.
    let alone = g.config_view(None).unwrap();
    assert!(alone.entries.iter().all(|e| e.local.is_none()));
    assert_eq!(
        alone.get("user.name").unwrap().global.as_deref(),
        Some("Grace Hopper")
    );

    // No global file at all is "nothing set", not an error.
    let none = isolated_git(root.path()).config_view(Some(&repo)).unwrap();
    assert!(none.entries.iter().all(|e| e.global.is_none()));
    assert_eq!(
        none.get("user.email").unwrap().local.as_deref(),
        Some("ada@example.invalid")
    );
}

#[test]
fn config_set_and_unset_write_the_named_scope_only() {
    let root = tempfile::tempdir().expect("tempdir");
    let global = root.path().join("global.gitconfig");
    std::fs::write(&global, "[user]\n\tname = Grace Hopper\n").unwrap();
    let g = git::Git::new()
        .with_env("GIT_CONFIG_GLOBAL", &global)
        .with_env("GIT_CONFIG_NOSYSTEM", "1");
    let repo = root.path().join("project");
    g.init(&repo).unwrap();
    let global_before = std::fs::read_to_string(&global).unwrap();

    // A local write: the repository's file, and only it.
    g.config_set(
        ConfigScope::Local,
        Some(&repo),
        "user.useConfigOnly",
        " true ",
    )
    .unwrap();
    g.config_set(ConfigScope::Local, Some(&repo), "core.autocrlf", "input")
        .unwrap();
    assert_eq!(
        raw_git(&repo, &["config", "--local", "--get", "user.useConfigOnly"]),
        "true"
    );
    assert_eq!(
        raw_git(&repo, &["config", "--local", "--get", "core.autocrlf"]),
        "input"
    );
    assert_eq!(
        std::fs::read_to_string(&global).unwrap(),
        global_before,
        "a local write never touches the global file"
    );

    // With useConfigOnly and no email anywhere, git itself refuses to guess.
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    g.add_all(&repo).unwrap();
    assert!(matches!(
        g.commit(&repo, "no guess", false),
        Err(VcsError::IdentityUnset(_))
    ));

    // A global write: the person's file, and only it — the one place the
    // platform writes it, at their request.
    g.config_set(
        ConfigScope::Global,
        None,
        "user.email",
        "grace@example.invalid",
    )
    .unwrap();
    g.config_set(ConfigScope::Global, None, "init.defaultBranch", "main")
        .unwrap();
    let after = std::fs::read_to_string(&global).unwrap();
    assert!(
        after.contains("email = grace@example.invalid") && after.contains("defaultBranch = main"),
        "{after}"
    );
    assert_eq!(
        raw_git_optional(&repo, &["config", "--local", "--get-all", "user.email"]),
        None,
        "nothing landed locally"
    );
    assert_eq!(
        g.identity(&repo).unwrap().source,
        git::IdentitySource::Global
    );

    // Refusals, before anything is spawned.
    assert!(
        g.config_set(
            ConfigScope::Local,
            Some(&repo),
            "init.defaultBranch",
            "main"
        )
        .is_err(),
        "global-only key"
    );
    assert!(
        g.config_set(ConfigScope::Local, Some(&repo), "core.hooksPath", "x")
            .is_err(),
        "unknown key"
    );
    assert!(
        g.config_set(ConfigScope::Local, Some(&repo), "core.autocrlf", "maybe")
            .is_err(),
        "not a choice"
    );
    assert!(
        g.config_set(ConfigScope::Local, Some(&repo), "user.name", "--global")
            .is_err(),
        "option-shaped"
    );
    assert!(
        g.config_set(ConfigScope::Global, Some(&repo), "user.name", "Ada")
            .is_err(),
        "a global write takes no path"
    );
    assert!(
        g.config_set(ConfigScope::Local, None, "user.name", "Ada")
            .is_err(),
        "a local write needs a path"
    );
    assert_eq!(
        raw_git(&repo, &["config", "--local", "--get", "core.autocrlf"]),
        "input",
        "the refused writes wrote nothing"
    );

    // Unset falls through to the layer beneath; unsetting what is not set is fine.
    g.config_unset(ConfigScope::Local, Some(&repo), "core.autocrlf")
        .unwrap();
    assert_eq!(
        raw_git_optional(&repo, &["config", "--local", "--get-all", "core.autocrlf"]),
        None
    );
    g.config_unset(ConfigScope::Local, Some(&repo), "core.autocrlf")
        .unwrap();
    g.config_unset(ConfigScope::Global, None, "init.defaultBranch")
        .unwrap();
    assert!(!std::fs::read_to_string(&global)
        .unwrap()
        .contains("defaultBranch"));
    let view = g.config_view(Some(&repo)).unwrap();
    assert_eq!(
        view.get("user.email").unwrap().effective(),
        Some("grace@example.invalid"),
        "inherited again"
    );
}

// ---------------------------------------------------------------------------
// Remotes, upstreams and what git has left half-done
// ---------------------------------------------------------------------------

#[test]
fn a_remote_is_re_pointed_in_place_and_ensured_whichever_state_it_is_in() {
    let fx = Fixture::new();
    let elsewhere = fx.path("elsewhere.git");
    std::fs::create_dir_all(&elsewhere).unwrap();
    raw_git(&elsewhere, &["init", "--bare", "--quiet"]);
    let there = elsewhere.to_str().unwrap();

    git::remote_set_url(&fx.repo, "origin", there).unwrap();
    assert_eq!(
        git::remote_get(&fx.repo, "origin").unwrap().as_deref(),
        Some(there)
    );
    assert!(
        matches!(
            git::remote_set_url(&fx.repo, "nowhere", there),
            Err(VcsError::NoRemote(_))
        ),
        "set-url on a remote that does not exist is not an add"
    );

    // ensure: same url is a no-op, another url re-points, a new name adds.
    git::ensure_remote(&fx.repo, "origin", there).unwrap();
    git::ensure_remote(&fx.repo, "origin", fx.origin.to_str().unwrap()).unwrap();
    assert_eq!(
        git::remote_get(&fx.repo, "origin").unwrap().as_deref(),
        fx.origin.to_str()
    );
    git::ensure_remote(&fx.repo, "mirror", there).unwrap();
    let names: Vec<String> = git::remote_list(&fx.repo)
        .unwrap()
        .into_iter()
        .map(|r| r.name)
        .collect();
    assert_eq!(names, vec!["mirror", "origin"]);

    // Argument safety holds here as everywhere.
    assert!(git::remote_set_url(&fx.repo, "origin", "--upload-pack=x").is_err());
    assert!(git::ensure_remote(&fx.repo, "bad name", there).is_err());
}

#[test]
fn upstream_fast_forward_and_in_progress_read_the_repository_not_prose() {
    let fx = Fixture::new();
    assert_eq!(
        git::upstream_of(&fx.repo).unwrap(),
        None,
        "never pushed: no upstream"
    );
    assert_eq!(git::in_progress(&fx.repo).unwrap(), None);
    assert!(git::conflicted_paths(&fx.repo).unwrap().is_empty());

    git::push(&fx.repo, "origin", &fx.branch, true).unwrap();
    let upstream = format!("origin/{}", fx.branch);
    assert_eq!(
        git::upstream_of(&fx.repo).unwrap().as_deref(),
        Some(upstream.as_str())
    );
    assert!(
        git::can_fast_forward(&fx.repo, &upstream).unwrap(),
        "equal is a fast-forward"
    );

    std::fs::write(fx.repo.join("local.txt"), "local\n").unwrap();
    git::add_all(&fx.repo).unwrap();
    git::commit(&fx.repo, "local", false).unwrap();
    assert!(
        !git::can_fast_forward(&fx.repo, &upstream).unwrap(),
        "HEAD is ahead: the upstream is not reachable forward from it"
    );
    assert!(git::can_fast_forward(&fx.repo, "HEAD").unwrap());
    assert!(git::can_fast_forward(&fx.repo, "nonexistent-ref").is_err());

    // A merge left half-done is read from MERGE_HEAD, and its paths from the
    // index: a branch from before `local.txt` existed adds its own copy.
    let wt = fx.path("wt");
    let before_local = raw_git(&fx.repo, &["rev-parse", "HEAD~1"]);
    git::worktree_add(&fx.repo, &wt, "theirs", &before_local).unwrap();
    std::fs::write(wt.join("local.txt"), "theirs\n").unwrap();
    git::add_all(&wt).unwrap();
    git::commit(&wt, "theirs", false).unwrap();
    git::worktree_remove(&wt, false).unwrap();
    let merge = Command::new("git")
        .arg("-C")
        .arg(&fx.repo)
        .args(["merge", "--no-edit", "theirs"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(!merge.status.success(), "the fixture conflicts on purpose");
    assert_eq!(
        git::in_progress(&fx.repo).unwrap(),
        Some(bisa_vcs::InProgress::Merge)
    );
    assert_eq!(
        git::conflicted_paths(&fx.repo).unwrap(),
        vec![PathBuf::from("local.txt")]
    );
}

// ---------------------------------------------------------------------------
// Credentials — git's own helpers, asked through `git credential fill`.
// Every helper here is a fake in a temp global config; the developer's real
// helpers and keychain are hidden by `GIT_CONFIG_NOSYSTEM` and a temp global.
// ---------------------------------------------------------------------------

/// A `Git` whose only credential helper is a shell script printing a fixed
/// fake credential — the shape a keychain or `gh` helper answers with.
fn git_with_fake_helper(dir: &Path, password: &str) -> git::Git {
    let script = dir.join("helper.sh");
    std::fs::write(
        &script,
        format!("#!/bin/sh\nprintf 'username=%s\\npassword=%s\\n' someone '{password}'\n"),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let global = dir.join("global.gitconfig");
    std::fs::write(
        &global,
        format!("[credential]\n\thelper = !sh {}\n", script.display()),
    )
    .unwrap();
    git::Git::new()
        .with_env("GIT_CONFIG_GLOBAL", global)
        .with_env("GIT_CONFIG_NOSYSTEM", "1")
}

/// A `Git` that sees no helper at all — not the developer's either.
fn git_with_no_helper(dir: &Path) -> git::Git {
    git::Git::new()
        .with_env("GIT_CONFIG_GLOBAL", dir.join("no-global.gitconfig"))
        .with_env("GIT_CONFIG_NOSYSTEM", "1")
}

#[test]
fn a_credential_helper_is_asked_through_git_and_answers_the_password() {
    let dir = tempfile::tempdir().unwrap();
    let g = git_with_fake_helper(dir.path(), "fake-token-123");
    let cred = g
        .credential_fill("https", "github.com")
        .unwrap()
        .expect("the helper answered");
    assert_eq!(cred.username, "someone");
    assert_eq!(cred.password, "fake-token-123");
    // The password never reaches a log through Debug.
    let shown = format!("{cred:?}");
    assert!(
        shown.contains("someone")
            && shown.contains("<redacted>")
            && !shown.contains("fake-token-123"),
        "{shown}"
    );
}

#[test]
fn no_helper_means_no_credential_and_no_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let g = git_with_no_helper(dir.path());
    let started = std::time::Instant::now();
    // Under the hardened environment git cannot prompt, so an unanswered
    // question is an immediate "nobody", not a hang.
    assert_eq!(g.credential_fill("https", "github.com").unwrap(), None);
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
    assert_eq!(g.credential_helpers().unwrap(), Vec::<String>::new());
}

#[test]
fn the_configured_helpers_are_named_in_one_word_each() {
    let dir = tempfile::tempdir().unwrap();
    let global = dir.path().join("global.gitconfig");
    std::fs::write(
        &global,
        "[credential]\n\thelper = osxkeychain\n\thelper = !gh auth git-credential\n\thelper = /usr/local/bin/git-credential-manager\n\thelper = store --file /tmp/x\n",
    )
    .unwrap();
    let g = git::Git::new()
        .with_env("GIT_CONFIG_GLOBAL", global)
        .with_env("GIT_CONFIG_NOSYSTEM", "1");
    assert_eq!(
        g.credential_helpers().unwrap(),
        ["osxkeychain", "gh", "manager", "store"]
    );
    assert_eq!(git::summarise_helper("!sh /tmp/helper.sh"), "sh");
    assert_eq!(git::summarise_helper("cache --timeout=3600"), "cache");
    assert_eq!(git::summarise_helper("manager-core"), "manager");
}

// ---------------------------------------------------------------------------
// Profiles by organization — includes, the profile file, origins (ide/04)
// ---------------------------------------------------------------------------

/// A `Git` whose global config is one temp file this test owns — what the
/// engine's `gitprofiles` writes into. Never the developer's.
fn git_with_global(root: &Path) -> (git::Git, PathBuf) {
    let global = root.join("global.gitconfig");
    let g = git::Git::new()
        .with_env("GIT_CONFIG_GLOBAL", &global)
        .with_env("GIT_CONFIG_NOSYSTEM", "1");
    (g, global)
}

#[test]
fn the_version_is_parsed_and_the_hasconfig_floor_is_2_36() {
    let v = git::GitVersion::parse("git version 2.50.1 (Apple Git-155)").unwrap();
    assert_eq!((v.major, v.minor, v.patch), (2, 50, 1));
    assert!(v.supports_hasconfig());
    assert!(!git::GitVersion::parse("git version 2.35.9")
        .unwrap()
        .supports_hasconfig());
    assert!(git::GitVersion::parse("git version 2.36.0")
        .unwrap()
        .supports_hasconfig());
    assert_eq!(
        git::GitVersion::parse("git version 2.39")
            .unwrap()
            .to_string(),
        "2.39.0"
    );
    assert!(git::GitVersion::parse("no digits here").is_none());
    let real = git::Git::new()
        .version()
        .expect("the git on this machine has a version");
    assert!(real.major >= 2, "{real}");
}

#[test]
fn includes_are_set_idempotently_removed_by_file_and_never_written_locally() {
    let root = tempfile::tempdir().unwrap();
    let (g, global) = git_with_global(root.path());
    let file = root.path().join("profiles").join("acme.gitconfig");
    let cond = "hasconfig:remote.*.url:https://github.com/acme/**";
    let cond2 = "hasconfig:remote.*.url:git@github.com:acme/**";
    assert_eq!(
        g.includes().unwrap(),
        Vec::<git::Include>::new(),
        "an absent global file has no includes"
    );

    g.include_set(ConfigScope::Global, cond, &file).unwrap();
    g.include_set(ConfigScope::Global, cond, &file).unwrap();
    g.include_set(ConfigScope::Global, cond2, &file).unwrap();
    let text = std::fs::read_to_string(&global).unwrap();
    assert_eq!(
        text.matches("path = ").count(),
        2,
        "a re-save leaves one line per condition:\n{text}"
    );
    // A value written after the include would win over it — git reads top
    // to bottom — so a re-save moves the include below it.
    g.config_set(ConfigScope::Global, None, "user.name", "Grace Hopper")
        .unwrap();
    g.include_set(ConfigScope::Global, cond, &file).unwrap();
    let text = std::fs::read_to_string(&global).unwrap();
    assert!(
        text.rfind("name = Grace").unwrap() < text.rfind(cond).unwrap(),
        "the include is last:\n{text}"
    );
    let includes = g.includes().unwrap();
    assert_eq!(includes.len(), 2);
    assert!(includes.iter().all(|i| i.path == file));
    assert!(includes.iter().any(|i| i.condition == cond), "{includes:?}");
    assert!(
        includes.iter().any(|i| i.condition == cond2),
        "{includes:?}"
    );

    // Another file's include is left alone by a removal that names this one.
    let other = root.path().join("profiles").join("other.gitconfig");
    g.include_set(ConfigScope::Global, cond, &other).unwrap();
    g.include_remove(ConfigScope::Global, &file).unwrap();
    let left = g.includes().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].path, other);
    g.include_remove(ConfigScope::Global, &file)
        .unwrap_or_else(|e| panic!("removing nothing is not an error: {e}"));

    // The local layer is not a place for an include, and the condition and
    // path are checked before anything is spawned.
    assert!(g.include_set(ConfigScope::Local, cond, &file).is_err());
    assert!(g.include_remove(ConfigScope::Local, &file).is_err());
    assert!(
        g.include_set(ConfigScope::Global, "onbranch:main", &file)
            .is_err(),
        "only hasconfig and gitdir conditions"
    );
    assert!(
        g.include_set(
            ConfigScope::Global,
            "hasconfig:remote.*.url:https://x/\"]\n[core",
            &file
        )
        .is_err(),
        "a quote would end the header"
    );
    assert!(
        g.include_set(ConfigScope::Global, cond, Path::new("relative.gitconfig"))
            .is_err(),
        "an include path is absolute"
    );
    assert!(
        g.include_set(ConfigScope::Global, cond, Path::new("--global"))
            .is_err(),
        "option-shaped"
    );
    assert_eq!(text.matches("--global").count(), 0);
}

#[test]
fn a_profile_file_round_trips_and_a_matching_remote_resolves_it_with_its_origin() {
    let root = tempfile::tempdir().unwrap();
    let (g, _global) = git_with_global(root.path());
    let file = root.path().join("profiles").join("acme.gitconfig");
    let key = root.path().join("id_ed25519_acme");
    let profile = git::ProfileFile {
        label: Some("Acme".into()),
        name: "Ada Lovelace".into(),
        email: "ada@acme.example".into(),
        ssh_key: Some(key.clone()),
        credential_username: Some("ada-acme".into()),
        account: Some("Ada-Acme".into()),
    };
    g.config_file_write(&file, &profile).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(
            std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600,
            "owner-only"
        );
    }
    let read = g.config_file_read(&file).unwrap().expect("the file exists");
    assert_eq!(read.label.as_deref(), Some("Acme"));
    assert_eq!(read.name, "Ada Lovelace");
    assert_eq!(read.ssh_key.as_deref(), Some(key.as_path()));
    assert_eq!(
        read.account.as_deref(),
        Some("ada-acme"),
        "lowercased on the way in"
    );
    assert_eq!(read.credential_username.as_deref(), Some("ada-acme"));
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.contains(&format!(
            "sshCommand = ssh -i {} -o IdentitiesOnly=yes",
            key.display()
        )),
        "{text}"
    );
    assert_eq!(
        g.config_file_read(&root.path().join("nope.gitconfig"))
            .unwrap(),
        None
    );

    // Cleared optionals are unset by the next save.
    let plain = git::ProfileFile {
        ssh_key: None,
        credential_username: None,
        account: None,
        ..profile.clone()
    };
    g.config_file_write(&file, &plain).unwrap();
    let read = g.config_file_read(&file).unwrap().unwrap();
    assert_eq!(
        (read.ssh_key, read.credential_username, read.account),
        (None, None, None)
    );

    // What the file refuses before anything is spawned.
    let bad_key = git::ProfileFile {
        ssh_key: Some(PathBuf::from("/tmp/a key")),
        ..profile.clone()
    };
    assert!(
        g.config_file_write(&file, &bad_key).is_err(),
        "a space would need quoting in core.sshCommand"
    );
    let bad_login = git::ProfileFile {
        account: Some("bad login".into()),
        ..profile.clone()
    };
    assert!(g.config_file_write(&file, &bad_login).is_err());
    let bad_email = git::ProfileFile {
        email: "no-at".into(),
        ..profile.clone()
    };
    assert!(g.config_file_write(&file, &bad_email).is_err());

    // A repository whose origin falls under the glob resolves the profile.
    g.config_file_write(&file, &profile).unwrap();
    g.config_set(ConfigScope::Global, None, "user.name", "Grace Hopper")
        .unwrap();
    g.config_set(
        ConfigScope::Global,
        None,
        "user.email",
        "grace@example.invalid",
    )
    .unwrap();
    // The include comes last — the engine re-appends the platform's includes
    // after every global write for the same reason.
    g.include_set(
        ConfigScope::Global,
        "hasconfig:remote.*.url:git@github.com:acme/**",
        &file,
    )
    .unwrap();
    let repo = root.path().join("acme-web");
    g.init(&repo).unwrap();
    g.remote_add(&repo, "origin", "git@github.com:acme/web.git")
        .unwrap();
    let id = g.identity(&repo).unwrap();
    assert_eq!(
        id.name.as_deref(),
        Some("Ada Lovelace"),
        "the profile's author"
    );
    assert_eq!(
        id.source,
        git::IdentitySource::Global,
        "resolved outside the repository — the source stays Global"
    );
    assert_eq!(
        id.global.map(|i| i.name),
        Some("Grace Hopper".into()),
        "`global` is the global layer, not the origin of the effective value"
    );
    let origin = g
        .config_origin(Some(&repo), "user.name")
        .unwrap()
        .expect("set");
    assert_eq!(origin.value, "Ada Lovelace");
    assert!(
        same_path(&origin.file().expect("from a file"), &file),
        "{origin:?}"
    );
    assert_eq!(g.account_get(&repo).unwrap().as_deref(), Some("ada-acme"));
    assert_eq!(
        g.default_account("github").unwrap(),
        None,
        "no default account was set"
    );

    // A repository on another owner sees the global layer alone.
    let other = root.path().join("personal");
    g.init(&other).unwrap();
    g.remote_add(&other, "origin", "git@github.com:ada/notes.git")
        .unwrap();
    assert_eq!(
        g.identity(&other).unwrap().name.as_deref(),
        Some("Grace Hopper")
    );
    assert_eq!(g.account_get(&other).unwrap(), None);
    assert_eq!(
        g.config_origin(Some(&other), "codehost.account").unwrap(),
        None,
        "unset is None, not an error"
    );
}

#[test]
fn the_account_keys_are_schema_keys_a_kinds_default_is_global_and_a_local_pin_is_the_repositorys() {
    let root = tempfile::tempdir().unwrap();
    let (g, _global) = git_with_global(root.path());
    let repo = root.path().join("project");
    g.init(&repo).unwrap();
    g.config_set(
        ConfigScope::Global,
        None,
        "codehost.github.account",
        "Personal-Me",
    )
    .unwrap();
    assert_eq!(
        g.default_account("github").unwrap().as_deref(),
        Some("personal-me"),
        "lowercased, like every login key"
    );
    assert_eq!(
        g.default_account("gitlab").unwrap(),
        None,
        "one default per kind"
    );
    assert_eq!(
        g.default_account("gitea").unwrap(),
        None,
        "a kind the schema does not know has none"
    );
    assert!(
        g.config_set(
            ConfigScope::Local,
            Some(&repo),
            "codehost.github.account",
            "x"
        )
        .is_err(),
        "a kind's default is global only"
    );
    assert_eq!(
        g.account_get(&repo).unwrap(),
        None,
        "a kind's default is not the repository's pin — the engine falls back to it"
    );
    g.config_set(
        ConfigScope::Local,
        Some(&repo),
        "codehost.account",
        "work-me",
    )
    .unwrap();
    assert_eq!(
        g.account_get(&repo).unwrap().as_deref(),
        Some("work-me"),
        "a local pin"
    );
    assert_eq!(
        raw_git(&repo, &["config", "--local", "--get", "codehost.account"]),
        "work-me"
    );
    assert!(g
        .config_set(
            ConfigScope::Local,
            Some(&repo),
            "codehost.account",
            "not a login"
        )
        .is_err());
    g.config_unset(ConfigScope::Local, Some(&repo), "codehost.account")
        .unwrap();
    assert_eq!(g.account_get(&repo).unwrap(), None);
    assert_eq!(g.kind_get(&repo).unwrap(), None);
    g.config_set(ConfigScope::Local, Some(&repo), "codehost.kind", "gitlab")
        .unwrap();
    assert_eq!(
        g.kind_get(&repo).unwrap().as_deref(),
        Some("gitlab"),
        "a self-hosted remote says which kind it is"
    );
    assert!(g
        .config_set(ConfigScope::Local, Some(&repo), "codehost.kind", "gitea")
        .is_err());
    let view = g.config_view(Some(&repo)).unwrap();
    assert_eq!(
        view.get("codehost.github.account")
            .unwrap()
            .global
            .as_deref(),
        Some("personal-me"),
        "the form's view carries it like any key"
    );
}

#[test]
fn ls_remote_reads_the_heads_of_a_reachable_remote_and_refuses_an_option_shaped_name() {
    let f = Fixture::new();
    git::push(&f.repo, "origin", &f.branch, true).unwrap();
    let heads = git::ls_remote(&f.repo, "origin").unwrap();
    assert_eq!(heads.len(), 1);
    assert_eq!(heads[0].name, format!("refs/heads/{}", f.branch));
    assert_eq!(heads[0].sha.len(), 40);
    assert!(
        git::ls_remote(&f.repo, "--upload-pack=x").is_err(),
        "refused before spawn"
    );
    let missing = git::ls_remote(&f.repo, "nowhere").unwrap_err();
    assert!(
        !matches!(missing, VcsError::InvalidArg { .. }),
        "a remote git does not know is git's own refusal: {missing}"
    );
}

#[test]
fn the_safe_tier_reads_the_stash_list_and_diff_without_the_stash_command() {
    let fx = Fixture::new();
    assert!(
        git::stash_list(&fx.repo).unwrap().is_empty(),
        "no refs/stash is an empty list, not an error"
    );
    // The fixture stashes with git itself: the safe tier only reads.
    std::fs::write(fx.repo.join("README.md"), "hello\nworld\nchanged\n").unwrap();
    std::fs::write(fx.repo.join("new.txt"), "untracked too\n").unwrap();
    raw_git(&fx.repo, &["stash", "push", "-u", "-m", "half done"]);
    std::fs::write(fx.repo.join("README.md"), "hello\nworld\nagain\n").unwrap();
    raw_git(&fx.repo, &["stash", "push"]);

    let listed = git::stash_list(&fx.repo).unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].index, 0);
    assert_eq!(listed[0].branch.as_deref(), Some(fx.branch.as_str()));
    assert_eq!(listed[0].message, None, "git's own WIP subject");
    assert!(
        listed[0].subject.starts_with("WIP on "),
        "{}",
        listed[0].subject
    );
    assert!(!listed[0].untracked);
    assert_eq!(listed[1].index, 1);
    assert_eq!(listed[1].message.as_deref(), Some("half done"));
    assert_eq!(listed[1].subject, format!("On {}: half done", fx.branch));
    assert!(listed[1].untracked);
    assert!(listed[1].at > 0);
    assert_eq!(
        raw_git(&fx.repo, &["rev-parse", "stash@{1}"]),
        listed[1].commit.as_str(),
        "the index is what stash@{{n}} means right now"
    );

    let patch = git::stash_diff(&fx.repo, listed[1].commit.as_str()).unwrap();
    assert!(patch.contains("+changed"), "the tracked half: {patch}");
    assert!(
        patch.contains("new.txt") && patch.contains("+untracked too"),
        "the untracked half: {patch}"
    );
    let plain = git::stash_diff(&fx.repo, listed[0].commit.as_str()).unwrap();
    assert!(
        plain.contains("+again") && !plain.contains("new.txt"),
        "{plain}"
    );
    assert!(
        matches!(
            git::stash_diff(&fx.repo, "--output=x"),
            Err(VcsError::InvalidArg { .. })
        ),
        "an option-shaped commit is refused before spawn"
    );
}

// ---------------------------------------------------------------------------
// Remote branches: listed after a fetch, taken up on a tracking worktree
// ---------------------------------------------------------------------------

/// Push a branch to the local bare origin from a second clone, so that the
/// fixture's repository has never heard of it — the shape of a branch
/// somebody else pushed.
fn branch_pushed_from_elsewhere(fx: &Fixture, name: &str) -> String {
    let other = fx.path("other");
    git::clone(fx.origin.to_str().unwrap(), &other, None).expect("clone");
    set_identity(&other);
    raw_git(&other, &["switch", "-c", name]);
    std::fs::write(other.join("theirs.txt"), "from elsewhere\n").unwrap();
    git::add_all(&other).unwrap();
    let head = git::commit(&other, &format!("{name}: theirs"), false).unwrap();
    git::push(&other, "origin", name, true).expect("push from elsewhere");
    head.as_str().to_string()
}

#[test]
fn remote_branches_are_listed_from_the_last_fetch_and_never_from_the_network() {
    let fx = Fixture::new();
    git::push(&fx.repo, "origin", &fx.branch, true).expect("push base");
    // origin/HEAD is a pointer, not a branch: git writes it on clone, and a
    // fixture can set it by hand.
    raw_git(&fx.repo, &["remote", "set-head", "origin", &fx.branch]);
    let theirs = branch_pushed_from_elsewhere(&fx, "feature/theirs");

    let before = git::remote_branch_list(&fx.repo).unwrap();
    assert_eq!(
        before.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(),
        vec![fx.branch.as_str()],
        "unfetched, the repository knows only what it pushed; HEAD is not a branch"
    );

    git::fetch_branch(&fx.repo, "origin", "feature/theirs").expect("fetch one branch");
    let after = git::remote_branch_list(&fx.repo).unwrap();
    let got = after
        .iter()
        .find(|b| b.name == "feature/theirs")
        .expect("fetched branch listed");
    assert_eq!(got.remote, "origin");
    assert_eq!(got.head.as_str(), theirs);
    assert_eq!(got.subject, "feature/theirs: theirs");
    assert!(got.timestamp > 0);
    assert_eq!(after[0].name, "feature/theirs", "newest first");
    assert!(after.iter().all(|b| b.name != "HEAD"));

    // No local branch appeared: a fetch is a read of the remote, nothing more.
    assert!(git::branch_list(&fx.repo, None)
        .unwrap()
        .iter()
        .all(|b| b.name != "feature/theirs"));
}

#[test]
fn a_tracking_worktree_stands_on_a_new_local_branch_that_follows_the_remote_one() {
    let fx = Fixture::new();
    git::push(&fx.repo, "origin", &fx.branch, true).expect("push base");
    let theirs = branch_pushed_from_elsewhere(&fx, "feature/theirs");
    git::fetch_branch(&fx.repo, "origin", "feature/theirs").expect("fetch");

    let wt = fx.path("wt-theirs");
    git::worktree_add_tracking(&fx.repo, &wt, "feature/theirs", "origin", "feature/theirs")
        .expect("tracking worktree");
    assert_eq!(
        git::head(&wt).unwrap().as_str(),
        theirs,
        "HEAD starts where the remote branch is"
    );
    assert_eq!(
        git::upstream_of(&wt).unwrap().as_deref(),
        Some("origin/feature/theirs"),
        "the local branch tracks the remote one, so push and pull need no refspec"
    );
    assert!(wt.join("theirs.txt").exists());
    let status = git::status(&wt).unwrap();
    assert_eq!((status.ahead, status.behind), (0, 0));
    let local = git::branch_list(&fx.repo, None).unwrap();
    let mine = local
        .iter()
        .find(|b| b.name == "feature/theirs")
        .expect("local branch exists");
    assert_eq!(mine.upstream.as_deref(), Some("origin/feature/theirs"));

    // A second worktree on the same name fails the way a new branch would:
    // the branch exists — the caller opens it as an existing one instead.
    assert!(git::worktree_add_tracking(
        &fx.repo,
        &fx.path("wt-again"),
        "feature/theirs",
        "origin",
        "feature/theirs"
    )
    .is_err());
}

#[test]
fn fetching_a_branch_the_remote_lacks_is_gits_error_and_option_shaped_names_never_spawn() {
    let fx = Fixture::new();
    git::push(&fx.repo, "origin", &fx.branch, true).expect("push base");
    let err = git::fetch_branch(&fx.repo, "origin", "nope").unwrap_err();
    assert!(
        !matches!(err, VcsError::InvalidArg { .. }),
        "a legal name the remote lacks is git's refusal, not ours: {err}"
    );
    assert!(
        matches!(
            git::fetch_branch(&fx.repo, "origin", "--upload-pack=x"),
            Err(VcsError::InvalidArg { .. })
        ),
        "an option-shaped branch is refused before spawn"
    );
    assert!(
        matches!(
            git::worktree_add_tracking(&fx.repo, &fx.path("x"), "ok", "--mirror", "ok"),
            Err(VcsError::InvalidArg { .. })
        ),
        "an option-shaped remote is refused before spawn"
    );
}

/// A branch nothing holds goes in one call; a name shaped like an option is
/// refused before git is asked; the branch a worktree stands on is git's to
/// refuse, not ours to force.
#[test]
fn a_branch_no_worktree_holds_is_deleted_and_a_bad_name_is_refused() {
    let fx = Fixture::new();
    git::branch_create(&fx.repo, "spare", None, false).unwrap();
    assert!(git::branch_exists(&fx.repo, "spare").unwrap());
    git::branch_delete(&fx.repo, "spare").unwrap();
    assert!(!git::branch_exists(&fx.repo, "spare").unwrap());
    assert!(
        matches!(
            git::branch_delete(&fx.repo, "--delete=evil"),
            Err(VcsError::InvalidArg { .. })
        ),
        "an option-shaped name is refused before spawn"
    );
    assert!(
        git::branch_delete(&fx.repo, &fx.branch).is_err(),
        "the branch the checkout stands on is refused by git"
    );
}

#[test]
fn branch_exists_answers_by_name_and_refuses_a_bad_ref() {
    let fx = Fixture::new();
    assert!(git::branch_exists(&fx.repo, &fx.branch).unwrap());
    assert!(!git::branch_exists(&fx.repo, "nope").unwrap());
    assert!(
        git::branch_exists(&fx.repo, "--upload-pack=evil").is_err(),
        "a name that is an option is refused before git is asked"
    );
}

// ---------------------------------------------------------------------------
// The three sides of a conflict, and what the remote says about its trunk
// ---------------------------------------------------------------------------

#[test]
fn a_conflict_has_three_readable_sides_and_an_unconflicted_path_has_none() {
    let fx = Fixture::new();
    let wt = fx.path("wt-sides");
    git::worktree_add(&fx.repo, &wt, "sides", &fx.branch).unwrap();
    std::fs::write(wt.join("README.md"), "hello\ntheirs\n").unwrap();
    git::add_all(&wt).unwrap();
    git::commit(&wt, "theirs", false).unwrap();
    std::fs::write(fx.repo.join("README.md"), "hello\nours\n").unwrap();
    git::add_all(&fx.repo).unwrap();
    git::commit(&fx.repo, "ours", false).unwrap();
    let merged = Command::new("git")
        .arg("-C")
        .arg(&fx.repo)
        .args(["merge", "sides"])
        .output()
        .unwrap();
    assert!(!merged.status.success(), "the merge must conflict");

    // `:1:`, `:2:`, `:3:` — the bare path, never pathspec magic; bytes.
    let sides = git::conflict_blobs(&fx.repo, "README.md").unwrap();
    assert_eq!(sides.base.as_deref(), Some(&b"hello\nworld\n"[..]));
    assert_eq!(sides.ours.as_deref(), Some(&b"hello\nours\n"[..]));
    assert_eq!(sides.theirs.as_deref(), Some(&b"hello\ntheirs\n"[..]));

    // The row says what kind of conflict it is, and the facts name the sides.
    let rows = git::status_files(&fx.repo).unwrap();
    assert_eq!(rows[0].conflict, Some(ConflictKind::BothModified));
    let facts = git::operation_facts(&fx.repo)
        .unwrap()
        .expect("a merge is half-done");
    assert_eq!(facts.kind, InProgress::Merge);
    assert_eq!(facts.branch.as_deref(), Some(fx.branch.as_str()));
    assert_eq!(facts.ours.name.as_deref(), Some(fx.branch.as_str()));
    assert_eq!(facts.theirs.role, SideRole::Branch);
    assert_eq!(
        facts.theirs.name.as_deref(),
        Some("sides"),
        "named from the prepared message"
    );
    assert_eq!(
        facts.theirs.subject.as_deref(),
        Some("theirs"),
        "the commit's subject is filled in"
    );
    assert_eq!(facts.step, None);

    // A path with no conflict has no stages to read.
    let none = git::conflict_blobs(&fx.repo, "nothing-here.txt").unwrap();
    assert_eq!((none.base, none.ours, none.theirs), (None, None, None));

    // The pathspec is still validated like every other.
    assert!(
        git::conflict_blobs(&fx.repo, "-x").is_err(),
        "an option is not a path"
    );
    assert!(
        git::conflict_blobs(&fx.repo, "../elsewhere").is_err(),
        "nor is a path out of the tree"
    );
}

/// A file deleted on one side and changed on the other, and one added on
/// both: the kinds a person must choose between, not merge.
#[test]
fn a_deleted_and_an_added_on_both_sides_conflict_are_known_by_kind() {
    let fx = Fixture::new();
    let wt = fx.path("wt-kinds");
    git::worktree_add(&fx.repo, &wt, "kinds", &fx.branch).unwrap();
    // Theirs deletes README.md and adds NEW.md.
    std::fs::remove_file(wt.join("README.md")).unwrap();
    std::fs::write(wt.join("NEW.md"), "theirs\n").unwrap();
    git::add_all(&wt).unwrap();
    git::commit(&wt, "theirs", false).unwrap();
    // Ours changes README.md and adds a different NEW.md.
    std::fs::write(fx.repo.join("README.md"), "hello\nours\n").unwrap();
    std::fs::write(fx.repo.join("NEW.md"), "ours\n").unwrap();
    git::add_all(&fx.repo).unwrap();
    git::commit(&fx.repo, "ours", false).unwrap();
    let merged = Command::new("git")
        .arg("-C")
        .arg(&fx.repo)
        .args(["merge", "kinds"])
        .output()
        .unwrap();
    assert!(!merged.status.success(), "the merge must conflict");

    let rows = git::status_files(&fx.repo).unwrap();
    let kind = |name: &str| {
        rows.iter()
            .find(|r| r.path == Path::new(name))
            .and_then(|r| r.conflict)
    };
    assert_eq!(kind("README.md"), Some(ConflictKind::DeletedByThem));
    assert_eq!(kind("NEW.md"), Some(ConflictKind::BothAdded));
    let sides = git::conflict_blobs(&fx.repo, "README.md").unwrap();
    assert!(
        sides.theirs.is_none(),
        "the side that deleted it has no stage"
    );
    assert!(sides.ours.is_some() && sides.base.is_some());
    let added = git::conflict_blobs(&fx.repo, "NEW.md").unwrap();
    assert!(added.base.is_none(), "added on both sides: no base");
}

/// The facts of a rebase and of a pick of two commits: the branch replayed,
/// the commit stopped on with its subject, the step — read from git's
/// directory, so an operation started in a terminal is described too.
#[test]
fn a_rebase_and_a_pick_started_in_a_terminal_are_described_with_their_step() {
    let fx = Fixture::new();
    let wt = fx.path("wt-facts");
    git::worktree_add(&fx.repo, &wt, "facts", &fx.branch).unwrap();
    std::fs::write(wt.join("README.md"), "hello\ntheirs\n").unwrap();
    git::add_all(&wt).unwrap();
    git::commit(&wt, "theirs one", false).unwrap();
    std::fs::write(wt.join("OTHER.md"), "two\n").unwrap();
    git::add_all(&wt).unwrap();
    git::commit(&wt, "theirs two", false).unwrap();
    std::fs::write(fx.repo.join("README.md"), "hello\nours\n").unwrap();
    git::add_all(&fx.repo).unwrap();
    git::commit(&fx.repo, "ours", false).unwrap();

    // Rebase `facts` onto the fixture's branch, in the worktree, by raw git.
    let rebased = Command::new("git")
        .arg("-C")
        .arg(&wt)
        .args(["rebase", &fx.branch])
        .output()
        .unwrap();
    assert!(
        !rebased.status.success(),
        "the rebase must conflict on its first commit"
    );
    let facts = git::operation_facts(&wt)
        .unwrap()
        .expect("a rebase is half-done");
    assert_eq!(facts.kind, InProgress::Rebase);
    assert_eq!(
        facts.branch.as_deref(),
        Some("facts"),
        "the branch replayed, though HEAD is detached"
    );
    assert_eq!(facts.ours.role, SideRole::Branch);
    assert_eq!(
        facts.ours.name.as_deref(),
        Some(fx.branch.as_str()),
        "git's ours is the branch rebased onto"
    );
    assert_eq!(facts.theirs.role, SideRole::Commit);
    assert_eq!(
        facts.theirs.subject.as_deref(),
        Some("theirs one"),
        "git's theirs is the commit replayed"
    );
    assert_eq!(facts.step, Some(Step { done: 1, total: 2 }));
    assert!(
        git::operation_facts(&fx.repo).unwrap().is_none(),
        "the other checkout has nothing half-done"
    );
    let aborted = Command::new("git")
        .arg("-C")
        .arg(&wt)
        .args(["rebase", "--abort"])
        .output()
        .unwrap();
    assert!(aborted.status.success());

    // A pick of the two commits onto the fixture's branch stops on the first.
    let first = git::log(&wt, None, 10).unwrap();
    let shas: Vec<String> = first.iter().map(|c| c.id.to_string()).collect();
    let picked = Command::new("git")
        .arg("-C")
        .arg(&fx.repo)
        .args(["cherry-pick", &shas[1], &shas[0]])
        .output()
        .unwrap();
    assert!(!picked.status.success(), "the pick must conflict");
    let facts = git::operation_facts(&fx.repo)
        .unwrap()
        .expect("a pick is half-done");
    assert_eq!(facts.kind, InProgress::CherryPick);
    assert_eq!(facts.ours.name.as_deref(), Some(fx.branch.as_str()));
    assert_eq!(facts.theirs.subject.as_deref(), Some("theirs one"));
    assert_eq!(
        facts.step,
        Some(Step { done: 1, total: 2 }),
        "the first of two"
    );
}

/// What a merge would do, before it runs: clean, or the paths that would
/// conflict — and the tree and index untouched either way.
#[test]
fn a_merge_is_foreseen_without_moving_anything() {
    let fx = Fixture::new();
    let wt = fx.path("wt-preview");
    git::worktree_add(&fx.repo, &wt, "preview", &fx.branch).unwrap();
    std::fs::write(wt.join("README.md"), "hello\ntheirs\n").unwrap();
    git::add_all(&wt).unwrap();
    git::commit(&wt, "theirs", false).unwrap();

    let clean = git::merge_preview(&fx.repo, "HEAD", "preview")
        .unwrap()
        .expect("git 2.38 or later");
    assert!(
        clean.clean && clean.paths.is_empty(),
        "nothing changed on ours: a clean merge"
    );

    std::fs::write(fx.repo.join("README.md"), "hello\nours\n").unwrap();
    git::add_all(&fx.repo).unwrap();
    git::commit(&fx.repo, "ours", false).unwrap();
    let foreseen = git::merge_preview(&fx.repo, "HEAD", "preview")
        .unwrap()
        .unwrap();
    assert!(!foreseen.clean);
    assert_eq!(foreseen.paths, [PathBuf::from("README.md")]);
    let st = git::status(&fx.repo).unwrap();
    assert!(st.is_clean && st.conflicted == 0, "a preview moves nothing");
    assert!(git::in_progress(&fx.repo).unwrap().is_none());
    assert_eq!(
        std::fs::read_to_string(fx.repo.join("README.md")).unwrap(),
        "hello\nours\n"
    );
    assert!(
        git::merge_preview(&fx.repo, "-x", "preview").is_err(),
        "an option is not a revision"
    );
}

/// A file's whole text at one commit and at its parent: the two sides of a
/// commit's change (ide/05) — `None` where that revision has no such path,
/// and a revision spelled with a suffix refused, since a parent is an id
/// the caller holds.
#[test]
fn a_blob_is_read_at_a_commit_by_its_id_and_a_suffix_is_refused() {
    let fx = Fixture::new();
    let first = git::head(&fx.repo).unwrap();
    std::fs::write(fx.repo.join("README.md"), "hello\nagain\n").unwrap();
    std::fs::write(fx.repo.join("new.txt"), "new\n").unwrap();
    git::add_all(&fx.repo).unwrap();
    let second = git::commit(&fx.repo, "second", false).unwrap();

    assert_eq!(
        git::blob(&fx.repo, git::BlobRev::Rev(second.to_string()), "README.md")
            .unwrap()
            .as_deref(),
        Some(b"hello\nagain\n".as_slice()),
        "the file as the commit holds it"
    );
    assert_eq!(
        git::blob(&fx.repo, git::BlobRev::Rev(first.to_string()), "README.md")
            .unwrap()
            .as_deref(),
        Some(b"hello\nworld\n".as_slice()),
        "the file as its parent held it"
    );
    assert_eq!(
        git::blob(&fx.repo, git::BlobRev::Rev(first.to_string()), "new.txt").unwrap(),
        None,
        "a new file has nothing at the parent"
    );
    assert!(
        git::blob(
            &fx.repo,
            git::BlobRev::Rev(format!("{second}^")),
            "README.md"
        )
        .is_err(),
        "a parent is an id, never a suffix"
    );
    assert!(
        git::blob(
            &fx.repo,
            git::BlobRev::Rev("--output=x".into()),
            "README.md"
        )
        .is_err(),
        "an option is not a revision"
    );
}

/// One commit's patch, whole or one file of it, and a rename read as one
/// change with its old path — in the detail and in the patch alike, since
/// the two are asked with the same detection.
#[test]
fn a_commits_patch_is_read_whole_or_by_file_and_a_rename_is_one_change() {
    let fx = Fixture::new();
    std::fs::write(fx.repo.join("other.txt"), "other\n").unwrap();
    std::fs::write(fx.repo.join("README.md"), "hello\nworld\nmore\n").unwrap();
    git::add_all(&fx.repo).unwrap();
    let second = git::commit(&fx.repo, "second", false).unwrap();

    let whole = git::commit_diff(&fx.repo, second.as_str(), &[]).unwrap();
    assert!(
        whole.contains("+other") && whole.contains("+more"),
        "{whole}"
    );
    let one = git::commit_diff(&fx.repo, second.as_str(), &["README.md"]).unwrap();
    assert!(one.contains("+more") && !one.contains("other.txt"), "{one}");
    assert!(
        git::commit_diff(&fx.repo, second.as_str(), &["../elsewhere"]).is_err(),
        "a path is validated as every pathspec is"
    );

    // `git mv` renames into a folder that exists; git makes none.
    std::fs::create_dir_all(fx.repo.join("docs")).unwrap();
    raw_git(&fx.repo, &["mv", "README.md", "docs/README.md"]);
    let moved = git::commit(&fx.repo, "move", false).unwrap();
    let detail = git::commit_detail(&fx.repo, moved.as_str()).unwrap();
    assert_eq!(detail.files.len(), 1, "{:?}", detail.files);
    assert_eq!(detail.files[0].kind, git::ChangeKind::Renamed);
    assert_eq!(
        detail.files[0].old_path.as_deref(),
        Some(Path::new("README.md"))
    );
    assert_eq!(detail.files[0].path, PathBuf::from("docs/README.md"));
    let rename =
        git::commit_diff(&fx.repo, moved.as_str(), &["README.md", "docs/README.md"]).unwrap();
    assert!(
        rename.contains("rename from README.md") && rename.contains("rename to docs/README.md"),
        "{rename}"
    );
}

/// A file's whole text at HEAD and in the index: the two sides of a staged
/// change and the left side of a working-tree one — read whole, never
/// staged, and `None` where the revision has no such path.
#[test]
fn a_blob_is_read_at_head_or_in_the_index_and_is_none_where_absent() {
    let fx = Fixture::new();
    assert_eq!(
        git::blob(&fx.repo, git::BlobRev::Head, "README.md")
            .unwrap()
            .as_deref(),
        Some(b"hello\nworld\n".as_slice())
    );
    assert_eq!(
        git::blob(&fx.repo, git::BlobRev::Index, "README.md")
            .unwrap()
            .as_deref(),
        Some(b"hello\nworld\n".as_slice()),
        "the index holds what HEAD does until something is staged"
    );

    // Staged but not committed: the index moved, HEAD did not.
    std::fs::write(fx.repo.join("README.md"), "hello\nstaged\n").unwrap();
    git::stage(&fx.repo, &["README.md"]).expect("stage");
    std::fs::write(fx.repo.join("README.md"), "hello\ntree\n").unwrap();
    assert_eq!(
        git::blob(&fx.repo, git::BlobRev::Index, "README.md")
            .unwrap()
            .as_deref(),
        Some(b"hello\nstaged\n".as_slice())
    );
    assert_eq!(
        git::blob(&fx.repo, git::BlobRev::Head, "README.md")
            .unwrap()
            .as_deref(),
        Some(b"hello\nworld\n".as_slice()),
        "the working tree's text is nobody's blob"
    );

    // A file git has never seen: nothing at HEAD, nothing in the index.
    std::fs::write(fx.repo.join("new.txt"), "fresh\n").unwrap();
    assert_eq!(
        git::blob(&fx.repo, git::BlobRev::Head, "new.txt").unwrap(),
        None
    );
    assert_eq!(
        git::blob(&fx.repo, git::BlobRev::Index, "new.txt").unwrap(),
        None
    );
    assert!(
        git::status_files(&fx.repo)
            .unwrap()
            .iter()
            .any(|f| f.untracked),
        "reading a blob staged nothing"
    );

    // Bytes, not text: a NUL survives the read.
    std::fs::write(fx.repo.join("blob.bin"), b"\x00\x01\x02").unwrap();
    git::stage(&fx.repo, &["blob.bin"]).expect("stage");
    assert_eq!(
        git::blob(&fx.repo, git::BlobRev::Index, "blob.bin")
            .unwrap()
            .as_deref(),
        Some(b"\x00\x01\x02".as_slice())
    );

    // The pathspec is still validated like every other.
    assert!(
        git::blob(&fx.repo, git::BlobRev::Head, "-x").is_err(),
        "an option is not a path"
    );
    assert!(
        git::blob(&fx.repo, git::BlobRev::Index, "../elsewhere").is_err(),
        "nor is a path out of the tree"
    );
}

#[test]
fn the_remotes_default_branch_is_only_what_origin_head_says() {
    let fx = Fixture::new();
    assert_eq!(
        git::remote_default_branch(&fx.repo).unwrap(),
        None,
        "a bare origin seeded by a push has told this checkout nothing"
    );
    assert_eq!(
        git::default_branch(&fx.repo).unwrap(),
        fx.branch,
        "so the trunk falls back to the branch checked out"
    );
    git::push(&fx.repo, "origin", &fx.branch, true).unwrap();
    let dest = fx.path("clone");
    git::clone(fx.origin.to_str().unwrap(), &dest, None).unwrap();
    assert_eq!(
        git::remote_default_branch(&dest).unwrap().as_deref(),
        Some(fx.branch.as_str()),
        "a clone records origin/HEAD"
    );
}
