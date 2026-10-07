//! Model plans, the health ledger, and failover — driven end to end through
//! the engine against the mock harness.
//!
//! `ModelPlan::order` is already exhaustively unit-tested as a pure function
//! in `bisa-harness`. What these tests are for is the other half: that
//! the *engine* feeds it correctly (the right plan, the right health view, a
//! rotation that advances), and that what it does with the answer is right —
//! a retry rather than a failure, the same workstream, the same clock, a
//! journal that says what happened, and a bound that always stops.
//!
//! No real harness is ever started: every session here is a `MockAdapter`
//! told which models to refuse and how.

use crate::common;
use bisa_core::event::JournalPayload;
use bisa_core::workitem::{WorkItemSpec, WorkItemState};
use bisa_core::Budget;
use bisa_core::{
    Effort, EffortChoice, GoalId, Project, RespondPolicy, SettingScope, ToolTier, WorkItemId,
    WorkstreamKind,
};
use bisa_engine::{projects, Engine, EngineConfig, EnginePayload, SubmitRequest};
use bisa_harness::mock::{DeadModel, IntakeScript, MockAdapter, ModelFailure};
use bisa_harness::{HarnessAdapter as _, HarnessCatalog, ModelChoice, ModelPlan, ModelStrategy};
use bisa_store::{MemoryKeyStore, NewAgent, NewProject, Workspace, WorkstreamFilter};
use common::run_spec;
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn ulid() -> ulid::Ulid {
    ulid::Ulid::from_datetime(SystemTime::now())
}

fn workspace(dir: &tempfile::TempDir) -> Workspace {
    Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap()
}

fn config() -> EngineConfig {
    EngineConfig {
        design_enabled: false,
        ..Default::default()
    }
}

/// One mock harness, its refusals, and a handle the test keeps so it can read
/// back exactly what was launched.
fn engine_with(
    dir: &tempfile::TempDir,
    adapter: MockAdapter,
    config: EngineConfig,
) -> (Engine, Arc<MockAdapter>) {
    let adapter = Arc::new(adapter);
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<dyn bisa_harness::HarnessAdapter>);
    let engine = Engine::start(workspace(dir), catalog, config).unwrap();
    (engine, adapter)
}

/// A step is judged by the result its session yields, so the mock's every
/// live session submits one through the intake — a session that merely
/// ends is a failed item, whatever model it ran on.
fn yielding_intake() -> IntakeScript {
    IntakeScript::new(vec![json!({
        "op": "result_submit", "work_item": "{{work_item}}", "output": {"ok": true}
    })])
}

fn mock_with(dead: Vec<ModelFailure>) -> MockAdapter {
    MockAdapter {
        dead_models: dead,
        intake_script: Some(yielding_intake()),
        ..Default::default()
    }
}

fn plan(strategy: ModelStrategy, models: &[(&str, u32)]) -> ModelPlan {
    ModelPlan {
        strategy,
        effort: None,
        models: models
            .iter()
            .map(|(m, w)| ModelChoice::weighted(*m, *w))
            .collect(),
    }
}

/// An agent on the mock harness with this model plan.
fn agent_with_plan(ws: &Workspace, name: &str, plan: ModelPlan) -> String {
    ws.add_agent(NewAgent {
        name: name.into(),
        photo: None,
        description: None,
        system_prompt: format!("You are {name}."),
        harness: "mock".into(),
        models: plan,
        skills: vec![],
        mcps: vec![],
        tags: Default::default(),
        respond: RespondPolicy::OwnerOnly,
        decision_making: false,
    })
    .unwrap()
    .id
    .to_string()
}

/// A managed project attached to `goal`. Projects belong to the workspace;
/// the attachment is what makes one visible to a goal's work.
fn attached_project(ws: &Workspace, goal: GoalId, new: NewProject) -> Project {
    let project = ws.create_project(new).unwrap();
    ws.attach(goal, project.id).unwrap();
    project
}

fn item(goal: GoalId, agent: &str) -> WorkItemSpec {
    WorkItemSpec {
        id: WorkItemId::from_ulid(ulid()),
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
        assignees: vec![bisa_core::Assignee::Agent(agent.to_string())],
        tier_ceiling: ToolTier::Write,
        // The agent is the request: the engine picks it and writes it back.
        agent: None,
        spawn_allowlist: vec![],
        depth_budget: 0,
        result_attempts: 0,
        interruptions: 0,
    }
}

/// Capture a manual goal, ready to run.
fn goal_with_a_running_step(engine: &Engine, statement: &str) -> GoalId {
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..SubmitRequest::captured(statement)
        })
        .unwrap();
    goal.id
}

/// Run one item as the single step of a workflow on the goal, and wait for
/// it to settle. Returns the item id.
async fn run_item(
    engine: &Engine,
    rx: &mut tokio::sync::broadcast::Receiver<bisa_engine::EngineEvent>,
    goal: GoalId,
    spec: WorkItemSpec,
) -> WorkItemId {
    let item_id = run_spec(engine, goal, spec).await;
    settle(rx).await;
    item_id
}

/// Wait for the next `ExecutionEnded`, returning its outcome.
async fn settle(
    rx: &mut tokio::sync::broadcast::Receiver<bisa_engine::EngineEvent>,
) -> bisa_engine::ExecutionOutcome {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    if let EnginePayload::ExecutionEnded { outcome } = &ev.payload {
                        return outcome.clone();
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(e) => panic!("bus closed: {e}"),
            }
        }
    })
    .await
    .expect("the run must settle")
}

/// Every `Note` in the goal's journal.
fn notes(engine: &Engine, goal: GoalId) -> Vec<String> {
    engine
        .workspace()
        .journal(&bisa_core::Home::from(goal))
        .unwrap()
        .into_iter()
        .filter_map(|e| match e.payload {
            JournalPayload::Note { text } => Some(text),
            _ => None,
        })
        .collect()
}

