//! A stop, a restart, a close or a deletion ends every harness it names —
//! and says so only once that is true.
//!
//! The one door (`bisa_engine::ending`) is held to its promises here: a verb
//! answers once the harness processes are gone — or were terminated at the
//! deadline, and counted; a stop that lands while a harness starts leaves no
//! harness; the goals a goal spawned go with it; a run's check command goes
//! with the run; the engine's own stop leaves nothing behind.

use crate::common;

use bisa_core::event::JournalPayload;
use bisa_core::{
    AgentId, CheckKind, ClosureReason, GoalOrigin, MessageBody, RunStatus, StepKind, WorkItemState,
    WorkItemTransition,
};
use bisa_engine::registry::SessionKind;
use bisa_engine::retire::{Fate, GoalPlan};
use bisa_engine::{Engine, EngineConfig, SessionPresence, SessionState};
use bisa_harness::mock::{Close, MockAdapter};
use bisa_store::PostOrigin;
use common::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A harness whose session starts and never ends by itself — an agent at
/// work — and the ledger of how its sessions were closed from outside.
fn at_work(id: &str) -> (MockAdapter, Arc<Mutex<Vec<Close>>>) {
    let adapter = MockAdapter {
        id: id.into(),
        script: Some(vec![]),
        ..Default::default()
    };
    let closes = Arc::clone(&adapter.closes);
    (adapter, closes)
}

fn closed(closes: &Arc<Mutex<Vec<Close>>>) -> Vec<Close> {
    closes.lock().unwrap().clone()
}

async fn live_row(engine: &Engine, kind: SessionKind) -> SessionPresence {
    until("a live row of the kind in the roster", || {
        engine
            .inner()
            .presence
            .snapshot()
            .into_iter()
            .find(|row| row.kind == kind && row.state.is_live())
    })
    .await
}

fn live_rows(engine: &Engine) -> Vec<SessionPresence> {
    engine
        .inner()
        .presence
        .snapshot()
        .into_iter()
        .filter(|row| row.state.is_live())
        .collect()
}

fn state_of(engine: &Engine, row: &SessionPresence) -> Option<SessionState> {
    engine.inner().presence.get(row.id).map(|row| row.state)
}

/// An engine whose wait for a stop is short, so a harness that ignores its
/// abort is terminated within a test's patience.
fn quick_deadline() -> EngineConfig {
    EngineConfig {
        stop_deadline_ms: 400,
        ..design_off_config()
    }
}

fn spawn_step(id: &str, child: bisa_core::WorkflowId) -> bisa_core::Step {
    step(
        id,
        StepKind::Spawn {
            statement_template: "do the rest".into(),
            workflow: Some(child),
            assignees: vec![],
            inputs: Default::default(),
            wait: true,
        },
    )
}

async fn child_of(engine: &Engine, parent: bisa_core::GoalId) -> bisa_core::Goal {
    until("the child goal", || {
        engine
            .workspace()
            .list_goals(None)
            .ok()?
            .into_iter()
            .find(|g| g.origin == GoalOrigin::Spawned { parent })
    })
    .await
}

// ---------------------------------------------------------------------------
// The verb returns to a process that is gone
// ---------------------------------------------------------------------------

/// A goal's stop answers once its worker's harness is aborted and let go
/// of — not a moment before — and says what it ended.
#[tokio::test(flavor = "multi_thread")]
async fn stopping_a_goal_returns_only_once_its_workers_harness_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let (adapter, closes) = at_work("mock");
    let engine = engine_with(&dir, vec![adapter]);
    let (goal, _) = run_on(
        &engine,
        "work that is stopped",
        new_workflow("stopped", vec![agent_step("work", "mock")]),
    );
    let row = live_row(&engine, SessionKind::Worker).await;

    let stopped = engine.stop_goal(goal.id, None).await.unwrap();

    // Read right after the verb answered: the harness was aborted and let
    // go of before, not after.
    assert_eq!(closed(&closes), vec![Close::Aborted, Close::Disposed]);
    assert_eq!(stopped.ended.sessions, 1, "{:?}", stopped.ended);
    assert_eq!(stopped.ended.terminated, 0, "{:?}", stopped.ended);
    assert_eq!(stopped.ended.still_live, 0, "{:?}", stopped.ended);
    assert!(stopped.ended.children.is_empty());
    assert!(
        engine.inner().ending.is_empty(),
        "nothing is waited for once the verb answered"
    );
    assert_eq!(state_of(&engine, &row), Some(SessionState::Aborted));
    engine.shutdown().await;
}

