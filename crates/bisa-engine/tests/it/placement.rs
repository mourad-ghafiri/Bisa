//! Placement: where a session runs.
//!
//! One rule, asserted from outside the engine: **a session runs in the folder
//! of the thing it works on, and that folder is inside the workspace.** An
//! agent step with a project runs in a workstream of it — the step's, or the
//! goal's only one; a step with none runs in the goal's own `scratch/`, and
//! nothing is made for it — no project is ever born of a step. That
//! `scratch/` also holds the Workflow Agent's design session, a `check`
//! command when the goal has no project, and `TMPDIR`.
//!
//! The assertions are made against `MockAdapter::launches()`, which is the
//! `SessionSpec` a real harness would have been handed, so what is being
//! checked is the directory a harness process would actually have started in
//! rather than the engine's opinion about it.
//!
//! What every test here is really guarding is the negative:
//! [`no_session_ever_launches_in_the_workspace_root`]. The workspace root
//! holds the signed journal, the index and the key files; an agent step runs in
//! a project's workstream or the goal's `scratch/`, and a guided wake in that
//! same `scratch/`.

use crate::common;
use bisa_core::workitem::{WorkItemSpec, WorkItemState};
use bisa_core::Budget;
use bisa_core::HarnessCaps;
use bisa_core::{
    AgentId, CheckKind, GoalId, Project, RunOutcome, StepKind, ToolTier, ValueRef, WorkItemId,
};
use bisa_engine::{projects, Engine, EngineConfig, SubmitRequest};
use bisa_harness::mock::MockAdapter;
use bisa_harness::{
    HarnessAdapter, HarnessCatalog, HarnessError, HarnessSession, ProbeResult, ResumeToken,
    SessionSpec,
};
use bisa_store::{MemoryKeyStore, NewProject, Workspace, WorkstreamFilter};
use common::{agent_step, chain, finished_run, guided, new_workflow, run_spec, sid, step};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn workspace(dir: &tempfile::TempDir) -> Workspace {
    Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap()
}

fn config(designs: bool) -> EngineConfig {
    EngineConfig {
        design_enabled: designs,
        ..Default::default()
    }
}

/// A mock harness that writes a file into whatever cwd it is launched in.
///
/// Placement is only worth anything if what the session produces is still
/// findable afterwards, and the only way to know that from outside is to have
/// a session actually write something.
struct WritingAdapter {
    inner: Arc<MockAdapter>,
    file: String,
}

#[async_trait::async_trait]
impl HarnessAdapter for WritingAdapter {
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

/// An engine on one mock harness, plus the handle a test reads launches back
/// from.
fn engine_with(dir: &tempfile::TempDir, designs: bool) -> (Engine, Arc<MockAdapter>) {
    let adapter = Arc::new(MockAdapter::default());
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<dyn HarnessAdapter>);
    let ws = workspace(dir);
    drive_on(&ws, adapter.id());
    let engine = Engine::start(ws, catalog, config(designs)).unwrap();
    (engine, adapter)
}

/// The same, with a harness that leaves a file behind.
fn writing_engine(dir: &tempfile::TempDir, file: &str) -> (Engine, Arc<MockAdapter>) {
    writing_engine_over(dir, file, MockAdapter::default())
}

/// A writing harness over the mock given — one that yields a result, for a
/// step that must finish `Done`.
fn writing_engine_over(
    dir: &tempfile::TempDir,
    file: &str,
    inner: MockAdapter,
) -> (Engine, Arc<MockAdapter>) {
    let inner = Arc::new(inner);
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::new(WritingAdapter {
        inner: Arc::clone(&inner),
        file: file.to_string(),
    }) as Arc<dyn HarnessAdapter>);
    let engine = Engine::start(workspace(dir), catalog, config(false)).unwrap();
    (engine, inner)
}

/// Point the Workflow Agent at the mock harness. Which agent drives a guided
/// goal is not configurable; its harness is the one thing an owner may edit,
/// so it is the only way a test can watch a guided wake launch.
fn drive_on(ws: &Workspace, harness: &str) {
    let mut def = ws.get_agent(&AgentId::workflow()).unwrap();
    def.harness = harness.into();
    ws.update_agent(def).unwrap();
}

/// A managed project attached to `goal`. Projects belong to the workspace;
/// the attachment is what makes one visible to a goal's work.
fn attached_project(ws: &Workspace, goal: GoalId, new: NewProject) -> Project {
    let project = ws.create_project(new).unwrap();
    ws.attach(goal, project.id).unwrap();
    project
}

/// A managed project attached to `goal` that is a repository with a first
/// commit: what an item's worktree branches from. One that is no repository
/// is copied for its item instead, and the copy goes with its run, its patch kept.
async fn committed_project(engine: &Engine, goal: GoalId, slug: &str) -> Project {
    use bisa_vcs::{git, ConfigScope};
    let project = attached_project(engine.workspace(), goal, NewProject::managed(slug).unwrap());
    let project = projects::init_git(engine.inner(), &project)
        .await
        .expect("a repository in the project's folder");
    let root = engine.workspace().project_root_path(&project);
    for (key, value) in [
        ("user.name", "Bisa Test"),
        ("user.email", "test@example.invalid"),
    ] {
        git::config_set(ConfigScope::Local, Some(&root), key, value).expect("the committer");
    }
    std::fs::write(root.join("README.md"), "baseline\n").unwrap();
    git::add_all(&root).unwrap();
    git::commit(&root, "baseline", false).unwrap();
    project
}