fn asked(adapter: &MockAdapter) -> Vec<String> {
    adapter
        .launched_models()
        .into_iter()
        .map(|m| m.unwrap_or_else(|| "<default>".into()))
        .collect()
}

// ---------------------------------------------------------------------------
// Fallback — the headline case
// ---------------------------------------------------------------------------

/// `Fallback`, first model dead at launch: the item **completes on the second
/// model**, the journal shows the switch, and the first model is left in
/// cooldown for whoever comes next.
///
/// The refusal is `NoProgress`, which is what a *failed launch* actually looks
/// like coming from a subprocess harness — a terminal `ModelUnavailable` with
/// nothing before it. See `bisa-adapters`'s "Launch versus mid-run".
#[tokio::test(flavor = "multi_thread")]
async fn fallback_completes_on_the_next_model_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(
        &dir,
        mock_with(vec![ModelFailure::new("fable-5", DeadModel::NoProgress)
            .reason("rate limited")
            .retry_after(240)]),
        config(),
    );
    let mut rx = engine.events();
    let agent = agent_with_plan(
        engine.workspace(),
        "Builder",
        plan(ModelStrategy::Fallback, &[("fable-5", 1), ("opus-5", 1)]),
    );

    let goal = goal_with_a_running_step(&engine, "ship it");
    let item_id = run_item(&engine, &mut rx, goal, item(goal, &agent)).await;

    // Both models were tried, in plan order, and only twice.
    assert_eq!(asked(&adapter), vec!["fable-5", "opus-5"]);

    // The item is not blocked: it ran to completion on opus-5.
    let settled = engine
        .workspace()
        .get_work_item(&bisa_core::Home::from(goal), item_id)
        .unwrap();
    assert!(
        !matches!(settled.state, WorkItemState::Blocked { .. }),
        "a dead model must never settle the item as failed: {:?}",
        settled.state
    );

    // The switch is in the journal, in words a human can act on.
    let note = notes(&engine, goal)
        .into_iter()
        .find(|n| n.contains("fable-5"))
        .expect("the switch must be journaled");
    assert!(note.contains("rate limited"), "{note}");
    assert!(note.contains("retrying on opus-5"), "{note}");
    assert!(
        note.contains("retry in 2"),
        "the harness said 240s; the ledger must honour it: {note}"
    );

    // ...and fable-5 is in cooldown for the next item.
    let health = engine.model_health();
    let row = health
        .iter()
        .find(|r| r.model == "fable-5")
        .expect("fable-5 in the ledger");
    assert_eq!(row.harness, "mock");
    assert_eq!(row.consecutive_failures, 1);
    assert!(
        matches!(row.retry_in_secs, Some(s) if (200..=240).contains(&s)),
        "{row:?}"
    );
    // The model that worked is healthy: it holds nothing at all.
    assert!(
        !health.iter().any(|r| r.model == "opus-5"),
        "a model that ran keeps no failure state: {health:?}"
    );
}

/// The same fact arriving the *other* way — a synchronous
/// `HarnessError::ModelUnavailable` from `launch()`, which only an adapter
/// with a real round-trip launch (a2a) can produce. The engine must not care
/// which shape it came in.
#[tokio::test(flavor = "multi_thread")]
async fn a_synchronous_launch_refusal_walks_the_same_chain() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(
        &dir,
        mock_with(vec![ModelFailure::new("fable-5", DeadModel::AtLaunch)]),
        config(),
    );
    let mut rx = engine.events();
    let agent = agent_with_plan(
        engine.workspace(),
        "Builder",
        plan(ModelStrategy::Fallback, &[("fable-5", 1), ("opus-5", 1)]),
    );
    let goal = goal_with_a_running_step(&engine, "ship it");
    let item_id = run_item(&engine, &mut rx, goal, item(goal, &agent)).await;

    assert_eq!(asked(&adapter), vec!["fable-5", "opus-5"]);
    let settled = engine
        .workspace()
        .get_work_item(&bisa_core::Home::from(goal), item_id)
        .unwrap();
    assert!(!matches!(settled.state, WorkItemState::Blocked { .. }));
    assert!(engine.model_health().iter().any(|r| r.model == "fable-5"));
}

/// The third way the same fact arrives: a session that is over before
/// anybody listens. An agent that speaks a protocol is asked for its model in
/// the handshake, and one that does not offer it ends there — before the
/// engine has subscribed, and before it is prompted. The end is the last
/// thing the session said and the engine hears it whenever it comes: the
/// plan walks to its next model, and the step is never failed with *session
/// terminated* for a model that was merely not on offer.
#[tokio::test(flavor = "multi_thread")]
async fn a_session_over_before_anybody_listened_walks_the_same_chain() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(
        &dir,
        mock_with(vec![ModelFailure::new(
            "fable-5",
            DeadModel::BeforeListening,
        )
        .reason("not offered by this session")]),
        config(),
    );
    let mut rx = engine.events();
    let agent = agent_with_plan(
        engine.workspace(),
        "Builder",
        plan(ModelStrategy::Fallback, &[("fable-5", 1), ("opus-5", 1)]),
    );
    let goal = goal_with_a_running_step(&engine, "ship it");
    let item_id = run_item(&engine, &mut rx, goal, item(goal, &agent)).await;

    assert_eq!(asked(&adapter), vec!["fable-5", "opus-5"]);
    let settled = engine
        .workspace()
        .get_work_item(&bisa_core::Home::from(goal), item_id)
        .unwrap();
    assert!(
        !matches!(settled.state, WorkItemState::Blocked { .. }),
        "a model that was not on offer never settles the item as failed: {:?}",
        settled.state
    );
    let note = notes(&engine, goal)
        .into_iter()
        .find(|n| n.contains("fable-5"))
        .expect("the switch is journaled");
    assert!(note.contains("not offered by this session"), "{note}");
    assert!(note.contains("retrying on opus-5"), "{note}");
}

