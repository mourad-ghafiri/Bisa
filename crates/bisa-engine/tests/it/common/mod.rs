//! Shared fixtures for the engine's integration tests: a workspace on a temp
//! dir, mock harnesses, workflows built from steps, and the waits that turn
//! an asynchronous engine into something a test can assert on.
//!
//! Nothing here deletes, forces or resets anything: a fixture builds state
//! and reads it back.

#![allow(dead_code)]

use bisa_core::{
    Assignee, Flow, Goal, GoalId, Home, Join, OnFail, RunId, Step, StepId, StepKind, ToolTier,
    WorkItemSpec, Workflow, WorkflowRun,
};
use bisa_engine::{Engine, EngineConfig, EngineEvent, SubmitRequest};
use bisa_harness::mock::{IntakeScript, MockAdapter};
use bisa_harness::HarnessCatalog;
use bisa_store::{FileKeyStore, NewWorkflow, Paths, Workspace};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// The workspace in `dir`, its keys in files under its own `identity/` — so
/// a second start over the same directory is the same owner and the same
/// agents, which is what a restart is. Keys kept in memory would be minted
/// anew by every start: another owner, and no agent left to sign as.
pub fn workspace(dir: &tempfile::TempDir) -> Workspace {
    Workspace::open_with_keystore(
        dir.path(),
        Box::new(FileKeyStore::new(Paths::new(dir.path()).identity_dir())),
    )
    .unwrap()
}

pub fn catalog_with(adapters: Vec<MockAdapter>) -> HarnessCatalog {
    let mut catalog = HarnessCatalog::new();
    for a in adapters {
        catalog.register(Arc::new(a));
    }
    catalog
}

/// A mock harness whose every session yields `output` as its result — the
/// shape of a worker that finishes its step.
pub fn yielding(id: &str, output: Value) -> MockAdapter {
    MockAdapter {
        id: id.into(),
        intake_script: Some(IntakeScript::new(vec![json!({
            "op": "result_submit", "work_item": "{{work_item}}", "output": output
        })])),
        ..Default::default()
    }
}

/// A capture in guided mode — the Workflow Agent proposes and the test adopts
/// through the gate, which is what most of these tests drive.
pub fn guided(statement: &str) -> bisa_engine::SubmitRequest {
    bisa_engine::SubmitRequest {
        mode: bisa_core::GoalMode::Guided,
        ..bisa_engine::SubmitRequest::captured(statement)
    }
}

/// The engine's `git` with no global and no system configuration: the
/// credential chain ends at git's own helper (`codehost::git_credentials`),
/// and on a developer's machine that helper is the OS keychain. A fixture
/// hands this in so no test reaches it, however the suite was started.
pub fn isolated_git() -> bisa_vcs::Git {
    bisa_vcs::Git::new()
        .with_env("GIT_CONFIG_GLOBAL", "/dev/null")
        .with_env("GIT_CONFIG_NOSYSTEM", "1")
}

/// Engine config with the autonomous parts off: no guided wakes, and no
/// listening tasks (the ear, the ticker, the signal worker). Tests drive
/// them by hand.
pub fn design_off_config() -> EngineConfig {
    EngineConfig {
        design_enabled: false,
        events_enabled: false,
        git: Some(isolated_git()),
        ..Default::default()
    }
}

pub fn engine_with(dir: &tempfile::TempDir, adapters: Vec<MockAdapter>) -> Engine {
    Engine::start(workspace(dir), catalog_with(adapters), design_off_config()).unwrap()
}