fn item(goal: GoalId) -> WorkItemSpec {
    WorkItemSpec {
        id: WorkItemId::from_ulid(ulid::Ulid::from_datetime(SystemTime::now())),
        home: bisa_core::Home::Goal { goal },
        run: None,
        step: None,
        instructions: "do the work".into(),
        state: WorkItemState::Open,
        project: None,
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

/// Run one item as the single step of a workflow on the goal, and wait for
/// it to settle.
async fn run_item(engine: &Engine, goal: GoalId, spec: WorkItemSpec) -> WorkItemId {
    run_spec(engine, goal, spec).await
}

async fn until<T>(what: &str, mut probe: impl FnMut() -> Option<T>) -> T {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(v) = probe() {
            return v;
        }
        assert!(std::time::Instant::now() < deadline, "timed out: {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The invariant, applied to every session a test happened to start.
///
/// Kept as a helper and called from each test rather than living only in the
/// dedicated one, because the cheapest way for a future placement to go wrong
/// is for it to be added somewhere none of these tests looks.
fn assert_never_the_root(adapter: &MockAdapter, root: &Path) {
    let launches = adapter.launches();
    assert!(!launches.is_empty(), "no session was launched at all");
    for spec in launches {
        assert_ne!(
            spec.cwd, root,
            "a session was launched in the workspace root"
        );
        assert!(
            spec.cwd.starts_with(root),
            "a session was launched outside the workspace: {}",
            spec.cwd.display()
        );
    }
}

// ---------------------------------------------------------------------------
// A step names a project or runs in the goal's scratch
// ---------------------------------------------------------------------------

/// The step that named no project on a goal with none runs in the goal's
/// scratch folder: no project is made, no workstream is opened, the file the
/// session wrote is there as scratch, and the prompt said where it stood and
/// how files that must be kept get a home.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_step_with_no_project_runs_in_the_goals_scratch_and_makes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = writing_engine(&dir, "report.md");
    let root = engine.workspace().root().to_path_buf();
    let ws = engine.workspace();

    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            title: Some("Summarise the quarter".into()),
            ..guided("a summary of the quarter's numbers")
        })
        .unwrap();
    assert!(ws.projects_for(goal.id).unwrap().is_empty());
    let item_id = run_item(&engine, goal.id, item(goal.id)).await;

    assert!(
        ws.projects_for(goal.id).unwrap().is_empty(),
        "no project was born of the step"
    );
    assert!(
        ws.list_projects().unwrap().is_empty(),
        "nor anywhere else in the workspace"
    );
    assert!(
        ws.list_workstreams(WorkstreamFilter::WorkItem(item_id))
            .unwrap()
            .is_empty(),
        "no workstream was opened"
    );
    assert_eq!(
        ws.get_work_item(&bisa_core::Home::from(goal.id), item_id)
            .unwrap()
            .project,
        None
    );
    let scratch = ws.paths().goal(goal.id).scratch();
    let launches = adapter.launches();
    assert_eq!(launches.len(), 1);
    assert_eq!(launches[0].cwd, scratch);
    assert!(
        scratch.join("report.md").exists(),
        "what the session wrote is scratch, where it wrote it"
    );
    let prompt = adapter.prompts().remove(0);
    assert!(prompt.contains("scratch folder"), "{prompt}");
    assert!(prompt.contains("create_project"), "{prompt}");
    assert!(!prompt.contains("committed to that branch"), "{prompt}");
    assert_never_the_root(&adapter, &root);
    engine.shutdown().await;
}

/// Two such steps share the scratch and still make nothing.
#[tokio::test(flavor = "multi_thread")]
async fn two_no_project_steps_share_the_scratch_and_still_make_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(&dir, false);
    let ws = engine.workspace();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..guided("two answers, one place")
        })
        .unwrap();
    run_item(&engine, goal.id, item(goal.id)).await;
    run_item(&engine, goal.id, item(goal.id)).await;
    assert!(
        ws.projects_for(goal.id).unwrap().is_empty(),
        "still no project"
    );
    let scratch = ws.paths().goal(goal.id).scratch();
    let launches = adapter.launches();
    assert_eq!(launches.len(), 2);
    for l in &launches {
        assert_eq!(l.cwd, scratch, "{}", l.cwd.display());
    }
    engine.shutdown().await;
}

/// A spawned goal works where its parent works: every project attached to
/// the parent is attached to the child at capture, and a later child sees
/// what the parent has by then — each once.
#[tokio::test(flavor = "multi_thread")]
async fn a_spawned_child_inherits_its_parents_projects() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, _adapter) = engine_with(&dir, false);
    let ws = engine.workspace();
    let parent = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..guided("the whole")
        })
        .unwrap();
    let a = attached_project(ws, parent.id, NewProject::managed("alpha").unwrap());
    let b = attached_project(ws, parent.id, NewProject::managed("beta").unwrap());
    let child = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            origin: bisa_core::GoalOrigin::Spawned {
                parent: parent.id,
                step: None,
            },
            ..guided("a part")
        })
        .unwrap();
    let ids = |goal: GoalId| {
        let mut ids: Vec<_> = ws
            .projects_for(goal)
            .unwrap()
            .into_iter()
            .map(|p| p.id)
            .collect();
        ids.sort();
        ids
    };
    let mut want = vec![a.id, b.id];
    want.sort();
    assert_eq!(ids(child.id), want, "the child sees its parent's projects");

    let c = attached_project(ws, parent.id, NewProject::managed("gamma").unwrap());
    let later = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            origin: bisa_core::GoalOrigin::Spawned {
                parent: parent.id,
                step: None,
            },
            ..guided("another part")
        })
        .unwrap();
    let mut want = vec![a.id, b.id, c.id];
    want.sort();
    assert_eq!(
        ids(later.id),
        want,
        "a later child sees what the parent has by then"
    );
    assert_eq!(ids(child.id).len(), 2, "the first child is not touched");
    assert_eq!(
        ids(parent.id).len(),
        3,
        "the parent's own list is unchanged"
    );
    engine.shutdown().await;
}

/// A goal with one attached project: an unnamed step runs there, and no
/// second project is invented.
#[tokio::test(flavor = "multi_thread")]
async fn a_goal_with_one_attached_project_runs_an_unnamed_step_there() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(&dir, false);
    let ws = engine.workspace();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..guided("ship checkout")
        })
        .unwrap();
    let project = attached_project(ws, goal.id, NewProject::managed("storefront").unwrap());
    let item_id = run_item(&engine, goal.id, item(goal.id)).await;
    assert_eq!(ws.projects_for(goal.id).unwrap().len(), 1);
    assert_eq!(
        ws.get_work_item(&bisa_core::Home::from(goal.id), item_id)
            .unwrap()
            .project,
        Some(project.id)
    );
    let launches = adapter.launches();
    assert!(
        launches[0]
            .cwd
            .starts_with(ws.paths().project(&project.slug).dir()),
        "{}",
        launches[0].cwd.display()
    );
    assert!(
        !launches[0]
            .cwd
            .starts_with(ws.paths().goal(goal.id).scratch()),
        "a step with a project is not in scratch"
    );
    assert_eq!(
        ws.list_workstreams(WorkstreamFilter::WorkItem(item_id))
            .unwrap()
            .len(),
        1,
        "one workstream of that project"
    );
    engine.shutdown().await;
}