// ---------------------------------------------------------------------------
// Mid-run — the sharp edge
// ---------------------------------------------------------------------------

/// A model that dies **after doing real work** relaunches into the *same
/// workstream*, and the journal says the work was already underway.
///
/// This is the integration edge the whole wave turns on: Wave 2A opens a git
/// worktree before launch, and a retry that called `open_workstream` again would
/// try a second `git worktree add` on a branch that already exists — which
/// fails outright. The assertion is direct: both launches got the same `cwd`,
/// and exactly one workstream exists for the item.
#[tokio::test(flavor = "multi_thread")]
async fn a_mid_run_death_relaunches_in_the_same_workstream() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(
        &dir,
        mock_with(vec![
            ModelFailure::new("fable-5", DeadModel::MidRun).reason("quota exhausted mid-turn")
        ]),
        config(),
    );
    let mut rx = engine.events();
    let agent = agent_with_plan(
        engine.workspace(),
        "Builder",
        plan(ModelStrategy::Fallback, &[("fable-5", 1), ("opus-5", 1)]),
    );

    // A real git project, so the workstream is a worktree on its own branch —
    // the case where reopening would actually fail.
    let goal = goal_with_a_running_step(&engine, "ship checkout");
    let ws = engine.workspace();
    let project = attached_project(ws, goal, NewProject::managed("storefront").unwrap());
    let project = projects::init_git(engine.inner(), &project).await.unwrap();
    let root = ws.project_root_path(&project);
    for (k, v) in [("user.name", "Bisa Test"), ("user.email", "t@e.invalid")] {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["config", k, v])
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .status()
            .unwrap();
    }
    std::fs::write(root.join("README.md"), "baseline\n").unwrap();
    bisa_vcs::git::add_all(&root).unwrap();
    bisa_vcs::git::commit(&root, "baseline", false).unwrap();

    let mut spec = item(goal, &agent);
    spec.project = Some(project.id);
    let item_id = run_item(&engine, &mut rx, goal, spec).await;

    assert_eq!(asked(&adapter), vec!["fable-5", "opus-5"]);

    // One workstream for the item, and it is a worktree on a branch.
    let workstreams = ws
        .list_workstreams(WorkstreamFilter::WorkItem(item_id))
        .unwrap();
    assert_eq!(
        workstreams.len(),
        1,
        "a retry must not open a second workstream"
    );
    assert!(matches!(
        workstreams[0].kind,
        WorkstreamKind::Worktree { .. }
    ));

    // ...and the relaunch actually re-entered it.
    let cwds: Vec<_> = adapter.launches().into_iter().map(|s| s.cwd).collect();
    assert_eq!(cwds.len(), 2);
    assert_eq!(
        cwds[0], cwds[1],
        "the retry must run where the dead session ran"
    );
    assert_eq!(
        cwds[0],
        engine
            .workspace()
            .workstream_checkout(&workstreams[0])
            .unwrap()
    );

    // The journal warns that a partial change may be sitting there.
    let note = notes(&engine, goal)
        .into_iter()
        .find(|n| n.contains("fable-5"))
        .expect("the switch must be journaled");
    assert!(note.contains("died mid-run"), "{note}");
    assert!(note.contains("partial change"), "{note}");
    assert!(note.contains("retrying on opus-5"), "{note}");
}

/// A failover must not buy the item a fresh clock.
///
/// The item's wall clock is 2s and every turn costs 1.2s. The first model
/// dies mid-run at 1.2s; the retry therefore has 0.8s left and must time out.
/// If the clock restarted, the second model would finish comfortably and the
/// item would complete — so the assertion discriminates precisely.
#[tokio::test(flavor = "multi_thread")]
async fn a_failover_does_not_reset_the_wall_clock() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(
        &dir,
        MockAdapter {
            dead_models: vec![ModelFailure::new("fable-5", DeadModel::MidRun)],
            turn_delay: Duration::from_millis(1200),
            intake_script: Some(yielding_intake()),
            ..Default::default()
        },
        // A step's item runs on the engine's clock: two seconds, so the
        // first attempt's 1.2 s leaves the retry 0.8 s — less than its turn.
        EngineConfig {
            default_wall_clock_secs: 2,
            ..config()
        },
    );
    let mut rx = engine.events();
    let agent = agent_with_plan(
        engine.workspace(),
        "Builder",
        plan(ModelStrategy::Fallback, &[("fable-5", 1), ("opus-5", 1)]),
    );

    let goal = goal_with_a_running_step(&engine, "ship it");
    let item_id = run_item(&engine, &mut rx, goal, item(goal, &agent)).await;

    assert_eq!(asked(&adapter), vec!["fable-5", "opus-5"]);
    let settled = engine
        .workspace()
        .get_work_item(&bisa_core::Home::from(goal), item_id)
        .unwrap();
    match settled.state {
        WorkItemState::Blocked { reason, .. } => assert_eq!(
            reason, "wall clock exceeded",
            "the retry must inherit what is left of the clock, not a fresh one"
        ),
        other => panic!("expected the retry to run out of clock, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// The strategies, as the engine feeds them
// ---------------------------------------------------------------------------

/// `RoundRobin` advances **across runs**: the engine takes one rotation per
/// work item, so three consecutive items lead with a, b, a.
#[tokio::test(flavor = "multi_thread")]
async fn round_robin_rotates_across_work_items() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(&dir, MockAdapter::default(), config());
    let mut rx = engine.events();
    let agent = agent_with_plan(
        engine.workspace(),
        "Builder",
        plan(ModelStrategy::RoundRobin, &[("alpha", 1), ("beta", 1)]),
    );

    for i in 0..3 {
        let goal = goal_with_a_running_step(&engine, &format!("run {i}"));
        run_item(&engine, &mut rx, goal, item(goal, &agent)).await;
    }
    assert_eq!(asked(&adapter), vec!["alpha", "beta", "alpha"]);
}

/// `Weighted` spends its rotations proportionally: with 3:1 the first three
/// runs lead with the heavy model and the fourth with the light one.
#[tokio::test(flavor = "multi_thread")]
async fn weighted_leads_proportionally_to_weight() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(&dir, MockAdapter::default(), config());
    let mut rx = engine.events();
    let agent = agent_with_plan(
        engine.workspace(),
        "Builder",
        plan(ModelStrategy::Weighted, &[("heavy", 3), ("light", 1)]),
    );

    for i in 0..4 {
        let goal = goal_with_a_running_step(&engine, &format!("run {i}"));
        run_item(&engine, &mut rx, goal, item(goal, &agent)).await;
    }
    assert_eq!(asked(&adapter), vec!["heavy", "heavy", "heavy", "light"]);
}