/// A harness that swallows its abort — the process goes on — is terminated
/// by the engine at the deadline: the stop says so, the process is gone,
/// the home carries a note, and the record no longer names the pid.
#[tokio::test(flavor = "multi_thread")]
async fn a_harness_that_ignores_the_abort_is_terminated_at_the_deadline() {
    let dir = tempfile::tempdir().unwrap();
    // A real process standing for the harness's child — nobody's but this
    // test's — announced by the mock at launch, as an adapter announces its
    // child.
    let mut child = std::process::Command::new("sleep")
        .arg("300")
        .spawn()
        .unwrap();
    let adapter = MockAdapter {
        id: "stubborn".into(),
        script: Some(vec![]),
        ignore_abort: true,
        pid: Some(child.id()),
        ..Default::default()
    };
    let closes = Arc::clone(&adapter.closes);
    let engine = Engine::start(
        workspace(&dir),
        catalog_with(vec![adapter]),
        quick_deadline(),
    )
    .unwrap();
    let (goal, _) = run_on(
        &engine,
        "work whose harness will not stop",
        new_workflow("stubborn", vec![agent_step("work", "stubborn")]),
    );
    let row = live_row(&engine, SessionKind::Worker).await;
    until("the row to carry the child's pid", || {
        engine.inner().presence.get(row.id)?.pid.map(|_| ())
    })
    .await;

    let started = std::time::Instant::now();
    let stopped = engine.stop_goal(goal.id, None).await.unwrap();

    assert!(
        closed(&closes).contains(&Close::Aborted),
        "the harness was told: {:?}",
        closed(&closes)
    );
    assert_eq!(stopped.ended.terminated, 1, "{:?}", stopped.ended);
    assert_eq!(stopped.ended.still_live, 0, "{:?}", stopped.ended);
    assert!(
        started.elapsed() >= Duration::from_millis(400),
        "the deadline was waited for before the process was terminated"
    );
    until("the child to be gone", || {
        child.try_wait().ok().flatten().map(|_| ())
    })
    .await;
    let notes: Vec<String> = engine
        .workspace()
        .journal(&bisa_core::Home::from(goal.id))
        .unwrap()
        .into_iter()
        .filter_map(|e| match e.payload {
            JournalPayload::Note { text } => Some(text),
            _ => None,
        })
        .collect();
    assert!(
        notes.iter().any(|n| n.contains("was terminated")),
        "the home says the process was terminated: {notes:?}"
    );
    let record = engine
        .workspace()
        .session_by_id(&row.session_id.unwrap().to_string())
        .unwrap()
        .unwrap();
    assert_eq!(record.pid, None, "the record no longer names a process");
    engine.shutdown().await;
}

/// A stop that lands while the worker's harness is still starting leaves no
/// harness: the launch is raced against the stop, and a session that did
/// start is aborted before it is prompted.
#[tokio::test(flavor = "multi_thread")]
async fn a_stop_during_a_workers_launch_leaves_no_harness() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = MockAdapter {
        id: "slow".into(),
        script: Some(vec![]),
        probe_delay: Duration::from_millis(300),
        ..Default::default()
    };
    let closes = Arc::clone(&adapter.closes);
    let launches = Arc::clone(&adapter.launches);
    let engine = engine_with(&dir, vec![adapter]);
    let (goal, run) = run_on(
        &engine,
        "work stopped as it starts",
        new_workflow("slow", vec![agent_step("work", "slow")]),
    );
    // Not a moment of patience: the stop lands while the harness is probed.
    engine.stop_goal(goal.id, None).await.unwrap();

    until("no item in flight and no live row", || {
        (engine.inner().inflight.is_empty() && live_rows(&engine).is_empty()).then_some(())
    })
    .await;
    let after = run_of(&engine, run.id);
    assert_eq!(after.status(), RunStatus::Cancelled, "{after:?}");
    let launched = launches.lock().unwrap().len();
    let closes = closed(&closes);
    assert!(
        launched == 0 || closes == vec![Close::Aborted, Close::Disposed],
        "a harness that did start was aborted before it said a word: launched {launched}, closed {closes:?}"
    );
    for item in items_of(&engine, goal.id) {
        assert!(
            matches!(
                item.state,
                WorkItemState::Cancelled | WorkItemState::Blocked { .. }
            ),
            "{:?}",
            item.state
        );
    }
    engine.shutdown().await;
}

