//! A goal's doors at their edges: a close or a delete that takes the
//! question it was asked and the work it ran with it; a retirement with no
//! fate; a restart after the goal moved to another workflow; an amendment
//! that does not validate; an interrupted item found claimed or blocked at
//! the restart; a run of the workspace failing with no designer to wake.
//! Fakes only.

use crate::common;

use bisa_core::{
    CheckKind, ClosureReason, Home, RunOutcome, RunStatus, StepKind, WaitFor, WorkItemState,
    WorkItemTransition as T,
};
use bisa_engine::retire::{Fate, GoalPlan, WorkflowPlan};
use bisa_engine::{Engine, EngineError};
use bisa_harness::mock::MockAdapter;
use common::*;
use std::collections::BTreeMap;

fn endless() -> MockAdapter {
    MockAdapter {
        script: Some(vec![]),
        ..Default::default()
    }
}

fn asks() -> bisa_store::NewWorkflow {
    new_workflow(
        "asks",
        vec![step(
            "ask",
            StepKind::Human {
                prompt: "Which?".into(),
                options: vec![bisa_core::AskOption::new("a", "A")],
                multi: false,
                assignee: None,
            },
        )],
    )
}

fn says(name: &str) -> bisa_store::NewWorkflow {
    new_workflow(
        name,
        vec![step(
            "say",
            StepKind::Notify {
                scope: None,
                template: "hello".into(),
                mentions: vec![],
                author: None,
            },
        )],
    )
}

fn holds(name: &str) -> bisa_store::NewWorkflow {
    new_workflow(
        name,
        vec![step(
            "hold",
            StepKind::Wait {
                until: WaitFor::Release,
            },
        )],
    )
}

/// Delete a goal through the one door that forgets one: a retirement.
async fn delete(engine: &Engine, goal: bisa_core::GoalId) {
    engine
        .retire_goal(
            goal,
            GoalPlan {
                goal: Fate::Delete,
                projects: Fate::Keep,
                tree: false,
            },
        )
        .await
        .unwrap();
}

/// The one gate a goal's run opened.
async fn gate_of(engine: &Engine, goal: bisa_core::GoalId) -> String {
    until("the question", || {
        engine
            .inbox()
            .into_iter()
            .find(|g| g.home.goal() == Some(goal))
            .map(|g| g.id)
    })
    .await
}

/// Closing a goal withdraws the question its run was asking; deleting one
/// withdraws its question, turns its listening off and stops the item it
/// was running.
#[tokio::test(flavor = "multi_thread")]
async fn closing_or_deleting_a_goal_withdraws_its_question_its_listening_and_its_work() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![endless()]);

    let (closed, _) = run_on(&engine, "closed while asking", asks());
    let gate = gate_of(&engine, closed.id).await;
    engine
        .close_goal(closed.id, ClosureReason::Abandoned { rationale: None })
        .unwrap();
    until("the question to be withdrawn", || {
        matches!(
            engine.decide(&gate, true, None, None, None),
            Err(EngineError::UnknownGate(_))
        )
        .then_some(())
    })
    .await;
    let run = engine
        .workspace()
        .get_current_run(closed.id)
        .unwrap()
        .unwrap();
    assert_eq!(run.status(), RunStatus::Cancelled, "{run:?}");

    let (deleted, _) = run_on(&engine, "deleted while asking", asks());
    let gate = gate_of(&engine, deleted.id).await;
    delete(&engine, deleted.id).await;
    assert!(matches!(
        engine.decide(&gate, true, None, None, None),
        Err(EngineError::UnknownGate(_))
    ));
    assert!(engine.workspace().get_goal(deleted.id).is_err());

    let mut on_hook = step(
        "ticket",
        StepKind::Start {
            on: bisa_core::StartOn::Hook { public: false },
            inputs: BTreeMap::new(),
            guard: bisa_core::Guard::default(),
        },
    );
    on_hook.then = vec![bisa_core::Flow::to(sid("say"))];
    let mut hears = says("hears");
    hears.steps.insert(0, on_hook);
    let (listening, _) = goal_on(&engine, "deleted while listening", hears);
    let host = bisa_core::ListenerHost::Goal { goal: listening.id };
    engine.set_listening(host, BTreeMap::new(), None).unwrap();
    assert!(engine.workspace().listening(&host).unwrap().is_some());
    delete(&engine, listening.id).await;
    assert!(engine.workspace().get_goal(listening.id).is_err());
    assert!(
        engine.workspace().listening(&host).is_err(),
        "nothing is left to listen with"
    );

    let (working, _) = run_on(
        &engine,
        "deleted while working",
        new_workflow("works", vec![agent_step("work", "mock")]),
    );
    step_in_state(&engine, working.id, "work", "running").await;
    until("the item to be launched", || {
        (!items_of(&engine, working.id).is_empty()).then_some(())
    })
    .await;
    delete(&engine, working.id).await;
    assert!(engine.workspace().get_goal(working.id).is_err());
    until("the worker's row to end", || {
        engine
            .inner()
            .presence
            .snapshot()
            .iter()
            .all(|row| !row.state.is_live() || row.goal != Some(working.id))
            .then_some(())
    })
    .await;
    engine.shutdown().await;
}