/// An item that names a project runs in that project's workstream — the folder
/// of the thing it works on, exactly as the rule says.
#[tokio::test(flavor = "multi_thread")]
async fn an_item_with_a_project_runs_in_its_workstream() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(&dir, false);
    let root = engine.workspace().root().to_path_buf();

    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..guided("ship checkout")
        })
        .unwrap();
    // A plain (non-git) project: the workstream is a copy, which is enough to
    // show the placement without dragging a repository fixture in. The git
    // worktree case is proven end to end in `projects.rs`.
    let project = attached_project(
        engine.workspace(),
        goal.id,
        NewProject::managed("storefront").unwrap(),
    );

    let mut spec = item(goal.id);
    spec.project = Some(project.id);
    let item_id = run_item(&engine, goal.id, spec).await;

    let workstreams = engine
        .workspace()
        .list_workstreams(WorkstreamFilter::WorkItem(item_id))
        .unwrap();
    assert_eq!(workstreams.len(), 1, "one item, one workstream");
    let launches = adapter.launches();
    assert_eq!(
        launches[0].cwd,
        engine
            .workspace()
            .workstream_checkout(&workstreams[0])
            .unwrap()
    );
    // The workstream is under the project's own `workstreams/`, never inside
    // the project tree itself and never under a goal.
    assert!(launches[0].cwd.starts_with(
        engine
            .workspace()
            .paths()
            .project(&project.slug)
            .workstreams()
    ));
    assert!(!launches[0]
        .cwd
        .starts_with(engine.workspace().project_root_path(&project)));
    assert_never_the_root(&adapter, &root);

    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// A run of the workspace: no goal behind it
// ---------------------------------------------------------------------------

/// A run of the workspace, of one agent step placed by `project`, once the
/// step's item settled.
async fn workspace_run_of_one_step(
    engine: &Engine,
    project: Option<bisa_core::ProjectId>,
) -> (bisa_core::WorkflowRun, WorkItemId) {
    let mut work = agent_step("work", "mock");
    if let StepKind::Agent { project: named, .. } = &mut work.kind {
        *named = project.map(ValueRef::Fixed);
    }
    let (_, run) = common::workspace_run(engine, new_workflow("one step", vec![work]));
    let item = until("the step's item to settle", || {
        engine
            .workspace()
            .list_work_items(&run.home())
            .ok()?
            .into_iter()
            .find(|i| {
                !matches!(
                    i.state,
                    WorkItemState::Open
                        | WorkItemState::Claimed { .. }
                        | WorkItemState::InProgress { .. }
                )
            })
            .map(|i| i.id)
    })
    .await;
    (run, item)
}

/// A run of the workspace has no goal whose scratch it could stand in: a
/// step that names no project runs in the run's own, and nothing is made.
#[tokio::test(flavor = "multi_thread")]
async fn a_workspace_runs_step_with_no_project_runs_in_the_runs_scratch_and_makes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(&dir, false);
    let root = engine.workspace().root().to_path_buf();

    let (run, item) = workspace_run_of_one_step(&engine, None).await;

    let launches = adapter.launches();
    assert_eq!(launches.len(), 1);
    let scratch = engine.workspace().paths().home(&run.home()).scratch();
    assert_eq!(launches[0].cwd, scratch);
    assert!(
        scratch.starts_with(engine.workspace().paths().workspace_runs_dir()),
        "under the runs of the workspace, under no goal: {}",
        scratch.display()
    );
    assert_eq!(engine.workspace().list_projects().unwrap(), vec![]);
    assert_eq!(
        engine
            .workspace()
            .list_workstreams(WorkstreamFilter::WorkItem(item))
            .unwrap(),
        vec![]
    );
    assert_eq!(engine.workspace().list_goals(None).unwrap().len(), 0);
    assert_never_the_root(&adapter, &root);
    engine.shutdown().await;
}

/// The project a step names is where it runs, whatever the run is for: a
/// workstream of it, found again by the run that opened it — which is how a
/// stop of the run reaches the session standing there.
#[tokio::test(flavor = "multi_thread")]
async fn a_workspace_runs_step_that_names_a_project_runs_in_a_workstream_of_it() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(&dir, false);
    let root = engine.workspace().root().to_path_buf();
    // Attached to nothing: a run of the workspace has no goal to attach to.
    let project = engine
        .workspace()
        .create_project(NewProject::managed("storefront").unwrap())
        .unwrap();

    let (run, item) = workspace_run_of_one_step(&engine, Some(project.id)).await;

    let of_the_item = engine
        .workspace()
        .list_workstreams(WorkstreamFilter::WorkItem(item))
        .unwrap();
    assert_eq!(of_the_item.len(), 1, "one item, one workstream");
    let of_the_run = engine
        .workspace()
        .list_workstreams(WorkstreamFilter::Run(run.id))
        .unwrap();
    assert_eq!(
        of_the_run.iter().map(|w| w.id).collect::<Vec<_>>(),
        vec![of_the_item[0].id],
        "the run finds the workstream its item opened"
    );
    let launches = adapter.launches();
    assert_eq!(
        launches[0].cwd,
        engine
            .workspace()
            .workstream_checkout(&of_the_item[0])
            .unwrap()
    );
    assert!(launches[0].cwd.starts_with(
        engine
            .workspace()
            .paths()
            .project(&project.slug)
            .workstreams()
    ));
    assert_never_the_root(&adapter, &root);
    engine.shutdown().await;
}

/// Two projects and a step that names neither: refused before the run exists,
/// by name, rather than guessed.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_on_a_goal_with_two_projects_refuses_an_unnamed_step_at_start() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(&dir, false);
    let ws = engine.workspace();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..guided("which one?")
        })
        .unwrap();
    attached_project(ws, goal.id, NewProject::managed("web").unwrap());
    attached_project(ws, goal.id, NewProject::managed("api").unwrap());
    let wf = engine
        .create_workflow(common::new_workflow(
            "one step",
            vec![common::agent_step("work", "mock")],
        ))
        .unwrap();
    engine.set_workflow(goal.id, Some(wf.id)).unwrap();
    let refused = engine.start_run(goal.id, std::collections::BTreeMap::new());
    match refused {
        Err(bisa_engine::EngineError::ProjectAmbiguous { step, count }) => {
            assert_eq!(step.as_str(), "work");
            assert_eq!(count, 2);
        }
        other => panic!("expected ProjectAmbiguous, got {other:?}"),
    }
    assert!(
        ws.get_goal(goal.id).unwrap().run.is_none(),
        "no run was started"
    );
    assert!(adapter.launches().is_empty(), "nothing was launched");
    engine.shutdown().await;
}

/// A workflow of one agent step that works in the project its `project`
/// input names.
fn works_in_a_project_given() -> bisa_store::NewWorkflow {
    let work: bisa_core::Step = serde_json::from_value(serde_json::json!({
        "id": "work",
        "name": "Work",
        "kind": "agent",
        "instructions": "do the work",
        "project": { "input": "project" },
        "harness": ["mock"],
    }))
    .expect("an agent step");
    let mut draft = common::new_workflow("in a project", vec![work]);
    draft.inputs = vec![bisa_core::InputDef {
        name: bisa_core::InputName::new("project").unwrap(),
        label: "The project".into(),
        kind: bisa_core::InputKind::Project,
        default: None,
        required: true,
    }];
    draft
}

