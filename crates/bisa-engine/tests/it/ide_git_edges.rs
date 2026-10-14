//! Git in the IDE at its edges (ide/04, ide/05, ide/07): the safe reads
//! where git has nothing, or something broken, to answer with; a conflict
//! read whole when its file went or became a folder; the commit graph laid
//! out behind the first screen and laid again when a ref moves; review
//! notes sent by id, with their scopes, to every goal, under the wire's
//! bound; a suggestion that names a rename and says when the diff was cut;
//! a workstream opened from git's own default and a taken short name
//! retried; a branch pushed by hand recorded before the push.
//!
//! Every repository is made by the test inside a `tempfile` directory and
//! dies with it; "origin" is a bare repository on this disk; the General
//! Agent is a mock that echoes its prompt. Everything consented — a rebase,
//! an amend, a forced push with a lease — is the node's to prove, with a
//! person's own request.

use bisa_core::workitem::{WorkItemSpec, WorkItemState};
use bisa_core::{
    AgentId, Budget, DiffScope, GoalId, Project, PublishPolicy, ToolTier, Vcs, WorkItemId,
    WorkstreamId, WorkstreamKind, WorkstreamSource, WorkstreamState, WorkstreamTransition,
};
use bisa_engine::ide::{git as ide_git, graph, review};
use bisa_engine::{projects, Engine, EngineConfig, EngineError};
use bisa_harness::mock::MockAdapter;
use bisa_harness::HarnessCatalog;
use bisa_store::{FileScope, MemoryKeyStore, NewGoal, NewProject, Workspace};
use bisa_vcs::git;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

fn sealed(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn git")
}

