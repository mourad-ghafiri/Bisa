//! Projects on disk, workstreams, and the `Publish` gate.
//!
//! **Every repository here is created by the test inside a `tempfile`
//! directory and dies with it.** No user repository is touched, "origin" is a
//! local bare repository addressed by its path, and GitHub is never reached —
//! the pull-request path runs against an in-memory `FakeCodeHost` that claims
//! the bare origin by name, so this suite proves the gate and the record
//! without a network. What GitHub's API accepts is the code-host crate's own
//! suite (`bisa-codehost/tests/github.rs`, against a stub server).

use crate::common;
use bisa_codehost::CodeHost as _;
use bisa_core::workitem::{WorkItemSpec, WorkItemState};
use bisa_core::Budget;
use bisa_core::HarnessCaps;
use bisa_core::WorkstreamTransition;
use bisa_core::{
    Gate, GoalId, Project, ProjectRoot, PublishPolicy, ToolTier, Vcs, WorkItemId, WorkstreamKind,
    WorkstreamSource, WorkstreamState,
};
use bisa_engine::{projects, Engine, EngineConfig, EngineError, EnginePayload, SubmitRequest};
use bisa_harness::mock::MockAdapter;
use bisa_harness::{
    HarnessCatalog, HarnessError, HarnessSession, ProbeResult, ResumeToken, SessionSpec,
};
use bisa_store::{MemoryKeyStore, NewProject, Workspace, WorkstreamFilter};
use common::run_spec;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn ulid() -> ulid::Ulid {
    ulid::Ulid::from_datetime(SystemTime::now())
}

/// Raw git, for the fixture steps the platform deliberately does not expose
/// (`init --bare`, writing a repository-local identity) and for verifying its
/// work independently.
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

/// Identity goes in the repository's *local* config: a global write would be a
/// side effect on the developer's machine, and an environment variable would
/// race across parallel tests.
/// A project an agent's `create_project` makes for a goal — the one path a
/// project is made by: managed, `git init`, a root commit, attached and
/// journaled (`projects::create_for_agent`).
async fn made_for(engine: &Engine, goal: bisa_core::GoalId, slug: &str) -> Project {
    let mut new = NewProject::managed(slug).unwrap();
    new.origin = bisa_core::ProjectOrigin::from_goal(goal);
    projects::create_for_agent(engine.inner(), new, Some(goal), None)
        .await
        .expect("create_for_agent")
        .project
}

fn set_identity(repo: &Path) {
    raw_git(repo, &["config", "user.name", "Bisa Test"]);
    raw_git(repo, &["config", "user.email", "test@example.invalid"]);
}

fn workspace(dir: &tempfile::TempDir) -> Workspace {
    Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap()
}

fn catalog() -> HarnessCatalog {
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::new(MockAdapter::default()));
    catalog
}

fn config() -> EngineConfig {
    EngineConfig {
        design_enabled: false,
        // Every pull-request test here builds on this: git's own credential
        // helper — the OS keychain on a developer's machine — is never asked.
        git: Some(crate::common::isolated_git()),
        ..Default::default()
    }
}

fn engine_with(dir: &tempfile::TempDir, config: EngineConfig) -> Engine {
    Engine::start(workspace(dir), catalog(), config).unwrap()
}

/// A `git` that cannot see the developer's global or system config: what the
/// engine observes under it is the repository and the workspace's settings,
/// nothing else — so "nobody is set to commit" can be produced on purpose.
/// Per handle, never the process environment: tests run in parallel.
fn isolated_git(dir: &tempfile::TempDir) -> bisa_vcs::Git {
    bisa_vcs::Git::new()
        .with_env("GIT_CONFIG_GLOBAL", dir.path().join("no-global.gitconfig"))
        .with_env("GIT_CONFIG_NOSYSTEM", "1")
}

/// [`isolated_git`] whose global file names one pair — a person who *has* a
/// global identity, as the engine would see them.
fn git_with_global(dir: &tempfile::TempDir, name: &str, email: &str) -> bisa_vcs::Git {
    let global = dir.path().join("global.gitconfig");
    std::fs::write(
        &global,
        format!("[user]\n\tname = {name}\n\temail = {email}\n"),
    )
    .unwrap();
    bisa_vcs::Git::new()
        .with_env("GIT_CONFIG_GLOBAL", global)
        .with_env("GIT_CONFIG_NOSYSTEM", "1")
}

fn config_with_git(git: bisa_vcs::Git) -> EngineConfig {
    EngineConfig {
        git: Some(git),
        ..config()
    }
}

/// A one-off commit that writes no config at all — `-c` on the command line —
/// so a repository can have history and still have nobody set to commit.
fn commit_without_identity(repo: &Path, message: &str) {
    raw_git(
        repo,
        &[
            "-c",
            "user.name=Nobody In Particular",
            "-c",
            "user.email=nobody@example.invalid",
            "commit",
            "--quiet",
            "--allow-empty",
            "-m",
            message,
        ],
    );
}

/// A managed project attached to `goal`. Projects belong to the workspace;
/// the attachment is what makes one visible to a goal's work.
fn attached_project(ws: &Workspace, goal: GoalId, new: NewProject) -> Project {
    let project = ws.create_project(new).unwrap();
    ws.attach(goal, project.id).unwrap();
    project
}

fn item(goal: GoalId, project: Option<&Project>, instructions: &str) -> WorkItemSpec {
    WorkItemSpec {
        id: WorkItemId::from_ulid(ulid()),
        home: bisa_core::Home::Goal { goal },
        run: None,
        step: None,
        instructions: instructions.into(),
        state: WorkItemState::Open,
        project: project.map(|p| p.id),
        harness_candidates: vec!["mock".into()],
        model: None,
        effort: None,
        output_schema: None,
        budget: Budget::default(),
        assignees: vec![],
        tier_ceiling: ToolTier::Write,
        agent: None,
        spawn_allowlist: vec![],
        depth_budget: 0,
        result_attempts: 0,
        interruptions: 0,
    }
}

/// A managed project whose folder exists but has no repository in it.
fn plain_project(engine: &Engine, slug: &str) -> (GoalId, Project) {
    let ws = engine.workspace();
    let goal = ws
        .create_goal(bisa_store::NewGoal::captured(&format!("works on {slug}")))
        .unwrap();
    let project = attached_project(ws, goal.id, NewProject::managed(slug).unwrap());
    (goal.id, project)
}

/// A git project with one commit, plus a bare repository next to it standing
/// in for `origin`.
async fn git_project(
    engine: &Engine,
    slug: &str,
    publish: PublishPolicy,
) -> (GoalId, Project, PathBuf, PathBuf) {
    let (goal, mut project) = plain_project(engine, slug);
    project.publish = publish;
    let project = engine.workspace().update_project(project).unwrap();

    let project = projects::init_git(engine.inner(), &project)
        .await
        .expect("init_git");
    let root = engine.workspace().project_root_path(&project);
    set_identity(&root);
    std::fs::write(root.join("README.md"), "baseline\n").unwrap();
    bisa_vcs::git::add_all(&root).unwrap();
    bisa_vcs::git::commit(&root, "baseline", false).unwrap();

    let origin = engine.workspace().root().join(format!("{slug}-origin.git"));
    std::fs::create_dir_all(&origin).unwrap();
    raw_git(&origin, &["init", "--bare", "--quiet"]);
    bisa_vcs::git::remote_add(&root, "origin", origin.to_str().unwrap()).unwrap();

    (goal, project, root, origin)
}

/// Branches the bare origin has heard about.
fn origin_branches(origin: &Path) -> Vec<String> {
    raw_git(
        origin,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
    )
    .lines()
    .map(str::to_string)
    .collect()
}

async fn until<T>(what: &str, mut probe: impl FnMut() -> Option<T>) -> T {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(v) = probe() {
            return v;
        }
        assert!(std::time::Instant::now() < deadline, "timed out: {what}");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// The id of the pending `Publish` gate, once one exists.
async fn pending_publish_gate(engine: &Engine) -> String {
    until("a pending publish gate", || {
        engine
            .inbox()
            .into_iter()
            .find(|g| g.gate == Gate::Publish)
            .map(|g| g.id)
    })
    .await
}

// ---------------------------------------------------------------------------
// Branch naming
// ---------------------------------------------------------------------------

/// Readable *and* unique, whatever the instructions look like.
#[test]
fn branch_names_are_readable_and_always_legal() {
    let id = WorkItemId::from_ulid(ulid());
    let tail = {
        let s = id.to_string();
        s[s.len() - 6..].to_ascii_lowercase()
    };

    let branch = projects::branch_for("Implement the parser", id, false);
    assert_eq!(branch, format!("work/implement-the-parser-{tail}"));

    // Only the first 40 characters of the instructions ride along.
    let long = projects::branch_for(
        "Rewrite the entire billing subsystem from scratch and also the invoices",
        id,
        false,
    );
    assert_eq!(
        long,
        format!("work/rewrite-the-entire-billing-subsystem-fro-{tail}"),
        "the cut is at 40 characters, not at a word boundary"
    );

    // Nothing produces an illegal ref, and the id tail always survives.
    for nasty in [
        "",
        "   ",
        "***",
        "///",
        "..",
        ".lock",
        "-leading-dash",
        "日本語のタスク",
        "caf\u{e9} r\u{e9}sum\u{e9}",
        "a/b/c: do the thing?",
        "new\nline\ttab",
        "\u{0}\u{7f}",
        &"x".repeat(500),
    ] {
        let b = projects::branch_for(nasty, id, false);
        assert!(
            bisa_core::workstream::is_valid_branch_name(&b),
            "{nasty:?} produced {b:?}, which git would refuse"
        );
        assert!(
            b.ends_with(&tail),
            "{nasty:?} produced {b:?}, which lost its unique tail"
        );
    }
}

/// Two items whose instructions start identically still get distinct
/// branches, and the long form is a genuinely different name.
#[test]
fn branch_names_do_not_collide_on_identical_instructions() {
    let shared = "Fix the failing test in the parser module, again";
    let a = WorkItemId::from_ulid(ulid());
    let b = WorkItemId::from_ulid(ulid());
    assert_ne!(
        projects::branch_for(shared, a, false),
        projects::branch_for(shared, b, false)
    );
    assert_ne!(
        projects::branch_for(shared, a, false),
        projects::branch_for(shared, a, true)
    );
    assert!(projects::branch_for(shared, a, true).ends_with(&a.to_string().to_ascii_lowercase()));
}

// ---------------------------------------------------------------------------
// Materialization
// ---------------------------------------------------------------------------

/// Creating a project makes a folder and nothing else. `git init` is its own
/// step, and adopting an external folder never writes into it.
#[tokio::test(flavor = "multi_thread")]
async fn materialize_makes_a_folder_and_never_a_repository() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (_goal, project) = plain_project(&engine, "notes");

    let root = projects::materialize(engine.inner(), &project)
        .await
        .unwrap();
    assert!(root.is_dir());
    assert!(
        !root.join(".git").exists(),
        "materialize must not run git init"
    );

    // Only the explicit step creates a repository.
    let project = projects::init_git(engine.inner(), &project).await.unwrap();
    assert!(root.join(".git").exists());
    assert!(project.vcs.is_git());

    // ...and it is idempotent: a second call adopts rather than re-inits.
    let head_before = raw_git(&root, &["rev-parse", "--git-dir"]);
    let again = projects::init_git(engine.inner(), &project).await.unwrap();
    assert_eq!(raw_git(&root, &["rev-parse", "--git-dir"]), head_before);
    assert!(again.vcs.is_git());

    engine.shutdown().await;
}

/// A plain folder adopted as a project: the external root recorded, nothing
/// written into it (`detect_vcs` alone).
async fn adopted_plain_folder(
    engine: &Engine,
    dir: &tempfile::TempDir,
    slug: &str,
) -> (Project, PathBuf) {
    let theirs = dir.path().join(slug);
    std::fs::create_dir_all(&theirs).unwrap();
    std::fs::write(theirs.join("notes.txt"), "theirs\n").unwrap();
    let created = projects::create(
        engine.inner(),
        projects::NewProjectRequest {
            new: NewProject {
                origin: bisa_core::ProjectOrigin::Workspace,
                root: ProjectRoot::External {
                    path: theirs.display().to_string(),
                },
                ..NewProject::managed(slug).unwrap()
            },
            source: projects::ProjectSource::Adopt,
            git_config: Vec::new(),
        },
    )
    .await
    .expect("adopt");
    assert!(
        matches!(created.project.vcs, Vcs::None),
        "adopt records what it finds"
    );
    assert!(!theirs.join(".git").exists(), "adopt writes nothing");
    (created.project, theirs)
}

/// *Initialise a repository* on a managed plain project: `.git` appears, the
/// record says git, the who-commits policy runs as at creation, and — an
/// identity resolving — the empty root commit gives the first workstream a
/// commit to branch from. Both frames follow: the project's and its primary's,
/// so the rail's mark and the Git panel turn together.
#[tokio::test(flavor = "multi_thread")]
async fn init_repository_turns_a_plain_project_into_a_repository_with_a_root_commit() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(
        &dir,
        config_with_git(git_with_global(
            &dir,
            "Grace Hopper",
            "grace@example.invalid",
        )),
    );
    let (_goal, project) = plain_project(&engine, "notes");
    let root = projects::materialize(engine.inner(), &project)
        .await
        .unwrap();
    assert!(!root.join(".git").exists());
    let mut rx = engine.events();

    let done = projects::init_repository(engine.inner(), project.id, Vec::new())
        .await
        .expect("init_repository");
    assert!(root.join(".git").is_dir());
    assert!(done.project.vcs.is_git());
    assert!(engine
        .workspace()
        .get_project(project.id)
        .unwrap()
        .vcs
        .is_git());
    assert!(matches!(
        done.committer,
        bisa_engine::Committer::Inherit { .. }
    ));
    assert_eq!(
        raw_git(&root, &["log", "-1", "--format=%s|%an"]),
        "project notes initialised|Grace Hopper",
        "the root commit, by the inherited identity"
    );

    common::wait_for(
        &mut rx,
        "project_changed",
        |e| matches!(&e.payload, EnginePayload::ProjectChanged { project: p } if *p == project.id),
    )
    .await;
    common::wait_for(&mut rx, "the primary's workstream_changed", |e| {
        matches!(&e.payload, EnginePayload::WorkstreamChanged { workstream, .. } if *workstream == bisa_core::WorkstreamId::primary_of(project.id))
    })
    .await;
    engine.shutdown().await;
}

/// Adopt writes nothing — until the person asks. `init_repository` on an
/// adopted external folder is that ask, and the one write it allows: the
/// `.git` lands in *their* folder, their files untouched, the record turns git.
#[tokio::test(flavor = "multi_thread")]
async fn init_repository_writes_into_an_adopted_folder_only_on_the_explicit_ask() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(
        &dir,
        config_with_git(git_with_global(
            &dir,
            "Grace Hopper",
            "grace@example.invalid",
        )),
    );
    let (project, theirs) = adopted_plain_folder(&engine, &dir, "theirs").await;

    let done = projects::init_repository(engine.inner(), project.id, Vec::new())
        .await
        .expect("init_repository");
    assert!(theirs.join(".git").is_dir(), "the one write adopt allows");
    assert_eq!(
        std::fs::read_to_string(theirs.join("notes.txt")).unwrap(),
        "theirs\n"
    );
    assert!(done.project.vcs.is_git());
    assert!(
        matches!(done.project.root, ProjectRoot::External { .. }),
        "still their folder"
    );
    assert_eq!(
        raw_git(&theirs, &["log", "-1", "--format=%s"]),
        "project theirs initialised"
    );
    engine.shutdown().await;
}

/// Nobody to commit: the repository is made, the person is asked once, and no
/// commit is written — HEAD stays unborn, as a new managed project's does,
/// until who commits is set.
#[tokio::test(flavor = "multi_thread")]
async fn init_repository_with_nobody_to_commit_asks_and_makes_no_commit() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config_with_git(isolated_git(&dir)));
    let (_goal, project) = plain_project(&engine, "quiet");
    let mut rx = engine.events();

    let done = projects::init_repository(engine.inner(), project.id, Vec::new())
        .await
        .expect("init_repository");
    let root = engine.workspace().project_root_path(&done.project);
    assert!(root.join(".git").is_dir());
    assert!(matches!(done.committer, bisa_engine::Committer::Ask));
    assert!(
        bisa_vcs::git::status(&root).unwrap().oid.is_none(),
        "no root commit without an identity: HEAD stays unborn"
    );

    let mut asks = 0;
    loop {
        let ev = common::wait_for(&mut rx, "the initialisation's frames", |_| true).await;
        match ev.payload {
            EnginePayload::CommitterNeeded { project: p, .. } if p == project.id => asks += 1,
            EnginePayload::WorkstreamChanged { .. } => break,
            _ => {}
        }
    }
    assert_eq!(
        asks, 1,
        "asked once; the root commit's own refusal raises no second"
    );
    assert!(bisa_engine::identity::overview(engine.inner())
        .await
        .unwrap()
        .pending
        .iter()
        .any(|p| p.project == project.id));
    engine.shutdown().await;
}

/// A record that already says git is refused: the button is never drawn then,
/// so a second call is a double click, told rather than repeated.
#[tokio::test(flavor = "multi_thread")]
async fn init_repository_refuses_a_project_that_already_is_a_repository() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (_goal, project) = plain_project(&engine, "twice");

    projects::init_repository(engine.inner(), project.id, Vec::new())
        .await
        .expect("first");
    let refused = projects::init_repository(engine.inner(), project.id, Vec::new())
        .await
        .expect_err("second");
    assert!(
        matches!(&refused, EngineError::Conflict(words) if words.to_string().contains("already a repository")),
        "{refused}"
    );
    engine.shutdown().await;
}

/// A folder that grew a repository with history behind the record's back is
/// recorded, never committed over: HEAD is exactly what it was.
#[tokio::test(flavor = "multi_thread")]
async fn init_repository_never_commits_over_history_it_did_not_make() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(
        &dir,
        config_with_git(git_with_global(
            &dir,
            "Grace Hopper",
            "grace@example.invalid",
        )),
    );
    let (project, theirs) = adopted_plain_folder(&engine, &dir, "history").await;
    raw_git(&theirs, &["init", "--quiet"]);
    commit_without_identity(&theirs, "their first");
    let head_before = raw_git(&theirs, &["rev-parse", "HEAD"]);

    let done = projects::init_repository(engine.inner(), project.id, Vec::new())
        .await
        .expect("init_repository");
    assert!(done.project.vcs.is_git(), "recorded");
    assert_eq!(
        raw_git(&theirs, &["rev-parse", "HEAD"]),
        head_before,
        "not committed over"
    );
    engine.shutdown().await;
}

/// `clone` is its own explicit step too, and it refuses to write over a
/// folder that already has something in it.
///
/// The "remote" is a local bare repository addressed by its path, so the whole
/// clone path runs with no network anywhere near it.
#[tokio::test(flavor = "multi_thread")]
async fn cloning_is_explicit_and_refuses_a_non_empty_folder() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());

    // Something to clone from: a repository with one commit, pushed into a
    // bare repository that stands in for the code host.
    let (_goal, _source, source_root, origin) =
        git_project(&engine, "source", PublishPolicy::Auto).await;
    let default_branch = raw_git(&source_root, &["symbolic-ref", "--short", "HEAD"]);
    bisa_vcs::git::push(&source_root, "origin", &default_branch, true).unwrap();

    let (_goal, project) = plain_project(&engine, "cloned");
    let cloned = projects::clone(engine.inner(), &project, origin.to_str().unwrap(), Some(1))
        .await
        .expect("clone from a local bare repository");
    let root = engine.workspace().project_root_path(&cloned);
    assert!(root.join("README.md").exists());
    assert!(cloned.vcs.is_git());
    match &cloned.vcs {
        Vcs::Git {
            remote, code_host, ..
        } => {
            assert_eq!(remote.as_deref(), origin.to_str());
            // A bare origin on disk is no code host this build knows by name:
            // the record says so honestly rather than guessing a kind.
            assert_eq!(*code_host, None, "{code_host:?}");
        }
        other => panic!("expected a git project, got {other:?}"),
    }

    // A second clone into the same folder would destroy what is there.
    let err = projects::clone(engine.inner(), &cloned, origin.to_str().unwrap(), None)
        .await
        .refused("cloning over a populated folder must refuse");
    assert!(matches!(err, EngineError::Invalid(_)), "got {err}");

    engine.shutdown().await;
}

/// An external root that is not there is a refusal, not a silent mkdir
/// somewhere unexpected.
#[tokio::test(flavor = "multi_thread")]
async fn an_absent_external_root_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let ws = engine.workspace();
    let missing = dir.path().join("nowhere");
    let project = ws
        .create_project(NewProject {
            origin: bisa_core::ProjectOrigin::Workspace,
            root: ProjectRoot::External {
                path: missing.display().to_string(),
            },
            ..NewProject::managed("adopted").unwrap()
        })
        .unwrap();

    let err = projects::materialize(engine.inner(), &project)
        .await
        .refused("an absent external root cannot be materialized");
    assert!(matches!(err, EngineError::Invalid(_)), "got {err}");
    assert!(!missing.exists(), "and it certainly was not created");

    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// Workstreams
// ---------------------------------------------------------------------------