/// *Keep* is no fate for the thing retired: a goal and a workflow are
/// retired by archiving or deleting, never kept.
#[tokio::test(flavor = "multi_thread")]
async fn retiring_with_keep_as_the_fate_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let (goal, _) = goal_on(&engine, "kept", says("kept"));
    let refused = engine
        .retire_goal(
            goal.id,
            GoalPlan {
                goal: Fate::Keep,
                projects: Fate::Keep,
                tree: false,
            },
        )
        .await;
    assert!(
        matches!(refused, Err(EngineError::Invalid(_))),
        "{refused:?}"
    );
    assert!(!engine.workspace().get_goal(goal.id).unwrap().is_closed());
    let wf = engine.create_workflow(says("library")).unwrap();
    let refused = engine
        .retire_workflow(
            wf.id,
            WorkflowPlan {
                workflow: Fate::Keep,
                projects: Fate::Keep,
                tree: false,
            },
        )
        .await;
    assert!(
        matches!(refused, Err(EngineError::Invalid(_))),
        "{refused:?}"
    );
    assert!(!engine
        .workspace()
        .get_workflow(wf.id)
        .unwrap()
        .is_archived());
    engine.shutdown().await;
}

/// A restart runs the last run's workflow: a goal pointed elsewhere since
/// is pointed back at it first.
#[tokio::test(flavor = "multi_thread")]
async fn a_restart_runs_the_last_runs_workflow_when_the_goal_moved_on() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let (goal, first) = run_on(&engine, "moved on", says("first"));
    assert_eq!(
        finished_run(&engine, goal.id).await.outcome,
        Some(RunOutcome::Done)
    );
    let other = engine.create_workflow(says("other")).unwrap();
    engine.set_workflow(goal.id, Some(other.id)).unwrap();
    let again = engine.restart_goal(goal.id).await.unwrap();
    assert_ne!(again.id, first.id);
    assert_eq!(again.workflow.id, first.workflow.id, "{again:?}");
    assert_eq!(
        engine.workspace().get_goal(goal.id).unwrap().workflow,
        Some(first.workflow.id)
    );
    assert_eq!(
        run_finished(&engine, again.id).await.outcome,
        Some(RunOutcome::Done)
    );
    engine.shutdown().await;
}

/// An amendment whose definition does not validate is refused with its
/// problems; the run goes on as it was.
#[tokio::test(flavor = "multi_thread")]
async fn an_amendment_that_does_not_validate_is_refused_and_the_run_goes_on() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let (goal, run) = run_on(&engine, "amended badly", holds("held"));
    step_in_state(&engine, goal.id, "hold", "waiting").await;
    let mut broken = holds("held");
    broken.steps[0].then = vec![bisa_core::Flow::to(sid("nowhere"))];
    let refused = engine.amend_run(goal.id, broken);
    assert!(
        matches!(
            &refused,
            Err(EngineError::Store(bisa_store::StoreError::WorkflowInvalid(
                _
            )))
        ),
        "{refused:?}"
    );
    let same = engine.workspace().get_run(run.id).unwrap();
    assert_eq!(same.status(), RunStatus::Waiting);
    assert_eq!(same.workflow.revision, run.workflow.revision);
    engine.release_step(run.id, &sid("hold"), None).unwrap();
    assert_eq!(
        finished_run(&engine, goal.id).await.outcome,
        Some(RunOutcome::Done)
    );
    engine.shutdown().await;
}

/// An agent step running at the crash, and the item the restart finds.
async fn interrupted(
    dir: &tempfile::TempDir,
    statement: &str,
) -> (bisa_core::GoalId, bisa_core::WorkItemSpec) {
    let engine = engine_with(dir, vec![endless()]);
    let (goal, _) = run_on(
        &engine,
        statement,
        new_workflow("A", vec![agent_step("work", "mock")]),
    );
    step_in_state(&engine, goal.id, "work", "running").await;
    let item = until("the item to be launched", || {
        items_of(&engine, goal.id).into_iter().next()
    })
    .await;
    engine.shutdown().await;
    (goal.id, item)
}

