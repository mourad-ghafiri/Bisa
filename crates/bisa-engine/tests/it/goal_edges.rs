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
                        project: None,
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

/// Everything a capture is refused for is refused before its first write:
/// an archived workflow — started or not — another goal's own design, a
/// document with no name. No goal is left behind, nothing is announced.
#[tokio::test(flavor = "multi_thread")]
async fn a_capture_refused_for_its_workflow_or_its_documents_leaves_no_goal_behind() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    let put_away = engine.create_workflow(says("put away")).unwrap();
    ws.set_workflow_archived(put_away.id, true).unwrap();
    let owner = engine
        .submit_goal(bisa_engine::SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..bisa_engine::SubmitRequest::captured("owns a design")
        })
        .unwrap();
    let (design, _) = engine
        .design_workflow(owner.id, says("its own"), None)
        .unwrap();
    let brief = ws
        .put_attachment(b"%PDF the brief", "brief.pdf", "application/pdf")
        .unwrap();
    let mut nameless = brief.clone();
    nameless.name = "   ".into();
    let before = ws.list_goals(None).unwrap().len();
    let mut bus = engine.events();
    for start in [false, true] {
        let refused = engine.submit_goal(bisa_engine::SubmitRequest {
            workflow: Some(put_away.id),
            start,
            ..bisa_engine::SubmitRequest::captured("on a put-away workflow")
        });
        let words = refused.unwrap_err().to_string();
        assert!(words.contains("archived"), "start={start}: {words}");
    }
    let words = engine
        .submit_goal(bisa_engine::SubmitRequest {
            workflow: Some(design.id),
            ..bisa_engine::SubmitRequest::captured("borrowing a design")
        })
        .unwrap_err()
        .to_string();
    assert!(words.contains("designed"), "{words}");
    let words = engine
        .submit_goal(bisa_engine::SubmitRequest {
            documents: vec![nameless],
            ..bisa_engine::SubmitRequest::captured("with a nameless document")
        })
        .unwrap_err()
        .to_string();
    assert!(words.contains("name"), "{words}");
    assert_eq!(
        ws.list_goals(None).unwrap().len(),
        before,
        "no goal left behind"
    );
    while let Ok(e) = bus.try_recv() {
        assert!(
            !matches!(e.payload, bisa_engine::EnginePayload::GoalCreated { .. }),
            "nothing announced: {e:?}"
        );
    }
    engine.shutdown().await;
}

/// A goal captured without a budget takes the workspace default; one that
/// names its own keeps it as given, a ceiling left out lifting it whatever
/// the default says.
#[tokio::test(flavor = "multi_thread")]
async fn a_capture_without_a_budget_takes_the_workspace_default_and_a_zero_lifts_it() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    engine
        .set_setting(
            bisa_core::SettingScope::Workspace,
            None,
            bisa_core::Budget::SETTING_TOKENS,
            serde_json::json!(1000),
        )
        .unwrap();
    let defaulted = engine
        .submit_goal(bisa_engine::SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..bisa_engine::SubmitRequest::captured("defaulted")
        })
        .unwrap();
    assert_eq!(defaulted.budget.max_tokens, Some(1000));
    assert_eq!(defaulted.budget.max_usd_cents, None);
    let lifted = engine
        .submit_goal(bisa_engine::SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            budget: Some(bisa_core::Budget::default()),
            ..bisa_engine::SubmitRequest::captured("unlimited by choice")
        })
        .unwrap();
    assert!(lifted.budget.is_unlimited(), "{:?}", lifted.budget);
    let own = engine
        .submit_goal(bisa_engine::SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            budget: Some(bisa_core::Budget {
                max_usd_cents: Some(5),
                ..Default::default()
            }),
            ..bisa_engine::SubmitRequest::captured("its own ceiling")
        })
        .unwrap();
    assert_eq!(own.budget.max_usd_cents, Some(5));
    assert_eq!(own.budget.max_tokens, None, "kept as given, not defaulted");
    engine.shutdown().await;
}