/// Two work items on one project, opened concurrently: two branches, two
/// independent trees, and a commit in one is invisible in the other.
#[tokio::test(flavor = "multi_thread")]
async fn two_workstreams_on_one_project_are_independent() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (goal, project, root, _origin) =
        git_project(&engine, "storefront", PublishPolicy::Gated).await;

    let a = item(goal, Some(&project), "Add the checkout flow");
    let b = item(goal, Some(&project), "Fix the cart total");
    let (pa, pb) = tokio::join!(
        projects::open_workstream(engine.inner(), &a, &project),
        projects::open_workstream(engine.inner(), &b, &project),
    );
    let (pa, pb) = (pa.expect("workstream a"), pb.expect("workstream b"));
    assert!(pa.is_worktree() && pb.is_worktree());
    let (wa, wb) = (pa.workstream.clone(), pb.workstream.clone());
    let checkout = |w: &bisa_core::Workstream| engine.workspace().workstream_checkout(w).unwrap();
    let (path_a, path_b) = (checkout(&wa), checkout(&wb));
    assert_eq!(path_a, pa.cwd);
    assert!(pa.iso.is_none(), "a git workstream *is* the isolation");
    assert_ne!(path_a, path_b);
    assert!(path_a.is_dir() && path_b.is_dir());

    let (branch_a, branch_b) = match (&wa.kind, &wb.kind) {
        (
            WorkstreamKind::Worktree { branch: a, .. },
            WorkstreamKind::Worktree { branch: b, .. },
        ) => (a.clone(), b.clone()),
        other => panic!("expected two worktree workstreams, got {other:?}"),
    };
    assert!(branch_a.starts_with("work/add-the-checkout-flow-"));
    assert!(branch_b.starts_with("work/fix-the-cart-total-"));

    // git agrees there are three working trees: the project and the two
    // workstreams, each on its own branch.
    let listed = bisa_vcs::git::worktree_list(&root).unwrap();
    assert_eq!(listed.len(), 3, "{listed:#?}");
    let branches: Vec<String> = listed.iter().filter_map(|w| w.branch.clone()).collect();
    assert!(branches.contains(&branch_a) && branches.contains(&branch_b));

    // The trees really are independent.
    std::fs::write(path_a.join("checkout.rs"), "fn checkout() {}\n").unwrap();
    let commit = projects::commit_workstream(engine.inner(), wa.id, "add checkout")
        .await
        .expect("commit a");
    assert!(!path_b.join("checkout.rs").exists(), "b never saw a's file");
    assert!(bisa_vcs::git::status(&path_b).unwrap().is_clean);
    assert_eq!(
        engine.workspace().get_workstream(wa.id).unwrap().state,
        WorkstreamState::Committed
    );
    assert_eq!(
        engine.workspace().get_workstream(wb.id).unwrap().state,
        WorkstreamState::Open
    );

    // The commit is a fact on the goal's journal, not just a git object.
    let journal = engine
        .workspace()
        .journal(&bisa_core::Home::from(goal))
        .unwrap();
    use bisa_core::event::JournalPayload as P;
    assert!(
        journal.iter().any(|e| matches!(&e.payload,
            P::Progress { verb, object, outcome, .. }
                if verb == "committed" && object == &branch_a
                    && outcome.as_deref() == Some(commit.short()))),
        "the commit must be journaled: {journal:#?}"
    );

    // Closing removes the trees and the registrations.
    projects::close_workstream(engine.inner(), wa.id, true)
        .await
        .unwrap();
    projects::close_workstream(engine.inner(), wb.id, true)
        .await
        .unwrap();
    assert!(!path_a.exists() && !path_b.exists());
    assert_eq!(bisa_vcs::git::worktree_list(&root).unwrap().len(), 1);
    assert_eq!(
        engine.workspace().get_workstream(wa.id).unwrap().state,
        WorkstreamState::Closed
    );

    engine.shutdown().await;
}

/// Re-running the same work item comes back to its own worktree instead of
/// fighting its own branch name.
#[tokio::test(flavor = "multi_thread")]
async fn a_re_run_reuses_the_items_workstream() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (goal, project, _root, _origin) =
        git_project(&engine, "storefront", PublishPolicy::Gated).await;

    let spec = item(goal, Some(&project), "Add the checkout flow");
    // The item exists before it is placed, as it does when the executor runs
    // it: the index links a workstream to a work item it knows.
    engine.workspace().put_work_item(&spec).unwrap();
    let first = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap()
        .workstream;
    let second = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap()
        .workstream;
    assert_eq!(first.id, second.id);
    assert_eq!(
        engine
            .workspace()
            .list_workstreams(WorkstreamFilter::WorkItem(spec.id))
            .unwrap()
            .len(),
        1
    );

    engine.shutdown().await;
}

/// A repository with no commits has nothing to branch from, so the item runs
/// in the project root — with the files that are actually there — rather than
/// in a worktree cut from a commit nobody made.
#[tokio::test(flavor = "multi_thread")]
async fn an_unborn_head_runs_in_the_project_root() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (goal, project) = plain_project(&engine, "fresh");
    let project = projects::init_git(engine.inner(), &project).await.unwrap();
    let root = engine.workspace().project_root_path(&project);
    // The files a user just dropped in, which a worktree from an empty commit
    // would have hidden.
    std::fs::write(root.join("draft.md"), "notes\n").unwrap();

    let spec = item(goal, Some(&project), "Start the thing");
    let place = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap();
    assert!(
        place.workstream.is_primary(),
        "an unborn HEAD runs in the primary workstream"
    );
    assert_eq!(place.workstream.id.to_string(), project.id.to_string());
    assert_eq!(place.cwd, root);
    assert!(place.cwd.join("draft.md").exists());
    assert!(
        raw_git(&root, &["log", "--oneline", "--all"]).is_empty(),
        "no commit was invented on the user's behalf"
    );
    // Nothing was minted: the only workstream is the primary the project was
    // born with.
    let all = engine
        .workspace()
        .list_workstreams(WorkstreamFilter::All)
        .unwrap();
    assert_eq!(all.len(), 1, "{all:#?}");
    assert!(all[0].is_primary());

    // One commit later, workstreams open normally: the condition heals itself.
    set_identity(&root);
    bisa_vcs::git::add_all(&root).unwrap();
    bisa_vcs::git::commit(&root, "first", false).unwrap();
    let place = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap();
    assert!(place.is_worktree());

    engine.shutdown().await;
}

/// A non-git project keeps the copy floor: an isolated tree, and the `.patch`
/// settlement path the executor already had.
#[tokio::test(flavor = "multi_thread")]
async fn a_non_git_project_gets_a_copy_workstream() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (goal, project) = plain_project(&engine, "notes");
    let root = projects::materialize(engine.inner(), &project)
        .await
        .unwrap();
    std::fs::write(root.join("outline.md"), "one\n").unwrap();

    let spec = item(goal, Some(&project), "Rewrite the outline");
    let place = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap();
    let w = place.workstream.clone();
    assert_eq!(w.kind, WorkstreamKind::Copy);
    assert!(!place.is_worktree());
    assert!(place.iso.is_some(), "a copy workstream rides the iso chain");
    assert_ne!(place.cwd, root);
    assert_eq!(
        std::fs::read_to_string(place.cwd.join("outline.md")).unwrap(),
        "one\n"
    );

    // Writing in the copy leaves the project untouched.
    std::fs::write(place.cwd.join("outline.md"), "two\n").unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join("outline.md")).unwrap(),
        "one\n"
    );

    // There is no branch to commit, and saying so is a typed refusal.
    let err = projects::commit_workstream(engine.inner(), w.id, "nope")
        .await
        .refused("a copy is not a git worktree");
    assert!(matches!(err, EngineError::Invalid(_)), "got {err}");

    projects::close_workstream(engine.inner(), w.id, true)
        .await
        .unwrap();
    assert!(!engine.workspace().workstream_checkout(&w).unwrap().exists());

    engine.shutdown().await;
}

/// A clean tree is the ordinary outcome of a work item that only read, so it
/// is a named refusal rather than an empty commit.
#[tokio::test(flavor = "multi_thread")]
async fn committing_a_clean_workstream_is_refused_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (goal, project, _root, _origin) =
        git_project(&engine, "storefront", PublishPolicy::Gated).await;
    let spec = item(goal, Some(&project), "Read the code");
    let w = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap()
        .workstream;

    let err = projects::commit_workstream(engine.inner(), w.id, "nothing")
        .await
        .refused("a clean tree has nothing to commit");
    assert!(matches!(err, EngineError::NothingToCommit(_)), "got {err}");
    assert_eq!(
        engine.workspace().get_workstream(w.id).unwrap().state,
        WorkstreamState::Open
    );

    engine.shutdown().await;
}

/// A pull request's draft is asked from what the branch carries beyond its
/// base — its commits and its diff — and a branch with nothing beyond it is
/// refused as opening the pull request would be, before anyone is asked. The
/// core agent answers on the mock, which echoes the prompt it was given.
#[tokio::test(flavor = "multi_thread")]
async fn a_pull_request_draft_is_asked_from_the_branchs_commits_and_diff() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let mut general = engine
        .workspace()
        .get_agent(&bisa_core::AgentId::general())
        .unwrap();
    general.harness = MockAdapter::default().id;
    engine.workspace().update_agent(general).unwrap();
    let (goal, project, _root, _origin) = git_project(&engine, "cart", PublishPolicy::Gated).await;
    let spec = item(goal, Some(&project), "Add the cart total");
    let w = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap()
        .workstream;

    let err = projects::suggest_pull_request(engine.inner(), w.id)
        .await
        .refused("a branch with nothing beyond its base has nothing to describe");
    assert!(
        matches!(err, EngineError::NothingToPublish { .. }),
        "got {err}"
    );

    let path = engine.workspace().workstream_checkout(&w).unwrap();
    std::fs::write(path.join("cart.txt"), "total\n").unwrap();
    projects::commit_workstream(engine.inner(), w.id, "Add the cart total")
        .await
        .unwrap();
    let draft = projects::suggest_pull_request(engine.inner(), w.id)
        .await
        .expect("a draft");
    let said = format!("{}\n{}", draft.title, draft.body);
    assert!(!draft.title.is_empty(), "the first line is the title");
    assert!(
        said.contains("- Add the cart total"),
        "the branch's commits: {said}"
    );
    assert!(said.contains("+total"), "its diff against the base: {said}");
    assert!(
        !said.contains("- baseline"),
        "the base's own history is not the branch's: {said}"
    );
    // It suggests: nothing was pushed, nothing opened, the record did not move.
    assert_eq!(
        engine.workspace().get_workstream(w.id).unwrap().state,
        WorkstreamState::Committed
    );

    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// The Publish gate
// ---------------------------------------------------------------------------

/// Nothing leaves the machine until a human says so.
#[tokio::test(flavor = "multi_thread")]
async fn a_push_waits_for_the_publish_gate() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(engine_with(&dir, config()));
    let (goal, project, _root, origin) =
        git_project(&engine, "storefront", PublishPolicy::Gated).await;

    let spec = item(goal, Some(&project), "Add the checkout flow");
    let w = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap()
        .workstream;
    let path = engine.workspace().workstream_checkout(&w).unwrap();
    std::fs::write(path.join("checkout.rs"), "fn checkout() {}\n").unwrap();
    projects::commit_workstream(engine.inner(), w.id, "add checkout")
        .await
        .unwrap();

    let pushing = tokio::spawn({
        let (engine, id) = (Arc::clone(&engine), w.id);
        async move { projects::push_workstream(engine.inner(), id).await }
    });

    let gate = pending_publish_gate(&engine).await;
    assert!(
        origin_branches(&origin).is_empty(),
        "the gate is open and nothing has been pushed"
    );

    engine
        .decide(&gate, true, Some("ship it"), None, None)
        .unwrap();
    pushing.await.unwrap().expect("push after approval");

    let branch = w.branch().expect("a worktree has a branch").to_string();
    assert_eq!(origin_branches(&origin), vec![branch.clone()]);
    assert_eq!(
        engine.workspace().get_workstream(w.id).unwrap().state,
        WorkstreamState::Pushed
    );
    use bisa_core::event::JournalPayload as P;
    assert!(engine
        .workspace()
        .journal(&bisa_core::Home::from(goal))
        .unwrap()
        .iter()
        .any(|e| matches!(&e.payload, P::Progress { verb, object, .. }
            if verb == "pushed" && object == &branch)));

    Arc::try_unwrap(engine).ok().unwrap().shutdown().await;
}

/// A declined gate is a refusal with a name, and the branch stays home.
#[tokio::test(flavor = "multi_thread")]
async fn a_declined_publish_gate_pushes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(engine_with(&dir, config()));
    let (goal, project, _root, origin) =
        git_project(&engine, "storefront", PublishPolicy::Gated).await;

    let spec = item(goal, Some(&project), "Add the checkout flow");
    let w = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap()
        .workstream;
    let path = engine.workspace().workstream_checkout(&w).unwrap();
    std::fs::write(path.join("checkout.rs"), "fn checkout() {}\n").unwrap();
    projects::commit_workstream(engine.inner(), w.id, "add checkout")
        .await
        .unwrap();

    let pushing = tokio::spawn({
        let (engine, id) = (Arc::clone(&engine), w.id);
        async move { projects::push_workstream(engine.inner(), id).await }
    });
    let gate = pending_publish_gate(&engine).await;
    engine
        .decide(&gate, false, Some("not yet"), None, None)
        .unwrap();

    let err = pushing.await.unwrap().refused("a declined gate refuses");
    assert!(
        matches!(err, EngineError::PublishDeclined { .. }),
        "got {err}"
    );
    assert!(origin_branches(&origin).is_empty());
    assert_eq!(
        engine.workspace().get_workstream(w.id).unwrap().state,
        WorkstreamState::Committed
    );

    Arc::try_unwrap(engine).ok().unwrap().shutdown().await;
}

/// `Manual` refuses outright — it never even opens a gate, because there is no
/// answer that would let the platform push.
#[tokio::test(flavor = "multi_thread")]
async fn manual_publishing_refuses_without_opening_a_gate() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (goal, project, _root, origin) =
        git_project(&engine, "storefront", PublishPolicy::Manual).await;

    let spec = item(goal, Some(&project), "Add the checkout flow");
    let w = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap()
        .workstream;
    let path = engine.workspace().workstream_checkout(&w).unwrap();
    std::fs::write(path.join("checkout.rs"), "fn checkout() {}\n").unwrap();
    projects::commit_workstream(engine.inner(), w.id, "add checkout")
        .await
        .unwrap();

    let err = projects::push_workstream(engine.inner(), w.id)
        .await
        .refused("manual publishing refuses");
    assert!(
        matches!(err, EngineError::PublishManual { .. }),
        "got {err}"
    );
    assert!(engine.inbox().is_empty(), "no gate was opened");
    assert!(origin_branches(&origin).is_empty());

    engine.shutdown().await;
}

/// `Auto` is the opt-in that skips the gate entirely.
#[tokio::test(flavor = "multi_thread")]
async fn auto_publishing_skips_the_gate() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (goal, project, _root, origin) =
        git_project(&engine, "storefront", PublishPolicy::Auto).await;

    let spec = item(goal, Some(&project), "Add the checkout flow");
    let w = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap()
        .workstream;
    let path = engine.workspace().workstream_checkout(&w).unwrap();
    std::fs::write(path.join("checkout.rs"), "fn checkout() {}\n").unwrap();
    projects::commit_workstream(engine.inner(), w.id, "add checkout")
        .await
        .unwrap();

    projects::push_workstream(engine.inner(), w.id)
        .await
        .expect("auto publishing needs no gate");
    assert!(engine.inbox().is_empty());
    assert_eq!(origin_branches(&origin).len(), 1);

    engine.shutdown().await;
}

/// The record moves only when this engine commits; a commit made in a
/// terminal leaves it at `open`. A push reconciles the record with the
/// checkout first, so the push goes out and is recorded — instead of running
/// `git push` and then being refused by the table.
#[tokio::test(flavor = "multi_thread")]
async fn a_commit_made_in_a_terminal_is_pushed_and_recorded() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (goal, project, _root, origin) =
        git_project(&engine, "storefront", PublishPolicy::Auto).await;

    let spec = item(goal, Some(&project), "Add the checkout flow");
    let w = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap()
        .workstream;
    assert_eq!(w.state, WorkstreamState::Open);
    let path = engine.workspace().workstream_checkout(&w).unwrap();
    std::fs::write(path.join("checkout.rs"), "fn checkout() {}\n").unwrap();
    raw_git(&path, &["add", "-A"]);
    raw_git(&path, &["commit", "--quiet", "-m", "add checkout, by hand"]);

    projects::push_workstream(engine.inner(), w.id)
        .await
        .expect("the record catches up with the checkout before the push");
    assert_eq!(
        engine.workspace().get_workstream(w.id).unwrap().state,
        WorkstreamState::Pushed
    );
    assert_eq!(origin_branches(&origin).len(), 1);

    engine.shutdown().await;
}

/// A gated project whose workstream belongs to no goal has nobody to ask on
/// the bus: the refusal names that — not the manual policy — so a person can
/// attach the project to a goal instead of hunting for a setting.
#[tokio::test(flavor = "multi_thread")]
async fn a_gated_project_names_the_missing_goal_rather_than_claiming_manual() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (_goal, project, _root, origin) =
        git_project(&engine, "storefront", PublishPolicy::Gated).await;

    let by_hand = projects::WorkstreamRequest {
        goal: None,
        work_item: None,
        label: "hand".to_string(),
        agent: None,
        source: WorkstreamSource::default(),
        base: None,
    };
    let w = projects::open_workstream_for(engine.inner(), &project, by_hand)
        .await
        .unwrap()
        .workstream;
    let path = engine.workspace().workstream_checkout(&w).unwrap();
    std::fs::write(path.join("hand.rs"), "fn hand() {}\n").unwrap();
    projects::commit_workstream(engine.inner(), w.id, "by hand")
        .await
        .unwrap();

    let err = projects::push_workstream(engine.inner(), w.id)
        .await
        .refused("nobody to ask");
    assert!(
        matches!(err, EngineError::PublishNoGoal { .. }),
        "got {err}"
    );
    assert!(engine.inbox().is_empty(), "no gate was opened");
    assert!(origin_branches(&origin).is_empty());

    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// Pull requests, against the in-memory code host
// ---------------------------------------------------------------------------

/// The gate stands in front of the code host, not behind it: a declined gate means
/// the code host is never asked at all.
#[tokio::test(flavor = "multi_thread")]
async fn opening_a_pull_request_passes_the_publish_gate_first() {
    let dir = tempfile::tempdir().unwrap();
    // An in-memory code host that claims the test's bare `origin` by name, so the
    // push is real and the pull request never leaves the process.
    let fake = Arc::new(bisa_codehost::fake::FakeCodeHost::minimal(
        "storefront-origin",
    ));
    let engine = Arc::new(engine_with(
        &dir,
        EngineConfig {
            code_hosts: Some(bisa_codehost::CodeHostRegistry::new(vec![fake.clone()])),
            ..config()
        },
    ));
    let (goal, project, _root, origin) =
        git_project(&engine, "storefront", PublishPolicy::Gated).await;
    let repo = fake
        .detect(&bisa_codehost::RemoteUrl::parse(origin.to_str().unwrap()))
        .expect("the fake claims the bare origin");

    let spec = item(goal, Some(&project), "Add the checkout flow");
    let w = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap()
        .workstream;
    let branch = w.branch().expect("a worktree has a branch").to_string();
    // A pull request is opened on pushed work: the executor commits and pushes
    // before it ever gets here, so the record is walked to that state.
    for step in [
        WorkstreamTransition::Committed,
        WorkstreamTransition::Pushed,
    ] {
        engine
            .workspace()
            .transition_workstream(w.id, &step)
            .unwrap();
    }

    // Declined: the code host is never reached.
    let opening = tokio::spawn({
        let (engine, id) = (Arc::clone(&engine), w.id);
        async move { projects::open_pr(engine.inner(), id, "Add checkout", "Closes #4").await }
    });
    let gate = pending_publish_gate(&engine).await;
    engine.decide(&gate, false, None, None, None).unwrap();
    let err = opening.await.unwrap().refused("declined");
    assert!(
        matches!(err, EngineError::PublishDeclined { .. }),
        "got {err}"
    );
    assert!(
        fake.list_prs(&repo, Default::default())
            .await
            .unwrap()
            .is_empty(),
        "the code host must not have been asked"
    );

    // Approved: the pull request exists on the code host and is recorded.
    let opening = tokio::spawn({
        let (engine, id) = (Arc::clone(&engine), w.id);
        async move { projects::open_pr(engine.inner(), id, "Add checkout", "Closes #4").await }
    });
    let gate = pending_publish_gate(&engine).await;
    engine.decide(&gate, true, None, None, None).unwrap();
    let pr = opening.await.unwrap().expect("pr created");
    let listed = fake.list_prs(&repo, Default::default()).await.unwrap();
    assert_eq!(
        listed.len(),
        1,
        "one pull request, on the fake: {listed:#?}"
    );
    assert_eq!(listed[0].number, pr.number);
    assert_eq!(listed[0].head, branch);

    assert_eq!(
        engine.workspace().get_workstream(w.id).unwrap().state,
        WorkstreamState::PrOpen {
            number: pr.number,
            url: pr.url.clone(),
        }
    );
    use bisa_core::event::JournalPayload as P;
    assert!(engine
        .workspace()
        .journal(&bisa_core::Home::from(goal))
        .unwrap()
        .iter()
        .any(
            |e| matches!(&e.payload, P::Progress { verb, object, outcome, .. }
            if verb == "opened pr" && object == &branch
                && outcome.as_deref() == Some(pr.url.as_str()))
        ));

    Arc::try_unwrap(engine).ok().unwrap().shutdown().await;
}