/// An interrupted item a previous attempt left blocked is unblocked on the
/// restart and runs; one somebody claimed and never started is started.
#[tokio::test(flavor = "multi_thread")]
async fn an_interrupted_item_left_blocked_or_claimed_runs_again_at_the_restart() {
    for how in ["blocked", "claimed"] {
        let dir = tempfile::tempdir().unwrap();
        let (goal, item) = interrupted(&dir, how).await;
        let store = workspace(&dir);
        let home = Home::Goal { goal };
        let transitions = if how == "blocked" {
            vec![T::Block {
                reason: "the harness died".into(),
            }]
        } else {
            vec![
                T::Release,
                T::Claim {
                    by: store.owner_principal(),
                },
            ]
        };
        for t in &transitions {
            store.transition_work_item(&home, item.id, t).unwrap();
        }
        let engine = engine_with(&dir, vec![endless()]);
        step_in_state(&engine, goal, "work", "running").await;
        let resumed = until("the item to run again", || {
            items_of(&engine, goal)
                .into_iter()
                .find(|i| i.id == item.id && matches!(i.state, WorkItemState::InProgress { .. }))
        })
        .await;
        assert_eq!(resumed.interruptions, 1, "{how}: {resumed:?}");
        engine.shutdown().await;
    }
}

/// A run of the workspace that fails has no goal and so no designer to
/// wake: it ends failed, and nothing else happens.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_of_the_workspace_that_fails_wakes_no_designer() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let (_, run) = workspace_run(
        &engine,
        new_workflow(
            "fails",
            vec![step(
                "verify",
                StepKind::Check {
                    check: CheckKind::Command {
                        command: "exit 1".into(),
                    },
                },
            )],
        ),
    );
    assert_eq!(
        run_finished(&engine, run.id).await.outcome,
        Some(RunOutcome::Failed)
    );
    assert!(engine.workspace().list_goals(None).unwrap().is_empty());
    engine.shutdown().await;
}

/// Closing a parent through the door that cannot wait closes the goal it
/// spawned after it: the child is closed and its run cancelled, the
/// parent's own run with it.
#[tokio::test(flavor = "multi_thread")]
async fn closing_a_parent_closes_the_child_it_spawned_after_the_close_returns() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let child_wf = engine.create_workflow(holds("child")).unwrap();
    let (parent, _) = run_on(
        &engine,
        "a parent closed",
        new_workflow(
            "spawns",
            vec![step(
                "child",
                StepKind::Spawn {
                    statement_template: "the child's part".into(),
                    workflow: Some(child_wf.id),
                    assignees: vec![],
                    inputs: BTreeMap::new(),
                    wait: true,
                },
            )],
        ),
    );
    let child = until("the child to hold", || {
        let child = engine
            .workspace()
            .list_goals(None)
            .unwrap()
            .into_iter()
            .find(|g| matches!(g.origin, bisa_core::GoalOrigin::Spawned { parent: p, .. } if p == parent.id))?;
        let run = engine.workspace().get_current_run(child.id).ok().flatten()?;
        (run.status() == RunStatus::Waiting).then_some(child)
    })
    .await;
    engine
        .close_goal(parent.id, ClosureReason::Abandoned { rationale: None })
        .unwrap();
    until("the child to be closed with its parent", || {
        let child = engine.workspace().get_goal(child.id).ok()?;
        let run = engine
            .workspace()
            .get_current_run(child.id)
            .ok()
            .flatten()?;
        (child.is_closed() && run.status() == RunStatus::Cancelled).then_some(())
    })
    .await;
    assert!(engine.workspace().get_goal(parent.id).unwrap().is_closed());
    engine.shutdown().await;
}