/// A run that ends as a restart reads it: the restart's mark held the queue,
/// nothing is cancelled in the ended run's place, and the new run starts at
/// once ahead of the queue the ended run left.
#[tokio::test(flavor = "multi_thread")]
async fn a_restart_of_a_run_that_just_ended_starts_ahead_of_the_queue_it_left() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    let (goal, first) = run_on(&engine, "ended as the restart read it", holds("held"));
    let queued = engine.start_run(goal.id, BTreeMap::new()).unwrap();
    assert!(queued.is_queued());
    // The window between a restart's mark and its cancel, held open by hand.
    engine.inner().ending_goals.insert(goal.id);
    engine.release_step(first.id, &sid("hold"), None).unwrap();
    let ended = run_finished(&engine, first.id).await;
    assert_eq!(ended.outcome, Some(RunOutcome::Done));
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    assert!(
        ws.get_run(queued.id).unwrap().is_queued(),
        "held by the mark"
    );
    let restarted = engine.restart_goal(goal.id).await.unwrap();
    engine.inner().ending_goals.remove(&goal.id);
    assert!(![first.id, queued.id].contains(&restarted.id));
    assert_eq!(
        ws.live_run(goal.id).unwrap().map(|r| r.id),
        Some(restarted.id),
        "the restart's run is the live one"
    );
    assert!(
        ws.get_run(queued.id).unwrap().is_queued(),
        "the queue keeps its place behind it"
    );
    let first_now = ws.get_run(first.id).unwrap();
    assert!(
        first_now.cancelled.is_none(),
        "left as it ended: {first_now:?}"
    );
    assert_eq!(restarted.start, first.start);
    assert_eq!(restarted.inputs, first.inputs);
    engine.shutdown().await;
}

/// A restart whose workflow was put away since is refused before anything
/// is cancelled.
#[tokio::test(flavor = "multi_thread")]
async fn a_restart_of_an_archived_workflow_is_refused_before_anything_is_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    let (goal, first) = run_on(&engine, "its workflow put away", holds("put away"));
    ws.set_workflow_archived(first.workflow.id, true).unwrap();
    let err = engine.restart_goal(goal.id).await.unwrap_err();
    assert!(matches!(err, EngineError::Conflict(_)), "{err:?}");
    assert!(err.to_string().contains("archived"), "{err}");
    let live = ws.live_run(goal.id).unwrap().expect("still live");
    assert_eq!(live.id, first.id, "nothing was cancelled");
    assert_eq!(live.steps[&sid("hold")].state.as_str(), "waiting");
    engine.shutdown().await;
}

/// A restart whose last inputs no longer bind — the workflow asks for one
/// more now — is refused before anything is cancelled, naming the input.
#[tokio::test(flavor = "multi_thread")]
async fn a_restart_whose_inputs_no_longer_bind_is_refused_before_anything_is_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    let (goal, wf) = goal_on(&engine, "asked for more since", holds("asks later"));
    let first = engine.start_run(goal.id, BTreeMap::new()).unwrap();
    step_in_state(&engine, goal.id, "hold", "waiting").await;
    let say = step(
        "say",
        StepKind::Notify {
            scope: None,
            template: "for {inputs.customer}".into(),
            mentions: vec![],
            author: None,
        },
    );
    let mut asking = holds("asks now");
    asking.steps[0].then = vec![bisa_core::Flow::to(sid("say"))];
    asking.steps.push(say);
    asking.inputs = vec![bisa_core::InputDef {
        name: bisa_core::InputName::new("customer").unwrap(),
        label: "Customer".into(),
        kind: bisa_core::InputKind::Text,
        default: None,
        required: true,
    }];
    let (_, problems) = engine.save_workflow(wf.id, asking, wf.revision).unwrap();
    assert!(problems.is_empty(), "{problems:?}");
    let err = engine.restart_goal(goal.id).await.unwrap_err();
    assert!(matches!(err, EngineError::Conflict(_)), "{err:?}");
    assert!(err.to_string().contains("customer"), "{err}");
    let live = ws.live_run(goal.id).unwrap().expect("still live");
    assert_eq!(live.id, first.id, "nothing was cancelled");
    engine.shutdown().await;
}