/// One credential opens a pull request and reviews it, so the reviewer is the
/// author: the engine refuses an approval or a change request on the
/// platform's own pull request by name, before the code host is asked, and
/// takes a comment. On somebody else's pull request the verdict goes through.
#[tokio::test(flavor = "multi_thread")]
async fn a_verdict_on_the_platforms_own_pull_request_is_refused_and_a_comment_taken() {
    use bisa_codehost::{Review, ReviewEvent};
    use bisa_engine::codehost::Reviewer;
    let dir = tempfile::tempdir().unwrap();
    let fake = Arc::new(bisa_codehost::fake::FakeCodeHost::with_capabilities(
        "storefront-origin",
        bisa_codehost::CodeHostCapabilities {
            review_threads: true,
            ..Default::default()
        },
    ));
    let engine = Arc::new(engine_with(
        &dir,
        EngineConfig {
            code_hosts: Some(bisa_codehost::CodeHostRegistry::new(vec![fake.clone()])),
            ..config()
        },
    ));
    let (goal, project, _root, origin) =
        git_project(&engine, "storefront", PublishPolicy::Auto).await;
    let repo = fake
        .detect(&bisa_codehost::RemoteUrl::parse(origin.to_str().unwrap()))
        .expect("the fake claims the bare origin");
    let spec = item(goal, Some(&project), "Add the checkout flow");
    let w = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap()
        .workstream;
    for step in [
        WorkstreamTransition::Committed,
        WorkstreamTransition::Pushed,
    ] {
        engine
            .workspace()
            .transition_workstream(w.id, &step)
            .unwrap();
    }
    let pr = projects::open_pr(engine.inner(), w.id, "Add checkout", "Closes #4")
        .await
        .expect("pr created");
    assert_eq!(
        pr.author.as_deref(),
        Some(bisa_codehost::fake::DEFAULT_LOGIN),
        "the platform's account opened it"
    );

    let approve = bisa_engine::codehost::review(
        engine.inner(),
        w.id,
        Review {
            event: ReviewEvent::Approve,
            body: None,
            comments: vec![],
        },
        Reviewer::Person,
    )
    .await
    .refused("an approval of your own pull request");
    assert!(
        matches!(&approve, EngineError::OwnPullRequest { author } if author == "you"),
        "got {approve}"
    );
    assert!(approve.to_string().contains("ask a reviewer"), "{approve}");
    assert!(
        fake.state.reviews.lock().unwrap().is_empty(),
        "the code host was never asked to take the verdict"
    );

    bisa_engine::codehost::review(
        engine.inner(),
        w.id,
        Review {
            event: ReviewEvent::Comment,
            body: Some("looks right to me".into()),
            comments: vec![],
        },
        Reviewer::Person,
    )
    .await
    .expect("a comment from the author is taken");
    let taken = bisa_engine::codehost::pr_reviews(engine.inner(), w.id)
        .await
        .unwrap();
    assert_eq!(taken.reviews.len(), 1);
    assert_eq!(taken.reviews[0].state, "commented");

    // Somebody else's pull request takes a verdict from this account.
    let theirs = fake.seed_pr(&repo, "alice", "feature/theirs", "main");
    let spec = item(goal, Some(&project), "Review Alice's work");
    let other = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap()
        .workstream;
    for step in [
        WorkstreamTransition::Committed,
        WorkstreamTransition::Pushed,
        WorkstreamTransition::PrOpened {
            number: theirs.number,
            url: theirs.url.clone(),
        },
    ] {
        engine
            .workspace()
            .transition_workstream(other.id, &step)
            .unwrap();
    }
    bisa_engine::codehost::review(
        engine.inner(),
        other.id,
        Review {
            event: ReviewEvent::Approve,
            body: None,
            comments: vec![],
        },
        Reviewer::Person,
    )
    .await
    .expect("a verdict on another author's pull request");
    assert_eq!(fake.state.reviews.lock().unwrap().len(), 2);

    Arc::try_unwrap(engine).ok().unwrap().shutdown().await;
}

/// One credential posts every review, so the code host cannot tell an agent's
/// from the person's — the engine signs an agent's: the first line is the
/// mark and the agent's id, a blank line, then the agent's words; an agent's
/// wordless approval is the mark alone, so the attribution never drops. A
/// person's review reaches the code host untouched.
#[tokio::test(flavor = "multi_thread")]
async fn an_agents_review_is_signed_with_its_id_and_a_persons_is_not() {
    use bisa_codehost::{Review, ReviewEvent};
    use bisa_engine::codehost::{Reviewer, AGENT_REVIEW_MARK};
    let dir = tempfile::tempdir().unwrap();
    let fake = Arc::new(bisa_codehost::fake::FakeCodeHost::with_capabilities(
        "storefront-origin",
        bisa_codehost::CodeHostCapabilities {
            review_threads: true,
            ..Default::default()
        },
    ));
    let engine = Arc::new(engine_with(
        &dir,
        EngineConfig {
            code_hosts: Some(bisa_codehost::CodeHostRegistry::new(vec![fake.clone()])),
            ..config()
        },
    ));
    let (goal, project, _root, origin) =
        git_project(&engine, "storefront", PublishPolicy::Auto).await;
    let repo = fake
        .detect(&bisa_codehost::RemoteUrl::parse(origin.to_str().unwrap()))
        .expect("the fake claims the bare origin");
    let spec = item(goal, Some(&project), "Add the checkout flow");
    let w = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap()
        .workstream;
    for step in [
        WorkstreamTransition::Committed,
        WorkstreamTransition::Pushed,
    ] {
        engine
            .workspace()
            .transition_workstream(w.id, &step)
            .unwrap();
    }
    projects::open_pr(engine.inner(), w.id, "Add checkout", "Closes #4")
        .await
        .expect("pr created");

    // The agent's comment: signed, its words after a blank line.
    bisa_engine::codehost::review(
        engine.inner(),
        w.id,
        Review {
            event: ReviewEvent::Comment,
            body: Some("Looks good. One nit in cart.rs:12.".into()),
            comments: vec![],
        },
        Reviewer::Agent("general-agent".into()),
    )
    .await
    .expect("an agent's comment is taken");
    // The person's comment: exactly their words.
    bisa_engine::codehost::review(
        engine.inner(),
        w.id,
        Review {
            event: ReviewEvent::Comment,
            body: Some("Agreed, merging.".into()),
            comments: vec![],
        },
        Reviewer::Person,
    )
    .await
    .expect("the person's comment is taken");
    {
        let taken = fake.state.reviews.lock().unwrap();
        assert_eq!(taken.len(), 2);
        assert_eq!(
            taken[0].1.body.as_deref(),
            Some(&*format!(
                "{AGENT_REVIEW_MARK}general-agent\n\nLooks good. One nit in cart.rs:12."
            )),
            "the agent's review opens with the mark and its id"
        );
        assert_eq!(
            taken[1].1.body.as_deref(),
            Some("Agreed, merging."),
            "a person's review is untouched"
        );
    }
    // What the IDE reads back carries the mark, so `reviewerOf` can tell the two apart.
    let read = bisa_engine::codehost::pr_reviews(engine.inner(), w.id)
        .await
        .unwrap();
    assert!(
        read.reviews[0].body.starts_with(AGENT_REVIEW_MARK),
        "{}",
        read.reviews[0].body
    );
    assert!(!read.reviews[1].body.starts_with(AGENT_REVIEW_MARK));

    // An agent's wordless approval on somebody else's pull request: the mark alone.
    let theirs = fake.seed_pr(&repo, "alice", "feature/theirs", "main");
    let spec = item(goal, Some(&project), "Review Alice's work");
    let other = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap()
        .workstream;
    for step in [
        WorkstreamTransition::Committed,
        WorkstreamTransition::Pushed,
        WorkstreamTransition::PrOpened {
            number: theirs.number,
            url: theirs.url.clone(),
        },
    ] {
        engine
            .workspace()
            .transition_workstream(other.id, &step)
            .unwrap();
    }
    bisa_engine::codehost::review(
        engine.inner(),
        other.id,
        Review {
            event: ReviewEvent::Approve,
            body: None,
            comments: vec![],
        },
        Reviewer::Agent("reviewer-agent".into()),
    )
    .await
    .expect("an agent's approval of another author's pull request");
    {
        let taken = fake.state.reviews.lock().unwrap();
        assert_eq!(
            taken[2].1.body.as_deref(),
            Some(&*format!("{AGENT_REVIEW_MARK}reviewer-agent"))
        );
    }

    Arc::try_unwrap(engine).ok().unwrap().shutdown().await;
}

/// Closing a workstream stops what stands in it first — a harness a person
/// opened in its terminal is ended on the roster (the desktop closes the tab
/// on that frame) — and says how many; the primary's sessions are left
/// alone, the primary itself is refused before anything is stopped, and a
/// second close stops nothing.
#[tokio::test(flavor = "multi_thread")]
async fn closing_a_workstream_stops_what_stands_in_it_and_the_primary_is_left_alone() {
    use bisa_engine::interactive::OpenInteractive;
    use bisa_engine::SessionState;
    use bisa_harness::{InteractiveLaunch, ReportingPlan};
    use bisa_store::FileScope;
    let dir = tempfile::tempdir().unwrap();
    let mock = MockAdapter {
        id: "mock".into(),
        interactive: Some(InteractiveLaunch::new("mock")),
        reporting: ReportingPlan {
            args: vec!["--report".into()],
            ..Default::default()
        },
        ..Default::default()
    };
    let engine =
        Engine::start(workspace(&dir), common::catalog_with(vec![mock]), config()).unwrap();
    let (goal, project, _root, _origin) =
        git_project(&engine, "storefront", PublishPolicy::Auto).await;
    let spec = item(goal, Some(&project), "Add the checkout flow");
    let w = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap()
        .workstream;
    let open = |scope_id: String| {
        engine
            .inner()
            .interactive
            .open(
                engine.inner(),
                OpenInteractive {
                    scope: FileScope::Workstream,
                    id: scope_id,
                    harness: "mock".into(),
                },
            )
            .expect("the mock has an interactive form")
            .expect("the mock has a plan")
    };
    let primary = bisa_core::WorkstreamId::primary_of(project.id);
    let in_checkout = open(w.id.to_string()).session;
    let on_primary = open(primary.to_string()).session;
    let state = |session| engine.inner().presence.get(session).map(|p| p.state);
    assert!(state(in_checkout).is_some_and(|s| s.is_live()));

    // The primary is refused before anything is stopped.
    let refused = projects::close_workstream(engine.inner(), primary, false)
        .await
        .unwrap_err();
    assert!(refused.to_string().contains("primary"), "{refused}");
    assert!(
        state(on_primary).is_some_and(|s| s.is_live()),
        "a refused close stops nothing"
    );

    let closed = projects::close_workstream(engine.inner(), w.id, false)
        .await
        .unwrap();
    assert_eq!(
        closed.stopped_sessions, 1,
        "the harness standing in the checkout"
    );
    assert!(
        closed.workstream.state.is_terminal(),
        "the record is closed"
    );
    assert_eq!(
        state(in_checkout),
        Some(SessionState::Aborted),
        "the harness's row is ended on the roster"
    );
    assert!(
        state(on_primary).is_some_and(|s| s.is_live()),
        "the primary's harness is left alone"
    );

    let again = projects::close_workstream(engine.inner(), w.id, false)
        .await
        .unwrap();
    assert_eq!(again.stopped_sessions, 0, "a second close stops nothing");

    engine.shutdown().await;
}

/// A reply on a review thread goes as the one credential too, so an agent's
/// is signed with its own mark and a person's is untouched; `resolve` folds
/// the resolution into the same act; empty words are refused before the code
/// host is asked. The intake ops behind `pr_thread_reply` and
/// `pr_thread_resolve` are the same doors, off a workstream refused by name.
#[tokio::test(flavor = "multi_thread")]
async fn an_agents_reply_on_a_thread_is_signed_and_may_resolve_it_in_the_same_act() {
    use bisa_codehost::{ReviewThread, ReviewThreadComment};
    use bisa_engine::codehost::{Reviewer, AGENT_REPLY_MARK};
    let dir = tempfile::tempdir().unwrap();
    let fake = Arc::new(bisa_codehost::fake::FakeCodeHost::with_capabilities(
        "storefront-origin",
        bisa_codehost::CodeHostCapabilities {
            review_threads: true,
            review_thread_replies: true,
            ..Default::default()
        },
    ));
    let engine = Arc::new(engine_with(
        &dir,
        EngineConfig {
            code_hosts: Some(bisa_codehost::CodeHostRegistry::new(vec![fake.clone()])),
            ..config()
        },
    ));
    let (goal, project, _root, _origin) =
        git_project(&engine, "storefront", PublishPolicy::Auto).await;
    let spec = item(goal, Some(&project), "Add the checkout flow");
    let w = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap()
        .workstream;
    for step in [
        WorkstreamTransition::Committed,
        WorkstreamTransition::Pushed,
    ] {
        engine
            .workspace()
            .transition_workstream(w.id, &step)
            .unwrap();
    }
    projects::open_pr(engine.inner(), w.id, "Add checkout", "Closes #4")
        .await
        .expect("pr created");
    for id in ["T1", "T2", "T3"] {
        fake.state.threads.lock().unwrap().push(ReviewThread {
            id: id.into(),
            path: Some("src/cart.rs".into()),
            line: Some(12),
            is_resolved: false,
            is_outdated: false,
            comments: vec![ReviewThreadComment {
                author: Some("alice".into()),
                body: "This rounds twice.".into(),
                created_at: None,
            }],
        });
    }

    // The agent's reply, resolving in the same act: signed, then resolved.
    bisa_engine::codehost::reply_review_thread(
        engine.inner(),
        w.id,
        "T1",
        "Fixed in abc123 — one rounding now.",
        true,
        Reviewer::Agent("fixer".into()),
    )
    .await
    .expect("an agent's reply is taken");
    // The person's reply, leaving the thread open: their words exactly.
    bisa_engine::codehost::reply_review_thread(
        engine.inner(),
        w.id,
        "T2",
        "  Will do after lunch. ",
        false,
        Reviewer::Person,
    )
    .await
    .expect("the person's reply is taken");
    // No words: refused before the host is asked.
    let empty = bisa_engine::codehost::reply_review_thread(
        engine.inner(),
        w.id,
        "T3",
        "   ",
        true,
        Reviewer::Person,
    )
    .await
    .unwrap_err();
    assert!(matches!(empty, EngineError::Invalid(_)), "{empty}");
    {
        let threads = fake.state.threads.lock().unwrap();
        assert_eq!(
            threads[0].comments[1].body,
            format!("{AGENT_REPLY_MARK}fixer\n\nFixed in abc123 — one rounding now."),
            "the agent's reply opens with the mark and its id"
        );
        assert!(threads[0].is_resolved, "resolved in the same act");
        assert_eq!(
            threads[1].comments[1].body, "Will do after lunch.",
            "a person's reply is untouched, trimmed"
        );
        assert!(
            !threads[1].is_resolved,
            "a reply without `resolve` leaves the thread open"
        );
        assert_eq!(
            threads[2].comments.len(),
            1,
            "nothing reached the host for empty words"
        );
        assert!(!threads[2].is_resolved);
    }
    // What the IDE reads back carries the mark, so `replierOf` can tell the two apart.
    let read = bisa_engine::codehost::pr_reviews(engine.inner(), w.id)
        .await
        .unwrap();
    assert!(read.threads[0].comments[1]
        .body
        .starts_with(AGENT_REPLY_MARK));
    assert!(!read.threads[1].comments[1]
        .body
        .starts_with(AGENT_REPLY_MARK));

    // The same doors from a harness session, through the intake socket: the
    // session is a turn of a conversation about the workstream.
    let conversation = engine
        .workspace()
        .create_conversation(bisa_store::NewConversation {
            origin: bisa_core::ConversationOrigin::Workstream {
                id: w.id,
                project: project.id,
            },
            title: None,
            mode: bisa_core::ConversationMode::Auto,
        })
        .unwrap();
    let scope = conversation.id.to_string();
    let replied = common::intake_roundtrip(
        engine.socket_path(),
        serde_json::json!({"op": "pr_thread_reply", "scope": scope, "agent": "fixer", "thread": "T3", "body": "Left as is: the second rounding is the tax line's.", "resolve": false}),
    )
    .await;
    assert_eq!(replied["ok"], true, "{replied}");
    assert_eq!(replied["resolved"], false);
    let resolved = common::intake_roundtrip(
        engine.socket_path(),
        serde_json::json!({"op": "pr_thread_resolve", "scope": scope, "thread": "T3"}),
    )
    .await;
    assert_eq!(resolved["ok"], true, "{resolved}");
    assert_eq!(
        resolved["resolved"], true,
        "`resolved` unsaid means resolve"
    );
    {
        let threads = fake.state.threads.lock().unwrap();
        assert!(
            threads[2].comments[1]
                .body
                .starts_with(&format!("{AGENT_REPLY_MARK}fixer")),
            "an intake reply is always an agent's, signed"
        );
        assert!(threads[2].is_resolved);
    }
    let reopened = common::intake_roundtrip(
        engine.socket_path(),
        serde_json::json!({"op": "pr_thread_resolve", "scope": scope, "thread": "T3", "resolved": false}),
    )
    .await;
    assert_eq!(reopened["ok"], true, "{reopened}");
    assert!(!fake.state.threads.lock().unwrap()[2].is_resolved);
    // Off a workstream chat — a goal's scope — the tool refuses by name.
    let off = common::intake_roundtrip(
        engine.socket_path(),
        serde_json::json!({"op": "pr_thread_reply", "scope": goal.to_string(), "agent": "fixer", "thread": "T1", "body": "x"}),
    )
    .await;
    assert_eq!(off["ok"], false, "{off}");
    assert!(
        off.to_string()
            .contains("a session of a conversation about a workstream or a project"),
        "{off}"
    );

    Arc::try_unwrap(engine).ok().unwrap().shutdown().await;
}