/// Wait until an engine event matching `pred` arrives (or panic on timeout).
pub async fn wait_for(
    rx: &mut tokio::sync::broadcast::Receiver<EngineEvent>,
    what: &str,
    mut pred: impl FnMut(&EngineEvent) -> bool,
) -> EngineEvent {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            match rx.recv().await {
                Ok(ev) if pred(&ev) => return ev,
                Ok(_) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(e) => panic!("bus closed while waiting for {what}: {e}"),
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {what}"))
}

/// Poll until `probe` answers, or panic — for state the engine reaches
/// asynchronously.
pub async fn until<T>(what: &str, mut probe: impl FnMut() -> Option<T>) -> T {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(v) = probe() {
            return v;
        }
        assert!(std::time::Instant::now() < deadline, "timed out: {what}");
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}

/// One intake request/response over the engine's socket — the door a
/// harness session's tools use.
pub async fn intake_roundtrip(socket: &std::path::Path, req: Value) -> Value {
    let stream = UnixStream::connect(socket).await.expect("connect intake");
    let (read, mut write) = stream.into_split();
    let mut line = req.to_string();
    line.push('\n');
    write.write_all(line.as_bytes()).await.unwrap();
    let mut lines = BufReader::new(read).lines();
    let reply = lines.next_line().await.unwrap().expect("reply line");
    serde_json::from_str(&reply).unwrap()
}

// ---------------------------------------------------------------------------
// Workflows from steps
// ---------------------------------------------------------------------------

pub fn sid(s: &str) -> StepId {
    StepId::new(s).unwrap()
}

/// A step with the defaults: no flows, `join: all`, `on_fail: fail`, no
/// retries, three visits.
pub fn step(id: &str, kind: StepKind) -> Step {
    Step {
        id: sid(id),
        name: id.replace(['-', '_'], " "),
        kind,
        then: vec![],
        boundaries: vec![],
        join: Join::All,
        on_fail: OnFail::Fail,
        retries: 0,
        max_visits: 3,
        position: None,
    }
}

/// An `agent` step on `harness`, with no assignee and no schema. Its
/// instructions read no `{goal.…}`, so the same step runs on a goal and in
/// the workspace (a goal's run is told its goal by the first prompt's note).
pub fn agent_step(id: &str, harness: &str) -> Step {
    step(
        id,
        StepKind::Agent {
            instructions: format!("do {id}"),
            assignee: None,
            project: None,
            harness: vec![harness.into()],
            model: None,
            effort: None,
            output_schema: None,
            tier_ceiling: ToolTier::Write,
        },
    )
}

/// The same step, with the ceiling `ceiling` — a `read` step that may change
/// nothing, an `exec` one that runs commands on its own.
pub fn agent_step_at(id: &str, harness: &str, ceiling: ToolTier) -> Step {
    let mut s = agent_step(id, harness);
    if let StepKind::Agent { tier_ceiling, .. } = &mut s.kind {
        *tier_ceiling = ceiling;
    }
    s
}

/// An `agent` step on `harness` whose output schema requires `fields` —
/// what a later step may read of it (`{steps.<id>.output.<field>}`, an
/// `output_equals` rule).
pub fn agent_step_promising(id: &str, harness: &str, fields: &[&str]) -> Step {
    let mut s = agent_step(id, harness);
    if let StepKind::Agent { output_schema, .. } = &mut s.kind {
        *output_schema = Some(json!({ "type": "object", "required": fields }));
    }
    s
}

/// Run one hand-built work item spec as the single agent step of a fresh
/// workflow on `goal`: the placement, assignees, harness chain, model and
/// effort pins, schema and ceiling become the step's, the run starts, and the
/// item the engine made for the step is returned once it settled.
pub async fn run_spec(engine: &Engine, goal: GoalId, spec: WorkItemSpec) -> bisa_core::WorkItemId {
    let mut s = step(
        "work",
        StepKind::Agent {
            instructions: spec.instructions.clone(),
            assignee: spec
                .assignees
                .first()
                .cloned()
                .map(bisa_core::ValueRef::Fixed),
            project: spec.project.map(bisa_core::ValueRef::Fixed),
            harness: spec.harness_candidates.clone(),
            model: spec.model.clone(),
            effort: spec.effort,
            output_schema: spec.output_schema.clone(),
            tier_ceiling: spec.tier_ceiling,
        },
    );
    s.name = "Work".into();
    let wf = engine
        .create_workflow(new_workflow("one step", vec![s]))
        .unwrap();
    engine.set_workflow(goal, Some(wf.id)).unwrap();
    let run = engine.start_run(goal, BTreeMap::new()).unwrap();
    until("the step's item to settle", || {
        engine
            .workspace()
            .list_work_items(&Home::Goal { goal })
            .ok()?
            .into_iter()
            .filter(|i| i.run == Some(run.id))
            .find(|i| {
                !matches!(
                    i.state,
                    bisa_core::WorkItemState::Open
                        | bisa_core::WorkItemState::Claimed { .. }
                        | bisa_core::WorkItemState::InProgress { .. }
                )
            })
            .map(|i| i.id)
    })
    .await
}

/// The same one-step run, but through the goal's **own design**
/// (`design_workflow`, `WorkflowOrigin::Goal`) rather than a library workflow —
/// the difference a project's origin hangs on.
pub async fn run_spec_on_design(
    engine: &Engine,
    goal: GoalId,
    spec: WorkItemSpec,
) -> bisa_core::WorkItemId {
    let mut s = step(
        "work",
        StepKind::Agent {
            instructions: spec.instructions.clone(),
            assignee: spec
                .assignees
                .first()
                .cloned()
                .map(bisa_core::ValueRef::Fixed),
            project: spec.project.map(bisa_core::ValueRef::Fixed),
            harness: spec.harness_candidates.clone(),
            model: spec.model.clone(),
            effort: spec.effort,
            output_schema: spec.output_schema.clone(),
            tier_ceiling: spec.tier_ceiling,
        },
    );
    s.name = "Work".into();
    let (wf, problems) = engine
        .design_workflow(goal, new_workflow("the goal's own", vec![s]), None)
        .unwrap();
    assert!(problems.is_empty(), "the design has problems: {problems:?}");
    assert_eq!(wf.origin, bisa_core::WorkflowOrigin::Goal { goal });
    let run = engine.start_run(goal, BTreeMap::new()).unwrap();
    until("the step's item to settle", || {
        engine
            .workspace()
            .list_work_items(&Home::Goal { goal })
            .ok()?
            .into_iter()
            .filter(|i| i.run == Some(run.id))
            .find(|i| {
                !matches!(
                    i.state,
                    bisa_core::WorkItemState::Open
                        | bisa_core::WorkItemState::Claimed { .. }
                        | bisa_core::WorkItemState::InProgress { .. }
                )
            })
            .map(|i| i.id)
    })
    .await
}

/// An `agent` step assigned to one agent.
pub fn assigned_agent_step(id: &str, harness: &str, agent: &str) -> Step {
    let mut s = agent_step(id, harness);
    if let StepKind::Agent { assignee, .. } = &mut s.kind {
        *assignee = Some(bisa_core::ValueRef::Fixed(Assignee::Agent(agent.into())));
    }
    s
}

/// Link the steps in order: each one flows to the next.
pub fn chain(mut steps: Vec<Step>) -> Vec<Step> {
    let ids: Vec<StepId> = steps.iter().map(|s| s.id.clone()).collect();
    for (i, s) in steps.iter_mut().enumerate() {
        if let Some(next) = ids.get(i + 1) {
            s.then = vec![Flow::to(next.clone())];
        }
    }
    steps
}

pub fn new_workflow(name: &str, steps: Vec<Step>) -> NewWorkflow {
    NewWorkflow {
        name: name.into(),
        description: format!("{name}, for a test"),
        inputs: vec![],
        steps,
        tags: Default::default(),
        decision_making: false,
    }
}

/// Record the workflow and capture a manual goal that points at it.
pub fn goal_on(engine: &Engine, statement: &str, draft: NewWorkflow) -> (Goal, Workflow) {
    let wf = engine.create_workflow(draft).unwrap();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            workflow: Some(wf.id),
            ..SubmitRequest::captured(statement)
        })
        .unwrap();
    (goal, wf)
}