/// A restart ends every session of the goal whether or not its last run is
/// live: a turn in the thread of a goal whose run finished is aborted
/// before the new run starts.
#[tokio::test(flavor = "multi_thread")]
async fn restarting_a_goal_ends_the_turn_in_its_thread_before_the_new_run() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::general(), "endless");
    let (turn, closes) = at_work("endless");
    let engine = Engine::start(
        ws,
        catalog_with(vec![
            yielding("mock", serde_json::json!({"ok": true})),
            turn,
        ]),
        design_off_config(),
    )
    .unwrap();
    let (goal, first) = run_on(
        &engine,
        "work, then a chat, then a restart",
        new_workflow("quick", vec![agent_step("work", "mock")]),
    );
    finished_run(&engine, goal.id).await;
    engine
        .workspace()
        .post_message(
            &goal.id.to_string(),
            MessageBody::post("how did it go?"),
            None,
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();
    let row = live_row(&engine, SessionKind::Conversation).await;

    let (restarted, ended) = engine.restart_goal_ended(goal.id).await.unwrap();

    assert_ne!(restarted.id, first.id, "a new run");
    assert!(
        closed(&closes).contains(&Close::Aborted),
        "{:?}",
        closed(&closes)
    );
    assert_eq!(state_of(&engine, &row), Some(SessionState::Aborted));
    assert_eq!(ended.sessions, 1, "{ended:?}");
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// The goals a goal spawned go with it
// ---------------------------------------------------------------------------

/// A parent's stop stops the goal it spawned: the child's run is cancelled,
/// its worker aborted, the child itself left open — and the stop names it.
#[tokio::test(flavor = "multi_thread")]
async fn stopping_a_parent_goal_stops_its_children() {
    let dir = tempfile::tempdir().unwrap();
    let (adapter, closes) = at_work("mock");
    let engine = engine_with(&dir, vec![adapter]);
    let child_wf = engine
        .create_workflow(new_workflow("the rest", vec![agent_step("do", "mock")]))
        .unwrap();
    let (goal, run) = run_on(
        &engine,
        "hand work on",
        new_workflow("spawns", vec![spawn_step("hand-on", child_wf.id)]),
    );
    step_in_state(&engine, goal.id, "hand-on", "waiting").await;
    let child = child_of(&engine, goal.id).await;
    let row = live_row(&engine, SessionKind::Worker).await;
    assert_eq!(row.goal, Some(child.id), "the worker is the child's");

    let stopped = engine.stop_goal(goal.id, None).await.unwrap();

    assert_eq!(stopped.ended.children, vec![child.id]);
    assert_eq!(stopped.ended.sessions, 1, "{:?}", stopped.ended);
    assert_eq!(closed(&closes), vec![Close::Aborted, Close::Disposed]);
    assert_eq!(state_of(&engine, &row), Some(SessionState::Aborted));
    assert_eq!(run_of(&engine, run.id).status(), RunStatus::Cancelled);
    assert_eq!(
        current_run(&engine, child.id).status(),
        RunStatus::Cancelled,
        "the child's run was stopped with its parent's"
    );
    assert!(
        !engine.workspace().get_goal(child.id).unwrap().is_closed(),
        "a stop leaves the child open, as it leaves the parent"
    );
    assert!(
        run_of(&engine, run.id).steps[&sid("hand-on")]
            .state
            .as_str()
            != "done",
        "the child's end did not complete the parent's step"
    );
    engine.shutdown().await;
}

/// A parent's close closes the goal it spawned, and a deletion closes it
/// and leaves it — the child is its own record still.
#[tokio::test(flavor = "multi_thread")]
async fn closing_or_deleting_a_parent_goal_closes_its_children() {
    let dir = tempfile::tempdir().unwrap();
    let (adapter, closes) = at_work("mock");
    let engine = engine_with(&dir, vec![adapter]);
    let child_wf = engine
        .create_workflow(new_workflow("the rest", vec![agent_step("do", "mock")]))
        .unwrap();
    let (goal, _) = run_on(
        &engine,
        "hand work on, then close",
        new_workflow("spawns", vec![spawn_step("hand-on", child_wf.id)]),
    );
    step_in_state(&engine, goal.id, "hand-on", "waiting").await;
    let child = child_of(&engine, goal.id).await;
    live_row(&engine, SessionKind::Worker).await;

    let (_, ended) = engine
        .close_goal_settled(goal.id, ClosureReason::Abandoned { rationale: None })
        .await
        .unwrap();

    assert_eq!(ended.children, vec![child.id]);
    assert!(closed(&closes).contains(&Close::Aborted));
    let child_after = engine.workspace().get_goal(child.id).unwrap();
    assert!(child_after.is_closed(), "closed with its parent");
    assert!(live_rows(&engine).is_empty());

    // A deletion of a parent with another open child: closed, and left.
    let (goal2, _) = run_on(
        &engine,
        "hand work on, then delete",
        new_workflow("spawns-again", vec![spawn_step("hand-on", child_wf.id)]),
    );
    step_in_state(&engine, goal2.id, "hand-on", "waiting").await;
    let child2 = child_of(&engine, goal2.id).await;
    live_row(&engine, SessionKind::Worker).await;
    let retired = engine
        .retire_goal(
            goal2.id,
            GoalPlan {
                goal: Fate::Delete,
                projects: Fate::Keep,
                tree: false,
            },
        )
        .await
        .unwrap();
    assert_eq!(retired.ended.children, vec![child2.id]);
    assert!(
        engine.workspace().get_goal(goal2.id).is_err(),
        "the parent is gone"
    );
    let child2_after = engine.workspace().get_goal(child2.id).unwrap();
    assert!(child2_after.is_closed(), "closed, not deleted");
    assert!(live_rows(&engine).is_empty());
    engine.shutdown().await;
}

/// A run of the workspace stopped stops the goal born of it.
#[tokio::test(flavor = "multi_thread")]
async fn stopping_a_run_of_the_workspace_stops_the_goals_born_of_it() {
    let dir = tempfile::tempdir().unwrap();
    let (adapter, closes) = at_work("mock");
    let engine = engine_with(&dir, vec![adapter]);
    let child_wf = engine
        .create_workflow(new_workflow("the rest", vec![agent_step("do", "mock")]))
        .unwrap();
    let (_, run) = workspace_run(
        &engine,
        new_workflow(
            "spawns-from-the-workspace",
            vec![spawn_step("hand-on", child_wf.id)],
        ),
    );
    run_step_in_state(&engine, run.id, "hand-on", "waiting").await;
    let child = until("the child goal", || {
        engine
            .workspace()
            .list_goals(None)
            .ok()?
            .into_iter()
            .find(|g| matches!(&g.origin, GoalOrigin::Run { run: r, .. } if *r == run.id))
    })
    .await;
    live_row(&engine, SessionKind::Worker).await;

    let (after, ended) = engine.stop_run_ended(run.id, None).await.unwrap();

    assert_eq!(after.status(), RunStatus::Cancelled);
    assert_eq!(ended.children, vec![child.id]);
    assert!(closed(&closes).contains(&Close::Aborted));
    assert_eq!(
        current_run(&engine, child.id).status(),
        RunStatus::Cancelled
    );
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// What else goes with the thing
// ---------------------------------------------------------------------------

/// A `check` whose command would run for a long time goes with the goal's
/// stop: the stop answers at once, not at the command's timeout, and no
/// check task of the run is left.
#[tokio::test(flavor = "multi_thread")]
async fn stopping_a_goal_aborts_its_running_check_command() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let (goal, run) = run_on(
        &engine,
        "a slow check",
        new_workflow(
            "slow-check",
            vec![step(
                "slow",
                StepKind::Check {
                    check: CheckKind::Command {
                        command: "sleep 30".into(),
                    },
                },
            )],
        ),
    );
    step_in_state(&engine, goal.id, "slow", "running").await;
    until("the check's task to be registered", || {
        engine
            .inner()
            .step_tasks
            .contains_key(&(run.id, sid("slow")))
            .then_some(())
    })
    .await;

    let started = std::time::Instant::now();
    engine.stop_goal(goal.id, None).await.unwrap();

    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the stop did not wait for the command's timeout"
    );
    assert!(
        !engine
            .inner()
            .step_tasks
            .contains_key(&(run.id, sid("slow"))),
        "the check's task went with the run"
    );
    assert_eq!(run_of(&engine, run.id).status(), RunStatus::Cancelled);
    engine.shutdown().await;
}