/// A pull request is opened on pushed work — so a branch that is not on the
/// remote yet is pushed first, under **one** gate that says so. Declined:
/// nothing on the origin, nothing on the code host. Approved: both.
#[tokio::test(flavor = "multi_thread")]
async fn opening_a_pull_request_on_an_unpushed_branch_pushes_first_under_one_gate() {
    let dir = tempfile::tempdir().unwrap();
    let fake = Arc::new(bisa_codehost::fake::FakeCodeHost::minimal(
        "storefront-origin",
    ));
    let engine = Arc::new(engine_with(
        &dir,
        EngineConfig {
            code_hosts: Some(bisa_codehost::CodeHostRegistry::new(vec![fake.clone()])),
            ..config()
        },
    ));
    let (goal, project, _root, origin) =
        git_project(&engine, "storefront", PublishPolicy::Gated).await;
    let repo = fake
        .detect(&bisa_codehost::RemoteUrl::parse(origin.to_str().unwrap()))
        .expect("the fake claims the bare origin");

    let spec = item(goal, Some(&project), "Add the checkout flow");
    let w = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap()
        .workstream;
    let branch = w.branch().expect("a worktree has a branch").to_string();
    let path = engine.workspace().workstream_checkout(&w).unwrap();
    std::fs::write(path.join("checkout.rs"), "fn checkout() {}\n").unwrap();
    projects::commit_workstream(engine.inner(), w.id, "add checkout")
        .await
        .unwrap();
    assert_eq!(
        engine.workspace().get_workstream(w.id).unwrap().state,
        WorkstreamState::Committed,
        "committed, never pushed"
    );

    // Declined: one gate, naming both acts; nothing left the machine.
    let opening = tokio::spawn({
        let (engine, id) = (Arc::clone(&engine), w.id);
        async move { projects::open_pr(engine.inner(), id, "Add checkout", "Closes #4").await }
    });
    let gate = pending_publish_gate(&engine).await;
    let pending: Vec<_> = engine
        .inbox()
        .into_iter()
        .filter(|g| g.gate == Gate::Publish)
        .collect();
    assert_eq!(
        pending.len(),
        1,
        "one gate for push and pull request together"
    );
    assert!(
        pending[0]
            .question
            .contains(&format!("push {branch} and open a pull request")),
        "the question says what one decision covers: {}",
        pending[0].question
    );
    engine.decide(&gate, false, None, None, None).unwrap();
    let err = opening.await.unwrap().refused("declined");
    assert!(
        matches!(err, EngineError::PublishDeclined { .. }),
        "got {err}"
    );
    assert!(
        origin_branches(&origin).is_empty(),
        "declined: no push either"
    );
    assert!(fake
        .list_prs(&repo, Default::default())
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        engine.workspace().get_workstream(w.id).unwrap().state,
        WorkstreamState::Committed
    );

    // Approved: the branch is on the origin and the pull request on the code host.
    let opening = tokio::spawn({
        let (engine, id) = (Arc::clone(&engine), w.id);
        async move { projects::open_pr(engine.inner(), id, "Add checkout", "Closes #4").await }
    });
    let gate = pending_publish_gate(&engine).await;
    engine.decide(&gate, true, None, None, None).unwrap();
    let pr = opening.await.unwrap().expect("pushed, then opened");
    assert_eq!(origin_branches(&origin), vec![branch.clone()]);
    let listed = fake.list_prs(&repo, Default::default()).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].head, branch);
    assert_eq!(
        engine.workspace().get_workstream(w.id).unwrap().state,
        WorkstreamState::PrOpen {
            number: pr.number,
            url: pr.url.clone(),
        }
    );
    use bisa_core::event::JournalPayload as P;
    let verbs: Vec<String> = engine
        .workspace()
        .journal(&bisa_core::Home::from(goal))
        .unwrap()
        .iter()
        .filter_map(|e| match &e.payload {
            P::Progress { verb, object, .. } if object == &branch => Some(verb.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        verbs,
        vec!["committed", "pushed", "opened pr"],
        "both acts are journaled, in order"
    );

    Arc::try_unwrap(engine).ok().unwrap().shutdown().await;
}

/// A branch with nothing beyond its base has nothing to publish: refused by
/// name before any gate opens or the code host is asked.
#[tokio::test(flavor = "multi_thread")]
async fn a_pull_request_with_nothing_to_publish_is_refused_before_the_code_host() {
    let dir = tempfile::tempdir().unwrap();
    let fake = Arc::new(bisa_codehost::fake::FakeCodeHost::minimal(
        "storefront-origin",
    ));
    let engine = engine_with(
        &dir,
        EngineConfig {
            code_hosts: Some(bisa_codehost::CodeHostRegistry::new(vec![fake.clone()])),
            ..config()
        },
    );
    let (goal, project, _root, origin) =
        git_project(&engine, "storefront", PublishPolicy::Auto).await;
    let repo = fake
        .detect(&bisa_codehost::RemoteUrl::parse(origin.to_str().unwrap()))
        .unwrap();

    let spec = item(goal, Some(&project), "Add the checkout flow");
    let w = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap()
        .workstream;

    let err = projects::open_pr(engine.inner(), w.id, "Empty", "")
        .await
        .refused("nothing beyond the base");
    assert!(
        matches!(err, EngineError::NothingToPublish { .. }),
        "got {err}"
    );
    assert!(origin_branches(&origin).is_empty());
    assert!(fake
        .list_prs(&repo, Default::default())
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        engine.workspace().get_workstream(w.id).unwrap().state,
        WorkstreamState::Open
    );

    engine.shutdown().await;
}

/// A merge may delete the head branch on the code host — when the person asked
/// and the code host can. The outcome says which; a code host that cannot says so
/// rather than failing a merge that already landed.
/// One merge through a fake code host, from a fresh project to the merged
/// record: whether the remote branch was deleted, which branches the host was
/// asked to delete, and the branch that went in. Its three callers are three
/// tests, so the three git flows run side by side, not in a line.
async fn merged_with(
    fake: Arc<bisa_codehost::fake::FakeCodeHost>,
    delete_branch: bool,
) -> (Option<bool>, Vec<String>, String) {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(
        &dir,
        EngineConfig {
            code_hosts: Some(bisa_codehost::CodeHostRegistry::new(vec![fake.clone()])),
            ..config()
        },
    );
    let (goal, project, _root, _origin) =
        git_project(&engine, "storefront", PublishPolicy::Auto).await;
    let spec = item(goal, Some(&project), "Add the checkout flow");
    let w = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap()
        .workstream;
    let branch = w.branch().unwrap().to_string();
    let path = engine.workspace().workstream_checkout(&w).unwrap();
    std::fs::write(path.join("checkout.rs"), "fn checkout() {}\n").unwrap();
    projects::commit_workstream(engine.inner(), w.id, "add checkout")
        .await
        .unwrap();
    projects::open_pr(engine.inner(), w.id, "Add checkout", "")
        .await
        .expect("auto: pushed and opened");
    let out = bisa_engine::codehost::merge(
        engine.inner(),
        w.id,
        bisa_codehost::MergeStrategy::Merge,
        delete_branch,
    )
    .await
    .expect("merged");
    assert!(out.merged);
    // The merged record still names the pull request it went in through.
    assert!(matches!(
        engine.workspace().get_workstream(w.id).unwrap().state,
        WorkstreamState::Merged { number, .. } if number > 0
    ));
    let deleted = fake.state.deleted_branches.lock().unwrap().clone();
    engine.shutdown().await;
    (out.remote_branch_deleted, deleted, branch)
}

fn deleting_host(delete_branch: bool) -> Arc<bisa_codehost::fake::FakeCodeHost> {
    Arc::new(bisa_codehost::fake::FakeCodeHost::with_capabilities(
        "storefront-origin",
        bisa_codehost::CodeHostCapabilities {
            merge_strategies: vec![bisa_codehost::MergeStrategy::Merge],
            delete_branch,
            ..Default::default()
        },
    ))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_merge_deletes_the_remote_branch_when_asked_and_the_code_host_can() {
    let (outcome, deleted, branch) = merged_with(deleting_host(true), true).await;
    assert_eq!(outcome, Some(true));
    assert_eq!(deleted, vec![branch]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_merge_nobody_asked_to_delete_the_branch_of_deletes_nothing() {
    let (outcome, deleted, _) = merged_with(deleting_host(true), false).await;
    assert_eq!(outcome, None, "nobody asked");
    assert!(deleted.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_merge_asked_to_delete_a_branch_the_code_host_cannot_says_so_and_deletes_nothing() {
    let (outcome, deleted, _) = merged_with(deleting_host(false), true).await;
    assert_eq!(outcome, Some(false), "asked, but this code host cannot");
    assert!(deleted.is_empty());
}

/// A workstream with an open pull request on `fake`, under `Auto` publishing.
async fn with_open_pr(
    dir: &tempfile::TempDir,
    fake: &Arc<bisa_codehost::fake::FakeCodeHost>,
) -> (Engine, bisa_core::WorkstreamId, u64) {
    let engine = engine_with(
        dir,
        EngineConfig {
            code_hosts: Some(bisa_codehost::CodeHostRegistry::new(vec![fake.clone()])),
            ..config()
        },
    );
    let (goal, project, _root, _origin) =
        git_project(&engine, "storefront", PublishPolicy::Auto).await;
    let spec = item(goal, Some(&project), "Add the checkout flow");
    let w = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap()
        .workstream;
    let path = engine.workspace().workstream_checkout(&w).unwrap();
    std::fs::write(path.join("checkout.rs"), "fn checkout() {}\n").unwrap();
    projects::commit_workstream(engine.inner(), w.id, "add checkout")
        .await
        .unwrap();
    projects::open_pr(engine.inner(), w.id, "Add checkout", "")
        .await
        .expect("auto: pushed and opened");
    let number = match engine.workspace().get_workstream(w.id).unwrap().state {
        WorkstreamState::PrOpen { number, .. } => number,
        other => panic!("{other:?}"),
    };
    (engine, w.id, number)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_merge_the_code_host_refuses_leaves_the_pull_request_open_and_says_why() {
    let dir = tempfile::tempdir().unwrap();
    let fake = deleting_host(true);
    let (engine, wid, number) = with_open_pr(&dir, &fake).await;
    let inner = engine.inner();
    fake.refuse_merges(Some("2 of 5 required checks are failing"));

    let refused =
        bisa_engine::codehost::merge(inner, wid, bisa_codehost::MergeStrategy::Merge, true)
            .await
            .unwrap_err();
    assert!(
        refused.to_string().contains("required checks are failing"),
        "the code host's own reason reaches the person: {refused}"
    );
    assert!(
        matches!(
            engine.workspace().get_workstream(wid).unwrap().state,
            WorkstreamState::PrOpen { number: n, .. } if n == number
        ),
        "nothing landed, so nothing is recorded as landed"
    );
    assert!(fake.state.deleted_branches.lock().unwrap().is_empty());

    // The checks pass, and the same pull request merges.
    fake.refuse_merges(None);
    let out = bisa_engine::codehost::merge(inner, wid, bisa_codehost::MergeStrategy::Merge, false)
        .await
        .unwrap();
    assert!(out.merged);
    // A strategy the host does not take is refused before the host is asked.
    let dir2 = tempfile::tempdir().unwrap();
    let (engine2, wid2, _) = with_open_pr(&dir2, &fake).await;
    let asked = fake.asked_as().len();
    let unsupported = bisa_engine::codehost::merge(
        engine2.inner(),
        wid2,
        bisa_codehost::MergeStrategy::Squash,
        false,
    )
    .await;
    assert!(unsupported.is_err());
    assert_eq!(fake.asked_as().len(), asked, "the host was not asked");
    engine2.shutdown().await;
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pull_request_closed_elsewhere_is_seen_after_a_refused_merge_and_after_the_accounts_change(
) {
    let dir = tempfile::tempdir().unwrap();
    let fake = deleting_host(false);
    let (engine, wid, number) = with_open_pr(&dir, &fake).await;
    let inner = engine.inner();
    let state = |pr: Option<bisa_codehost::PullRequest>| pr.expect("linked").state;

    // Read once: cached. Somebody closes it on the code host; the cache still
    // says open — that is what a cache is — until the host is asked to act.
    let seen = bisa_engine::codehost::linked_pr(inner, wid).await.unwrap();
    assert_eq!(state(seen), bisa_codehost::PrState::Open);
    fake.set_pr_state(number, bisa_codehost::PrState::Closed);
    let cached = bisa_engine::codehost::linked_pr(inner, wid).await.unwrap();
    assert_eq!(state(cached), bisa_codehost::PrState::Open);

    let refused =
        bisa_engine::codehost::merge(inner, wid, bisa_codehost::MergeStrategy::Merge, false)
            .await
            .unwrap_err();
    assert!(refused.to_string().contains("not open"), "{refused}");
    let fresh = bisa_engine::codehost::linked_pr(inner, wid).await.unwrap();
    assert_eq!(
        state(fresh),
        bisa_codehost::PrState::Closed,
        "a refusal is news: the next read asks the code host"
    );

    // Reopened elsewhere while that answer is cached; the accounts change,
    // and no checkout keeps a view read as the account before.
    fake.set_pr_state(number, bisa_codehost::PrState::Open);
    let cached = bisa_engine::codehost::linked_pr(inner, wid).await.unwrap();
    assert_eq!(state(cached), bisa_codehost::PrState::Closed);
    bisa_engine::codehost::forget_account(inner, bisa_codehost::CodeHostKind::GitHub, "nobody")
        .unwrap();
    let fresh = bisa_engine::codehost::linked_pr(inner, wid).await.unwrap();
    assert_eq!(state(fresh), bisa_codehost::PrState::Open);
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// The executor, end to end
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Who commits — inherit the global config, write local config on request,
// ask only when nothing resolves
// ---------------------------------------------------------------------------

/// Read the primary's identity as the engine sees it.
async fn identity_of(engine: &Engine, project: &Project) -> bisa_vcs::GitIdentity {
    bisa_engine::ide::git::identity(
        engine.inner(),
        bisa_core::WorkstreamId::primary_of(project.id),
    )
    .await
    .expect("identity")
}

fn pair(k: &str, v: &str) -> (String, String) {
    (k.to_string(), v.to_string())
}

/// A person with a global identity: every repository the engine makes
/// inherits it — nothing is written into the repository, nothing is asked, and
/// the goal's project starts with its root commit authored by it.
#[tokio::test(flavor = "multi_thread")]
async fn a_made_project_under_a_global_identity_inherits_it_and_gets_its_root_commit_without_asking(
) {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(
        &dir,
        config_with_git(git_with_global(
            &dir,
            "Grace Hopper",
            "grace@example.invalid",
        )),
    );
    let ws = engine.workspace();
    let mut rx = engine.events();

    let goal = ws
        .create_goal(bisa_store::NewGoal::captured("inherit it"))
        .unwrap();
    let project = made_for(&engine, goal.id, "made").await;
    let root = ws.project_root_path(&project);

    let identity = identity_of(&engine, &project).await;
    assert_eq!(
        identity.source,
        bisa_vcs::IdentitySource::Global,
        "inherited, not pinned"
    );
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["config", "--local", "--get-all", "user.name"])
            .output()
            .unwrap()
            .stdout
            .is_empty(),
        "nothing was written into the repository"
    );
    assert_eq!(
        raw_git(&root, &["log", "-1", "--format=%an <%ae>"]),
        "Grace Hopper <grace@example.invalid>",
        "the root commit is made, by the inherited identity"
    );
    let notes = common::notes(&engine, goal.id);
    assert!(
        notes
            .iter()
            .any(|n| n.contains("inherits the global git config") && n.contains("Grace Hopper")),
        "{notes:?}"
    );

    // Every frame up to the attachment: no committer frame among them.
    let mut asked = false;
    loop {
        let ev = common::wait_for(&mut rx, "the creation's frames", |_| true).await;
        match ev.payload {
            EnginePayload::CommitterNeeded { .. } => asked = true,
            EnginePayload::AttachmentChanged { .. } => break,
            _ => {}
        }
    }
    assert!(!asked, "an inherited identity is not a question");
    assert!(bisa_engine::identity::overview(engine.inner())
        .await
        .unwrap()
        .pending
        .is_empty());
    engine.shutdown().await;
}

/// No global identity: the project is made, nothing is written, one
/// `committer_needed` is raised (the root commit's own refusal does not raise
/// a second), the root commit is skipped with the note that says so, and the
/// overview lists the project as pending.
#[tokio::test(flavor = "multi_thread")]
async fn a_made_project_with_no_identity_anywhere_raises_committer_needed_once_and_skips_the_root_commit(
) {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config_with_git(isolated_git(&dir)));
    let ws = engine.workspace();
    let mut rx = engine.events();

    let goal = ws
        .create_goal(bisa_store::NewGoal::captured("ask me"))
        .unwrap();
    let project = made_for(&engine, goal.id, "made").await;
    let root = ws.project_root_path(&project);

    assert_eq!(
        identity_of(&engine, &project).await.source,
        bisa_vcs::IdentitySource::None
    );
    assert!(
        bisa_vcs::git::status(&root).unwrap().oid.is_none(),
        "no root commit without an identity: HEAD stays unborn"
    );

    let mut asks = Vec::new();
    loop {
        let ev = common::wait_for(&mut rx, "the creation's frames", |_| true).await;
        match ev.payload {
            EnginePayload::CommitterNeeded { reason, .. } => asks.push(reason),
            EnginePayload::AttachmentChanged { .. } => break,
            _ => {}
        }
    }
    assert_eq!(
        asks,
        vec![bisa_engine::CommitterReason::Created],
        "asked once"
    );

    let overview = bisa_engine::identity::overview(engine.inner())
        .await
        .unwrap();
    assert_eq!(overview.pending.len(), 1, "{overview:?}");
    assert_eq!(overview.pending[0].project, project.id);
    assert_eq!(overview.pending[0].slug, project.slug.to_string());
    assert_eq!(
        overview.pending[0].workstream,
        bisa_core::WorkstreamId::primary_of(project.id)
    );
    assert_eq!(
        overview.pending[0].reason,
        bisa_engine::CommitterReason::Created
    );
    assert_eq!(overview.global, None);

    let notes = common::notes(&engine, goal.id);
    assert!(
        notes.iter().any(|n| n.contains("no commit identity")),
        "{notes:?}"
    );
    assert!(
        notes.iter().any(|n| n.contains("the person is asked")),
        "{notes:?}"
    );
    engine.shutdown().await;
}

/// Git config named on the request is written into the new repository's local
/// layer, key by key, validated by the schema — a made tree or an adopted one
/// alike, because the request asked. A refused entry fails the creation and
/// the record is taken back.
#[tokio::test(flavor = "multi_thread")]
async fn explicit_git_config_on_the_request_is_written_locally_and_validated() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(
        &dir,
        config_with_git(git_with_global(
            &dir,
            "Grace Hopper",
            "grace@example.invalid",
        )),
    );
    let ws = engine.workspace();
    let global_before = std::fs::read_to_string(dir.path().join("global.gitconfig")).unwrap();

    let request = |slug: &str, git_config: Vec<(String, String)>| projects::NewProjectRequest {
        new: NewProject::managed(slug).unwrap(),
        source: projects::ProjectSource::New,
        git_config,
    };
    let created = projects::create(
        engine.inner(),
        request(
            "explicit",
            vec![
                pair("user.name", "Ada Lovelace"),
                pair("user.email", "ada@example.invalid"),
                pair("user.useConfigOnly", "true"),
                pair("pull.rebase", "true"),
            ],
        ),
    )
    .await
    .unwrap();
    let root = ws.project_root_path(&created.project);
    assert_eq!(
        raw_git(&root, &["config", "--local", "--get", "user.name"]),
        "Ada Lovelace"
    );
    assert_eq!(
        raw_git(&root, &["config", "--local", "--get", "user.useConfigOnly"]),
        "true"
    );
    assert_eq!(
        raw_git(&root, &["config", "--local", "--get", "pull.rebase"]),
        "true"
    );
    assert_eq!(
        identity_of(&engine, &created.project).await.source,
        bisa_vcs::IdentitySource::Local
    );
    assert!(
        matches!(&created.committer, bisa_engine::Committer::Applied { keys } if keys.len() == 4),
        "{:?}",
        created.committer
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("global.gitconfig")).unwrap(),
        global_before,
        "a creation never writes the global layer"
    );

    // An adopted folder is written only because the request asked.
    let theirs = dir.path().join("theirs");
    std::fs::create_dir_all(&theirs).unwrap();
    raw_git(&theirs, &["init", "--quiet"]);
    let adopted = projects::create(
        engine.inner(),
        projects::NewProjectRequest {
            new: NewProject {
                origin: bisa_core::ProjectOrigin::Workspace,
                root: ProjectRoot::External {
                    path: theirs.display().to_string(),
                },
                ..NewProject::managed("adopted").unwrap()
            },
            source: projects::ProjectSource::Adopt,
            git_config: vec![pair("core.autocrlf", "input")],
        },
    )
    .await
    .unwrap();
    assert_eq!(
        raw_git(&theirs, &["config", "--local", "--get", "core.autocrlf"]),
        "input"
    );
    assert!(matches!(
        adopted.committer,
        bisa_engine::Committer::Applied { .. }
    ));

    // Refused by the schema: a global-only key locally, a choice that is not one.
    for (slug, bad) in [
        ("bad-branch", pair("init.defaultBranch", "main")),
        ("bad-crlf", pair("core.autocrlf", "maybe")),
        ("bad-key", pair("core.hooksPath", "x")),
    ] {
        let refused = projects::create(engine.inner(), request(slug, vec![bad]))
            .await
            .expect_err(slug);
        assert!(matches!(refused, EngineError::Vcs(_)), "{slug}: {refused}");
        assert!(
            ws.get_project_by_slug(&bisa_core::Slug::new(slug).unwrap())
                .is_err(),
            "{slug}: the record was taken back"
        );
    }
    engine.shutdown().await;
}

/// The identity named on a **clone** or an **import** request lands in the new
/// repository's local layer too — while a different identity resolves
/// globally, which is exactly the case the project dialog once dropped. The
/// global layer is never written.
#[tokio::test(flavor = "multi_thread")]
async fn git_config_on_a_clone_or_an_import_is_written_locally_over_a_global_identity() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(
        &dir,
        config_with_git(git_with_global(
            &dir,
            "Grace Hopper",
            "grace@example.invalid",
        )),
    );
    let ws = engine.workspace();
    let global_before = std::fs::read_to_string(dir.path().join("global.gitconfig")).unwrap();

    // Something to clone from and to import: a repository with one commit,
    // pushed into a bare repository that stands in for the code host.
    let (_goal, _source, source_root, origin) =
        git_project(&engine, "source", PublishPolicy::Auto).await;
    let default_branch = raw_git(&source_root, &["symbolic-ref", "--short", "HEAD"]);
    bisa_vcs::git::push(&source_root, "origin", &default_branch, true).unwrap();
    // A bare `git init` points HEAD at git's own default name; a clone checks
    // out what HEAD names, so the stand-in code host must name the pushed branch.
    raw_git(
        &origin,
        &[
            "symbolic-ref",
            "HEAD",
            &format!("refs/heads/{default_branch}"),
        ],
    );
    let identity = || {
        vec![
            pair("user.name", "Ada Lovelace"),
            pair("user.email", "ada@example.invalid"),
        ]
    };

    let cloned = projects::create(
        engine.inner(),
        projects::NewProjectRequest {
            new: NewProject::managed("cloned").unwrap(),
            source: projects::ProjectSource::Clone {
                url: origin.to_str().unwrap().to_string(),
                depth: None,
            },
            git_config: identity(),
        },
    )
    .await
    .expect("clone with an identity");
    let root = ws.project_root_path(&cloned.project);
    assert!(root.join("README.md").exists(), "the clone happened");
    assert_eq!(
        raw_git(&root, &["config", "--local", "--get", "user.name"]),
        "Ada Lovelace"
    );
    assert_eq!(
        raw_git(&root, &["config", "--local", "--get", "user.email"]),
        "ada@example.invalid"
    );
    let who = identity_of(&engine, &cloned.project).await;
    assert_eq!(
        who.source,
        bisa_vcs::IdentitySource::Local,
        "the repository's own identity, not the inherited one"
    );
    assert!(
        matches!(&cloned.committer, bisa_engine::Committer::Applied { keys } if keys.len() == 2),
        "{:?}",
        cloned.committer
    );

    let imported = projects::create(
        engine.inner(),
        projects::NewProjectRequest {
            new: NewProject::managed("imported").unwrap(),
            source: projects::ProjectSource::Import {
                source: source_root.clone(),
            },
            git_config: identity(),
        },
    )
    .await
    .expect("import with an identity");
    let root = ws.project_root_path(&imported.project);
    assert!(root.join("README.md").exists(), "the import happened");
    assert_eq!(
        raw_git(&root, &["config", "--local", "--get", "user.name"]),
        "Ada Lovelace"
    );
    assert_eq!(
        raw_git(&root, &["config", "--local", "--get", "user.email"]),
        "ada@example.invalid"
    );
    assert_eq!(
        identity_of(&engine, &imported.project).await.source,
        bisa_vcs::IdentitySource::Local
    );

    assert_eq!(
        std::fs::read_to_string(dir.path().join("global.gitconfig")).unwrap(),
        global_before,
        "a creation never writes the global layer"
    );
    engine.shutdown().await;
}

/// An adopted folder with nothing on the request is left exactly as found:
/// it inherits a global identity like any other, and asks when there is none.
#[tokio::test(flavor = "multi_thread")]
async fn an_adopted_repository_is_never_written_without_a_request() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config_with_git(isolated_git(&dir)));
    let theirs = dir.path().join("theirs");
    std::fs::create_dir_all(&theirs).unwrap();
    raw_git(&theirs, &["init", "--quiet"]);
    let created = projects::create(
        engine.inner(),
        projects::NewProjectRequest {
            new: NewProject {
                origin: bisa_core::ProjectOrigin::Workspace,
                root: ProjectRoot::External {
                    path: theirs.display().to_string(),
                },
                ..NewProject::managed("adopted").unwrap()
            },
            source: projects::ProjectSource::Adopt,
            git_config: Vec::new(),
        },
    )
    .await
    .unwrap();
    assert_eq!(created.committer, bisa_engine::Committer::Ask);
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(&theirs)
            .args(["config", "--local", "--list"])
            .output()
            .unwrap()
            .stdout
            .is_empty()
            || !String::from_utf8_lossy(
                &Command::new("git")
                    .arg("-C")
                    .arg(&theirs)
                    .args(["config", "--local", "--list"])
                    .output()
                    .unwrap()
                    .stdout
            )
            .contains("user."),
        "nothing was written into the adopted folder"
    );
    let pending = bisa_engine::identity::overview(engine.inner())
        .await
        .unwrap()
        .pending;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].project, created.project.id);
    engine.shutdown().await;
}

/// The Git tab's config: set writes the local layer, unset lets a key fall
/// back to the global one, the view shows both layers, and an identity that
/// comes to resolve locally answers the ask.
#[tokio::test(flavor = "multi_thread")]
async fn local_config_is_set_and_unset_per_repository_and_answers_the_ask() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(
        &dir,
        config_with_git(git_with_global(
            &dir,
            "Grace Hopper",
            "grace@example.invalid",
        )),
    );
    let (_goal, project) = plain_project(&engine, "cfg");
    let project = projects::init_git(engine.inner(), &project).await.unwrap();
    let root = engine.workspace().project_root_path(&project);
    let wid = bisa_core::WorkstreamId::primary_of(project.id);
    let mut rx = engine.events();

    let view = bisa_engine::ide::git::local_config(engine.inner(), wid)
        .await
        .unwrap();
    assert_eq!(view.entries.len(), bisa_vcs::GIT_CONFIG_KEYS.len());
    let name = view.get("user.name").unwrap();
    assert_eq!(
        (name.local.as_deref(), name.global.as_deref()),
        (None, Some("Grace Hopper"))
    );

    let view = bisa_engine::ide::git::set_local_config(
        engine.inner(),
        wid,
        vec![
            pair("user.name", "Ada Lovelace"),
            pair("user.email", "ada@example.invalid"),
            pair("pull.rebase", "true"),
        ],
        Vec::new(),
    )
    .await
    .unwrap();
    assert_eq!(
        view.get("user.name").unwrap().local.as_deref(),
        Some("Ada Lovelace")
    );
    assert_eq!(view.get("pull.rebase").unwrap().effective(), Some("true"));
    assert_eq!(
        raw_git(&root, &["config", "--local", "--get", "user.email"]),
        "ada@example.invalid"
    );
    let set = common::wait_for(&mut rx, "committer_set", |e| {
        matches!(&e.payload, EnginePayload::CommitterSet { .. })
    })
    .await;
    assert!(
        matches!(&set.payload, EnginePayload::CommitterSet { identity, .. } if identity.name == "Ada Lovelace")
    );

    let view = bisa_engine::ide::git::set_local_config(
        engine.inner(),
        wid,
        Vec::new(),
        vec!["pull.rebase".to_string(), "user.name".to_string()],
    )
    .await
    .unwrap();
    assert_eq!(view.get("pull.rebase").unwrap().local, None);
    assert_eq!(
        view.get("user.name").unwrap().effective(),
        Some("Grace Hopper"),
        "back to the inherited value"
    );

    let refused = bisa_engine::ide::git::set_local_config(
        engine.inner(),
        wid,
        vec![pair("init.defaultBranch", "main")],
        Vec::new(),
    )
    .await
    .refused("global-only key");
    assert!(matches!(refused, EngineError::Vcs(_)), "{refused}");
    engine.shutdown().await;
}