/// `LeastBusy` reads the **live** ledger, not the plan order: a model with a
/// session in flight loses to an idle one behind it.
#[tokio::test(flavor = "multi_thread")]
async fn least_busy_reads_the_live_in_flight_count() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(&dir, MockAdapter::default(), config());
    let mut rx = engine.events();
    let agent = agent_with_plan(
        engine.workspace(),
        "Builder",
        plan(ModelStrategy::LeastBusy, &[("busy", 1), ("idle", 1)]),
    );

    // Stand in for a session already running on the plan's first model.
    let held = engine.inner().models.acquire("mock", "busy");
    let goal = goal_with_a_running_step(&engine, "ship it");
    run_item(&engine, &mut rx, goal, item(goal, &agent)).await;
    assert_eq!(asked(&adapter), vec!["idle"]);

    // Release it, and plan order stands again.
    drop(held);
    let goal = goal_with_a_running_step(&engine, "ship it again");
    run_item(&engine, &mut rx, goal, item(goal, &agent)).await;
    assert_eq!(asked(&adapter), vec!["idle", "busy"]);
}

// ---------------------------------------------------------------------------
// The boundaries
// ---------------------------------------------------------------------------

/// A hard pin bypasses the plan, and an unavailable pin **fails loudly**.
///
/// This is a correctness boundary, not an ergonomic one: a user who named a
/// model named it for a reason, and quietly running a different one would
/// produce work they never asked for, attributed to a model that never ran.
#[tokio::test(flavor = "multi_thread")]
async fn a_pinned_model_is_never_substituted() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(
        &dir,
        mock_with(vec![ModelFailure::new("pinned-5", DeadModel::NoProgress)]),
        config(),
    );
    let mut rx = engine.events();
    // The agent's plan offers a perfectly good alternative — which is exactly
    // what must NOT be used.
    let agent = agent_with_plan(
        engine.workspace(),
        "Builder",
        plan(ModelStrategy::Fallback, &[("pinned-5", 1), ("opus-5", 1)]),
    );

    let goal = goal_with_a_running_step(&engine, "ship it");
    let mut spec = item(goal, &agent);
    spec.model = Some("pinned-5".into());
    let item_id = run_item(&engine, &mut rx, goal, spec).await;

    assert_eq!(
        asked(&adapter),
        vec!["pinned-5"],
        "the pin bypasses the plan; nothing else may be launched"
    );
    let settled = engine
        .workspace()
        .get_work_item(&bisa_core::Home::from(goal), item_id)
        .unwrap();
    match settled.state {
        WorkItemState::Blocked { reason, .. } => {
            assert!(reason.contains("pinned-5"), "{reason}");
            assert!(!reason.contains("opus-5"), "{reason}");
        }
        other => panic!("an unavailable pin must fail loudly, got {other:?}"),
    }
}

/// An agent whose every model is dead settles as one ordinary failure, with a
/// journal trail — and never launches more than `max_model_attempts` times.
#[tokio::test(flavor = "multi_thread")]
async fn max_model_attempts_bounds_a_hopeless_plan() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(
        &dir,
        mock_with(vec![
            ModelFailure::new("a", DeadModel::NoProgress),
            ModelFailure::new("b", DeadModel::NoProgress),
            ModelFailure::new("c", DeadModel::NoProgress),
        ]),
        EngineConfig {
            max_model_attempts: 2,
            ..config()
        },
    );
    let mut rx = engine.events();
    let agent = agent_with_plan(
        engine.workspace(),
        "Builder",
        plan(ModelStrategy::Fallback, &[("a", 1), ("b", 1), ("c", 1)]),
    );

    let goal = goal_with_a_running_step(&engine, "ship it");
    let item_id = run_item(&engine, &mut rx, goal, item(goal, &agent)).await;

    assert_eq!(
        asked(&adapter),
        vec!["a", "b"],
        "the budget is 2; the third model is never reached"
    );
    let settled = engine
        .workspace()
        .get_work_item(&bisa_core::Home::from(goal), item_id)
        .unwrap();
    match settled.state {
        WorkItemState::Blocked { reason, .. } => {
            assert!(
                reason.contains("every model tried is unavailable"),
                "{reason}"
            );
            // Both dead models are named, not just the last one — the board
            // has to say what actually happened.
            assert!(
                reason.contains("a unavailable on mock")
                    && reason.contains("b unavailable on mock"),
                "{reason}"
            );
        }
        other => panic!("expected one ordinary failure, got {other:?}"),
    }
    // Both walls are journaled, and the last one says the plan ran out.
    let notes = notes(&engine, goal);
    let walls: Vec<_> = notes.iter().filter(|n| n.contains("unavailable")).collect();
    assert_eq!(walls.len(), 2, "{notes:?}");
    assert!(walls[0].contains("retrying on b"), "{:?}", walls[0]);
    assert!(
        walls[1].contains("no model left in the plan"),
        "{:?}",
        walls[1]
    );
}