fn raw_git(dir: &Path, args: &[&str]) -> String {
    let out = sealed(dir, args);
    assert!(
        out.status.success(),
        "git {args:?} in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Raw git whose failure is the point — a merge that conflicts.
fn git_fails(dir: &Path, args: &[&str]) -> bool {
    !sealed(dir, args).status.success()
}

fn isolated_git(dir: &Path) -> bisa_vcs::Git {
    bisa_vcs::Git::new()
        .with_env("GIT_CONFIG_GLOBAL", dir.join("no-global.gitconfig"))
        .with_env("GIT_CONFIG_NOSYSTEM", "1")
}

/// An engine whose General Agent is a mock echoing its prompt, over a git
/// that sees no config of the machine's.
fn engine(dir: &tempfile::TempDir) -> Engine {
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    let adapter = MockAdapter {
        id: "edges-mock".into(),
        ..Default::default()
    };
    let mut def = ws.get_agent(&AgentId::general()).unwrap();
    def.harness = adapter.id.clone();
    ws.update_agent(def).unwrap();
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::new(adapter) as Arc<_>);
    Engine::start(
        ws,
        catalog,
        EngineConfig {
            design_enabled: false,
            events_enabled: false,
            git: Some(isolated_git(dir.path())),
            ..Default::default()
        },
    )
    .unwrap()
}

/// A managed project that is a repository with one commit, a local
/// identity, and its default branch recorded.
async fn repo_project(engine: &Engine, slug: &str) -> (Project, PathBuf) {
    let ws = engine.workspace();
    let project = ws
        .create_project(NewProject::managed(slug).unwrap())
        .unwrap();
    let project = projects::init_git(engine.inner(), &project)
        .await
        .expect("init_git");
    let root = ws.project_root_path(&project);
    raw_git(&root, &["config", "user.name", "Bisa Test"]);
    raw_git(&root, &["config", "user.email", "test@example.invalid"]);
    std::fs::write(root.join("README.md"), "baseline\n").unwrap();
    git::add_all(&root).unwrap();
    git::commit(&root, "baseline", false).unwrap();
    (ws.get_project(project.id).unwrap(), root)
}

/// A bare repository beside the workspace, standing in for `origin`, whose
/// HEAD names `branch`.
fn bare_origin(dir: &tempfile::TempDir, name: &str, branch: &str) -> PathBuf {
    let origin = dir.path().join(format!("{name}.git"));
    std::fs::create_dir_all(&origin).unwrap();
    raw_git(
        &origin,
        &["init", "--bare", "--quiet", "--initial-branch", branch],
    );
    origin
}

fn by_hand(label: &str, branch: Option<&str>) -> projects::WorkstreamRequest {
    projects::WorkstreamRequest {
        goal: None,
        work_item: None,
        label: label.into(),
        agent: None,
        source: WorkstreamSource::NewBranch {
            name: branch.map(str::to_string),
            start: None,
        },
        base: None,
    }
}

fn item(goal: GoalId, project: &Project, instructions: &str) -> WorkItemSpec {
    WorkItemSpec {
        id: WorkItemId::from_ulid(ulid::Ulid::from_datetime(SystemTime::now())),
        home: bisa_core::Home::Goal { goal },
        run: None,
        step: None,
        instructions: instructions.into(),
        state: WorkItemState::Open,
        project: Some(project.id),
        harness_candidates: vec!["edges-mock".into()],
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

/// `count` commits on `branch`, each adding one file, through `fast-import`
/// — a long history in well under a second, without touching the tree.
fn fast_import(root: &Path, branch: &str, count: usize, from: usize) {
    let mut stream = String::from("blob\nmark :1\ndata 2\nx\n\n");
    for n in from..from + count {
        let message = format!("c{n}");
        stream.push_str(&format!(
            "commit refs/heads/{branch}\ncommitter Bisa Test <test@example.invalid> {} +0000\ndata {}\n{message}\n",
            1_700_000_000 + n,
            message.len()
        ));
        if n == from {
            stream.push_str(&format!("from refs/heads/{branch}^0\n"));
        }
        stream.push_str(&format!("M 100644 :1 f{n}.txt\n\n"));
    }
    let mut child = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["fast-import", "--quiet"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn git fast-import");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stream.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "fast-import: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

async fn window(engine: &Engine, pid: &str, refs: graph::RefScope, refresh: bool) -> graph::Window {
    graph::window(
        engine.inner(),
        FileScope::Workstream,
        pid,
        refs,
        0,
        50,
        refresh,
    )
    .await
    .unwrap()
}

/// The safe reads where git has nothing, or something broken, to answer
/// with: a hunk that does not fit taken out of the index; nothing half-done;
/// a remote that is not `origin` leaving the record alone and `origin`
/// recorded once; a recorded default branch the repository lacks replaced by
/// the trunk that exists, then by the remote's word; a tracked file gone
/// from the disk with no working-tree side and a path that became a folder
/// an error; a workstream with an open pull request saying so; a checkout
/// git can no longer read skipped from the rows; a copy's status, a plain
/// folder's branches, a push of a copy and a commit or a push on a detached
/// root each refused by name; a path reaching out of the checkout refused.
#[tokio::test(flavor = "multi_thread")]
async fn the_safe_reads_at_their_edges() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let inner = engine.inner();
    let (project, root) = repo_project(&engine, "web").await;
    let primary = WorkstreamId::primary_of(project.id);
    let ws = engine.workspace();

    assert!(
        ide_git::stage_hunk(inner, primary, "not a patch\n".into(), true)
            .await
            .is_err(),
        "a hunk that does not fit"
    );
    assert_eq!(ide_git::in_progress(inner, primary).await.unwrap(), None);

    assert!(ide_git::remotes(inner, primary).await.unwrap().is_empty());
    let branch = raw_git(&root, &["symbolic-ref", "--short", "HEAD"]);
    let origin = bare_origin(&dir, "origin", &branch);
    let url = origin.to_string_lossy().into_owned();
    let listed = ide_git::remote_add(inner, primary, "upstream".into(), url.clone())
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert!(
        matches!(
            &ws.get_project(project.id).unwrap().vcs,
            Vcs::Git { remote: None, .. }
        ),
        "a remote that is not origin is git's alone"
    );
    ide_git::remote_add(inner, primary, "origin".into(), url.clone())
        .await
        .unwrap();
    assert!(
        matches!(&ws.get_project(project.id).unwrap().vcs, Vcs::Git { remote: Some(r), .. } if r == &url)
    );
    ide_git::set_remote(inner, primary, "origin".into(), url.clone())
        .await
        .expect("pointed where it already points: nothing written");

    let mut trunkless = ws.get_project(project.id).unwrap();
    trunkless.vcs = Vcs::Git {
        default_branch: "trunk".into(),
        remote: Some(url.clone()),
        code_host: None,
    };
    ws.update_project(trunkless).unwrap();
    let branches = ide_git::branches(inner, primary).await.unwrap();
    let current = branches.iter().find(|b| b.current).expect("the branch");
    assert_eq!(current.name, branch);
    assert!(
        current.merged,
        "judged against the trunk that exists: {branches:?}"
    );
    raw_git(&root, &["push", "--quiet", "-u", "origin", &branch]);
    raw_git(&root, &["remote", "set-head", "origin", &branch]);
    let branches = ide_git::branches(inner, primary).await.unwrap();
    assert!(branches.iter().find(|b| b.current).unwrap().merged);

    std::fs::remove_file(root.join("README.md")).unwrap();
    let sides = ide_git::file_sides(inner, primary, "README.md".into(), false)
        .await
        .unwrap();
    assert_eq!(sides.modified.text, None, "{sides:?}");
    assert!(sides.original.text.is_some());
    std::fs::create_dir(root.join("README.md")).unwrap();
    assert!(
        ide_git::file_sides(inner, primary, "README.md".into(), false)
            .await
            .is_err(),
        "a folder where the file was"
    );
    std::fs::remove_dir(root.join("README.md")).unwrap();
    raw_git(&root, &["checkout", "--quiet", "--", "README.md"]);

    let place = projects::open_workstream_for(inner, &project, by_hand("hand", Some("feature/pr")))
        .await
        .unwrap();
    let w = place.workstream;
    for t in [
        WorkstreamTransition::Committed,
        WorkstreamTransition::Pushed,
        WorkstreamTransition::PrOpened {
            number: 7,
            url: "https://example.test/pr/7".into(),
        },
    ] {
        ws.transition_workstream(w.id, &t).unwrap();
    }
    let status = ide_git::workstream_status(inner, w.id).await.unwrap();
    assert_eq!(
        status.pr,
        Some(ide_git::PrRef {
            number: 7,
            url: "https://example.test/pr/7".into()
        })
    );
    assert_eq!(
        status.state,
        WorkstreamState::PrOpen {
            number: 7,
            url: "https://example.test/pr/7".into()
        }
    );

    std::fs::write(place.cwd.join(".git"), "gitdir: /nowhere/at/all\n").unwrap();
    inner.ide_status.invalidate(w.id);
    let rows = ide_git::workstream_statuses(inner, Some(project.id))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "the broken checkout is skipped: {rows:?}");
    assert_eq!(rows[0].workstream, primary);

    let plain = ws
        .create_project(NewProject::managed("plain").unwrap())
        .unwrap();
    let plain_root = projects::materialize(inner, &plain).await.unwrap();
    let copy = projects::open_workstream_for(inner, &plain, by_hand("copy", None))
        .await
        .unwrap()
        .workstream;
    assert_eq!(copy.kind, WorkstreamKind::Copy);
    let status = ide_git::workstream_status(inner, copy.id).await.unwrap();
    assert!(!status.git && status.exists, "{status:?}");
    assert!(ide_git::branches(inner, WorkstreamId::primary_of(plain.id))
        .await
        .is_err());
    // A repository made by hand under a record that says none: git takes
    // the remote, the record is not git's to carry.
    raw_git(&plain_root, &["init", "--quiet"]);
    ide_git::remote_add(
        inner,
        WorkstreamId::primary_of(plain.id),
        "origin".into(),
        url.clone(),
    )
    .await
    .unwrap();
    assert!(matches!(ws.get_project(plain.id).unwrap().vcs, Vcs::None));

    // A config written where nobody is set to commit resolves no identity,
    // and says nothing to the committer desk.
    let nobody = ws
        .create_project(NewProject::managed("nobody").unwrap())
        .unwrap();
    let nobody = projects::init_git(inner, &nobody).await.unwrap();
    let view = ide_git::set_local_config(
        inner,
        WorkstreamId::primary_of(nobody.id),
        vec![("pull.rebase".into(), "true".into())],
        vec![],
    )
    .await
    .unwrap();
    assert!(view
        .entries
        .iter()
        .any(|e| e.key == "pull.rebase" && e.local.as_deref() == Some("true")));
    assert_eq!(
        ide_git::identity(inner, WorkstreamId::primary_of(nobody.id))
            .await
            .unwrap()
            .source,
        bisa_vcs::IdentitySource::None
    );
    assert!(matches!(
        projects::push_workstream(inner, copy.id).await,
        Err(EngineError::Invalid(_))
    ));
    assert!(
        projects::unstage_in(inner, primary, vec!["../escape".into()])
            .await
            .is_err(),
        "a path reaching out"
    );

    raw_git(&root, &["checkout", "--quiet", "--detach"]);
    std::fs::write(root.join("detached.txt"), "x\n").unwrap();
    assert!(matches!(
        projects::commit_workstream(inner, primary, "detached").await,
        Err(EngineError::Invalid(_))
    ));
    assert!(matches!(
        projects::push_workstream(inner, primary).await,
        Err(EngineError::Invalid(_))
    ));
    engine.shutdown().await;
}

/// A conflict is read whole with its three sides; once the file is gone
/// from the disk it has no text and no hash; a folder in its place is an
/// error, never a side.
#[tokio::test(flavor = "multi_thread")]
async fn a_conflict_is_read_when_its_file_is_gone_or_a_folder() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let inner = engine.inner();
    let (project, root) = repo_project(&engine, "web").await;
    let primary = WorkstreamId::primary_of(project.id);
    let branch = raw_git(&root, &["symbolic-ref", "--short", "HEAD"]);
    let wt = dir.path().join("wt-theirs");
    git::worktree_add(&root, &wt, "theirs", &branch).unwrap();
    std::fs::write(wt.join("README.md"), "theirs\n").unwrap();
    git::add_all(&wt).unwrap();
    git::commit(&wt, "theirs", false).unwrap();
    git::worktree_remove(&wt, false).unwrap();
    std::fs::write(root.join("README.md"), "ours\n").unwrap();
    git::add_all(&root).unwrap();
    git::commit(&root, "ours", false).unwrap();
    assert!(git_fails(&root, &["merge", "--quiet", "theirs"]));

    let whole = ide_git::conflict(inner, primary, "README.md".into())
        .await
        .unwrap();
    assert!(whole.text.as_deref().unwrap_or("").contains("<<<<<<<"));
    assert_eq!(whole.ours.text.as_deref(), Some("ours\n"));
    assert_eq!(whole.theirs.text.as_deref(), Some("theirs\n"));
    assert_eq!(
        ide_git::in_progress(inner, primary).await.unwrap(),
        Some(bisa_vcs::InProgress::Merge)
    );

    std::fs::remove_file(root.join("README.md")).unwrap();
    let gone = ide_git::conflict(inner, primary, "README.md".into())
        .await
        .unwrap();
    assert_eq!((gone.text, gone.hash), (None, None));
    assert!(!gone.binary);

    std::fs::create_dir(root.join("README.md")).unwrap();
    assert!(ide_git::conflict(inner, primary, "README.md".into())
        .await
        .is_err());
    engine.shutdown().await;
}

/// A log too long for one screen answers its first screen at once and is
/// laid out whole behind it; asked again while that runs, the relay is not
/// started twice; a ref that moves answers the stale rows at once and lays
/// out again behind them; and more roots than the cache keeps let the
/// oldest go.
#[tokio::test(flavor = "multi_thread")]
async fn a_long_log_is_laid_out_behind_the_first_screen_and_again_when_a_ref_moves() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let (project, root) = repo_project(&engine, "web").await;
    let pid = project.id.to_string();
    let branch = raw_git(&root, &["symbolic-ref", "--short", "HEAD"]);
    fast_import(&root, &branch, 1_500, 0);

    let first = window(&engine, &pid, graph::RefScope::All, false).await;
    assert_eq!(first.total, graph::FIRST_SCREEN, "{first:?}");
    assert!(!first.done);
    assert_eq!(first.rows.len(), 50);
    let again = window(&engine, &pid, graph::RefScope::All, true).await;
    assert!(again.total >= graph::FIRST_SCREEN);
    let deadline = Instant::now() + Duration::from_secs(30);
    let whole = loop {
        let w = window(&engine, &pid, graph::RefScope::All, false).await;
        if w.done && w.total == 1_501 {
            break w;
        }
        assert!(Instant::now() < deadline, "the whole log never came: {w:?}");
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    assert!(!whole.stale);

    fast_import(&root, &branch, 1, 1_500);
    let moved = window(&engine, &pid, graph::RefScope::All, false).await;
    assert!(moved.stale || moved.total == 1_502, "{moved:?}");
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let w = window(&engine, &pid, graph::RefScope::All, false).await;
        if w.done && !w.stale && w.total == 1_502 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the moved ref never settled: {w:?}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    for n in 0..9 {
        let (p, _) = repo_project(&engine, &format!("root{n}")).await;
        for refs in graph::RefScope::ALL {
            let w = window(&engine, &p.id.to_string(), refs, false).await;
            assert_eq!(w.total, 1);
        }
    }
    engine.shutdown().await;
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

/// Notes are sent by id, each with its scope in words, to every goal the
/// project is attached to, and the chips on the message stay under the
/// wire's bound by losing their patch text from the end; an edit to nothing
/// is refused.
#[tokio::test(flavor = "multi_thread")]
async fn review_notes_are_sent_by_id_with_their_scopes_to_every_goal_within_the_wire_bound() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let inner = engine.inner();
    let (project, _root) = repo_project(&engine, "web").await;
    let ws = engine.workspace();
    let goal = ws.create_goal(NewGoal::captured("review it")).unwrap();
    ws.attach(goal.id, project.id).unwrap();

    let big = ("+".repeat(1000) + "\n").repeat(31);
    let mut staged = new_note(project.id, &big, "staged words");
    staged.scope = DiffScope::Staged;
    let mut vs_base = new_note(project.id, &big, "against the base");
    vs_base.scope = DiffScope::Branch {
        base: "main".into(),
    };
    let a = review::create(inner, staged).unwrap();
    let b = review::create(inner, vs_base).unwrap();
    let c = review::create(
        inner,
        new_note(project.id, "@@ -1 +1 @@\n-a\n+b\n", "small"),
    )
    .unwrap();
    assert!(matches!(
        review::edit(inner, project.id, c.id, "   ".into()),
        Err(EngineError::Invalid(_))
    ));

    let sent = review::send(inner, project.id, vec![a.id, b.id]).unwrap();
    assert_eq!(sent.notes.len(), 2);
    assert!(sent.notes.iter().all(|n| n.sent_at.is_some()));
    assert_eq!(sent.posted_to, vec![goal.id.to_string()]);
    let rest = review::send(inner, project.id, vec![]).unwrap();
    assert_eq!(rest.notes.len(), 1, "the third was still unsent");
    assert_eq!(rest.notes[0].id, c.id);
    engine.shutdown().await;
}

/// A suggestion describes what is staged: a renamed file is named with
/// what it was, and a diff over the budget is cut and says so.
#[tokio::test(flavor = "multi_thread")]
async fn a_suggestion_names_a_renamed_file_and_says_when_the_diff_was_cut() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let inner = engine.inner();
    let (project, root) = repo_project(&engine, "web").await;
    raw_git(&root, &["mv", "README.md", "READ.md"]);
    std::fs::write(root.join("big.txt"), "a line of it\n".repeat(8_000)).unwrap();
    git::stage(&root, &["big.txt"]).unwrap();
    let message = projects::suggest_commit_message(inner, WorkstreamId::primary_of(project.id))
        .await
        .expect("the mock echoes the prompt");
    assert!(message.contains("READ.md (was README.md)"), "{message}");
    assert!(message.contains("diff truncated"), "{message}");
    engine.shutdown().await;
}

/// A record that names no default branch opens a workstream from git's own;
/// a derived branch name a leftover branch already holds is retried with
/// the whole id; and a branch pushed by hand is recorded pushed before the
/// push itself runs.
#[tokio::test(flavor = "multi_thread")]
async fn a_workstream_opens_from_gits_default_a_taken_name_retries_and_a_hand_push_is_recorded() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let inner = engine.inner();
    let (project, root) = repo_project(&engine, "web").await;
    let ws = engine.workspace();
    let branch = raw_git(&root, &["symbolic-ref", "--short", "HEAD"]);
    let origin = bare_origin(&dir, "origin", &branch);
    raw_git(
        &root,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );

    let mut blank = ws.get_project(project.id).unwrap();
    blank.vcs = Vcs::Git {
        default_branch: String::new(),
        remote: None,
        code_host: None,
    };
    blank.publish = PublishPolicy::Auto;
    let project = ws.update_project(blank).unwrap();
    let place =
        projects::open_workstream_for(inner, &project, by_hand("blank", Some("feature/blank")))
            .await
            .unwrap();
    assert!(
        matches!(&place.workstream.kind, WorkstreamKind::Worktree { base, .. } if base == &branch),
        "{:?}",
        place.workstream.kind
    );

    let goal = ws.create_goal(NewGoal::captured("checkout")).unwrap();
    ws.attach(goal.id, project.id).unwrap();
    let spec = item(goal.id, &project, "Add the checkout flow");
    let short = projects::branch_for(&spec.instructions, spec.id, false);
    raw_git(&root, &["branch", &short]);
    let taken = projects::open_workstream(inner, &spec, &project)
        .await
        .unwrap();
    let long = projects::branch_for(&spec.instructions, spec.id, true);
    assert!(
        matches!(&taken.workstream.kind, WorkstreamKind::Worktree { branch, .. } if branch == &long),
        "{:?}",
        taken.workstream.kind
    );

    std::fs::write(place.cwd.join("hand.txt"), "by hand\n").unwrap();
    projects::commit_workstream(inner, place.workstream.id, "by hand")
        .await
        .unwrap();
    raw_git(
        &place.cwd,
        &["push", "--quiet", "-u", "origin", "feature/blank"],
    );
    projects::push_workstream(inner, place.workstream.id)
        .await
        .expect("the record catches up, then the push finds nothing new");
    assert_eq!(
        ws.get_workstream(place.workstream.id).unwrap().state,
        WorkstreamState::Pushed
    );
    engine.shutdown().await;
}