/// The global layer is written by three named functions and no other (I45):
/// `identity::set_global_config` (the schema keys, from Settings › Identity),
/// `gitprofiles::put` / `remove` / `reappend_includes` (the platform's own
/// profile includes) and `codehost::set_default_account` (the default account).
/// Every `ConfigScope::Global` in the crate sits inside one of them.
#[test]
fn only_the_three_named_writers_reach_the_global_layer() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let allowed: &[(&str, &[&str])] = &[
        ("identity.rs", &["set_global_config"]),
        ("gitprofiles.rs", &["put", "remove", "reappend_includes"]),
        ("codehost.rs", &["set_default_account"]),
    ];
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                out.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(&src, &mut files);
    let mut hits = 0;
    for path in files {
        let rel = path.strip_prefix(&src).unwrap().display().to_string();
        let text = std::fs::read_to_string(&path).unwrap();
        let shipped = text.split("#[cfg(test)]").next().unwrap_or_default();
        // The function each line sits in: the last `fn <name>` above it.
        let mut current_fn = String::new();
        for (i, line) in shipped.lines().enumerate() {
            if let Some(rest) = line
                .trim_start()
                .strip_prefix("pub fn ")
                .or_else(|| line.trim_start().strip_prefix("pub async fn "))
                .or_else(|| line.trim_start().strip_prefix("fn "))
                .or_else(|| line.trim_start().strip_prefix("async fn "))
            {
                current_fn = rest
                    .split(['(', '<'])
                    .next()
                    .unwrap_or_default()
                    .to_string();
            }
            if !line.contains("ConfigScope::Global") || line.trim_start().starts_with("//") {
                continue;
            }
            hits += 1;
            let ok = allowed
                .iter()
                .any(|(file, fns)| *file == rel && fns.contains(&current_fn.as_str()));
            assert!(ok, "{rel}:{}: the global layer is named in `{current_fn}`, not one of the three writers", i + 1);
        }
    }
    assert!(
        hits >= 3,
        "the three writers name the global layer ({hits} hits)"
    );
}

/// A mock harness that writes a file into whatever cwd it is launched in, so
/// a run leaves the workstream dirty exactly as a real session would. It is the
/// stock mock in every other respect — still no model anywhere.
struct WritingAdapter {
    inner: MockAdapter,
    file: String,
}

#[async_trait::async_trait]
impl bisa_harness::HarnessAdapter for WritingAdapter {
    fn id(&self) -> &str {
        self.inner.id()
    }
    fn display_name(&self) -> &str {
        "Writing Mock"
    }
    fn caps(&self) -> HarnessCaps {
        self.inner.caps()
    }
    async fn probe(&self) -> ProbeResult {
        self.inner.probe().await
    }
    async fn launch(&self, spec: SessionSpec) -> Result<Box<dyn HarnessSession>, HarnessError> {
        std::fs::write(spec.cwd.join(&self.file), "written by the session\n")
            .expect("the session must be able to write in its cwd");
        self.inner.launch(spec).await
    }
    async fn attach(&self, token: &ResumeToken) -> Result<Box<dyn HarnessSession>, HarnessError> {
        self.inner.attach(token).await
    }
}

/// The loop closes: a settlement refused for want of an identity leaves the
/// worktree dirty and asks; setting who commits — the same write the IDE, the
/// CLI and the API make — commits that settlement with the message it was
/// meant to carry, and the workstream reads committed.
#[tokio::test(flavor = "multi_thread")]
async fn a_refused_settlement_is_retried_when_who_commits_is_set() {
    let dir = tempfile::tempdir().unwrap();
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::new(WritingAdapter {
        inner: MockAdapter::default(),
        file: "checkout.rs".into(),
    }));
    let engine = Engine::start(
        workspace(&dir),
        catalog,
        config_with_git(isolated_git(&dir)),
    )
    .unwrap();
    let mut rx = engine.events();
    let ws = engine.workspace();

    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..SubmitRequest::captured("ship checkout")
        })
        .unwrap();
    let project = attached_project(ws, goal.id, NewProject::managed("storefront").unwrap());
    let project = projects::init_git(engine.inner(), &project).await.unwrap();
    let root = ws.project_root_path(&project);
    // History without an identity: a worktree can be cut, nobody can commit.
    commit_without_identity(&root, "baseline");
    assert_eq!(
        identity_of(&engine, &project).await.source,
        bisa_vcs::IdentitySource::None
    );

    let item_id = run_spec(
        &engine,
        goal.id,
        item(goal.id, Some(&project), "Add the checkout flow"),
    )
    .await;
    let asked = common::wait_for(&mut rx, "committer_needed", |e| {
        matches!(&e.payload, EnginePayload::CommitterNeeded { .. })
    })
    .await;
    assert!(
        matches!(&asked.payload, EnginePayload::CommitterNeeded { reason: bisa_engine::CommitterReason::SettlementRefused, project: p, .. } if *p == project.id),
        "{:?}",
        asked.payload
    );
    let workstreams = ws
        .list_workstreams(WorkstreamFilter::WorkItem(item_id))
        .unwrap();
    assert_eq!(workstreams.len(), 1);
    let w = until("the settlement to be refused", || {
        let w = ws.get_workstream(workstreams[0].id).unwrap();
        (w.state == WorkstreamState::Dirty).then_some(w)
    })
    .await;
    let checkout = ws.workstream_checkout(&w).unwrap();
    assert!(
        checkout.join("checkout.rs").exists(),
        "the work is kept, uncommitted"
    );
    assert!(!bisa_vcs::git::status(&checkout).unwrap().is_clean);

    // The answer — on the primary, as the dialog writes it.
    let wid = bisa_core::WorkstreamId::primary_of(project.id);
    bisa_engine::ide::git::set_identity(engine.inner(), wid, "Ada Lovelace", "ada@example.invalid")
        .await
        .unwrap();
    common::wait_for(&mut rx, "committer_set", |e| {
        matches!(&e.payload, EnginePayload::CommitterSet { .. })
    })
    .await;
    // The run finished meanwhile and released its workstreams as records;
    // the retry commits the work all the same — the checkout is what holds it.
    common::wait_for(&mut rx, "the settlement to be committed after all", |e| {
        matches!(&e.payload, EnginePayload::WorkstreamCommitted { workstream, .. } if *workstream == w.id)
    })
    .await;
    assert!(bisa_vcs::git::status(&checkout).unwrap().is_clean);
    assert_eq!(
        raw_git(&checkout, &["log", "-1", "--format=%s"]),
        "Add the checkout flow"
    );
    assert_eq!(
        raw_git(&checkout, &["log", "-1", "--format=%an <%ae>"]),
        "Ada Lovelace <ada@example.invalid>"
    );
    assert!(raw_git(&checkout, &["log", "-1", "--format=%b"]).contains(&item_id.to_string()));
    assert!(bisa_engine::identity::overview(engine.inner())
        .await
        .unwrap()
        .pending
        .is_empty());
    engine.shutdown().await;
}

/// The whole point: a work item with a project runs **inside its workstream**,
/// and what it leaves behind settles into a commit on its own branch. Nothing
/// is pushed — that is the `Publish` gate, and it was never asked.
#[tokio::test(flavor = "multi_thread")]
async fn a_work_item_runs_in_its_workstream_and_settles_into_a_commit() {
    let dir = tempfile::tempdir().unwrap();
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::new(WritingAdapter {
        inner: MockAdapter::default(),
        file: "checkout.rs".into(),
    }));
    let engine = Engine::start(workspace(&dir), catalog, config()).unwrap();
    let mut rx = engine.events();

    // The project has to exist before the plan can point at it.
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..SubmitRequest::captured("ship checkout")
        })
        .unwrap();
    let ws = engine.workspace();
    let project = attached_project(ws, goal.id, NewProject::managed("storefront").unwrap());
    let project = projects::init_git(engine.inner(), &project).await.unwrap();
    let root = ws.project_root_path(&project);
    set_identity(&root);
    std::fs::write(root.join("README.md"), "baseline\n").unwrap();
    bisa_vcs::git::add_all(&root).unwrap();
    bisa_vcs::git::commit(&root, "baseline", false).unwrap();

    let spec = item(goal.id, Some(&project), "Add the checkout flow");
    let item_id = run_spec(&engine, goal.id, spec).await;

    // The settlement is announced before the session's end is: the commit
    // is part of settling, so the bus carries it first.
    let deadline = Duration::from_secs(30);
    let committed = tokio::time::timeout(deadline, async {
        let mut committed = None;
        loop {
            let ev = rx.recv().await.expect("bus");
            match &ev.payload {
                EnginePayload::WorkstreamCommitted { .. } => committed = Some(ev),
                EnginePayload::ExecutionEnded { .. } => return committed,
                _ => {}
            }
        }
    })
    .await
    .expect("the run must settle")
    .expect("the settlement is announced before the session's end");
    assert_eq!(committed.goal, Some(goal.id));

    let workstreams = ws
        .list_workstreams(WorkstreamFilter::WorkItem(item_id))
        .unwrap();
    assert_eq!(workstreams.len(), 1, "one item, one workstream");
    let w = &workstreams[0];
    let branch = match &w.kind {
        WorkstreamKind::Worktree { branch, .. } => branch.clone(),
        other => panic!("expected a worktree workstream, got {other:?}"),
    };
    assert!(branch.starts_with("work/add-the-checkout-flow-"));
    let checkout = ws.workstream_checkout(w).unwrap();

    // The session ran in the workstream, not in the project root.
    assert!(checkout.join("checkout.rs").exists());
    assert!(!root.join("checkout.rs").exists());

    // ...and what it left is a commit on the branch, announced as such. The
    // run's end released the goal's workstreams — the record is closed — and
    // the checkout and its branch stay: an unpushed branch is the only copy
    // of the work.
    assert!(
        matches!(&committed.payload, EnginePayload::WorkstreamCommitted { workstream, branch: b, .. } if *workstream == w.id && *b == branch),
        "{:?}",
        committed.payload
    );
    let w = until("the run's end to release the record", || {
        let w = ws.get_workstream(w.id).unwrap();
        (w.state == WorkstreamState::Closed).then_some(w)
    })
    .await;
    assert!(checkout.is_dir(), "the workstream persists after the run");
    let subject = raw_git(&checkout, &["log", "-1", "--format=%s"]);
    assert_eq!(subject, "Add the checkout flow");
    let body = raw_git(&checkout, &["log", "-1", "--format=%b"]);
    assert!(body.contains(&item_id.to_string()), "body: {body:?}");
    assert!(bisa_vcs::git::status(&checkout).unwrap().is_clean);
    let _ = &w.state;
    assert!(
        bisa_vcs::git::branch_exists(&root, &branch).unwrap(),
        "a branch that carries a commit stays"
    );

    // No result was written as a patch; the commit *is* the result.
    assert!(
        !ws.paths().goal(goal.id).result(item_id).exists(),
        "a git workstream settles into a commit, not a patch"
    );
    // Nothing was pushed, and nobody was asked to approve a push.
    assert!(engine.inbox().is_empty());

    engine.shutdown().await;
}

/// The project the clean-worktree tests run in: git-initialised, with an
/// identity and one baseline commit to branch from.
async fn baselined_project(engine: &Engine, goal: GoalId, slug: &str) -> (Project, PathBuf) {
    let ws = engine.workspace();
    let project = attached_project(ws, goal, NewProject::managed(slug).unwrap());
    let project = projects::init_git(engine.inner(), &project).await.unwrap();
    let root = ws.project_root_path(&project);
    set_identity(&root);
    std::fs::write(root.join("README.md"), "baseline\n").unwrap();
    bisa_vcs::git::add_all(&root).unwrap();
    bisa_vcs::git::commit(&root, "baseline", false).unwrap();
    (project, root)
}

/// An item whose session read, judged or answered and left the tree as it
/// found it settles clean: its worktree is closed, its checkout removed and
/// its empty branch deleted, so a read-only step leaves nothing behind —
/// while the primary and its baseline are untouched.
#[tokio::test(flavor = "multi_thread")]
async fn a_work_item_that_leaves_the_tree_clean_closes_its_worktree_and_its_branch() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let mut rx = engine.events();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..SubmitRequest::captured("review the checkout")
        })
        .unwrap();
    let ws = engine.workspace();
    let (project, root) = baselined_project(&engine, goal.id, "storefront").await;

    let spec = item(goal.id, Some(&project), "Read the checkout and report");
    let item_id = run_spec(&engine, goal.id, spec).await;
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let ev = rx.recv().await.expect("bus");
            if matches!(&ev.payload, EnginePayload::ExecutionEnded { .. }) {
                return;
            }
        }
    })
    .await
    .expect("the run must settle");

    let workstreams = ws
        .list_workstreams(WorkstreamFilter::WorkItem(item_id))
        .unwrap();
    assert_eq!(workstreams.len(), 1, "one item, one workstream");
    let w = &workstreams[0];
    let WorkstreamKind::Worktree { branch, .. } = &w.kind else {
        panic!("expected a worktree workstream, got {:?}", w.kind);
    };
    let checkout = ws.paths().project(&project.slug).workstream_dir(w.id);
    until("the clean workstream to close", || {
        let w = ws.get_workstream(w.id).unwrap();
        (w.state == WorkstreamState::Closed).then_some(())
    })
    .await;
    assert!(!checkout.exists(), "the checkout is gone");
    assert!(
        !bisa_vcs::git::branch_exists(&root, branch).unwrap(),
        "the empty branch is gone"
    );
    assert_eq!(
        bisa_vcs::git::worktree_list(&root).unwrap().len(),
        1,
        "only the primary remains"
    );
    assert!(root.join("README.md").exists(), "the baseline is untouched");
    assert!(bisa_vcs::git::status(&root).unwrap().is_clean);
    assert!(
        !ws.paths().goal(goal.id).result(item_id).exists(),
        "nothing to keep, nothing kept"
    );
    engine.shutdown().await;
}

/// A re-run of an item whose clean worktree was closed opens a fresh one —
/// a new record on the same derived branch name, nothing colliding with the
/// branch that was deleted.
#[tokio::test(flavor = "multi_thread")]
async fn a_closed_clean_workstream_is_reopened_fresh_on_a_re_run() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let mut rx = engine.events();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..SubmitRequest::captured("look twice")
        })
        .unwrap();
    let ws = engine.workspace();
    let (project, _root) = baselined_project(&engine, goal.id, "storefront").await;

    let spec = item(goal.id, Some(&project), "Look at the checkout");
    let mut again = spec.clone();
    let item_id = run_spec(&engine, goal.id, spec).await;
    // The run's step made its own item; a re-run is that item's, and the
    // branch is derived from its id.
    again.id = item_id;
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let ev = rx.recv().await.expect("bus");
            if matches!(&ev.payload, EnginePayload::ExecutionEnded { .. }) {
                return;
            }
        }
    })
    .await
    .expect("the run must settle");
    let first = until("the clean workstream to close", || {
        ws.list_workstreams(WorkstreamFilter::WorkItem(item_id))
            .unwrap()
            .into_iter()
            .find(|w| w.state == WorkstreamState::Closed)
    })
    .await;

    let place = projects::open_workstream(engine.inner(), &again, &project)
        .await
        .expect("a fresh worktree");
    assert_ne!(place.workstream.id, first.id, "a new record");
    assert_eq!(
        place.workstream.branch(),
        first.branch(),
        "the same derived branch name, free again"
    );
    assert!(place.cwd.is_dir());
    let rows = ws
        .list_workstreams(WorkstreamFilter::WorkItem(item_id))
        .unwrap();
    assert_eq!(rows.len(), 2, "one closed, one open");
    assert!(rows.iter().any(|w| w.state == WorkstreamState::Open));
    engine.shutdown().await;
}

/// A tree that is clean because the session committed by hand is not a
/// clean branch: the branch carries a commit of its own, and it is kept.
#[tokio::test(flavor = "multi_thread")]
async fn a_clean_tree_whose_branch_carries_a_hand_made_commit_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..SubmitRequest::captured("commit by hand")
        })
        .unwrap();
    let ws = engine.workspace();
    let (project, root) = baselined_project(&engine, goal.id, "storefront").await;
    let spec = item(goal.id, Some(&project), "Commit something yourself");
    let place = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap();
    std::fs::write(place.cwd.join("by-hand.txt"), "mine\n").unwrap();
    raw_git(&place.cwd, &["add", "-A"]);
    raw_git(&place.cwd, &["commit", "--quiet", "-m", "by hand"]);
    assert!(bisa_vcs::git::status(&place.cwd).unwrap().is_clean);

    projects::close_clean_worktree(engine.inner(), &place.workstream).await;

    let w = ws.get_workstream(place.workstream.id).unwrap();
    assert_eq!(w.state, WorkstreamState::Open, "kept");
    assert!(place.cwd.is_dir(), "the checkout stays");
    assert!(
        bisa_vcs::git::branch_exists(&root, w.branch().unwrap()).unwrap(),
        "the branch with its commit stays"
    );
    engine.shutdown().await;
}

/// A **copy** workstream still settles into a `.patch`, and it is now the only
/// placement that does.
///
/// A non-git project's workstream is a copy of the project root that is torn
/// down when the run ends, so a diff is the only thing left to keep. The two
/// placements around it keep their own evidence instead: a git workstream
/// settles into a commit on its branch, and a project-less item's files stay
/// in the goal's `work/` (`placement.rs`).
#[tokio::test(flavor = "multi_thread")]
async fn a_copy_workstream_settles_into_a_patch() {
    let dir = tempfile::tempdir().unwrap();
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::new(WritingAdapter {
        inner: MockAdapter::default(),
        file: "scratch.txt".into(),
    }));
    let engine = Engine::start(workspace(&dir), catalog, config()).unwrap();
    let mut rx = engine.events();

    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..SubmitRequest::captured("plain project")
        })
        .unwrap();
    let ws = engine.workspace();
    let project = attached_project(ws, goal.id, NewProject::managed("notes").unwrap());
    std::fs::create_dir_all(ws.project_root_path(&project)).unwrap();
    std::fs::write(
        ws.project_root_path(&project).join("existing.txt"),
        "before\n",
    )
    .unwrap();

    let mut spec = item(goal.id, Some(&project), "Scribble something");
    spec.project = Some(project.id);
    let item_id = run_spec(&engine, goal.id, spec).await;

    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let ev = rx.recv().await.expect("bus");
            if matches!(&ev.payload, EnginePayload::ExecutionEnded { .. }) {
                return;
            }
        }
    })
    .await
    .expect("the run must settle");

    let expected = ws.paths().goal(goal.id).result(item_id);
    let patch = until("the patch result", || {
        expected.exists().then(|| expected.clone())
    })
    .await;
    assert!(std::fs::read_to_string(&patch)
        .unwrap()
        .contains("scratch.txt"));

    // The workstream is the copy, and it lived under the project's own
    // `workstreams/` — never inside the tree it copied, never under a goal.
    let workstreams = ws
        .list_workstreams(WorkstreamFilter::WorkItem(item_id))
        .unwrap();
    assert_eq!(workstreams.len(), 1);
    assert!(matches!(workstreams[0].kind, WorkstreamKind::Copy));
    let checkout = ws.workstream_checkout(&workstreams[0]).unwrap();
    assert!(checkout.starts_with(ws.paths().project(&project.slug).workstreams()));
    assert!(!checkout.starts_with(ws.project_root_path(&project)));

    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// The primary workstream
// ---------------------------------------------------------------------------

/// The project's own root is a workstream: listed first with the project's own
/// id, committed to like any checkout, never closed and never finished as a
/// branch would be.
#[tokio::test(flavor = "multi_thread")]
async fn statuses_start_with_the_primary_which_commits_but_never_closes() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (_goal, project, root, _origin) =
        git_project(&engine, "storefront", PublishPolicy::Auto).await;
    let inner = engine.inner();
    let primary = bisa_core::WorkstreamId::primary_of(project.id);

    let statuses = bisa_engine::ide::git::workstream_statuses(inner, Some(project.id))
        .await
        .unwrap();
    assert_eq!(statuses.len(), 1, "a fresh project has exactly its primary");
    assert_eq!(statuses[0].workstream, primary);
    assert!(matches!(
        statuses[0].kind,
        bisa_core::WorkstreamKind::Primary
    ));
    assert!(statuses[0].git, "the root of a git project is a repository");
    assert_eq!(
        statuses[0].base, None,
        "the primary has no base to be ahead of"
    );
    assert_eq!(statuses[0].ahead_of_base, None);
    assert!(
        statuses[0].branch.is_some(),
        "its branch is read live from HEAD"
    );

    // Committing to the root checkout is ordinary work.
    std::fs::write(root.join("notes.md"), "hello\n").unwrap();
    let commit = projects::commit_workstream(inner, primary, "notes")
        .await
        .expect("a commit on the primary");
    assert!(!commit.short().is_empty());
    assert_eq!(
        engine.workspace().get_workstream(primary).unwrap().state,
        WorkstreamState::Committed
    );

    // Closing it is refused by name — the project is the substitute.
    let err = projects::close_workstream(inner, primary, false)
        .await
        .refused("the primary never closes");
    assert!(
        err.to_string().contains("remove the project instead"),
        "got {err}"
    );
    assert!(engine.workspace().get_workstream(primary).is_ok());

    engine.shutdown().await;
}