/// Record the workflow, capture a manual goal on it and start the run.
pub fn run_on(engine: &Engine, statement: &str, draft: NewWorkflow) -> (Goal, WorkflowRun) {
    let (goal, _) = goal_on(engine, statement, draft);
    let run = engine.start_run(goal.id, BTreeMap::new()).unwrap();
    (goal, run)
}

/// The goal's current run, as the store has it.
pub fn current_run(engine: &Engine, goal: GoalId) -> WorkflowRun {
    engine
        .workspace()
        .get_current_run(goal)
        .unwrap()
        .expect("the goal has a run")
}

/// Wait for the goal's run to finish and return it.
pub async fn finished_run(engine: &Engine, goal: GoalId) -> WorkflowRun {
    until("the run to finish", || {
        let run = engine.workspace().get_current_run(goal).ok().flatten()?;
        run.is_finished().then_some(run)
    })
    .await
}

/// Wait until one step of the goal's run is in `state`.
pub async fn step_in_state(engine: &Engine, goal: GoalId, step: &str, state: &str) -> WorkflowRun {
    let step = sid(step);
    until(&format!("step `{step}` to be {state}"), || {
        let run = engine.workspace().get_current_run(goal).ok().flatten()?;
        (run.steps.get(&step)?.state.as_str() == state).then_some(run)
    })
    .await
}