/// A model in cooldown is skipped while it is cooling.
///
/// The cooldown is an hour so the assertion cannot lose a race: no amount of
/// machine load makes an hour pass between two work items. An earlier version
/// of this test used `retry_after(1)` and asserted that a *second full run*
/// finished inside that second — which held on an idle machine and failed
/// under load, reporting a scheduling artefact as a failover bug.
#[tokio::test(flavor = "multi_thread")]
async fn a_cooling_model_is_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(
        &dir,
        mock_with(vec![ModelFailure::new("fable-5", DeadModel::NoProgress)
            .retry_after(3600)
            // The quota comes back immediately; only the ledger's cooldown
            // keeps fable-5 out of the second run, which is the point.
            .only_first(1)]),
        config(),
    );
    let mut rx = engine.events();
    let agent = agent_with_plan(
        engine.workspace(),
        "Builder",
        plan(ModelStrategy::Fallback, &[("fable-5", 1), ("opus-5", 1)]),
    );

    // Run 1: fable-5 dies, opus-5 finishes.
    let goal = goal_with_a_running_step(&engine, "one");
    run_item(&engine, &mut rx, goal, item(goal, &agent)).await;
    assert_eq!(asked(&adapter), vec!["fable-5", "opus-5"]);
    assert!(
        engine
            .model_health()
            .iter()
            .any(|r| r.model == "fable-5" && r.is_cooling()),
        "{:?}",
        engine.model_health()
    );

    // Run 2: fable-5 would work now, but it is cooling, so the plan's second
    // entry leads and the first is never touched.
    let goal = goal_with_a_running_step(&engine, "two");
    run_item(&engine, &mut rx, goal, item(goal, &agent)).await;
    assert_eq!(asked(&adapter), vec!["fable-5", "opus-5", "opus-5"]);
}

/// Once the cooldown expires the model leads its plan again.
///
/// Split from the test above so neither half races: here a *longer* wait is
/// always safe, because the assertion wants the cooldown gone.
#[tokio::test(flavor = "multi_thread")]
async fn an_expired_cooldown_restores_plan_order() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(
        &dir,
        mock_with(vec![ModelFailure::new("fable-5", DeadModel::NoProgress)
            // The harness named one second, and the ledger honours what the
            // harness said rather than imposing its own floor on top of it.
            .retry_after(1)
            .only_first(1)]),
        config(),
    );
    let mut rx = engine.events();
    let agent = agent_with_plan(
        engine.workspace(),
        "Builder",
        plan(ModelStrategy::Fallback, &[("fable-5", 1), ("opus-5", 1)]),
    );

    let goal = goal_with_a_running_step(&engine, "one");
    run_item(&engine, &mut rx, goal, item(goal, &agent)).await;
    assert_eq!(asked(&adapter), vec!["fable-5", "opus-5"]);

    // Wait the cooldown out. Overshooting cannot make this test lie.
    let waited_out = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if !engine.model_health().iter().any(|r| r.is_cooling()) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    assert!(
        waited_out.is_ok(),
        "a 1s cooldown never expired: {:?}",
        engine.model_health()
    );

    // The *streak* survives the cooldown on purpose: it is what makes the next
    // backoff longer than the last. Only a success clears it.
    assert_eq!(
        engine
            .model_health()
            .iter()
            .find(|r| r.model == "fable-5")
            .map(|r| r.consecutive_failures),
        Some(1),
        "{:?}",
        engine.model_health()
    );

    let goal = goal_with_a_running_step(&engine, "two");
    run_item(&engine, &mut rx, goal, item(goal, &agent)).await;
    assert_eq!(asked(&adapter), vec!["fable-5", "opus-5", "fable-5"]);
    // ...and a model that ran clears its streak with it.
    assert!(
        !engine.model_health().iter().any(|r| r.model == "fable-5"),
        "{:?}",
        engine.model_health()
    );
}

/// An empty plan is not the absence of an attempt: the run launches once with
/// no model pinned, and the ledger names that pair honestly.
#[tokio::test(flavor = "multi_thread")]
async fn an_empty_plan_launches_the_harness_default() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(
        &dir,
        mock_with(vec![ModelFailure::new(
            "mock default",
            DeadModel::NoProgress,
        )]),
        config(),
    );
    let mut rx = engine.events();
    let agent = agent_with_plan(engine.workspace(), "Builder", ModelPlan::default());

    let goal = goal_with_a_running_step(&engine, "ship it");
    run_item(&engine, &mut rx, goal, item(goal, &agent)).await;

    assert_eq!(asked(&adapter), vec!["<default>"]);
    let health = engine.model_health();
    assert_eq!(health.len(), 1);
    assert_eq!(health[0].model, "mock default");
}