/// **What its person gives a goal to work in is attached to it.** The
/// guide's own line — `bisa new "…" --workflow bug-fix --input project=<id>`
/// — and the desktop's run form, which offers every project of the
/// workspace, began a run that failed at its first step: *goal … is not
/// attached to project …. Attach the project to the goal first* — of a goal
/// that did not exist before the call. A person who starts a goal's work
/// with a project says where it is done: the project is attached, the fact
/// is said on the bus, and the step runs in a workstream of it.
#[tokio::test(flavor = "multi_thread")]
async fn a_project_its_person_gives_a_run_is_attached_to_the_goal() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(&dir, false);
    let ws = engine.workspace();
    let web = ws
        .create_project(NewProject::managed("web").unwrap())
        .unwrap();
    let wf = engine.create_workflow(works_in_a_project_given()).unwrap();
    let given =
        std::collections::BTreeMap::from([("project".to_string(), serde_json::json!(web.id))]);

    // Captured and begun in one call.
    let mut attachments = engine.events();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            workflow: Some(wf.id),
            inputs: given.clone(),
            start: true,
            ..guided("fix the login redirect")
        })
        .unwrap();
    assert!(ws.is_attached(goal.id, web.id).unwrap());
    let launched = until("the step to be launched", || {
        adapter.launches().into_iter().next()
    })
    .await;
    assert!(
        launched
            .cwd
            .starts_with(ws.root().join("projects").join("web")),
        "it works in the project it was given: {}",
        launched.cwd.display()
    );
    let mut said = false;
    while let Ok(event) = attachments.try_recv() {
        said |= matches!(
            event.payload,
            bisa_engine::EnginePayload::AttachmentChanged { project, attached: true }
                if project == web.id
        );
    }
    assert!(said, "the attachment is said on the bus");

    // Captured first, begun later: the same.
    let api = ws
        .create_project(NewProject::managed("api").unwrap())
        .unwrap();
    let later = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            workflow: Some(wf.id),
            ..guided("fix the export")
        })
        .unwrap();
    assert!(!ws.is_attached(later.id, api.id).unwrap());
    let given =
        std::collections::BTreeMap::from([("project".to_string(), serde_json::json!(api.id))]);
    engine.start_run(later.id, given.clone()).unwrap();
    assert!(ws.is_attached(later.id, api.id).unwrap());

    // Attached beforehand: the start finds it there and says nothing anew.
    let already = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            workflow: Some(wf.id),
            ..guided("fix the import")
        })
        .unwrap();
    ws.attach(already.id, api.id).unwrap();
    let mut said_again = engine.events();
    engine.start_run(already.id, given).unwrap();
    assert!(ws.is_attached(already.id, api.id).unwrap());
    while let Ok(event) = said_again.try_recv() {
        assert!(
            !matches!(
                event.payload,
                bisa_engine::EnginePayload::AttachmentChanged { project, .. }
                    if project == api.id && event.goal == Some(already.id)
            ),
            "an attachment that stood is not said again"
        );
    }

    // A start that is refused attaches nothing.
    let never = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            workflow: Some(wf.id),
            ..guided("fix nothing")
        })
        .unwrap();
    let too_much = std::collections::BTreeMap::from([
        ("project".to_string(), serde_json::json!(api.id)),
        ("nobody_asked".to_string(), serde_json::json!("x")),
    ]);
    engine.start_run(never.id, too_much).unwrap_err();
    assert!(!ws.is_attached(never.id, api.id).unwrap());
    engine.shutdown().await;
}

/// A step that opens a goal on `child`, giving it what `gives` says and
/// going on without waiting.
fn opens(child: bisa_core::WorkflowId, gives: &[(&str, &str)]) -> bisa_core::Step {
    serde_json::from_value(serde_json::json!({
        "id": "open",
        "name": "Open the follow-up",
        "kind": "spawn",
        "statement_template": "follow {inputs.what} up, from {inputs.project}",
        "workflow": child,
        "inputs": gives.iter().copied().collect::<std::collections::BTreeMap<_, _>>(),
        "wait": false,
    }))
    .expect("a spawn step")
}

/// The goal a step of `run` opened.
async fn opened_by(engine: &Engine, run: bisa_core::RunId) -> bisa_core::Goal {
    let child = until("the step to open its goal", || {
        let run = engine.workspace().get_run(run).ok()?;
        run.steps[&sid("open")].output.clone()?["child"]
            .as_str()?
            .parse::<GoalId>()
            .ok()
    })
    .await;
    engine.workspace().get_goal(child).unwrap()
}