/// **A project is carried by somebody the workspace has.** A name nobody
/// answers to — `agent:reviwer` — was written on the record as it was typed,
/// and found out when work was routed and no agent took it. It is refused
/// where the project is made and where it is edited, by the name that is
/// nobody's, and the record is as it was.
#[tokio::test(flavor = "multi_thread")]
async fn a_project_is_carried_only_by_somebody_the_workspace_has() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let inner = engine.inner();
    let ghost = bisa_core::Assignee::Agent("nobody-here".into());
    let general = bisa_core::Assignee::Agent(bisa_core::AgentId::general().to_string());

    let mut new = NewProject::managed("shelf").unwrap();
    new.assignees = vec![ghost.clone()];
    let refused = projects::create(
        inner,
        bisa_engine::NewProjectRequest {
            new,
            source: bisa_engine::ProjectSource::New,
            git_config: vec![],
        },
    )
    .await
    .refused("nobody answers to that name");
    assert!(refused.to_string().contains("nobody-here"), "{refused}");
    assert!(
        engine.workspace().list_projects().unwrap().is_empty(),
        "nothing was made"
    );

    let (_goal, project, _root, _origin) =
        git_project(&engine, "storefront", PublishPolicy::Manual).await;
    let mut carried = project.clone();
    carried.assignees = vec![general.clone()];
    let carried = projects::update(inner, carried).expect("an agent the workspace has");
    assert_eq!(carried.assignees, vec![general.clone()]);
    let mut by_a_ghost = carried.clone();
    by_a_ghost.assignees = vec![general.clone(), ghost];
    let refused = projects::update(inner, by_a_ghost).refused("nobody answers to that name");
    assert!(refused.to_string().contains("nobody-here"), "{refused}");
    assert_eq!(
        engine
            .workspace()
            .get_project(project.id)
            .unwrap()
            .assignees,
        vec![general],
        "the record is as it was"
    );
    engine.shutdown().await;
}

/// **A commit is a commit, whichever door made it.** The Changes view's
/// commit — the one a person makes on the desktop, with the paths they chose
/// — wrote the commit and left the record `open`: the Board kept the card in
/// Backlog and the lifecycle kept offering *Commit* for work already
/// committed, until a push happened to reconcile it. It moves the record as
/// the workstream's own commit does, and says so on the bus.
#[tokio::test(flavor = "multi_thread")]
async fn a_commit_from_the_changes_view_moves_the_record_as_the_workstreams_own_does() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (goal, project, root, _origin) =
        git_project(&engine, "storefront", PublishPolicy::Manual).await;
    let inner = engine.inner();
    let spec = item(goal, Some(&project), "Add the checkout flow");
    let w = projects::open_workstream(inner, &spec, &project)
        .await
        .unwrap()
        .workstream;
    let path = engine.workspace().workstream_checkout(&w).unwrap();
    std::fs::write(path.join("checkout.rs"), "fn checkout() {}\n").unwrap();
    std::fs::write(path.join("notes.md"), "not yet\n").unwrap();

    let mut rx = engine.events();
    let commit = projects::commit_in(inner, w.id, "checkout", vec!["checkout.rs".into()])
        .await
        .expect("a commit of the paths chosen");
    assert_eq!(
        engine.workspace().get_workstream(w.id).unwrap().state,
        WorkstreamState::Committed
    );
    let mut said = Vec::new();
    while let Ok(event) = rx.try_recv() {
        match event.payload {
            EnginePayload::WorkstreamChanged { workstream, state } if workstream == w.id => {
                said.push(format!("changed to {}", state.as_str()))
            }
            EnginePayload::WorkstreamCommitted {
                workstream,
                commit: sha,
                ..
            } if workstream == w.id => {
                assert_eq!(sha, commit.to_string());
                said.push("committed".to_string());
            }
            _ => {}
        }
    }
    assert_eq!(said, vec!["changed to committed", "committed"]);
    // What was not chosen is still the tree's.
    assert!(path.join("notes.md").exists());

    // The primary's, the same; and a record that is closed keeps its state.
    let primary = bisa_core::WorkstreamId::primary_of(project.id);
    std::fs::write(root.join("primary.md"), "hello\n").unwrap();
    projects::commit_in(inner, primary, "primary", vec!["primary.md".into()])
        .await
        .expect("a commit on the primary");
    assert_eq!(
        engine.workspace().get_workstream(primary).unwrap().state,
        WorkstreamState::Committed
    );
    projects::close_workstream(inner, w.id, false)
        .await
        .unwrap();
    projects::commit_in(inner, w.id, "notes", vec!["notes.md".into()])
        .await
        .expect("the checkout is still there, and so is its branch");
    assert_eq!(
        engine.workspace().get_workstream(w.id).unwrap().state,
        WorkstreamState::Closed
    );
    engine.shutdown().await;
}

/// `running_agents` counts the **work** sessions by where they stand, not by
/// work item — a worker in a worktree counts, a session in a scratch folder
/// does not, and a conversation's turn standing in the checkout is its
/// conversation's, never the checkout's running agent.
#[tokio::test(flavor = "multi_thread")]
async fn running_agents_are_counted_by_workstream() {
    use bisa_engine::registry::{AgentRef, AgentStatus, SessionKind};
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (_goal, project, _root, _origin) =
        git_project(&engine, "storefront", PublishPolicy::Auto).await;
    let inner = engine.inner();
    let primary = bisa_core::WorkstreamId::primary_of(project.id);

    let register = |kind: SessionKind, workstream: Option<bisa_core::WorkstreamId>| {
        inner
            .registry
            .register_if(
                AgentRef {
                    id: bisa_engine::LiveRunId::mint(),
                    kind,
                    status: AgentStatus::Running,
                    generation: 1,
                    session_id: None,
                    work_item: None,
                    conversation: None,
                    goal: None,
                    workstream,
                    transcript_path: None,
                    last_activity: 0,
                },
                None,
            )
            .unwrap();
    };
    register(SessionKind::Worker, Some(primary));
    register(SessionKind::Worker, Some(primary));
    register(SessionKind::Worker, None);
    register(SessionKind::Conversation, Some(primary));

    let status = bisa_engine::ide::git::workstream_status(inner, primary)
        .await
        .unwrap();
    assert_eq!(
        status.running_agents, 2,
        "two workers stand in the primary, one stands nowhere, and a conversation's turn is not counted"
    );

    engine.shutdown().await;
}

/// The one creation path takes its record back when the tree cannot be made,
/// and deletes nothing on disk.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_clone_leaves_no_record_and_removes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let inner = engine.inner();
    let before = engine.workspace().list_projects().unwrap().len();

    let nowhere = dir.path().join("nowhere.git");
    let err = projects::create(
        inner,
        bisa_engine::NewProjectRequest {
            new: NewProject::managed("ghost").unwrap(),
            source: bisa_engine::ProjectSource::Clone {
                url: nowhere.display().to_string(),
                depth: None,
            },
            git_config: Vec::new(),
        },
    )
    .await
    .refused("cloning from a path that does not exist fails");
    assert!(!err.to_string().is_empty());

    let projects = engine.workspace().list_projects().unwrap();
    assert_eq!(projects.len(), before, "no record survives a failed clone");
    assert!(
        !projects.iter().any(|p| p.slug.as_str() == "ghost"),
        "and none is called ghost"
    );
    assert!(
        engine
            .workspace()
            .list_workstreams(WorkstreamFilter::All)
            .unwrap()
            .into_iter()
            .all(|w| projects.iter().any(|p| p.id == w.project)),
        "no orphan primary is left behind"
    );

    // A creation that succeeds announces itself and has its primary.
    let created = projects::create(
        inner,
        bisa_engine::NewProjectRequest {
            new: NewProject::managed("real").unwrap(),
            source: bisa_engine::ProjectSource::New,
            git_config: Vec::new(),
        },
    )
    .await
    .unwrap();
    assert!(engine
        .workspace()
        .primary_workstream(created.project.id)
        .unwrap()
        .is_primary());
    assert!(inner
        .ws
        .project_root_path(&created.project)
        .join(".git")
        .is_dir());

    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// Who commits here
// ---------------------------------------------------------------------------

/// A repository nobody is set to commit in refuses a commit by name — before
/// anything is staged — and a local identity, set through the engine, unblocks
/// it and signs the commit. The engine runs on an isolated `git`
/// (`EngineConfig::git`), so the developer's own global config cannot reach
/// the assertion and the first read is `none`, not merely "not local".
#[tokio::test(flavor = "multi_thread")]
async fn a_repository_without_a_local_identity_gets_one_through_the_engine_and_commits_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config_with_git(isolated_git(&dir)));
    let (_goal, project) = plain_project(&engine, "identity");
    let project = projects::init_git(engine.inner(), &project)
        .await
        .expect("init_git");
    let root = engine.workspace().project_root_path(&project);
    let wid = bisa_core::WorkstreamId::primary_of(project.id);

    let before = bisa_engine::ide::git::identity(engine.inner(), wid)
        .await
        .expect("identity");
    assert_eq!(
        before.source,
        bisa_engine::ide::git::IdentitySource::None,
        "a fresh repository has no identity of its own, and the engine sees no global one"
    );
    assert_eq!(before.global, None);

    // Refused by name — and the refusal puts the question to a person.
    let mut rx = engine.events();
    std::fs::write(root.join("early.txt"), "too soon\n").unwrap();
    let refused = projects::commit_in(engine.inner(), wid, "early", vec!["early.txt".into()])
        .await
        .refused("nobody is set to commit");
    assert!(
        matches!(refused, EngineError::IdentityUnset { .. }),
        "{refused}"
    );
    let asked = common::wait_for(&mut rx, "committer_needed", |e| {
        matches!(&e.payload, EnginePayload::CommitterNeeded { .. })
    })
    .await;
    match asked.payload {
        EnginePayload::CommitterNeeded {
            project: p,
            reason,
            workstream,
            global,
            ..
        } => {
            assert_eq!(p, project.id);
            assert_eq!(reason, bisa_engine::CommitterReason::CommitRefused);
            assert_eq!(workstream, wid);
            assert_eq!(global, None);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        bisa_vcs::git::status(&root).unwrap().staged,
        0,
        "refused before staging: the index is as it was"
    );
    std::fs::remove_file(root.join("early.txt")).unwrap();

    let after = bisa_engine::ide::git::set_identity(
        engine.inner(),
        wid,
        "Ada Lovelace",
        "ada@example.invalid",
    )
    .await
    .expect("set identity");
    assert_eq!(after.source, bisa_engine::ide::git::IdentitySource::Local);
    assert_eq!(after.name.as_deref(), Some("Ada Lovelace"));
    assert_eq!(
        raw_git(&root, &["config", "--local", "--get", "user.email"]),
        "ada@example.invalid",
        "written to the repository's own config"
    );
    // The answer is announced, and the desk is clear.
    let set = common::wait_for(&mut rx, "committer_set", |e| {
        matches!(&e.payload, EnginePayload::CommitterSet { .. })
    })
    .await;
    assert!(
        matches!(&set.payload, EnginePayload::CommitterSet { project: p, identity, .. } if *p == project.id && identity.name == "Ada Lovelace")
    );
    assert!(
        bisa_engine::identity::overview(engine.inner())
            .await
            .unwrap()
            .pending
            .is_empty(),
        "answered questions leave the desk"
    );

    std::fs::write(root.join("note.txt"), "hello\n").unwrap();
    let commit = projects::commit_in(engine.inner(), wid, "first", vec!["note.txt".into()])
        .await
        .expect("commit with the local identity");
    assert_eq!(
        raw_git(&root, &["log", "-1", "--format=%an <%ae>"]),
        "Ada Lovelace <ada@example.invalid>"
    );
    assert_eq!(raw_git(&root, &["rev-parse", "HEAD"]), commit.as_str());

    // The refusal is typed and names both fixes.
    let refusal = bisa_engine::EngineError::IdentityUnset {
        project: project.slug.to_string(),
    };
    assert!(refusal.is_refusal());
    let text = refusal.to_string();
    assert!(text.contains("Git → Repository"), "{text}");
    assert!(text.contains("bisa project identity identity"), "{text}");

    // Refused before staging: nothing was validated as an identity.
    let bad = bisa_engine::ide::git::set_identity(engine.inner(), wid, "", "nobody")
        .await
        .refused("an empty name is refused");
    assert!(matches!(bad, bisa_engine::EngineError::Vcs(_)), "{bad}");

    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// Workstream scripts (ide/07 §Workstream scripts)
// ---------------------------------------------------------------------------

/// Set one of the project's scripts, as the Workstream scripts card writes it.
/// The run command (ide/18) is the fourth script: listed and approved with
/// the others, answered on its own, and never run by the engine's lifecycle
/// runner — the IDE's Terminal menu opens it in a terminal.
#[tokio::test(flavor = "multi_thread")]
async fn the_run_command_is_the_fourth_script_and_never_a_lifecycle_run() {
    use bisa_engine::scripts::{self, Phase, ScriptContext, ScriptPolicy};
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (goal, project, root, _origin) = git_project(&engine, "served", PublishPolicy::Auto).await;
    let project = engine.workspace().get_project(project.id).unwrap();

    assert_eq!(
        scripts::run_command(engine.inner(), &project).unwrap(),
        None,
        "unset is none"
    );
    let status = scripts::status(engine.inner(), &project).unwrap();
    assert_eq!(status.len(), 4);
    assert_eq!(status[3].phase, Phase::Run);
    assert!(
        status[3].trusted,
        "an empty script is trusted, as the others are"
    );
    assert_eq!(Phase::Run.key(), "workstreams.script.run");
    assert_eq!(Phase::Run.as_str(), "run");
    assert!(!Phase::LIFECYCLE.contains(&Phase::Run));

    set_script(&engine, &project, "workstreams.script.run", "npm run dev");
    let run = scripts::run_command(engine.inner(), &project)
        .unwrap()
        .expect("set");
    assert_eq!(run.command, "npm run dev");
    assert!(!run.trusted, "nobody here approved it yet");
    assert_eq!(run.digest, scripts::digest("npm run dev"));
    // An approval is a setting of this machine moving, and is said as one:
    // an editor open on Settings re-reads it, as it does any other write.
    let mut bus = engine.events();
    approve_scripts(&engine, &project);
    let said = common::wait_for(&mut bus, "the approval to be said as a setting", |e| {
        matches!(&e.payload, EnginePayload::SettingsChanged { .. })
    })
    .await;
    let EnginePayload::SettingsChanged { scope, keys, .. } = said.payload else {
        unreachable!()
    };
    assert_eq!(scope, "machine");
    assert_eq!(keys, vec!["workstreams.script.trusted".to_string()]);
    assert!(
        scripts::run_command(engine.inner(), &project)
            .unwrap()
            .unwrap()
            .trusted
    );

    // The lifecycle runner refuses the run phase by name, trusted or not.
    let ctx = ScriptContext {
        project: &project,
        project_root: &root,
        workstream: bisa_core::WorkstreamId::primary_of(project.id),
        path: &root,
        branch: None,
        base: None,
        goal: Some(goal),
        work_item: None,
        agent: None,
    };
    let err = scripts::run_phase(
        engine.inner(),
        &ctx,
        Phase::Run,
        &root,
        ScriptPolicy::Report,
    )
    .await
    .expect_err("never a lifecycle run");
    assert!(
        matches!(err, EngineError::Invalid(ref m) if m.to_string().contains("Terminal menu")),
        "{err:?}"
    );
    assert!(!root.join("node_modules").exists(), "and nothing ran");
}

fn set_script(engine: &Engine, project: &Project, key: &str, text: &str) {
    engine
        .workspace()
        .set_setting(
            bisa_core::SettingScope::Project,
            Some(project.id),
            key,
            serde_json::json!(text),
        )
        .unwrap();
}

/// Approve the project's current scripts on this machine — what *Approve* does.
fn approve_scripts(engine: &Engine, project: &Project) {
    bisa_engine::scripts::approve(engine.inner(), project).unwrap();
}

fn by_hand(label: &str, branch: Option<&str>, goal: Option<GoalId>) -> projects::WorkstreamRequest {
    from_source(
        label,
        WorkstreamSource::NewBranch {
            name: branch.map(str::to_string),
            start: None,
        },
        goal,
    )
}

fn from_source(
    label: &str,
    source: WorkstreamSource,
    goal: Option<GoalId>,
) -> projects::WorkstreamRequest {
    projects::WorkstreamRequest {
        goal,
        work_item: None,
        label: label.into(),
        agent: None,
        source,
        base: None,
    }
}

/// The branch a checkout stands on and the commit it is at, read from git.
fn standing(path: &Path) -> (String, String) {
    (
        raw_git(path, &["rev-parse", "--abbrev-ref", "HEAD"]),
        raw_git(path, &["rev-parse", "HEAD"]),
    )
}

/// A branch pushed to the local bare origin from a second clone — one this
/// repository has never heard of, the shape of somebody else's branch.
fn pushed_from_elsewhere(dir: &tempfile::TempDir, origin: &Path, name: &str, base: &str) -> String {
    let other = dir.path().join(format!("other-{}", name.replace('/', "-")));
    bisa_vcs::git::clone(origin.to_str().unwrap(), &other, None).expect("clone");
    set_identity(&other);
    raw_git(&other, &["switch", "-c", name, base]);
    std::fs::write(other.join("theirs.txt"), format!("{name}\n")).unwrap();
    bisa_vcs::git::add_all(&other).unwrap();
    let head = bisa_vcs::git::commit(&other, &format!("{name}: theirs"), false).unwrap();
    bisa_vcs::git::push(&other, "origin", name, true).expect("push from elsewhere");
    head.as_str().to_string()
}

/// A workstream starts where its source says — an existing branch as it is,
/// a remote's branch on a tracking twin, a tag on a new branch at it — and the
/// record tells the truth about the branch and the base in every case.
#[tokio::test]
async fn a_workstream_starts_where_its_source_says() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (goal, project, root, origin) = git_project(&engine, "sourced", PublishPolicy::Auto).await;
    let base = bisa_vcs::git::default_branch(&root).unwrap();
    bisa_vcs::git::push(&root, "origin", &base, true).expect("push base");
    let base_head = raw_git(&root, &["rev-parse", "HEAD"]);

    // An existing local branch: checked out as it is, no new ref, the base as given.
    bisa_vcs::git::branch_create(&root, "topic/existing", None, false).unwrap();
    let place = projects::open_workstream_for(
        engine.inner(),
        &project,
        from_source(
            "ignored",
            WorkstreamSource::LocalBranch {
                name: "topic/existing".into(),
            },
            Some(goal),
        ),
    )
    .await
    .expect("from a local branch");
    assert_eq!(place.workstream.branch(), Some("topic/existing"));
    assert_eq!(place.workstream.base(), Some(base.as_str()));
    assert_eq!(
        standing(&place.cwd),
        ("topic/existing".to_string(), base_head.clone())
    );
    let before = bisa_vcs::git::branch_list(&root, None).unwrap().len();

    // A remote branch: fetched, then a local twin that tracks it.
    let theirs = pushed_from_elsewhere(&dir, &origin, "feature/theirs", &base);
    let place = projects::open_workstream_for(
        engine.inner(),
        &project,
        from_source(
            "ignored",
            WorkstreamSource::RemoteBranch {
                remote: "origin".into(),
                name: "feature/theirs".into(),
            },
            Some(goal),
        ),
    )
    .await
    .expect("from a remote branch");
    assert_eq!(place.workstream.branch(), Some("feature/theirs"));
    assert_eq!(standing(&place.cwd), ("feature/theirs".to_string(), theirs));
    assert_eq!(
        bisa_vcs::git::upstream_of(&place.cwd).unwrap().as_deref(),
        Some("origin/feature/theirs"),
        "the twin tracks the remote's branch"
    );
    assert_eq!(
        bisa_vcs::git::branch_list(&root, None).unwrap().len(),
        before + 1
    );

    // A tag: a new branch at it, named from the tag unless a person named one.
    raw_git(&root, &["tag", "v1.0.0"]);
    let place = projects::open_workstream_for(
        engine.inner(),
        &project,
        from_source(
            "ignored",
            WorkstreamSource::Tag {
                name: "v1.0.0".into(),
                branch: None,
                create_at: None,
            },
            Some(goal),
        ),
    )
    .await
    .expect("from a tag");
    assert_eq!(place.workstream.branch(), Some("from/v1.0.0"));
    assert_eq!(
        standing(&place.cwd),
        ("from/v1.0.0".to_string(), base_head.clone())
    );

    // A tag made on the way: it exists at the ref asked for, and the branch a
    // person named stands on it.
    let place = projects::open_workstream_for(
        engine.inner(),
        &project,
        from_source(
            "ignored",
            WorkstreamSource::Tag {
                name: "v1.1.0".into(),
                branch: Some("release/1.1".into()),
                create_at: Some(base.clone()),
            },
            Some(goal),
        ),
    )
    .await
    .expect("from a new tag");
    assert_eq!(raw_git(&root, &["rev-parse", "v1.1.0^{commit}"]), base_head);
    assert_eq!(place.workstream.branch(), Some("release/1.1"));
    assert_eq!(standing(&place.cwd).0, "release/1.1");

    // A typed new branch at a start of its own, with the base kept apart.
    let place = projects::open_workstream_for(
        engine.inner(),
        &project,
        projects::WorkstreamRequest {
            base: Some("topic/existing".into()),
            ..from_source(
                "ignored",
                WorkstreamSource::NewBranch {
                    name: Some("feature/from tag".into()),
                    start: Some("v1.0.0".into()),
                },
                Some(goal),
            )
        },
    )
    .await
    .expect("a new branch at a tag");
    assert_eq!(
        place.workstream.branch(),
        Some("feature/from-tag"),
        "made safe, never refused"
    );
    assert_eq!(
        place.workstream.base(),
        Some("topic/existing"),
        "the base is the branch the work returns to, not where it started"
    );

    // A new branch whose name exists is refused by name — never a silent
    // checkout of the other branch.
    let err = projects::open_workstream_for(
        engine.inner(),
        &project,
        by_hand("ignored", Some("topic/existing"), Some(goal)),
    )
    .await
    .refused("exists");
    assert!(
        matches!(&err, EngineError::Invalid(s) if s.to_string().contains("topic/existing exists") && s.to_string().contains("from the branch")),
        "{err:?}"
    );

    engine.shutdown().await;
}

