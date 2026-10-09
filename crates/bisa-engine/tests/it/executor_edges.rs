//! The executor's ways of not running an item — every harness disabled or
//! unavailable, the plan's attempts spent before a launch, a scratch
//! directory that cannot be made, a prompt the harness refuses, a session
//! the vendor suspends, a budget the first cost crosses — each blocking the
//! item with the reason and failing its step. Fakes only: the mock harness
//! and its knobs.

use crate::common;

use bisa_core::{Budget, Home, RunOutcome, StepKind, WorkItemState};
use bisa_engine::{Engine, EngineConfig, SubmitRequest};
use bisa_harness::mock::{DeadModel, MockAdapter, ModelFailure};
use bisa_harness::{
    HarnessAdapter, HarnessCatalog, LifecycleEvent, ModelChoice, ModelPlan, ModelStrategy, Outcome,
    ProgressEvent, SessionEvent,
};
use bisa_store::NewAgent;
use common::*;
use std::collections::BTreeMap;
use std::sync::Arc;

fn engine_on(dir: &tempfile::TempDir, adapters: Vec<MockAdapter>, config: EngineConfig) -> Engine {
    Engine::start(workspace(dir), catalog_with(adapters), config).unwrap()
}

/// The one item of the goal's run, blocked, and why.
fn blocked_reason(engine: &Engine, goal: bisa_core::GoalId) -> String {
    let items = items_of(engine, goal);
    assert_eq!(items.len(), 1, "{items:?}");
    match &items[0].state {
        WorkItemState::Blocked { reason, .. } => reason.clone(),
        other => panic!("the item is {other:?}, not blocked"),
    }
}

/// Run `draft` for a goal and wait for the run to fail; the item's reason.
async fn failed_with(
    engine: &Engine,
    draft: bisa_store::NewWorkflow,
) -> (bisa_core::WorkflowRun, String) {
    let (goal, _) = run_on(engine, "runs an item", draft);
    let run = finished_run(engine, goal.id).await;
    assert_eq!(run.outcome, Some(RunOutcome::Failed), "{run:?}");
    (run, blocked_reason(engine, goal.id))
}

fn ended(outcome: Outcome) -> SessionEvent {
    SessionEvent::Lifecycle(LifecycleEvent::Ended {
        outcome,
        is_terminal: true,
    })
}

/// A chain of two harnesses, one disabled by configuration and one whose
/// probe says it is not there: the item is blocked naming both reasons.
#[tokio::test(flavor = "multi_thread")]
async fn a_disabled_harness_and_an_absent_one_are_both_named() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(
        &dir,
        vec![
            MockAdapter::default(),
            MockAdapter {
                id: "other".into(),
                available: false,
                ..Default::default()
            },
        ],
        EngineConfig {
            disabled_harnesses: vec!["mock".into()],
            ..design_off_config()
        },
    );
    let mut work = agent_step("work", "mock");
    if let StepKind::Agent { harness, .. } = &mut work.kind {
        harness.push("other".into());
    }
    let (run, reason) = failed_with(&engine, new_workflow("two harnesses", vec![work])).await;
    assert!(reason.contains("mock: disabled"), "{reason}");
    assert!(reason.contains("other:"), "{reason}");
    assert!(
        run.steps[&sid("work")]
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("disabled"),
        "{run:?}"
    );
    engine.shutdown().await;
}

/// A plan whose every model is refused at launch spends the attempts the
/// node allows and stops before the next launch: the second model is never
/// tried, and the block names the wall that was.
#[tokio::test(flavor = "multi_thread")]
async fn the_attempts_are_spent_before_the_next_launch() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = Arc::new(MockAdapter {
        dead_models: vec![
            ModelFailure::new("a", DeadModel::AtLaunch),
            ModelFailure::new("b", DeadModel::AtLaunch),
        ],
        ..Default::default()
    });
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<dyn HarnessAdapter>);
    let engine = Engine::start(
        workspace(&dir),
        catalog,
        EngineConfig {
            max_model_attempts: 1,
            ..design_off_config()
        },
    )
    .unwrap();
    let agent = engine
        .workspace()
        .add_agent(NewAgent {
            name: "Builder".into(),
            photo: None,
            description: None,
            system_prompt: "You build.".into(),
            harness: "mock".into(),
            models: ModelPlan {
                strategy: ModelStrategy::Fallback,
                effort: None,
                models: vec![ModelChoice::weighted("a", 1), ModelChoice::weighted("b", 1)],
            },
            skills: vec![],
            mcps: vec![],
            tags: Default::default(),
            respond: bisa_core::RespondPolicy::OwnerOnly,
            decision_making: false,
        })
        .unwrap();
    let (_, reason) = failed_with(
        &engine,
        new_workflow(
            "hopeless",
            vec![assigned_agent_step("work", "mock", agent.id.as_str())],
        ),
    )
    .await;
    assert!(reason.contains("a unavailable on mock"), "{reason}");
    assert!(!reason.contains("b unavailable"), "{reason}");
    assert_eq!(
        adapter.launches.lock().unwrap().len(),
        1,
        "one attempt allowed, one launch made"
    );
    engine.shutdown().await;
}