/// **A spawn gives its child what the step says, each word read by the
/// kind the child's input declares** — and the child works where its parent
/// does: a goal's child has its parent's projects, a goal born of a run of
/// the workspace the ones its step gives it, as that run has its own.
#[tokio::test(flavor = "multi_thread")]
async fn a_spawned_child_is_given_what_its_step_says_and_works_where_its_parent_does() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(&dir, false);
    let ws = engine.workspace();
    let child = {
        let mut draft = works_in_a_project_given();
        draft.steps = vec![serde_json::from_value(serde_json::json!({
            "id": "work",
            "name": "Work",
            "kind": "agent",
            "instructions": "{inputs.report}, {inputs.rounds} times, dry: {inputs.dry}",
            "project": { "input": "project" },
            "harness": ["mock"],
        }))
        .expect("an agent step")];
        for (name, kind) in [
            ("report", bisa_core::InputKind::Text),
            ("rounds", bisa_core::InputKind::Number),
            ("dry", bisa_core::InputKind::Bool),
        ] {
            draft.inputs.push(bisa_core::InputDef {
                name: bisa_core::InputName::new(name).unwrap(),
                label: name.into(),
                kind,
                default: None,
                required: true,
            });
        }
        engine.create_workflow(draft).unwrap()
    };
    let parent = |gives: &[(&str, &str)]| {
        let mut draft = common::new_workflow("opens a follow-up", vec![opens(child.id, gives)]);
        draft.inputs = vec![
            bisa_core::InputDef {
                name: bisa_core::InputName::new("project").unwrap(),
                label: "The project".into(),
                kind: bisa_core::InputKind::Project,
                default: None,
                required: true,
            },
            bisa_core::InputDef {
                name: bisa_core::InputName::new("what").unwrap(),
                label: "What".into(),
                kind: bisa_core::InputKind::Text,
                default: None,
                required: true,
            },
        ];
        draft
    };
    let gives = [
        ("project", "{inputs.project}"),
        ("report", "about {inputs.what}"),
        ("rounds", "4"),
        ("dry", "true"),
    ];
    let given = |project: &Project| {
        std::collections::BTreeMap::from([
            ("project".to_string(), serde_json::json!(project.id)),
            ("what".to_string(), serde_json::json!("true")),
        ])
    };
    let launched_in = |goal: GoalId| {
        adapter
            .launches()
            .into_iter()
            .find(|spec| spec.cwd.starts_with(ws.root().join("projects")))
            .filter(|_| ws.get_goal(goal).is_ok())
    };

    // A goal's child.
    let web = ws
        .create_project(NewProject::managed("web").unwrap())
        .unwrap();
    let wf = engine.create_workflow(parent(&gives)).unwrap();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            workflow: Some(wf.id),
            inputs: given(&web),
            start: true,
            ..guided("watch the login")
        })
        .unwrap();
    let run = ws.get_current_run(goal.id).unwrap().expect("its run");
    let born = opened_by(&engine, run.id).await;
    assert_eq!(born.origin.parent(), Some(goal.id));
    assert!(ws.is_attached(born.id, web.id).unwrap(), "its parent's");
    let its = until("the child's run", || {
        ws.get_current_run(born.id).ok().flatten()
    })
    .await;
    assert_eq!(
        serde_json::json!(its.inputs),
        serde_json::json!({
            "project": web.id,
            "report": "about true",
            "rounds": 4,
            "dry": true,
        }),
        "a word where a word is asked, a number and a yes where they are"
    );
    let placed = until("the child's step to be launched", || launched_in(born.id)).await;
    assert!(
        placed
            .cwd
            .starts_with(ws.root().join("projects").join("web")),
        "{}",
        placed.cwd.display()
    );

    // A goal born of a run of the workspace.
    let api = ws
        .create_project(NewProject::managed("api").unwrap())
        .unwrap();
    let run = engine.start_workspace_run(wf.id, given(&api)).unwrap();
    let born = opened_by(&engine, run.id).await;
    assert!(
        matches!(born.origin, bisa_core::GoalOrigin::Run { .. }),
        "{:?}",
        born.origin
    );
    assert!(
        ws.is_attached(born.id, api.id).unwrap(),
        "where the run that made it says"
    );

    // What a goal's step names beside its parent's projects is attached by
    // nobody: the child is refused at the step that would work there.
    let elsewhere = ws
        .create_project(NewProject::managed("elsewhere").unwrap())
        .unwrap();
    let names_another = [
        ("project", elsewhere.id.to_string()),
        ("report", "r".to_string()),
        ("rounds", "1".to_string()),
        ("dry", "false".to_string()),
    ];
    let names_another: Vec<(&str, &str)> = names_another
        .iter()
        .map(|(name, said)| (*name, said.as_str()))
        .collect();
    let wf = engine.create_workflow(parent(&names_another)).unwrap();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            workflow: Some(wf.id),
            inputs: given(&web),
            start: true,
            ..guided("watch the export")
        })
        .unwrap();
    let run = ws.get_current_run(goal.id).unwrap().expect("its run");
    let born = opened_by(&engine, run.id).await;
    let refused = until("the child's run to end", || {
        let run = ws.get_current_run(born.id).ok().flatten()?;
        run.is_finished().then_some(run)
    })
    .await;
    assert_eq!(refused.outcome, Some(bisa_core::RunOutcome::Failed));
    assert!(!ws.is_attached(born.id, elsewhere.id).unwrap());
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// The scratch folder's three uses
// ---------------------------------------------------------------------------

/// A guided wake is a design session: it runs in the goal's `scratch/`, where
/// nothing commits and no deliverable belongs.
#[tokio::test(flavor = "multi_thread")]
async fn the_workflow_agents_design_session_runs_in_the_goals_scratch_folder() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(&dir, true);
    let root = engine.workspace().root().to_path_buf();

    let goal = engine
        .submit_goal(SubmitRequest {
            ..guided("guide me")
        })
        .unwrap();

    let expected = engine.workspace().paths().goal(goal.id).scratch();
    let cwd = until("the guided wake to launch", || {
        adapter.launches().first().map(|s| s.cwd.clone())
    })
    .await;
    assert_eq!(cwd, expected);
    assert!(
        adapter.launches()[0]
            .prompt
            .contains(bisa_engine::guided::DESIGN_PLACEMENT)
            || adapter.launches()[0].prompt.is_empty(),
        "the design session is told where it stands"
    );
    assert_never_the_root(&adapter, &root);

    engine.shutdown().await;
}

/// With no agent item to follow, a `check` command runs in the goal's one project root.
#[tokio::test(flavor = "multi_thread")]
async fn a_check_command_runs_in_the_runs_project() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, _adapter) = engine_with(&dir, false);
    let ws = engine.workspace();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..guided("checked in the project")
        })
        .unwrap();
    let project = attached_project(ws, goal.id, NewProject::managed("site").unwrap());
    let tree = ws.project_root_path(&project);
    std::fs::create_dir_all(&tree).unwrap();
    std::fs::write(tree.join("report.md"), "content\n").unwrap();
    assert_eq!(projects::goal_check_root(engine.inner(), goal.id), tree);
    assert!(!ws
        .paths()
        .goal(goal.id)
        .scratch()
        .join("report.md")
        .exists());
    engine.shutdown().await;
}