/// A person's edit of the goal's own record: one write, announced, a blank
/// title clearing it, tags and budget replaced; the statement trimmed, never
/// blank, noted with the one it replaced, and fixed while a run is live; a
/// closed goal and an edit that names nothing refused.
#[tokio::test(flavor = "multi_thread")]
async fn a_person_edits_a_goals_own_record_and_the_statement_is_fixed_while_a_run_is_live() {
    use bisa_engine::ops::{edit_goal, GoalEdit};
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    let (goal, _) = goal_on(&engine, "worded loosely", holds("held"));
    let mut rx = engine.events();
    let edited = edit_goal(
        engine.inner(),
        goal.id,
        GoalEdit {
            title: Some("  The release  ".into()),
            statement: Some("  ship the release by Friday  ".into()),
            tags: Some(bisa_core::Tags::new(["release".to_string()]).unwrap()),
            budget: Some(bisa_core::Budget {
                max_usd_cents: Some(500),
                ..Default::default()
            }),
        },
    )
    .unwrap();
    assert_eq!(edited.title.as_deref(), Some("The release"));
    assert_eq!(edited.statement, "ship the release by Friday");
    assert_eq!(edited.tags.iter().count(), 1);
    assert_eq!(edited.budget.max_usd_cents, Some(500));
    assert_eq!(edited.revision, goal.revision + 1, "one write");
    wait_for(&mut rx, "goal.updated", |e| {
        matches!(&e.payload, bisa_engine::EnginePayload::GoalUpdated { goal: g } if *g == goal.id)
    })
    .await;
    assert!(
        notes(&engine, goal.id)
            .iter()
            .any(|n| n.starts_with("statement edited") && n.contains("worded loosely")),
        "the statement it replaced is kept in a note"
    );
    let cleared = edit_goal(
        engine.inner(),
        goal.id,
        GoalEdit {
            title: Some("  ".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(cleared.title, None);
    assert_eq!(
        cleared.statement, "ship the release by Friday",
        "left out, kept"
    );

    let nothing = edit_goal(engine.inner(), goal.id, GoalEdit::default()).unwrap_err();
    assert!(matches!(nothing, EngineError::Invalid(_)), "{nothing:?}");
    let blank = edit_goal(
        engine.inner(),
        goal.id,
        GoalEdit {
            statement: Some("   ".into()),
            ..Default::default()
        },
    );
    assert!(blank.is_err());
    assert_eq!(
        ws.get_goal(goal.id).unwrap().statement,
        "ship the release by Friday"
    );

    engine.start_run(goal.id, BTreeMap::new()).unwrap();
    step_in_state(&engine, goal.id, "hold", "waiting").await;
    let fixed = edit_goal(
        engine.inner(),
        goal.id,
        GoalEdit {
            statement: Some("something else".into()),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(matches!(fixed, EngineError::Conflict(_)), "{fixed:?}");
    // The same words are no change, and the rest of the record still moves.
    let live = edit_goal(
        engine.inner(),
        goal.id,
        GoalEdit {
            title: Some("Renamed mid-run".into()),
            statement: Some("ship the release by Friday".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(live.title.as_deref(), Some("Renamed mid-run"));

    engine
        .close_goal(goal.id, ClosureReason::Abandoned { rationale: None })
        .unwrap();
    let closed = edit_goal(
        engine.inner(),
        goal.id,
        GoalEdit {
            title: Some("too late".into()),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(matches!(closed, EngineError::Conflict(_)), "{closed:?}");
    engine.shutdown().await;
}