/// A workstream opened from an open pull request stands on the pull
/// request's head, returns to its base, and is born linked: the lifecycle
/// reads it as `pr_open` from the first second, through the one writer.
#[tokio::test]
async fn a_workstream_from_a_pull_request_is_born_linked() {
    let dir = tempfile::tempdir().unwrap();
    let fake = Arc::new(bisa_codehost::fake::FakeCodeHost::minimal("sourced-origin"));
    let engine = Arc::new(engine_with(
        &dir,
        EngineConfig {
            code_hosts: Some(bisa_codehost::CodeHostRegistry::new(vec![fake.clone()])),
            ..config()
        },
    ));
    let (goal, project, root, origin) = git_project(&engine, "sourced", PublishPolicy::Gated).await;
    let repo = fake
        .detect(&bisa_codehost::RemoteUrl::parse(origin.to_str().unwrap()))
        .expect("the fake claims the bare origin");
    let base = bisa_vcs::git::default_branch(&root).unwrap();
    bisa_vcs::git::push(&root, "origin", &base, true).expect("push base");
    let theirs = pushed_from_elsewhere(&dir, &origin, "feature/pr", &base);
    let pr = fake.seed_pr(&repo, "someone-else", "feature/pr", &base);

    // The list a picker offers: the open ones, from the code host, nothing cached.
    let listed = bisa_engine::codehost::list_open_prs(engine.inner(), project.id)
        .await
        .unwrap();
    assert_eq!(
        listed.iter().map(|p| p.number).collect::<Vec<_>>(),
        vec![pr.number]
    );

    let place = projects::open_workstream_for(
        engine.inner(),
        &project,
        from_source(
            "ignored",
            WorkstreamSource::PullRequest { number: pr.number },
            Some(goal),
        ),
    )
    .await
    .expect("from a pull request");
    let w = &place.workstream;
    assert_eq!(w.branch(), Some("feature/pr"));
    assert_eq!(
        w.base(),
        Some(base.as_str()),
        "the pull request's base is the base"
    );
    assert_eq!(
        w.state,
        WorkstreamState::PrOpen {
            number: pr.number,
            url: pr.url.clone()
        },
        "born linked, before anyone reads it"
    );
    assert_eq!(standing(&place.cwd), ("feature/pr".to_string(), theirs));
    assert_eq!(
        bisa_vcs::git::upstream_of(&place.cwd).unwrap().as_deref(),
        Some("origin/feature/pr")
    );
    // The lifecycle's reads work from here on: the linked pull request is the seeded one.
    let linked = bisa_engine::codehost::linked_pr(engine.inner(), w.id)
        .await
        .unwrap();
    assert_eq!(linked.map(|p| p.number), Some(pr.number));
    // The store's one writer wrote it: the record on disk agrees.
    assert_eq!(
        engine.workspace().get_workstream(w.id).unwrap().state,
        w.state
    );

    // Only an open pull request has a branch to take up.
    fake.set_pr_state(pr.number, bisa_codehost::PrState::Closed);
    let err = projects::open_workstream_for(
        engine.inner(),
        &project,
        from_source(
            "ignored",
            WorkstreamSource::PullRequest { number: pr.number },
            Some(goal),
        ),
    )
    .await
    .refused("closed");
    assert!(
        matches!(&err, EngineError::PullRequestNotOpen { number, state } if *number == pr.number && state == "closed"),
        "{err:?}"
    );
    assert_eq!(
        bisa_vcs::git::worktree_list(&root).unwrap().len(),
        2,
        "nothing on disk for a refused one"
    );

    // A pull request whose head is not on origin — a fork's — fails at the
    // fetch, and the sentence says so.
    let forked = fake.seed_pr(&repo, "someone-else", "feature/in-a-fork", &base);
    let err = projects::open_workstream_for(
        engine.inner(),
        &project,
        from_source(
            "ignored",
            WorkstreamSource::PullRequest {
                number: forked.number,
            },
            Some(goal),
        ),
    )
    .await
    .refused("a fork's branch");
    assert!(
        matches!(&err, EngineError::Invalid(s) if s.to_string().contains("not on origin") && s.to_string().contains("fork")),
        "{err:?}"
    );

    Arc::try_unwrap(engine).ok().unwrap().shutdown().await;
}

/// Every phase runs where it should, with the checkout's facts in its
/// environment; a re-run that reuses its worktree runs none of them.
#[tokio::test]
async fn workstream_scripts_run_in_their_place_with_their_facts() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (goal, project, root, _origin) =
        git_project(&engine, "scripted", PublishPolicy::Auto).await;
    let project = engine.workspace().get_project(project.id).unwrap();

    set_script(
        &engine,
        &project,
        "workstreams.script.pre_create",
        // The checkout does not exist yet; the branch is the intended one.
        "test ! -e \"$BISA_WORKSTREAM_PATH\" && printf '%s|%s|%s' \"$BISA_BRANCH\" \"$BISA_BASE\" \"$BISA_SCRIPT_PHASE\" > \"$BISA_PROJECT_PATH/.pre\" && echo 1 >> \"$BISA_PROJECT_PATH/.count\"",
    );
    set_script(
        &engine,
        &project,
        "workstreams.script.post_create",
        // In the checkout: the cwd is the workstream.
        "printf '%s|%s|%s' \"$BISA_BRANCH\" \"$BISA_BASE\" \"$(pwd)\" > .post",
    );
    set_script(
        &engine,
        &project,
        "workstreams.script.clean",
        "test -d \"$BISA_WORKSTREAM_PATH\" && printf '%s' \"$BISA_SCRIPT_PHASE\" > \"$BISA_PROJECT_PATH/.clean-$BISA_WORKSTREAM_ID\"",
    );
    approve_scripts(&engine, &project);

    let place = projects::open_workstream_for(
        engine.inner(),
        &project,
        by_hand("Dark mode", Some("feature/dark-mode"), Some(goal)),
    )
    .await
    .expect("opened");
    let base = bisa_vcs::git::default_branch(&root).unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join(".pre")).unwrap(),
        format!("feature/dark-mode|{base}|pre_create"),
        "pre-create ran in the project root before the checkout existed, told the intended branch"
    );
    let post = std::fs::read_to_string(place.cwd.join(".post")).unwrap();
    let want = format!("feature/dark-mode|{base}|");
    assert!(post.starts_with(&want), "{post}");
    let cwd = std::path::PathBuf::from(post.trim_start_matches(&want));
    assert_eq!(
        cwd.canonicalize().unwrap(),
        place.cwd.canonicalize().unwrap(),
        "post-create ran in the checkout"
    );

    // Both are facts on the goal's journal, and both are events on the bus.
    let journal = engine
        .workspace()
        .journal(&bisa_core::Home::from(goal))
        .unwrap();
    use bisa_core::event::JournalPayload as P;
    let notes: Vec<String> = journal
        .iter()
        .filter_map(|e| match &e.payload {
            P::Note { text } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert!(
        notes
            .iter()
            .any(|t| t.contains("ran the pre-create script") && t.contains("succeeded")),
        "{notes:#?}"
    );
    assert!(
        notes
            .iter()
            .any(|t| t.contains("ran the post-create script")),
        "{notes:#?}"
    );

    // Clean runs while the tree is still there, then the tree goes.
    let wid = place.workstream.id;
    projects::close_workstream(engine.inner(), wid, true)
        .await
        .unwrap();
    assert!(!place.cwd.exists());
    assert_eq!(
        std::fs::read_to_string(root.join(format!(".clean-{wid}"))).unwrap(),
        "clean"
    );

    // A work item's re-run reuses its worktree: no script runs twice. The
    // index ties a workstream to a *stored* item — the executor's items
    // always are — so the fixture's is put first.
    let spec = item(goal, Some(&project), "Fix the cart total");
    engine.workspace().put_work_item(&spec).unwrap();
    let first = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap();
    let again = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap();
    assert_eq!(first.workstream.id, again.workstream.id);
    let count = std::fs::read_to_string(root.join(".count")).unwrap();
    assert_eq!(
        count.lines().count(),
        2,
        "one by hand, one for the item, none for the reuse: {count:?}"
    );

    engine.shutdown().await;
}

/// A failing pre-create script refuses the workstream — nothing on disk,
/// nothing on record; a failing post-create leaves the workstream and says
/// so; a failing clean keeps the tree, unless the caller asked to report only.
#[tokio::test]
async fn a_failing_script_refuses_what_it_guards_and_reports_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (goal, project, root, _origin) = git_project(&engine, "brittle", PublishPolicy::Auto).await;
    let project = engine.workspace().get_project(project.id).unwrap();

    set_script(
        &engine,
        &project,
        "workstreams.script.pre_create",
        "echo nope >&2; exit 3",
    );
    approve_scripts(&engine, &project);
    let err = projects::open_workstream_for(
        engine.inner(),
        &project,
        by_hand("Dark mode", None, Some(goal)),
    )
    .await
    .refused("refused");
    match &err {
        EngineError::WorkstreamScript {
            phase,
            reason,
            output,
        } => {
            assert_eq!(*phase, bisa_engine::scripts::Phase::PreCreate);
            assert!(reason.contains("status 3"), "{reason}");
            assert_eq!(output, "nope");
        }
        other => panic!("expected a script refusal, got {other:?}"),
    }
    assert!(err.is_refusal());
    assert!(
        err.to_string()
            .contains("pre-create script exited with status 3"),
        "{err}"
    );
    assert_eq!(
        bisa_vcs::git::worktree_list(&root).unwrap().len(),
        1,
        "no worktree was added"
    );
    assert_eq!(
        engine
            .workspace()
            .list_workstreams(WorkstreamFilter::Project(project.id))
            .unwrap()
            .len(),
        1,
        "only the primary is on record"
    );

    // Post-create: the workstream is real by then; its failure is a note.
    set_script(&engine, &project, "workstreams.script.pre_create", "");
    set_script(
        &engine,
        &project,
        "workstreams.script.post_create",
        "echo half-done; exit 2",
    );
    approve_scripts(&engine, &project);
    let place = projects::open_workstream_for(
        engine.inner(),
        &project,
        by_hand("Dark mode", None, Some(goal)),
    )
    .await
    .expect("the workstream exists whatever post-create said");
    assert!(place.cwd.is_dir());
    let journal = engine
        .workspace()
        .journal(&bisa_core::Home::from(goal))
        .unwrap();
    use bisa_core::event::JournalPayload as P;
    assert!(
        journal.iter().any(|e| matches!(&e.payload, P::Note { text } if text.contains("post-create script") && text.contains("status 2") && text.contains("half-done"))),
        "{journal:#?}"
    );

    // Clean: refused by default, so the tree and the record stay.
    set_script(&engine, &project, "workstreams.script.post_create", "");
    set_script(
        &engine,
        &project,
        "workstreams.script.clean",
        "echo still-running >&2; exit 4",
    );
    approve_scripts(&engine, &project);
    let wid = place.workstream.id;
    let err = projects::close_workstream(engine.inner(), wid, true)
        .await
        .refused("kept");
    assert!(
        matches!(&err, EngineError::WorkstreamScript { phase: bisa_engine::scripts::Phase::Clean, output, .. } if output == "still-running"),
        "{err:?}"
    );
    assert!(place.cwd.is_dir(), "the checkout stays");
    assert_eq!(
        engine.workspace().get_workstream(wid).unwrap().state,
        WorkstreamState::Open
    );
    // The paths that never asked a person report and carry on.
    projects::close_workstream_with(
        engine.inner(),
        wid,
        true,
        bisa_engine::scripts::ScriptPolicy::Report,
    )
    .await
    .unwrap();
    assert!(!place.cwd.exists());
    assert_eq!(
        engine.workspace().get_workstream(wid).unwrap().state,
        WorkstreamState::Closed
    );

    engine.shutdown().await;
}

/// A script's text syncs with the project; the trust to run it is this
/// machine's alone. Unapproved, it never runs — and a changed text is
/// unapproved again.
#[tokio::test]
async fn an_unapproved_script_never_runs() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (goal, project, root, _origin) = git_project(&engine, "synced", PublishPolicy::Auto).await;
    let project = engine.workspace().get_project(project.id).unwrap();

    set_script(
        &engine,
        &project,
        "workstreams.script.pre_create",
        "touch \"$BISA_PROJECT_PATH/.ran\"",
    );
    let status = bisa_engine::scripts::status(engine.inner(), &project).unwrap();
    assert_eq!(
        status.len(),
        4,
        "the three lifecycle scripts and the run command"
    );
    assert!(
        !status[0].trusted && status[1..].iter().all(|s| s.trusted),
        "an empty script needs no approval: {status:?}"
    );

    let err = projects::open_workstream_for(
        engine.inner(),
        &project,
        by_hand("Dark mode", None, Some(goal)),
    )
    .await
    .refused("not approved here");
    assert!(
        matches!(&err, EngineError::WorkstreamScript { reason, .. } if reason.contains("not approved on this machine")),
        "{err:?}"
    );
    assert!(
        !root.join(".ran").exists(),
        "an unapproved script never ran"
    );
    assert_eq!(bisa_vcs::git::worktree_list(&root).unwrap().len(), 1);

    approve_scripts(&engine, &project);
    assert!(bisa_engine::scripts::status(engine.inner(), &project)
        .unwrap()
        .iter()
        .all(|s| s.trusted));
    projects::open_workstream_for(
        engine.inner(),
        &project,
        by_hand("Dark mode", None, Some(goal)),
    )
    .await
    .expect("approved, it runs");
    assert!(root.join(".ran").exists());

    // The text changed under the approval — a synced edit, say.
    set_script(
        &engine,
        &project,
        "workstreams.script.pre_create",
        "touch \"$BISA_PROJECT_PATH/.ran-again\"",
    );
    let err = projects::open_workstream_for(
        engine.inner(),
        &project,
        by_hand("Light mode", None, Some(goal)),
    )
    .await
    .refused("a changed text is unapproved again");
    assert!(matches!(err, EngineError::WorkstreamScript { .. }));
    assert!(!root.join(".ran-again").exists());

    engine.shutdown().await;
}

/// A script that hangs is terminated at the project's timeout and counts as
/// a failure — never a workstream that waits forever to be created.
#[tokio::test]
async fn a_hanging_script_is_terminated_at_the_timeout() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (goal, project, _root, _origin) = git_project(&engine, "slow", PublishPolicy::Auto).await;
    let project = engine.workspace().get_project(project.id).unwrap();
    set_script(
        &engine,
        &project,
        "workstreams.script.pre_create",
        "sleep 30",
    );
    engine
        .workspace()
        .set_setting(
            bisa_core::SettingScope::Project,
            Some(project.id),
            "workstreams.script.timeout_secs",
            serde_json::json!(1),
        )
        .unwrap();
    approve_scripts(&engine, &project);

    let started = std::time::Instant::now();
    let err = projects::open_workstream_for(
        engine.inner(),
        &project,
        by_hand("Dark mode", None, Some(goal)),
    )
    .await
    .refused("timed out");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "terminated at the timeout, not at the sleep"
    );
    assert!(
        matches!(&err, EngineError::WorkstreamScript { reason, .. } if reason.contains("timed out after 1s") && reason.contains("terminated")),
        "{err:?}"
    );

    engine.shutdown().await;
}

/// `expect_err` for a result whose success type has no `Debug` (a `Workplace`).
trait Refused {
    fn refused(self, what: &str) -> EngineError;
}

impl<T> Refused for Result<T, EngineError> {
    fn refused(self, what: &str) -> EngineError {
        match self {
            Ok(_) => panic!("expected a refusal: {what}"),
            Err(e) => e,
        }
    }
}

/// Answering a review is a commit on a branch whose pull request is open —
/// and one on a branch already merged is a commit like any other: the commit
/// lands, is said, and the record keeps the pull request it knows.
#[tokio::test(flavor = "multi_thread")]
async fn a_commit_on_a_branch_whose_pull_request_is_open_lands_and_keeps_the_pull_request() {
    let fake = deleting_host(false);
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(
        &dir,
        EngineConfig {
            code_hosts: Some(bisa_codehost::CodeHostRegistry::new(vec![fake.clone()])),
            ..config()
        },
    );
    let (goal, project, _root, _origin) =
        git_project(&engine, "storefront", PublishPolicy::Auto).await;
    let spec = item(goal, Some(&project), "Add the checkout flow");
    let w = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap()
        .workstream;
    let path = engine.workspace().workstream_checkout(&w).unwrap();
    std::fs::write(path.join("checkout.rs"), "fn checkout() {}\n").unwrap();
    projects::commit_workstream(engine.inner(), w.id, "add checkout")
        .await
        .unwrap();
    projects::open_pr(engine.inner(), w.id, "Add checkout", "")
        .await
        .expect("auto: pushed and opened");
    let opened = engine.workspace().get_workstream(w.id).unwrap().state;
    assert!(
        matches!(opened, WorkstreamState::PrOpen { .. }),
        "{opened:?}"
    );

    // The reviewer asked for a change.
    std::fs::write(
        path.join("checkout.rs"),
        "fn checkout() { /* reviewed */ }\n",
    )
    .unwrap();
    let commit = projects::commit_workstream(engine.inner(), w.id, "address the review")
        .await
        .expect("a commit that landed is not an error");
    assert!(!commit.to_string().is_empty());
    assert_eq!(
        engine.workspace().get_workstream(w.id).unwrap().state,
        opened,
        "the record still names its pull request"
    );
    engine.shutdown().await;
}

/// The status a panel reads is never the one from before a write to the
/// index: a stage, an unstage and a hunk in or out are each seen by the very
/// next read, inside the cache's time.
#[tokio::test(flavor = "multi_thread")]
async fn the_status_is_never_stale_after_a_write_to_the_index() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (goal, project, _root, _origin) =
        git_project(&engine, "storefront", PublishPolicy::Gated).await;
    let inner = engine.inner();
    let w = projects::open_workstream(inner, &item(goal, Some(&project), "Cart"), &project)
        .await
        .unwrap()
        .workstream;
    let path = engine.workspace().workstream_checkout(&w).unwrap();
    std::fs::write(path.join("cart.rs"), "fn cart() {}\n").unwrap();
    std::fs::write(path.join("total.rs"), "fn total() {}\n").unwrap();

    let counts = |s: &bisa_engine::ide::git::WorkstreamStatus| (s.staged, s.unstaged, s.untracked);
    let read = || async {
        bisa_engine::ide::git::workstream_status(inner, w.id)
            .await
            .unwrap()
    };
    // Read once: what follows is asked well inside the cache's time.
    assert_eq!(counts(&read().await), (0, 0, 2));

    projects::stage_in(inner, w.id, vec!["cart.rs".into()])
        .await
        .unwrap();
    assert_eq!(counts(&read().await), (1, 0, 1), "a stage is seen at once");
    projects::stage_in(inner, w.id, vec!["total.rs".into()])
        .await
        .unwrap();
    assert_eq!(counts(&read().await), (2, 0, 0));
    projects::unstage_in(inner, w.id, vec!["cart.rs".into()])
        .await
        .unwrap();
    assert_eq!(counts(&read().await), (1, 0, 1), "and so is an unstage");

    // A refused write forgets the status too: whatever it did, git is asked again.
    assert!(
        projects::stage_in(inner, w.id, vec!["no-such-file.rs".into()])
            .await
            .is_err()
    );
    assert_eq!(counts(&read().await), (1, 0, 1));
    // A patch that is nothing, and one that is no patch, are refused in words.
    assert!(
        bisa_engine::ide::git::stage_hunk(inner, w.id, "  \n".into(), false)
            .await
            .is_err()
    );
    assert!(
        bisa_engine::ide::git::stage_hunk(inner, w.id, "not a patch\n".into(), false)
            .await
            .is_err()
    );
    assert_eq!(
        counts(&read().await),
        (1, 0, 1),
        "a refused hunk moved nothing"
    );
    engine.shutdown().await;
}

/// A card set down where it already is writes nothing and announces nothing,
/// however many times; and drops arriving together never share a rank.
#[tokio::test(flavor = "multi_thread")]
async fn a_drop_in_place_writes_nothing_and_drops_arriving_together_never_share_a_rank() {
    use bisa_core::BoardColumn;

    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (goal, project, _root, _origin) =
        git_project(&engine, "storefront", PublishPolicy::Gated).await;
    let inner = engine.inner();
    let mut cards = Vec::new();
    for title in ["One", "Two", "Three", "Four", "Five", "Six"] {
        let place = projects::open_workstream(inner, &item(goal, Some(&project), title), &project)
            .await
            .expect(title);
        cards.push(place.workstream.id);
    }
    for (index, card) in cards.iter().take(3).enumerate() {
        projects::place_workstream_card(inner, *card, BoardColumn::Doing, index).unwrap();
    }
    let before: Vec<Option<u32>> = cards
        .iter()
        .take(3)
        .map(|id| inner.ws.get_workstream(*id).unwrap().board.rank)
        .collect();

    // Each of the three, set down in its own place, twenty times over.
    let mut rx = engine.events();
    for _ in 0..20 {
        for (index, card) in cards.iter().take(3).enumerate() {
            let written =
                projects::place_workstream_card(inner, *card, BoardColumn::Doing, index).unwrap();
            assert!(written.is_empty(), "nothing moved, nothing written");
        }
    }
    // Past the end is the end: the last card is already there.
    assert!(
        projects::place_workstream_card(inner, cards[2], BoardColumn::Doing, 99)
            .unwrap()
            .is_empty()
    );
    let after: Vec<Option<u32>> = cards
        .iter()
        .take(3)
        .map(|id| inner.ws.get_workstream(*id).unwrap().board.rank)
        .collect();
    assert_eq!(after, before, "no record rewritten, no gap halved");
    assert!(
        rx.try_recv().is_err(),
        "and nothing was announced for a move that was none"
    );

    // Three more cards dropped at the front of the same column at once.
    let drops: Vec<_> = cards[3..]
        .iter()
        .map(|card| {
            let (inner, card) = (Arc::clone(inner), *card);
            tokio::task::spawn_blocking(move || {
                projects::place_workstream_card(&inner, card, BoardColumn::Doing, 0).unwrap();
            })
        })
        .collect();
    for drop in drops {
        drop.await.unwrap();
    }
    let mut ranks: Vec<u32> = cards
        .iter()
        .map(|id| inner.ws.get_workstream(*id).unwrap().board.rank.unwrap())
        .collect();
    ranks.sort_unstable();
    let distinct: std::collections::BTreeSet<u32> = ranks.iter().copied().collect();
    assert_eq!(
        distinct.len(),
        6,
        "every card has a place of its own: {ranks:?}"
    );
    engine.shutdown().await;
}