/// A `check` after an agent step runs where that step's item worked: the
/// project's workstream is a worktree beside the root, and a criterion such
/// as `test -s index.html` must see the file the agent just wrote there —
/// the root would not, and the run would fail on a file that exists.
#[tokio::test(flavor = "multi_thread")]
async fn a_check_after_an_agent_step_runs_where_the_agent_wrote() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = writing_engine_over(
        &dir,
        "index.html",
        common::yielding("mock", serde_json::json!({"ok": true})),
    );
    let ws = engine.workspace();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..guided("a page that remembers")
        })
        .unwrap();
    let project = committed_project(&engine, goal.id, "site").await;
    let mut write = agent_step("write", "mock");
    if let StepKind::Agent { project: p, .. } = &mut write.kind {
        *p = Some(ValueRef::Fixed(project.id));
    }
    let wf = engine
        .create_workflow(new_workflow(
            "page in project",
            chain(vec![
                write,
                step(
                    "exists",
                    StepKind::Check {
                        check: CheckKind::Command {
                            command: "test -s index.html".into(),
                            project: None,
                        },
                    },
                ),
            ]),
        ))
        .unwrap();
    engine.set_workflow(goal.id, Some(wf.id)).unwrap();
    engine
        .start_run(goal.id, std::collections::BTreeMap::new())
        .unwrap();
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done), "{done:?}");
    let item = done.steps[&sid("write")]
        .work_item
        .expect("the agent step ran on an item");
    let opened = ws
        .list_workstreams(WorkstreamFilter::WorkItem(item))
        .unwrap();
    assert!(
        matches!(
            opened.as_slice(),
            [w] if matches!(w.kind, bisa_core::WorkstreamKind::Worktree { .. })
        ),
        "one worktree, the item's: {opened:?}"
    );
    let checkout = ws.work_item_root(item).unwrap();
    assert!(
        checkout.join("index.html").is_file(),
        "the agent wrote in its checkout: {}",
        checkout.display()
    );
    let root = ws.project_root_path(&project);
    assert_ne!(
        checkout, root,
        "the item's checkout is not the project root"
    );
    assert!(
        !root.join("index.html").exists(),
        "the root never saw the file; the check did not need it to"
    );
    let evidence = &done.steps[&sid("exists")].output.as_ref().unwrap()["evidence"];
    assert!(
        evidence.to_string().contains("exit status: 0"),
        "{evidence}"
    );
    assert_never_the_root(&adapter, engine.workspace().root());
    engine.shutdown().await;
}

/// The same on a project that is no repository: the item works in a **copy**,
/// and the check that follows finds the file there — every time, not by
/// which of two tasks ran first. The copy used to be torn down the moment its
/// item settled, racing the next step; it stays now, its patch captured, and
/// goes with the run: the record closed, the tree gone, the patch kept.
#[tokio::test(flavor = "multi_thread")]
async fn a_check_after_an_agent_step_on_a_plain_project_finds_the_file_in_the_copy_every_time() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = writing_engine_over(
        &dir,
        "index.html",
        common::yielding("mock", serde_json::json!({"ok": true})),
    );
    let ws = engine.workspace();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..guided("a page in a plain folder")
        })
        .unwrap();
    let project = attached_project(ws, goal.id, NewProject::managed("plain").unwrap());
    std::fs::create_dir_all(ws.project_root_path(&project)).unwrap();
    let mut write = agent_step("write", "mock");
    if let StepKind::Agent { project: p, .. } = &mut write.kind {
        *p = Some(ValueRef::Fixed(project.id));
    }
    let wf = engine
        .create_workflow(new_workflow(
            "page in a plain project",
            chain(vec![
                write,
                step(
                    "exists",
                    StepKind::Check {
                        check: CheckKind::Command {
                            command: "test -s index.html".into(),
                            project: None,
                        },
                    },
                ),
            ]),
        ))
        .unwrap();
    engine.set_workflow(goal.id, Some(wf.id)).unwrap();

    // Several runs: a race that was lost one time in a few is met here.
    for round in 0..4 {
        engine
            .start_run(goal.id, std::collections::BTreeMap::new())
            .unwrap();
        let done = finished_run(&engine, goal.id).await;
        assert_eq!(
            done.outcome,
            Some(RunOutcome::Done),
            "round {round}: {done:?}"
        );
        let evidence = &done.steps[&sid("exists")].output.as_ref().unwrap()["evidence"];
        assert!(
            evidence.to_string().contains("exit status: 0"),
            "round {round}: the check ran where the agent wrote: {evidence}"
        );
        let item = done.steps[&sid("write")]
            .work_item
            .expect("the agent step ran on an item");
        let opened = ws
            .list_workstreams(WorkstreamFilter::WorkItem(item))
            .unwrap();
        let [copy] = opened.as_slice() else {
            panic!("one copy, the item's: {opened:?}")
        };
        assert!(matches!(copy.kind, bisa_core::WorkstreamKind::Copy));
        // The run ended: the copy's record is closed and its tree gone —
        // in a task of the run's settle, so awaited — and the patch stays.
        until("the copy to be closed with its run", || {
            let w = ws.get_workstream(copy.id).ok()?;
            let checkout = ws.checkout_in(&project, &w);
            (w.state.is_terminal() && !checkout.exists()).then_some(())
        })
        .await;
        let patch = ws.paths().home(&done.home()).result(item);
        let text = std::fs::read_to_string(&patch).unwrap_or_else(|e| {
            panic!(
                "round {round}: the patch is kept ({e}): {}",
                patch.display()
            )
        });
        assert!(
            text.contains("index.html"),
            "the patch names the file: {text}"
        );
    }
    let root = ws.project_root_path(&project);
    assert!(
        !root.join("index.html").exists(),
        "the root is never written to: the patch is the result"
    );
    assert_never_the_root(&adapter, engine.workspace().root());
    engine.shutdown().await;
}

/// With no project on the goal, a check has nowhere else to run than scratch.
#[tokio::test(flavor = "multi_thread")]
async fn a_check_command_with_no_project_runs_in_scratch() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, _adapter) = engine_with(&dir, false);
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..guided("no project here")
        })
        .unwrap();
    let scratch = engine.workspace().paths().goal(goal.id).scratch();
    assert_eq!(projects::goal_check_root(engine.inner(), goal.id), scratch);
    assert!(scratch.is_dir(), "created on demand");
    engine.shutdown().await;
}

/// `TMPDIR` points inside the goal's scratch folder even though the session
/// itself runs in a project: the scratch a well-behaved tool leaves behind
/// belongs to the goal that caused it, not to the system temp directory and
/// not to the project tree.
#[tokio::test(flavor = "multi_thread")]
async fn a_scratch_tmpdir_is_inside_the_goal() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(&dir, false);

    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..guided("tmp inside the goal")
        })
        .unwrap();
    // A project, so the session itself stands in a checkout and its TMPDIR
    // is the one thing of it in scratch.
    attached_project(
        engine.workspace(),
        goal.id,
        NewProject::managed("tmp-host").unwrap(),
    );
    run_item(&engine, goal.id, item(goal.id)).await;

    let paths = engine.workspace().paths().goal(goal.id);
    let launches = adapter.launches();
    let tmp = launches[0]
        .env
        .get("TMPDIR")
        .expect("TMPDIR must be injected");
    assert_eq!(PathBuf::from(tmp), paths.tmp());
    assert!(paths.tmp().is_dir(), "TMPDIR must exist before the launch");
    assert!(paths.tmp().starts_with(paths.scratch()));
    assert!(
        !launches[0].cwd.starts_with(paths.scratch()),
        "the session itself is in a project"
    );

    engine.shutdown().await;
}