/// A settings layer this node cannot read — not a torn file, which the
/// resolution works around, but one it may not open — refuses a capture
/// with the reason rather than making the goal *auto*, the one mode that
/// runs unattended, or giving it no ceiling.
#[tokio::test(flavor = "multi_thread")]
#[cfg(unix)]
async fn an_unreadable_settings_layer_refuses_a_capture_rather_than_making_it_unattended() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    engine
        .workspace()
        .set_setting(
            bisa_core::SettingScope::Machine,
            None,
            "goals.default_mode",
            serde_json::json!("manual"),
        )
        .unwrap();
    assert_eq!(
        engine.default_goal_mode().unwrap(),
        bisa_core::GoalMode::Manual
    );
    let machine = bisa_store::Paths::new(dir.path()).machine_settings();
    let mode = std::fs::metadata(&machine).unwrap().permissions();
    std::fs::set_permissions(&machine, std::fs::Permissions::from_mode(0o000)).unwrap();
    assert!(
        engine.default_goal_mode().is_err(),
        "a mode nobody can read is no mode"
    );
    assert!(
        engine
            .submit_goal(bisa_engine::SubmitRequest::captured("captured blind"))
            .is_err(),
        "a capture whose ceiling cannot be read is refused"
    );
    assert!(
        engine.workspace().list_goals(None).unwrap().is_empty(),
        "nothing was written"
    );
    std::fs::set_permissions(&machine, mode).unwrap();
    assert_eq!(
        engine.default_goal_mode().unwrap(),
        bisa_core::GoalMode::Manual
    );
    engine.shutdown().await;
}

/// A restart whose start is gone from the workflow as it stands — the run
/// began at an event start a later revision dropped — is refused before
/// anything is cancelled: the run it would have replaced goes on.
#[tokio::test(flavor = "multi_thread")]
async fn a_restart_whose_start_is_gone_is_refused_before_anything_is_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![endless()]);
    let mut go = step(
        "go",
        bisa_core::StepKind::Start {
            on: bisa_core::StartOn::Signal {
                filter: bisa_core::SignalFilter {
                    name: "go".into(),
                    fields: Default::default(),
                },
            },
            inputs: Default::default(),
            guard: Default::default(),
        },
    );
    go.then = vec![bisa_core::Flow::to(sid("work"))];
    let (goal, wf) = goal_on(
        &engine,
        "began on an event",
        new_workflow("on go", vec![go, agent_step("work", "mock")]),
    );
    let first = engine
        .test_run_goal(goal.id, &sid("go"), serde_json::json!({}), BTreeMap::new())
        .unwrap();
    step_in_state(&engine, goal.id, "work", "running").await;
    // The library moves on: the start the run began at is gone.
    let (_, problems) = engine
        .save_workflow(
            wf.id,
            new_workflow("by hand now", vec![agent_step("work", "mock")]),
            wf.revision,
        )
        .unwrap();
    assert!(problems.is_empty(), "{problems:?}");
    let err = engine.restart_goal(goal.id).await.unwrap_err();
    assert!(matches!(err, EngineError::Conflict(_)), "{err:?}");
    assert!(err.to_string().contains("go"), "{err}");
    let live = engine
        .workspace()
        .live_run(goal.id)
        .unwrap()
        .expect("still live");
    assert_eq!(live.id, first.id, "nothing was cancelled");
    assert_eq!(live.steps[&sid("work")].state.as_str(), "running");
    engine.shutdown().await;
}

/// An edit of the goal's own record — its assignees by a person, its
/// statement by an agent — is announced on the goal's row, so the screen
/// open on it reads it again.
#[tokio::test(flavor = "multi_thread")]
async fn an_edit_of_the_goals_record_is_announced() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    let assayer = ws
        .add_agent(bisa_store::NewAgent {
            name: "Assayer".into(),
            harness: "mock".into(),
            ..Default::default()
        })
        .unwrap();
    let goal = engine.submit_goal(guided("edited later")).unwrap();
    let mut rx = engine.events();
    bisa_engine::ops::set_goal_assignees(
        engine.inner(),
        goal.id,
        vec![bisa_core::Assignee::Agent(assayer.id.to_string())],
    )
    .unwrap();
    let said = wait_for(&mut rx, "the goal's update", |e| {
        matches!(&e.payload, bisa_engine::EnginePayload::GoalUpdated { goal: g } if *g == goal.id)
    })
    .await;
    assert_eq!(said.goal, Some(goal.id));
    let reply = intake_roundtrip(
        engine.socket_path(),
        serde_json::json!({"op": "revise_statement", "goal": goal.id.to_string(),
                           "statement": "edited by the agent", "why": "clearer"}),
    )
    .await;
    assert_eq!(reply["ok"], serde_json::json!(true), "{reply}");
    wait_for(&mut rx, "the statement's update", |e| {
        matches!(&e.payload, bisa_engine::EnginePayload::GoalUpdated { goal: g } if *g == goal.id)
    })
    .await;
    assert_eq!(
        ws.get_goal(goal.id).unwrap().statement,
        "edited by the agent"
    );
    engine.shutdown().await;
}