/// The bus carries the switch too, so a surface can show it live.
#[tokio::test(flavor = "multi_thread")]
async fn a_switch_is_broadcast_as_well_as_journaled() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, _adapter) = engine_with(
        &dir,
        mock_with(vec![ModelFailure::new("fable-5", DeadModel::MidRun)
            .reason("rate limited")
            .retry_after(120)]),
        config(),
    );
    let mut rx = engine.events();
    let agent = agent_with_plan(
        engine.workspace(),
        "Builder",
        plan(ModelStrategy::Fallback, &[("fable-5", 1), ("opus-5", 1)]),
    );
    let goal = goal_with_a_running_step(&engine, "ship it");
    let spec = item(goal, &agent);
    let item_id = run_spec(&engine, goal, spec).await;

    let switch = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    if let EnginePayload::ModelSwitched { .. } = &ev.payload {
                        return ev;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(e) => panic!("bus closed: {e}"),
            }
        }
    })
    .await
    .expect("a ModelSwitched event");

    match switch.payload {
        EnginePayload::ModelSwitched {
            work_item,
            from,
            to,
            reason,
            retry_in_secs,
            after_progress,
        } => {
            assert_eq!(work_item, Some(item_id));
            assert_eq!(from, "fable-5");
            assert_eq!(to, "opus-5");
            assert_eq!(reason, "rate limited");
            assert!((100..=120).contains(&retry_in_secs), "{retry_in_secs}");
            assert!(after_progress, "the mid-run shape must be carried through");
        }
        other => panic!("{other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Effort — who decides, and what each attempt is sent
// ---------------------------------------------------------------------------

/// What a model that takes `xhigh` takes, and what one that does not.
const FIVE: [Effort; 5] = [
    Effort::Low,
    Effort::Medium,
    Effort::High,
    Effort::Xhigh,
    Effort::Max,
];
const FOUR: [Effort; 4] = [Effort::Low, Effort::Medium, Effort::High, Effort::Max];

/// A mock with the effort control: every level for a model it keeps no list
/// for, and `lists` for the ones it does.
fn mock_taking(dead: Vec<ModelFailure>, lists: Vec<(&str, Vec<Effort>)>) -> MockAdapter {
    MockAdapter {
        efforts: Effort::ALL.to_vec(),
        model_efforts: lists
            .into_iter()
            .map(|(model, efforts)| (model.to_string(), efforts))
            .collect(),
        ..mock_with(dead)
    }
}

/// The roster's row for a work item's last session — finished rows stay a
/// minute, so a settled item still has its own.
fn roster_row(engine: &Engine, item: WorkItemId) -> bisa_engine::SessionPresence {
    engine
        .inner()
        .presence
        .snapshot()
        .into_iter()
        .find(|row| row.work_item == Some(item))
        .expect("the item's session is on the roster")
}

fn set(engine: &Engine, scope: SettingScope, project: Option<&Project>, word: &str) {
    engine
        .set_setting(scope, project.map(|p| p.id), "agents.effort", json!(word))
        .unwrap();
}

/// A git project with one commit, attached to `goal`: a work item placed in
/// it runs in a worktree of its own.
async fn committed_project(engine: &Engine, goal: GoalId, name: &str) -> Project {
    let ws = engine.workspace();
    let project = attached_project(ws, goal, NewProject::managed(name).unwrap());
    let project = projects::init_git(engine.inner(), &project).await.unwrap();
    let root = ws.project_root_path(&project);
    for (k, v) in [("user.name", "Bisa Test"), ("user.email", "t@e.invalid")] {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["config", k, v])
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .status()
            .unwrap();
    }
    std::fs::write(root.join("README.md"), "baseline\n").unwrap();
    bisa_vcs::git::add_all(&root).unwrap();
    bisa_vcs::git::commit(&root, "baseline", false).unwrap();
    project
}

/// Nobody said how hard to work — no step, no model, no plan, no setting
/// written: the session is sent `high`, and its row says so beside the model.
#[tokio::test(flavor = "multi_thread")]
async fn nothing_stated_anywhere_launches_at_high() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(&dir, mock_taking(vec![], vec![]), config());
    let mut rx = engine.events();
    let agent = agent_with_plan(
        engine.workspace(),
        "Builder",
        ModelPlan::fallback(["opus-5"]),
    );

    let goal = goal_with_a_running_step(&engine, "ship it");
    let item_id = run_item(&engine, &mut rx, goal, item(goal, &agent)).await;

    assert_eq!(asked(&adapter), vec!["opus-5"]);
    assert_eq!(adapter.launched_efforts(), vec![Some(Effort::High)]);
    let row = roster_row(&engine, item_id);
    assert_eq!(row.model.as_deref(), Some("opus-5"));
    assert_eq!(row.effort, Some(Effort::High));
    let wire = serde_json::to_value(&row).unwrap();
    assert_eq!(wire["effort"], json!("high"));

    // An empty plan is the harness's own model, at the same level.
    let bare = agent_with_plan(engine.workspace(), "Bare", ModelPlan::default());
    let goal = goal_with_a_running_step(&engine, "ship it again");
    run_item(&engine, &mut rx, goal, item(goal, &bare)).await;
    assert_eq!(asked(&adapter), vec!["opus-5", "<default>"]);
    assert_eq!(
        adapter.launched_efforts(),
        vec![Some(Effort::High), Some(Effort::High)]
    );
}

/// The chain, one link at a time: the agent's plan over the setting, a
/// model's own over the plan, a step's pin over all of them.
#[tokio::test(flavor = "multi_thread")]
async fn the_plan_then_the_model_then_the_step_each_win_in_turn() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(&dir, mock_taking(vec![], vec![]), config());
    let mut rx = engine.events();
    set(&engine, SettingScope::Workspace, None, "minimal");

    // The setting, when the plan says nothing.
    let silent = agent_with_plan(
        engine.workspace(),
        "Silent",
        ModelPlan::fallback(["opus-5"]),
    );
    // The plan's, over the setting.
    let planned = agent_with_plan(
        engine.workspace(),
        "Planned",
        ModelPlan::fallback(["opus-5"]).at(EffortChoice::Max),
    );
    // The model's own, over the plan's.
    let mut own = ModelPlan::fallback(["opus-5"]).at(EffortChoice::Max);
    own.models[0].effort = Some(EffortChoice::Low);
    let owned = agent_with_plan(engine.workspace(), "Owned", own);

    for agent in [&silent, &planned, &owned] {
        let goal = goal_with_a_running_step(&engine, "ship it");
        run_item(&engine, &mut rx, goal, item(goal, agent)).await;
    }
    // The step's pin, over the model's own.
    let goal = goal_with_a_running_step(&engine, "ship it, pinned");
    let mut pinned = item(goal, &owned);
    pinned.effort = Some(EffortChoice::Xhigh);
    let item_id = run_item(&engine, &mut rx, goal, pinned).await;

    assert_eq!(
        adapter.launched_efforts(),
        vec![
            Some(Effort::Minimal),
            Some(Effort::Max),
            Some(Effort::Low),
            Some(Effort::Xhigh),
        ]
    );
    // The pin travelled from the step to the item the engine made for it.
    let made = engine
        .workspace()
        .get_work_item(&bisa_core::Home::from(goal), item_id)
        .unwrap();
    assert_eq!(made.effort, Some(EffortChoice::Xhigh));
    assert_eq!(roster_row(&engine, item_id).effort, Some(Effort::Xhigh));
}