/// The home's scratch directory cannot be made — a file sits where it
/// goes — so the item never launches, and says so.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_scratch_directory_that_cannot_be_made_blocks_the_item() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let (goal, _) = goal_on(
        &engine,
        "no scratch",
        new_workflow("scratchless", vec![agent_step("work", "mock")]),
    );
    let tmp = engine
        .workspace()
        .paths()
        .home(&Home::Goal { goal: goal.id })
        .tmp();
    std::fs::create_dir_all(tmp.parent().unwrap()).unwrap();
    std::fs::write(&tmp, b"not a directory").unwrap();
    engine.start_run(goal.id, BTreeMap::new()).unwrap();
    let run = finished_run(&engine, goal.id).await;
    assert_eq!(run.outcome, Some(RunOutcome::Failed), "{run:?}");
    let reason = blocked_reason(&engine, goal.id);
    assert!(reason.contains("scratch directory failed"), "{reason}");
    engine.shutdown().await;
}

/// A harness that refuses its first prompt, and one whose session the
/// vendor suspends: the item is blocked with the harness's words.
#[tokio::test(flavor = "multi_thread")]
async fn a_refused_prompt_and_a_suspended_session_block_the_item() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(
        &dir,
        vec![
            MockAdapter {
                refuse_prompt: true,
                ..Default::default()
            },
            MockAdapter {
                id: "paused".into(),
                script: Some(vec![
                    SessionEvent::Lifecycle(LifecycleEvent::Started),
                    ended(Outcome::Suspended {
                        reason: "the vendor paused the account".into(),
                    }),
                ]),
                ..Default::default()
            },
        ],
        design_off_config(),
    );
    let (_, refused) = failed_with(
        &engine,
        new_workflow("refused", vec![agent_step("work", "mock")]),
    )
    .await;
    assert!(refused.contains("prompt failed"), "{refused}");
    let (_, suspended) = failed_with(
        &engine,
        new_workflow("suspended", vec![agent_step("work", "paused")]),
    )
    .await;
    assert!(
        suspended.contains("suspended: the vendor paused the account"),
        "{suspended}"
    );
    engine.shutdown().await;
}

/// The first cost a session reports crosses the goal's budget: the session
/// is aborted and the item is blocked as exhausted.
#[tokio::test(flavor = "multi_thread")]
async fn a_cost_past_the_goal_s_budget_ends_the_session() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(
        &dir,
        vec![MockAdapter {
            script: Some(vec![
                SessionEvent::Lifecycle(LifecycleEvent::Started),
                SessionEvent::Progress(ProgressEvent::TurnStarted),
                SessionEvent::Progress(ProgressEvent::CostDelta {
                    input_tokens: 10,
                    output_tokens: 10,
                    usd_cents: 500,
                }),
                SessionEvent::Progress(ProgressEvent::TurnEnded),
                ended(Outcome::Completed),
            ]),
            turn_delay: std::time::Duration::from_millis(200),
            ..Default::default()
        }],
        design_off_config(),
    );
    let wf = engine
        .create_workflow(new_workflow("spends", vec![agent_step("work", "mock")]))
        .unwrap();
    let goal = engine
        .submit_goal(SubmitRequest {
            workflow: Some(wf.id),
            start: true,
            budget: Some(Budget {
                max_usd_cents: Some(1),
                ..Default::default()
            }),
            ..SubmitRequest::captured("a cent to spend")
        })
        .unwrap();
    let run = finished_run(&engine, goal.id).await;
    assert_eq!(run.outcome, Some(RunOutcome::Failed), "{run:?}");
    let reason = blocked_reason(&engine, goal.id);
    assert!(reason.contains("budget exhausted"), "{reason}");
    engine.shutdown().await;
}