/// The invariant on its own: a guided wake and agent steps in one workspace,
/// and not one of them started where truth lives.
#[tokio::test(flavor = "multi_thread")]
async fn no_session_ever_launches_in_the_workspace_root() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(&dir, true);
    let root = engine.workspace().root().to_path_buf();

    // Guided: capture wakes the core agent.
    let _guided = engine
        .submit_goal(SubmitRequest {
            ..guided("guide me")
        })
        .unwrap();
    until("the guided wake to launch", || {
        (!adapter.launches().is_empty()).then_some(())
    })
    .await;

    // Manual: an item on a goal with no project (the engine makes one), and —
    // on its own goal — one with a project named.
    let plain = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..guided("do it myself")
        })
        .unwrap();
    run_item(&engine, plain.id, item(plain.id)).await;

    let coded = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..guided("ship checkout")
        })
        .unwrap();
    let project = attached_project(
        engine.workspace(),
        coded.id,
        NewProject::managed("storefront").unwrap(),
    );
    let mut spec = item(coded.id);
    spec.project = Some(project.id);
    run_item(&engine, coded.id, spec).await;

    assert!(
        adapter.launches().len() >= 3,
        "expected a guided wake and two items, got {}",
        adapter.launches().len()
    );
    assert_never_the_root(&adapter, &root);
    // And each one is under a goal's folder or a project's, which are the only
    // places any placement can legitimately be.
    let goals = engine.workspace().paths().goals_dir();
    let projects = engine.workspace().paths().projects_dir();
    for spec in adapter.launches() {
        assert!(
            spec.cwd.starts_with(&goals) || spec.cwd.starts_with(&projects),
            "{} is under neither {} nor {}",
            spec.cwd.display(),
            goals.display(),
            projects.display()
        );
    }
    engine.shutdown().await;
}

/// A proposal whose agent step has nowhere unambiguous to run — the goal has
/// several projects attached and the step names none — is not the agent's
/// tool call to fail with, nor a gate to burn: an auto goal opens the Adopt
/// gate saying why, and a guided goal's approval is refused with the gate
/// kept and nothing recorded.
#[tokio::test(flavor = "multi_thread")]
async fn a_proposal_that_cannot_be_placed_asks_rather_than_failing() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, _) = engine_with(&dir, false);
    let ws = engine.workspace();
    let draft = serde_json::json!({
        "name": "Unplaced",
        "steps": [{"id": "work", "name": "Work", "kind": "agent",
                   "instructions": "do it", "harness": ["mock"]}]
    });
    let socket = engine.socket_path().to_path_buf();
    let propose = |goal: GoalId| {
        let draft = draft.clone();
        let socket = socket.clone();
        async move {
            common::intake_roundtrip(
                &socket,
                serde_json::json!({"op": "propose_workflow", "agent": bisa_core::AgentId::WORKFLOW,
                                   "goal": goal.to_string(), "workflow": draft}),
            )
            .await
        }
    };

    let auto = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Auto,
            ..guided("auto, two places")
        })
        .unwrap();
    attached_project(ws, auto.id, NewProject::managed("web").unwrap());
    attached_project(ws, auto.id, NewProject::managed("api").unwrap());
    let reply = propose(auto.id).await;
    assert_eq!(reply["ok"], serde_json::json!(true), "{reply}");
    let gate = reply["gate"]
        .as_str()
        .unwrap_or_else(|| panic!("the adopt gate opens, saying why: {reply}"))
        .to_string();
    let question = engine.gate(&gate).unwrap().question;
    assert!(question.contains("project"), "{question}");
    assert!(ws.get_goal(auto.id).unwrap().run.is_none(), "nothing ran");

    let guided_goal = engine.submit_goal(guided("guided, two places")).unwrap();
    attached_project(ws, guided_goal.id, NewProject::managed("web2").unwrap());
    attached_project(ws, guided_goal.id, NewProject::managed("api2").unwrap());
    let reply = propose(guided_goal.id).await;
    let gate = reply["gate"].as_str().unwrap().to_string();
    let err = engine.decide(&gate, true, None, None, None).unwrap_err();
    assert!(
        matches!(err, bisa_engine::EngineError::ProjectAmbiguous { .. }),
        "{err:?}"
    );
    assert!(
        engine.gate(&gate).is_some_and(|g| g.resolution.is_none()),
        "the gate stands"
    );
    let decisions = ws
        .journal(&bisa_core::Home::Goal {
            goal: guided_goal.id,
        })
        .unwrap()
        .iter()
        .filter(|e| matches!(e.payload, bisa_core::event::JournalPayload::Decision { .. }))
        .count();
    assert_eq!(decisions, 0, "nothing was recorded");
    assert!(ws.get_goal(guided_goal.id).unwrap().run.is_none());
    engine.shutdown().await;
}

/// A goal with two projects, a file only the second holds, and the check
/// `test -f only-in-b.txt` on its own goal each time.
fn two_projects(engine: &Engine, statement: &str, tag: &str) -> (GoalId, Project, Project) {
    let ws = engine.workspace();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..guided(statement)
        })
        .unwrap();
    let a = attached_project(
        ws,
        goal.id,
        NewProject::managed(&format!("a-{tag}")).unwrap(),
    );
    let b = attached_project(
        ws,
        goal.id,
        NewProject::managed(&format!("b-{tag}")).unwrap(),
    );
    std::fs::create_dir_all(ws.project_root_path(&a)).unwrap();
    let tree_b = ws.project_root_path(&b);
    std::fs::create_dir_all(&tree_b).unwrap();
    std::fs::write(tree_b.join("only-in-b.txt"), "here\n").unwrap();
    (goal.id, a, b)
}

fn file_check(project: Option<bisa_core::ValueRef<bisa_core::ProjectId>>) -> bisa_core::Step {
    step(
        "exists",
        StepKind::Check {
            check: CheckKind::Command {
                command: "test -f only-in-b.txt".into(),
                project,
            },
        },
    )
}

/// The check run on the goal with the workflow of that one step and the
/// inputs given; the finished run.
async fn judged(
    engine: &Engine,
    goal: GoalId,
    mut draft: bisa_store::NewWorkflow,
    inputs: std::collections::BTreeMap<String, serde_json::Value>,
) -> bisa_core::WorkflowRun {
    draft.name = format!("{} {}", draft.name, goal);
    let wf = engine.create_workflow(draft).unwrap();
    engine.set_workflow(goal, Some(wf.id)).unwrap();
    engine.start_run(goal, inputs).unwrap();
    finished_run(engine, goal).await
}

fn evidence_of(run: &bisa_core::WorkflowRun) -> String {
    run.steps[&sid("exists")]
        .output
        .as_ref()
        .map(|o| o["evidence"].to_string())
        .unwrap_or_default()
}