/// `agents.effort` is read for the project the work stands in: the project's
/// word over the workspace's, and the workspace's where no project says.
#[tokio::test(flavor = "multi_thread")]
async fn the_projects_setting_wins_over_the_workspaces() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(&dir, mock_taking(vec![], vec![]), config());
    let mut rx = engine.events();
    let agent = agent_with_plan(
        engine.workspace(),
        "Builder",
        ModelPlan::fallback(["opus-5"]),
    );

    let goal = goal_with_a_running_step(&engine, "ship checkout");
    let project = committed_project(&engine, goal, "storefront").await;
    set(&engine, SettingScope::Workspace, None, "low");
    set(&engine, SettingScope::Project, Some(&project), "max");
    assert_eq!(
        bisa_engine::effort::setting(engine.inner(), Some(project.id)),
        EffortChoice::Max
    );
    assert_eq!(
        bisa_engine::effort::setting(engine.inner(), None),
        EffortChoice::Low
    );

    // Work placed in the project runs at the project's word.
    let mut placed = item(goal, &agent);
    placed.project = Some(project.id);
    run_item(&engine, &mut rx, goal, placed).await;
    // Work that stands in no project runs at the workspace's.
    let elsewhere = goal_with_a_running_step(&engine, "answer a question");
    run_item(&engine, &mut rx, elsewhere, item(elsewhere, &agent)).await;

    assert_eq!(
        adapter.launched_efforts(),
        vec![Some(Effort::Max), Some(Effort::Low)]
    );
}

/// Each attempt is fitted to its own model's list: `xhigh` goes to the model
/// that takes it as it is, and — after a wall — to one that does not as the
/// nearest level below.
#[tokio::test(flavor = "multi_thread")]
async fn each_attempt_is_fitted_to_what_its_own_model_takes() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(
        &dir,
        mock_taking(
            vec![ModelFailure::new("fable-5", DeadModel::NoProgress)],
            vec![("fable-5", FIVE.to_vec()), ("opus-5", FOUR.to_vec())],
        ),
        config(),
    );
    let mut rx = engine.events();
    let agent = agent_with_plan(
        engine.workspace(),
        "Builder",
        ModelPlan::fallback(["fable-5", "opus-5"]).at(EffortChoice::Xhigh),
    );

    let goal = goal_with_a_running_step(&engine, "ship it");
    let item_id = run_item(&engine, &mut rx, goal, item(goal, &agent)).await;

    assert_eq!(asked(&adapter), vec!["fable-5", "opus-5"]);
    assert_eq!(
        adapter.launched_efforts(),
        vec![Some(Effort::Xhigh), Some(Effort::High)]
    );
    // The row is the session that ran: the second model, at its fitted level.
    let row = roster_row(&engine, item_id);
    assert_eq!(row.model.as_deref(), Some("opus-5"));
    assert_eq!(row.effort, Some(Effort::High));

    // Nothing below what was asked: the lowest the model takes.
    let low = agent_with_plan(
        engine.workspace(),
        "Light",
        ModelPlan::fallback(["opus-5"]).at(EffortChoice::Minimal),
    );
    let goal = goal_with_a_running_step(&engine, "a small thing");
    run_item(&engine, &mut rx, goal, item(goal, &low)).await;
    assert_eq!(adapter.launched_efforts().last(), Some(&Some(Effort::Low)));
}

/// A model that takes no effort is sent none, whatever was asked for and
/// whatever the harness takes for its other models — and its row says none.
#[tokio::test(flavor = "multi_thread")]
async fn an_empty_list_sends_nothing_and_the_row_has_no_effort() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(
        &dir,
        mock_taking(vec![], vec![("haiku-4", vec![])]),
        config(),
    );
    assert!(
        adapter.caps().contains(bisa_core::HarnessCaps::EFFORT),
        "the harness has the control; this model takes none of it"
    );
    let mut rx = engine.events();
    let agent = agent_with_plan(
        engine.workspace(),
        "Quick",
        ModelPlan::fallback(["haiku-4"]).at(EffortChoice::Max),
    );

    let goal = goal_with_a_running_step(&engine, "ship it");
    let mut pinned = item(goal, &agent);
    pinned.effort = Some(EffortChoice::Xhigh);
    let item_id = run_item(&engine, &mut rx, goal, pinned).await;

    assert_eq!(asked(&adapter), vec!["haiku-4"]);
    assert_eq!(adapter.launched_efforts(), vec![None]);
    let row = roster_row(&engine, item_id);
    assert_eq!(row.model.as_deref(), Some("haiku-4"));
    assert_eq!(row.effort, None);
    assert!(
        serde_json::to_value(&row).unwrap().get("effort").is_none(),
        "no effort, no key"
    );
}

/// A harness with no effort control at all is sent nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_harness_without_the_control_is_sent_no_effort() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(&dir, mock_with(vec![]), config());
    assert!(!adapter.caps().contains(bisa_core::HarnessCaps::EFFORT));
    let mut rx = engine.events();
    let agent = agent_with_plan(
        engine.workspace(),
        "Builder",
        ModelPlan::fallback(["opus-5"]).at(EffortChoice::Max),
    );
    let goal = goal_with_a_running_step(&engine, "ship it");
    let item_id = run_item(&engine, &mut rx, goal, item(goal, &agent)).await;
    assert_eq!(adapter.launched_efforts(), vec![None]);
    assert_eq!(roster_row(&engine, item_id).effort, None);
}