/// The Board (ide/16): a card's column and rank are a view over the
/// workstream, placed by index — the state never moves; a closed card is
/// Archived and nothing else; a due date is the fourth person-editable
/// field; every write announces itself.
#[tokio::test]
async fn cards_are_placed_on_the_board_by_index_and_the_column_is_never_the_state() {
    use bisa_core::{BoardColumn, DueDate, WorkstreamBoard};
    use bisa_engine::WorkstreamEdit;

    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (goal, project, _root, _origin) =
        git_project(&engine, "storefront", PublishPolicy::Gated).await;
    let inner = engine.inner();
    let mut opened = Vec::new();
    for title in ["Checkout", "Cart", "Search"] {
        let place = projects::open_workstream(inner, &item(goal, Some(&project), title), &project)
            .await
            .expect(title);
        opened.push(place.workstream);
    }
    let (a, b, c) = (opened[0].clone(), opened[1].clone(), opened[2].clone());

    // Unplaced: the lifecycle says where, and the record carries no slot.
    assert!(a.board.is_empty());
    assert_eq!(
        BoardColumn::shown(a.board.column, &a.state),
        BoardColumn::Backlog
    );

    // a to Todo, b before it, c between: a rank between the neighbours each time.
    let w = projects::place_workstream_card(inner, a.id, BoardColumn::Todo, 0).unwrap();
    assert_eq!(w.len(), 1, "one record rewritten");
    assert_eq!(
        w[0].board,
        WorkstreamBoard {
            column: Some(BoardColumn::Todo),
            rank: Some(1024),
            due: None
        }
    );
    let w = projects::place_workstream_card(inner, b.id, BoardColumn::Todo, 0).unwrap();
    assert_eq!(w[0].board.rank, Some(512), "before the first");
    let w = projects::place_workstream_card(inner, c.id, BoardColumn::Todo, 1).unwrap();
    assert_eq!(w[0].board.rank, Some(768), "between");
    let todo = |inner: &bisa_engine::Inner| {
        let mut rows: Vec<_> = inner
            .ws
            .list_workstreams(WorkstreamFilter::All)
            .unwrap()
            .into_iter()
            .filter(|w| w.board.column == Some(BoardColumn::Todo))
            .collect();
        rows.sort_by_key(|w| w.board.rank);
        rows.into_iter().map(|w| w.id).collect::<Vec<_>>()
    };
    assert_eq!(todo(inner), vec![b.id, c.id, a.id]);
    assert_eq!(
        engine.workspace().get_workstream(a.id).unwrap().state,
        WorkstreamState::Open,
        "a column is not the state"
    );

    // The room before the first card halves with every card that takes the
    // front — the neighbours are the *other* cards, so a card dropped before
    // the same neighbour twice takes the same midpoint — until two ranks
    // touch: then the column is renumbered a step apart. `b` and `a` take
    // turns at the front, `a` first; the tenth move is `b`'s and closes the gap.
    let mut renumbered = None;
    for turn in 0..12 {
        // `b` is at the front already, and a card set down in its own place
        // moves nothing: `a` goes first, so every turn is a move.
        let card = if turn % 2 == 0 { a.id } else { b.id };
        let w = projects::place_workstream_card(inner, card, BoardColumn::Todo, 0).unwrap();
        if w.len() == 3 {
            renumbered = Some((turn, w));
            break;
        }
    }
    let (turn, w) = renumbered.expect("the gap closes within twelve moves");
    assert_eq!(
        turn, 9,
        "384, 192, 96, 48, 24, 12, 6, 3, 1 — then nothing between"
    );
    assert_eq!(
        w.iter().map(|w| w.id).collect::<Vec<_>>(),
        vec![b.id, a.id, c.id]
    );
    assert_eq!(
        w.iter().map(|w| w.board.rank.unwrap()).collect::<Vec<_>>(),
        vec![1024, 2048, 3072]
    );
    assert_eq!(todo(inner), vec![b.id, a.id, c.id]);

    // A due date lands and clears; the column stays.
    let w = projects::edit_workstream(
        inner,
        a.id,
        WorkstreamEdit {
            due: Some(Some(DueDate::parse("2026-09-30").unwrap())),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(w.board.due.as_ref().map(|d| d.as_str()), Some("2026-09-30"));
    assert_eq!(w.board.column, Some(BoardColumn::Todo));
    let w = projects::edit_workstream(
        inner,
        a.id,
        WorkstreamEdit {
            due: Some(None),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(w.board.due, None);
    assert_eq!(
        w.board.column,
        Some(BoardColumn::Todo),
        "clearing the date keeps the place"
    );

    // A closed workstream is Archived and nothing else.
    engine
        .workspace()
        .transition_workstream(c.id, &WorkstreamTransition::Close)
        .unwrap();
    assert!(matches!(
        projects::place_workstream_card(inner, c.id, BoardColumn::Doing, 0),
        Err(EngineError::Invalid(_))
    ));
    let w = projects::place_workstream_card(inner, c.id, BoardColumn::Archived, 0).unwrap();
    assert_eq!(w[0].board.column, Some(BoardColumn::Archived));

    // A placement announces itself as an edit, ids only.
    let mut rx = inner.subscribe();
    projects::place_workstream_card(inner, b.id, BoardColumn::Doing, 0).unwrap();
    let ev = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
        .await
        .expect("an event")
        .expect("bus");
    assert!(
        matches!(ev.payload, EnginePayload::WorkstreamEdited { workstream, .. } if workstream == b.id),
        "{:?}",
        ev.payload
    );
}

/// Who commits is answered by every door that makes a repository resolve an
/// identity — not only a local pair. The global identity written from
/// Settings is announced as the setup moving and settles the desk for every
/// project that now inherits it; one local key over a global one resolves
/// too, and is announced the same.
#[tokio::test(flavor = "multi_thread")]
async fn the_global_identity_and_a_partial_local_one_answer_the_desk() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config_with_git(isolated_git(&dir)));
    let (_goal, project) = plain_project(&engine, "waiting");
    let project = projects::init_git(engine.inner(), &project)
        .await
        .expect("init_git");
    let wid = bisa_core::WorkstreamId::primary_of(project.id);
    let mut rx = engine.events();
    // A commit nobody signs puts the project on the desk.
    std::fs::write(
        engine.workspace().project_root_path(&project).join("a.txt"),
        "a\n",
    )
    .unwrap();
    let refused = projects::commit_in(engine.inner(), wid, "early", vec!["a.txt".into()]).await;
    assert!(matches!(refused, Err(EngineError::IdentityUnset { .. })));
    assert_eq!(
        bisa_engine::identity::overview(engine.inner())
            .await
            .unwrap()
            .pending
            .len(),
        1
    );

    // The person sets their global identity: the setup moved, and the
    // repository that inherits it is answered without anyone touching it.
    bisa_engine::identity::set_global_config(
        engine.inner(),
        vec![
            ("user.name".into(), "Grace Hopper".into()),
            ("user.email".into(), "grace@example.invalid".into()),
        ],
        vec![],
    )
    .await
    .expect("global config");
    let moved = common::wait_for(&mut rx, "git_setup_changed", |e| {
        matches!(&e.payload, EnginePayload::GitSetupChanged { .. })
    })
    .await;
    assert!(matches!(
        moved.payload,
        EnginePayload::GitSetupChanged {
            what: bisa_engine::events::GitSetup::Identity
        }
    ));
    let set = common::wait_for(&mut rx, "committer_set", |e| {
        matches!(&e.payload, EnginePayload::CommitterSet { .. })
    })
    .await;
    assert!(
        matches!(&set.payload, EnginePayload::CommitterSet { project: p, identity, .. } if *p == project.id && identity.name == "Grace Hopper")
    );
    assert!(
        bisa_engine::identity::overview(engine.inner())
            .await
            .unwrap()
            .pending
            .is_empty(),
        "the desk is clear"
    );
    let now = bisa_engine::ide::git::identity(engine.inner(), wid)
        .await
        .unwrap();
    assert_eq!(now.source, bisa_engine::ide::git::IdentitySource::Global);

    // A second repository, asked for a committer; one local key over the
    // global layer resolves — the same reading the creation policy makes.
    let (_goal, second) = plain_project(&engine, "partial");
    let second = projects::init_git(engine.inner(), &second)
        .await
        .expect("init_git");
    let wid2 = bisa_core::WorkstreamId::primary_of(second.id);
    bisa_engine::identity::set_global_config(engine.inner(), vec![], vec!["user.name".into()])
        .await
        .expect("unset");
    std::fs::write(
        engine.workspace().project_root_path(&second).join("b.txt"),
        "b\n",
    )
    .unwrap();
    let refused = projects::commit_in(engine.inner(), wid2, "early", vec!["b.txt".into()]).await;
    assert!(
        matches!(refused, Err(EngineError::IdentityUnset { .. })),
        "an email alone is nobody"
    );
    let mut rx = engine.events();
    bisa_engine::ide::git::set_local_config(
        engine.inner(),
        wid2,
        vec![("user.name".into(), "Ada Lovelace".into())],
        vec![],
    )
    .await
    .expect("local name");
    let set = common::wait_for(&mut rx, "committer_set", |e| {
        matches!(&e.payload, EnginePayload::CommitterSet { .. })
    })
    .await;
    assert!(
        matches!(&set.payload, EnginePayload::CommitterSet { project: p, identity, .. } if *p == second.id && identity.name == "Ada Lovelace" && identity.email == "grace@example.invalid")
    );
    let now = bisa_engine::ide::git::identity(engine.inner(), wid2)
        .await
        .unwrap();
    assert_eq!(
        now.source,
        bisa_engine::ide::git::IdentitySource::Global,
        "one local key over the global layer is a global identity, and it resolves"
    );
    assert!(bisa_engine::identity::overview(engine.inner())
        .await
        .unwrap()
        .pending
        .is_empty());
    engine.shutdown().await;
}

/// A GitHub-shaped token that is not one — the built-in rule recognises the
/// shape, nothing accepts the value.
const FAKE_TOKEN: &str = "ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
const PLACEHOLDER: &str = "«secret:github_token:";

/// Trusted by hash, still read by the rules: a script whose line spells a
/// refused shape does not run, and the refusal names the line and the rule.
#[tokio::test]
async fn a_trusted_script_whose_line_a_rule_refuses_never_runs() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (goal, project, root, _origin) = git_project(&engine, "guarded", PublishPolicy::Auto).await;
    let project = engine.workspace().get_project(project.id).unwrap();
    engine
        .set_setting(
            bisa_core::SettingScope::Workspace,
            None,
            "security.guard.rules",
            serde_json::json!([{
                "id": "no_fake_tool",
                "label": "no_fake_tool rule",
                "action": "deny",
                "matcher": { "kind": "command", "regex": "^fake-tool\\b" }
            }]),
        )
        .unwrap();
    set_script(
        &engine,
        &project,
        "workstreams.script.pre_create",
        "# prepare\nfake-tool wipe --all\ntouch \"$BISA_PROJECT_PATH/.ran\"",
    );
    approve_scripts(&engine, &project);
    assert!(bisa_engine::scripts::status(engine.inner(), &project)
        .unwrap()
        .iter()
        .all(|s| s.trusted));

    let err = projects::open_workstream_for(
        engine.inner(),
        &project,
        by_hand("Dark mode", None, Some(goal)),
    )
    .await
    .refused("a refused line");
    assert!(
        matches!(&err, EngineError::WorkstreamScript { reason, .. } if reason.contains("line 2") && reason.contains("no_fake_tool rule")),
        "{err:?}"
    );
    assert!(!root.join(".ran").exists(), "no line of it ran");
    assert_eq!(bisa_vcs::git::worktree_list(&root).unwrap().len(), 1);
    engine.shutdown().await;
}

/// The title of a pull request leaves the machine: a token in it — whoever
/// wrote it — travels as a placeholder.
#[tokio::test(flavor = "multi_thread")]
async fn a_pull_request_title_carrying_a_token_leaves_as_a_placeholder() {
    let dir = tempfile::tempdir().unwrap();
    let fake = Arc::new(bisa_codehost::fake::FakeCodeHost::minimal(
        "redacted-origin",
    ));
    let engine = engine_with(
        &dir,
        EngineConfig {
            code_hosts: Some(bisa_codehost::CodeHostRegistry::new(vec![fake])),
            ..config()
        },
    );
    let (goal, project, _root, _origin) =
        git_project(&engine, "redacted", PublishPolicy::Auto).await;
    let spec = item(goal, Some(&project), "Add the checkout flow");
    let w = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap()
        .workstream;
    for step in [
        WorkstreamTransition::Committed,
        WorkstreamTransition::Pushed,
    ] {
        engine
            .workspace()
            .transition_workstream(w.id, &step)
            .unwrap();
    }
    let pr = projects::open_pr(
        engine.inner(),
        w.id,
        &format!("Rotate {FAKE_TOKEN}"),
        &format!("The old key {FAKE_TOKEN} is revoked."),
    )
    .await
    .expect("pr created");
    assert!(
        !pr.title.contains(FAKE_TOKEN) && pr.title.contains(PLACEHOLDER),
        "{}",
        pr.title
    );
    engine.shutdown().await;
}

/// The settlement commit's message is the work item's own words: a token in
/// the instructions is committed as a placeholder.
#[tokio::test(flavor = "multi_thread")]
async fn a_settlement_commit_message_carries_a_placeholder_not_the_token() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (goal, project, _root, _origin) =
        git_project(&engine, "settled", PublishPolicy::Auto).await;
    let spec = item(
        goal,
        Some(&project),
        &format!("Rotate {FAKE_TOKEN} in the deploy"),
    );
    let place = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap();
    std::fs::write(place.cwd.join("rotated.txt"), "done\n").unwrap();
    projects::commit_on_settle(engine.inner(), &spec, place.workstream.id).await;
    let out = Command::new("git")
        .args(["log", "-1", "--format=%B"])
        .current_dir(&place.cwd)
        .output()
        .unwrap();
    let message = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        !message.contains(FAKE_TOKEN) && message.contains(PLACEHOLDER),
        "{message}"
    );
    assert!(message.contains("bisa-work-item:"));
    engine.shutdown().await;
}

/// A put-away project takes no new work: opening a workstream on it is
/// refused by name before anything touches the disk, and the refusal lifts
/// with the archive.
#[tokio::test]
async fn an_archived_project_refuses_a_new_workstream_until_it_is_back() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (goal, project, root, _origin) = git_project(&engine, "shelved", PublishPolicy::Auto).await;
    let before = bisa_vcs::git::worktree_list(&root).unwrap().len();
    let archived = projects::archive(engine.inner(), project.id, true).unwrap();
    assert!(archived.is_archived());

    let err = projects::open_workstream_for(
        engine.inner(),
        &archived,
        by_hand("Dark mode", None, Some(goal)),
    )
    .await
    .refused("an archived project");
    assert!(
        matches!(&err, EngineError::Invalid(why) if why.to_string().contains("archived") && why.to_string().contains("unarchive")),
        "{err:?}"
    );
    assert_eq!(
        bisa_vcs::git::worktree_list(&root).unwrap().len(),
        before,
        "nothing was made"
    );

    let back = projects::archive(engine.inner(), project.id, false).unwrap();
    projects::open_workstream_for(
        engine.inner(),
        &back,
        by_hand("Dark mode", None, Some(goal)),
    )
    .await
    .expect("unarchived, it opens");
    engine.shutdown().await;
}

/// A publish gate has nobody durable behind it: the task awaiting the
/// person died with the process. A restart withdraws the question by its
/// subject and says so on the goal, so the inbox never shows a decision
/// nobody can carry out and the person knows to ask again.
#[tokio::test(flavor = "multi_thread")]
async fn a_publish_gate_awaiting_an_answer_is_withdrawn_with_a_note_after_a_restart() {
    use bisa_core::event::JournalPayload;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, config());
    let (goal, project, _root, origin) =
        git_project(&engine, "storefront", PublishPolicy::Gated).await;
    let spec = item(goal, Some(&project), "Add the checkout flow");
    let w = projects::open_workstream(engine.inner(), &spec, &project)
        .await
        .unwrap()
        .workstream;
    let path = engine.workspace().workstream_checkout(&w).unwrap();
    std::fs::write(path.join("checkout.rs"), "fn checkout() {}\n").unwrap();
    projects::commit_workstream(engine.inner(), w.id, "add checkout")
        .await
        .unwrap();
    let pushing = tokio::spawn({
        let (inner, id) = (Arc::clone(engine.inner()), w.id);
        async move { projects::push_workstream(&inner, id).await }
    });
    let gate = pending_publish_gate(&engine).await;
    let subject = format!("workstream:{}", w.id);
    assert!(
        engine
            .workspace()
            .journal(&bisa_core::Home::from(goal))
            .unwrap()
            .iter()
            .any(
                |e| matches!(&e.payload, JournalPayload::Question { gate: g, .. } if g == &subject)
            ),
        "the question behind the gate is a fact"
    );
    assert!(engine.gate(&gate).is_some());
    // The process dies with the question open and its asker in it.
    pushing.abort();
    engine.shutdown().await;

    let engine = engine_with(&dir, config());
    let journal = engine
        .workspace()
        .journal(&bisa_core::Home::from(goal))
        .unwrap();
    assert!(
        journal.iter().any(|e| matches!(&e.payload, JournalPayload::Withdrawn { subject: s, reason } if s == &subject && reason.contains("restart"))),
        "the question was withdrawn by name: {journal:#?}"
    );
    assert!(
        common::notes(&engine, goal)
            .iter()
            .any(|n| n.contains("interrupted by a restart") && n.contains("Publish:")),
        "the goal says which decision was lost: {:?}",
        common::notes(&engine, goal)
    );
    assert!(
        engine.inbox().iter().all(|g| g.gate != Gate::Publish),
        "no dead gate is rebuilt"
    );
    assert!(
        origin_branches(&origin).is_empty(),
        "nothing was pushed by anybody"
    );
    engine.shutdown().await;
}

/// A restart forgets nothing about who commits: the desk is read from the
/// repositories at every start, so a project nobody could commit in when
/// the last process died is asked for again — `Unresolved`, once — and the
/// overview seeds the dialog with the same facts the frame carries.
#[tokio::test(flavor = "multi_thread")]
async fn a_restart_asks_again_for_a_repository_nobody_commits_in() {
    let dir = tempfile::tempdir().unwrap();
    let (goal, project) = {
        let engine = engine_with(&dir, config_with_git(isolated_git(&dir)));
        let goal = engine
            .workspace()
            .create_goal(bisa_store::NewGoal::captured("ask me after a restart"))
            .unwrap();
        let project = made_for(&engine, goal.id, "made").await;
        engine.shutdown().await;
        (goal.id, project)
    };

    let engine = engine_with(&dir, config_with_git(isolated_git(&dir)));
    // The start spawns the rearm; calling it again is idempotent — the desk
    // takes a project once — so the test does not race the spawned walk.
    bisa_engine::identity::rearm(Arc::clone(engine.inner())).await;
    let overview = bisa_engine::identity::overview(engine.inner())
        .await
        .unwrap();
    assert_eq!(overview.pending.len(), 1, "{overview:?}");
    let ask = &overview.pending[0];
    assert_eq!(ask.project, project.id);
    assert_eq!(ask.reason, bisa_engine::CommitterReason::Unresolved);
    assert_eq!(ask.origin, bisa_core::ProjectOrigin::from_goal(goal));
    assert_eq!(
        ask.global, None,
        "nothing resolves globally under the isolated git"
    );
    assert_eq!(
        ask.workstream,
        bisa_core::WorkstreamId::primary_of(project.id)
    );
    engine.shutdown().await;
}

/// `git.committer` = `pin`: a made project gets the global pair written
/// into its own config at creation — local, never global — and nothing is
/// asked; its identity reads as its own from then on.
#[tokio::test(flavor = "multi_thread")]
async fn under_pin_a_made_project_keeps_the_global_pair_as_its_own_and_is_not_asked() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(
        &dir,
        config_with_git(git_with_global(
            &dir,
            "Grace Hopper",
            "grace@example.invalid",
        )),
    );
    engine
        .set_setting(
            bisa_core::SettingScope::Workspace,
            None,
            "git.committer",
            serde_json::json!("pin"),
        )
        .unwrap();
    let ws = engine.workspace();
    let mut rx = engine.events();
    let goal = ws
        .create_goal(bisa_store::NewGoal::captured("pin it"))
        .unwrap();
    let project = made_for(&engine, goal.id, "made").await;

    let identity = identity_of(&engine, &project).await;
    assert_eq!(
        identity.source,
        bisa_vcs::IdentitySource::Local,
        "{identity:?}"
    );
    assert_eq!(identity.name.as_deref(), Some("Grace Hopper"));
    assert_eq!(identity.email.as_deref(), Some("grace@example.invalid"));
    let mut asks = 0;
    loop {
        let ev = common::wait_for(&mut rx, "the creation's frames", |_| true).await;
        match ev.payload {
            EnginePayload::CommitterNeeded { .. } => asks += 1,
            EnginePayload::AttachmentChanged { .. } => break,
            _ => {}
        }
    }
    assert_eq!(asks, 0, "pinned, not asked");
    assert!(bisa_engine::identity::overview(engine.inner())
        .await
        .unwrap()
        .pending
        .is_empty());
    engine.shutdown().await;
}

/// `git.committer` = `ask`: a made project under a global identity is asked
/// about all the same — once, `Created`, the frame carrying the global pair
/// so the dialog offers it — and nothing is written into the repository.
#[tokio::test(flavor = "multi_thread")]
async fn under_ask_a_made_project_under_a_global_identity_is_asked_once_with_the_pair() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(
        &dir,
        config_with_git(git_with_global(
            &dir,
            "Grace Hopper",
            "grace@example.invalid",
        )),
    );
    engine
        .set_setting(
            bisa_core::SettingScope::Workspace,
            None,
            "git.committer",
            serde_json::json!("ask"),
        )
        .unwrap();
    let ws = engine.workspace();
    let mut rx = engine.events();
    let goal = ws
        .create_goal(bisa_store::NewGoal::captured("ask anyway"))
        .unwrap();
    let project = made_for(&engine, goal.id, "made").await;

    assert_eq!(
        identity_of(&engine, &project).await.source,
        bisa_vcs::IdentitySource::Global,
        "nothing written into the repository"
    );
    let mut asks = Vec::new();
    loop {
        let ev = common::wait_for(&mut rx, "the creation's frames", |_| true).await;
        match ev.payload {
            EnginePayload::CommitterNeeded { reason, global, .. } => asks.push((reason, global)),
            EnginePayload::AttachmentChanged { .. } => break,
            _ => {}
        }
    }
    assert_eq!(asks.len(), 1, "asked once: {asks:?}");
    assert_eq!(asks[0].0, bisa_engine::CommitterReason::Created);
    assert_eq!(
        asks[0].1.as_ref().map(|g| g.email.as_str()),
        Some("grace@example.invalid"),
        "the frame offers the global pair"
    );
    engine.shutdown().await;
}