/// **A `check` that names a project runs in it when the goal has several.**
/// Unnamed, the check still lands in the goal's scratch — nothing there to
/// judge — as it always did; named, it runs in that project's root; named by
/// an input of kind project, the same; and a project the goal is not
/// attached to fails the step with the attach remedy as its evidence.
#[tokio::test(flavor = "multi_thread")]
async fn a_check_naming_a_project_runs_in_it_when_the_goal_has_several() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, _adapter) = engine_with(&dir, false);
    let none = std::collections::BTreeMap::new();

    let (goal, _a, b) = two_projects(&engine, "unnamed", "one");
    let run = judged(
        &engine,
        goal,
        new_workflow("unnamed check", chain(vec![file_check(None)])),
        none.clone(),
    )
    .await;
    assert_eq!(
        run.outcome,
        Some(RunOutcome::Failed),
        "the scratch holds no such file: {run:?}"
    );

    let (goal, _a, b2) = two_projects(&engine, "named", "two");
    let run = judged(
        &engine,
        goal,
        new_workflow(
            "named check",
            chain(vec![file_check(Some(bisa_core::ValueRef::Fixed(b2.id)))]),
        ),
        none.clone(),
    )
    .await;
    assert_eq!(run.outcome, Some(RunOutcome::Done), "{}", evidence_of(&run));
    assert!(
        evidence_of(&run).contains("exit status: 0"),
        "{}",
        evidence_of(&run)
    );

    let (goal, _a, b3) = two_projects(&engine, "from an input", "three");
    let mut draft = new_workflow(
        "check from input",
        chain(vec![file_check(Some(bisa_core::ValueRef::Input {
            input: bisa_core::InputName::new("where").unwrap(),
        }))]),
    );
    draft.inputs = vec![bisa_core::InputDef {
        name: bisa_core::InputName::new("where").unwrap(),
        label: "Where".into(),
        kind: bisa_core::InputKind::Project,
        default: None,
        required: true,
    }];
    let run = judged(
        &engine,
        goal,
        draft,
        std::collections::BTreeMap::from([(
            "where".to_string(),
            serde_json::json!(b3.id.to_string()),
        )]),
    )
    .await;
    assert_eq!(run.outcome, Some(RunOutcome::Done), "{}", evidence_of(&run));

    // `b` belongs to the first goal: the fourth goal is not attached to it.
    let (goal, _a, _b4) = two_projects(&engine, "unattached", "four");
    let run = judged(
        &engine,
        goal,
        new_workflow(
            "check elsewhere",
            chain(vec![file_check(Some(bisa_core::ValueRef::Fixed(b.id)))]),
        ),
        none,
    )
    .await;
    assert_eq!(run.outcome, Some(RunOutcome::Failed), "{run:?}");
    let record = format!("{:?}", run.steps[&sid("exists")]);
    assert!(
        record.contains("not attached"),
        "the attach remedy is what the step says: {record}"
    );
    engine.shutdown().await;
}

/// A `check` naming the project an agent step worked in runs in that
/// step's checkout — the worktree beside the root, where the file is — and
/// one naming the goal's other project runs in that project's root, where
/// it is not.
#[tokio::test(flavor = "multi_thread")]
async fn a_check_naming_the_items_project_runs_in_its_checkout() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = writing_engine_over(
        &dir,
        "index.html",
        common::yielding("mock", serde_json::json!({"ok": true})),
    );
    let ws = engine.workspace();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..guided("a page, checked where it was written")
        })
        .unwrap();
    let site = committed_project(&engine, goal.id, "site").await;
    let other = attached_project(ws, goal.id, NewProject::managed("other").unwrap());
    std::fs::create_dir_all(ws.project_root_path(&other)).unwrap();
    let mut write = agent_step("write", "mock");
    if let StepKind::Agent { project: p, .. } = &mut write.kind {
        *p = Some(bisa_core::ValueRef::Fixed(site.id));
    }
    let check_in = |id: &str, project: bisa_core::ProjectId| {
        step(
            id,
            StepKind::Check {
                check: CheckKind::Command {
                    command: "test -s index.html".into(),
                    project: Some(bisa_core::ValueRef::Fixed(project)),
                },
            },
        )
    };
    let wf = engine
        .create_workflow(new_workflow(
            "page checked in its project",
            chain(vec![
                write,
                check_in("in_site", site.id),
                check_in("in_other", other.id),
            ]),
        ))
        .unwrap();
    engine.set_workflow(goal.id, Some(wf.id)).unwrap();
    engine
        .start_run(goal.id, std::collections::BTreeMap::new())
        .unwrap();
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(
        done.outcome,
        Some(RunOutcome::Failed),
        "the second check fails: {done:?}"
    );
    let in_site = &done.steps[&sid("in_site")];
    assert!(
        matches!(in_site.state, bisa_core::StepState::Done { .. }),
        "{in_site:?}"
    );
    let item = done.steps[&sid("write")].work_item.expect("the item");
    let checkout = ws.work_item_root(item).unwrap();
    assert_ne!(
        checkout,
        ws.project_root_path(&site),
        "the worktree, not the root"
    );
    assert!(checkout.join("index.html").is_file());
    let in_other = &done.steps[&sid("in_other")];
    assert!(
        matches!(in_other.state, bisa_core::StepState::Failed),
        "{in_other:?}"
    );
    assert_never_the_root(&adapter, engine.workspace().root());
    engine.shutdown().await;
}

/// A run of the workspace has no goal to attach a project to: a check that
/// names a project the workspace has runs in that project's root.
#[tokio::test(flavor = "multi_thread")]
async fn a_workspace_runs_check_runs_in_the_project_it_names() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, _adapter) = engine_with(&dir, false);
    let ws = engine.workspace();
    let project = ws
        .create_project(NewProject::managed("storefront").unwrap())
        .unwrap();
    let tree = ws.project_root_path(&project);
    std::fs::create_dir_all(&tree).unwrap();
    std::fs::write(tree.join("only-in-b.txt"), "here\n").unwrap();
    let (_, run) = common::workspace_run(
        &engine,
        new_workflow(
            "checked in the workspace",
            chain(vec![file_check(Some(bisa_core::ValueRef::Fixed(
                project.id,
            )))]),
        ),
    );
    let done = common::run_finished(&engine, run.id).await;
    assert_eq!(
        done.outcome,
        Some(RunOutcome::Done),
        "{}",
        evidence_of(&done)
    );
    assert!(evidence_of(&done).contains("exit status: 0"));
    engine.shutdown().await;
}