/// The work items of a goal, oldest first.
pub fn items_of(engine: &Engine, goal: GoalId) -> Vec<WorkItemSpec> {
    items_at(engine, &Home::Goal { goal })
}

/// The work items filed at a home — a goal, or a run of the workspace.
pub fn items_at(engine: &Engine, home: &Home) -> Vec<WorkItemSpec> {
    engine.workspace().list_work_items(home).unwrap()
}

/// The notes on a goal's journal, oldest first.
pub fn notes(engine: &Engine, goal: GoalId) -> Vec<String> {
    notes_at(engine, &Home::Goal { goal })
}

/// The notes on a home's journal, oldest first.
pub fn notes_at(engine: &Engine, home: &Home) -> Vec<String> {
    engine
        .workspace()
        .journal(home)
        .unwrap()
        .into_iter()
        .filter_map(|e| match e.payload {
            bisa_core::event::JournalPayload::Note { text } => Some(text),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Runs of the workspace
// ---------------------------------------------------------------------------

/// Record the workflow and start a run of it in the workspace — no goal.
pub fn workspace_run(engine: &Engine, draft: NewWorkflow) -> (Workflow, WorkflowRun) {
    let wf = engine.create_workflow(draft).unwrap();
    let run = engine.start_workspace_run(wf.id, BTreeMap::new()).unwrap();
    (wf, run)
}

/// A run as the store has it now.
pub fn run_of(engine: &Engine, run: RunId) -> WorkflowRun {
    engine.workspace().get_run(run).unwrap()
}

/// Wait for one run — of either kind — to finish and return it.
pub async fn run_finished(engine: &Engine, run: RunId) -> WorkflowRun {
    until("the run to finish", || {
        let run = engine.workspace().get_run(run).ok()?;
        run.is_finished().then_some(run)
    })
    .await
}

/// Wait until one step of a run — of either kind — is in `state`.
pub async fn run_step_in_state(
    engine: &Engine,
    run: RunId,
    step: &str,
    state: &str,
) -> WorkflowRun {
    let step = sid(step);
    until(&format!("step `{step}` of run {run} to be {state}"), || {
        let run = engine.workspace().get_run(run).ok()?;
        (run.steps.get(&step)?.state.as_str() == state).then_some(run)
    })
    .await
}

/// Point a core agent at a harness this test controls — the one edit its
/// definition accepts besides the model plan.
pub fn drive_on(ws: &Workspace, agent: &bisa_core::AgentId, harness: &str) {
    let mut def = ws.get_agent(agent).unwrap();
    def.harness = harness.into();
    ws.update_agent(def).unwrap();
}

/// A decision provider whose answer is a panic — the shape of a fault inside
/// a judgement, for the tasks that ask to be tested against a real unwind.
pub struct PanickingProvider;

#[async_trait::async_trait]
impl bisa_decision::DecisionProvider for PanickingProvider {
    fn descriptor(&self) -> bisa_decision::ProviderDescriptor {
        bisa_decision::ScriptedProvider::new(vec![]).descriptor()
    }

    async fn decide(
        &self,
        _request: &bisa_core::DecisionRequest,
        _deadline: Duration,
    ) -> Result<bisa_core::DecisionResponse, bisa_decision::ProviderError> {
        panic!("the provider broke in the middle of a judgement");
    }
}