/// A session parked after its idle time is never taken up again: its token
/// stays on its row — the harness's own way to resume on its side — the
/// adapter is asked to attach by nothing, and the row keeps saying what the
/// session ran at. The next wake launches afresh and decides again.
#[tokio::test(flavor = "multi_thread")]
async fn a_parked_session_is_never_revived_and_its_row_keeps_what_it_ran_at() {
    use bisa_engine::presence::SessionMeta;
    use bisa_engine::registry::{AgentRef, AgentStatus, SessionKind};
    use bisa_engine::{LiveRunId, SessionState};

    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with(&dir, mock_taking(vec![], vec![]), config());
    let inner = engine.inner();

    // A session launched by hand, as a launch walk would have sent it.
    let session = adapter
        .launch(bisa_harness::SessionSpec {
            work_item: None,
            cwd: dir.path().to_path_buf(),
            prompt: String::new(),
            model: Some("opus-5".into()),
            effort: Some(Effort::Xhigh),
            mcp_servers: vec![],
            env: Default::default(),
            env_remove: Vec::new(),
            tier_ceiling: ToolTier::Read,
            output_schema: None,
            skills: vec![],
        })
        .await
        .unwrap();
    let token = session.resume_token().unwrap();
    assert_eq!(token.model.as_deref(), Some("opus-5"));
    assert_eq!(token.effort, Some(Effort::Xhigh));

    let run = LiveRunId::mint();
    inner
        .registry
        .register_if(
            AgentRef {
                id: run,
                kind: SessionKind::Worker,
                status: AgentStatus::Idle,
                generation: 1,
                session_id: None,
                work_item: None,
                conversation: None,
                goal: None,
                workstream: None,
                transcript_path: None,
                last_activity: 0,
            },
            None,
        )
        .unwrap();
    inner.presence.register(
        inner,
        run,
        SessionMeta {
            kind: SessionKind::Worker,
            origin: bisa_core::SessionOrigin::Step {
                step: None,
                name: None,
                resumed: false,
            },
            harness: "mock".into(),
            model: Some("opus-5".into()),
            effort: Some(Effort::Xhigh),
            agent: None,
            session_id: None,
            work_item: None,
            conversation: None,
            goal: None,
            run: None,
            workstream: None,
            project: None,
            cwd: None,
            transcript_path: None,
        },
    );
    inner
        .ws
        .record_session(&bisa_store::SessionRow {
            id: "sess-effort".into(),
            adapter: "mock".into(),
            kind: SessionKind::Worker,
            conversation: None,
            work_item: None,
            agent_id: None,
            transcript_path: None,
            resume_token_json: Some(serde_json::to_string(&token).unwrap()),
            workstream: None,
            status: bisa_store::SessionStatus::Live,
            parked_at: None,
            pid: None,
            pid_seen_at: None,
            ended_at: None,
        })
        .unwrap();
    inner.lifecycle.adopt(
        inner,
        run,
        session,
        "sess-effort".into(),
        Duration::from_millis(50),
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while inner.registry.get(run).unwrap().status != AgentStatus::Parked {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("the session never parked");

    // Parked: the row is parked, the token stays on it, and nothing attaches.
    let row = inner.ws.session_by_id("sess-effort").unwrap().unwrap();
    assert_eq!(row.status, bisa_store::SessionStatus::Parked);
    assert!(
        row.resume_token_json.is_some(),
        "the token is the harness's to keep"
    );
    assert!(
        !inner.lifecycle.follow_up(inner, run, "go on").await,
        "no live session takes a follow-up"
    );
    assert!(
        adapter.attached().is_empty(),
        "the engine asks nobody to attach"
    );
    assert_eq!(
        adapter.launches().len(),
        1,
        "and launched nothing more by itself"
    );
    let presence = inner.presence.get(run).expect("the row stays");
    assert_eq!(presence.state, SessionState::Parked);
    assert_eq!(presence.effort, Some(Effort::Xhigh));
    assert_eq!(presence.model.as_deref(), Some("opus-5"));
    engine.shutdown().await;
}

/// What the store validates a step's effort pin against: the harnesses whose
/// adapter takes an effort, and no other.
#[tokio::test(flavor = "multi_thread")]
async fn the_runtime_names_the_harnesses_that_take_an_effort() {
    let dir = tempfile::tempdir().unwrap();
    let engine = common::engine_with(
        &dir,
        vec![
            MockAdapter {
                id: "plain".into(),
                ..Default::default()
            },
            MockAdapter {
                id: "taking".into(),
                efforts: FOUR.to_vec(),
                ..Default::default()
            },
            MockAdapter {
                id: "by-model".into(),
                model_efforts: vec![("opus-5".into(), FIVE.to_vec())],
                ..Default::default()
            },
        ],
    );
    // Said at boot, before any model list was read, and again with them.
    assert_eq!(
        engine.workspace().known_runtime().effort_harnesses,
        ["taking", "by-model"]
    );
    engine.describe_runtime().await;
    let known = engine.workspace().known_runtime();
    assert_eq!(known.harnesses, ["plain", "taking", "by-model"]);
    assert_eq!(known.effort_harnesses, ["taking", "by-model"]);

    // A step that pins an effort on the harness without the control is a
    // problem by name; on one that has it, none.
    let problems = |harness: &str| -> Vec<bisa_core::ProblemKind> {
        let mut step = common::agent_step("work", harness);
        if let bisa_core::StepKind::Agent { effort, .. } = &mut step.kind {
            *effort = Some(EffortChoice::High);
        }
        let (_, problems) = engine
            .create_workflow_draft(common::new_workflow(harness, vec![step]))
            .unwrap();
        problems.into_iter().map(|p| p.kind).collect()
    };
    assert_eq!(
        problems("plain"),
        [bisa_core::ProblemKind::UnsupportedEffort]
    );
    assert!(problems("taking").is_empty());
    engine.shutdown().await;
}