/// A work item cancelled by hand aborts the session working on it, on the
/// spot — the mark is stopped before it is dropped.
#[tokio::test(flavor = "multi_thread")]
async fn cancelling_a_work_item_by_hand_aborts_its_session() {
    let dir = tempfile::tempdir().unwrap();
    let (adapter, closes) = at_work("mock");
    let engine = engine_with(&dir, vec![adapter]);
    let (goal, _) = run_on(
        &engine,
        "work cancelled by hand",
        new_workflow("by-hand", vec![agent_step("work", "mock")]),
    );
    let row = live_row(&engine, SessionKind::Worker).await;
    let item = row.work_item.expect("a worker's row names its item");

    let home = bisa_core::Home::from(goal.id);
    let after = engine
        .transition_work_item(&home, item, &WorkItemTransition::Cancel)
        .unwrap();
    assert_eq!(after.state, WorkItemState::Cancelled, "{:?}", after.state);

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !closed(&closes).contains(&Close::Aborted) {
        assert!(
            std::time::Instant::now() < deadline,
            "the harness was never aborted: closes {:?}, row {:?}, in flight {}, registry {:?}, notes {:?}, item {:?}",
            closed(&closes),
            state_of(&engine, &row),
            engine.inner().inflight.contains_key(&item),
            engine.inner().registry.get(row.id).map(|a| a.status),
            notes(&engine, goal.id),
            engine.workspace().get_work_item(&home, item).map(|i| i.state)
        );
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    engine.shutdown().await;
}

/// No cycle follows a stopped design: the Workflow Agent is woken again by
/// a person, never by the stop it was given.
#[tokio::test(flavor = "multi_thread")]
async fn no_wake_follows_a_stopped_design() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "hanging");
    let (adapter, closes) = at_work("hanging");
    let launches = Arc::clone(&adapter.launches);
    let engine = Engine::start(
        ws,
        catalog_with(vec![adapter]),
        EngineConfig {
            design_enabled: true,
            events_enabled: false,
            ..Default::default()
        },
    )
    .unwrap();
    let goal = engine.submit_goal(guided("a goal stopped early")).unwrap();
    let row = live_row(&engine, SessionKind::Guided).await;

    engine.stop_goal(goal.id, None).await.unwrap();

    assert!(closed(&closes).contains(&Close::Aborted));
    assert_eq!(state_of(&engine, &row), Some(SessionState::Aborted));
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(
        launches.lock().unwrap().len(),
        1,
        "the stop was not answered by a new cycle"
    );
    assert!(live_rows(&engine).is_empty());
    engine.shutdown().await;
}

/// The engine's own stop ends every harness it drives — a worker at work
/// is aborted and let go of — so a node that stops leaves no harness.
#[tokio::test(flavor = "multi_thread")]
async fn the_engines_stop_ends_every_harness_it_drives() {
    let dir = tempfile::tempdir().unwrap();
    let (adapter, closes) = at_work("mock");
    let engine = engine_with(&dir, vec![adapter]);
    run_on(
        &engine,
        "work the engine stops",
        new_workflow("stopped-with-the-engine", vec![agent_step("work", "mock")]),
    );
    live_row(&engine, SessionKind::Worker).await;

    engine.stop().await;

    assert_eq!(closed(&closes), vec![Close::Aborted, Close::Disposed]);
    assert!(engine.inner().ending.is_empty());
    engine.shutdown().await;
}

/// A row the registry does not know — a registration that failed quietly —
/// is stopped by its kind all the same, and ends.
#[tokio::test(flavor = "multi_thread")]
async fn a_row_the_registry_does_not_know_is_still_stopped() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let id = bisa_engine::LiveRunId::mint();
    engine.inner().presence.register(
        engine.inner(),
        id,
        bisa_engine::presence::SessionMeta {
            kind: SessionKind::Worker,
            origin: bisa_core::SessionOrigin::Terminal,
            harness: "mock".into(),
            model: None,
            effort: None,
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
    let row = engine.inner().presence.get(id).unwrap();
    assert!(engine.inner().registry.get(id).is_none());

    assert!(bisa_engine::sessions::stop_row(
        engine.inner(),
        &row,
        bisa_engine::sessions::EndCause::Stop
    ));
    assert_eq!(state_of(&engine, &row), Some(SessionState::Aborted));
    engine.shutdown().await;
}
