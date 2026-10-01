//! Runs of the workspace: a workflow run with no goal behind it, driven end
//! to end through the engine — started from the library, several at once,
//! stopped and restarted one by one or by their workflow, asked and approved
//! through the run itself, speaking in `general`, spawning a goal of its own,
//! spending against the ceiling it carries — and never told of a goal it does
//! not have, where a goal's run always is.
//!
//! Every scenario is a workspace in a temporary directory, on mock harnesses.

use crate::common;
use bisa_core::event::JournalPayload;
use bisa_core::{
    Answer, AskOption, Budget, CancelCause, Flow, Gate, GoalOrigin, Home, ListenerHost,
    ListenerKey, RunOutcome, RunStatus, SettingScope, SignalSource, StartOn, StepKind, StepState,
    WaitFor,
};
use bisa_engine::{
    Engine, EngineError, EnginePayload, ExecutionOutcome, HomeStatus, HookDoor, SubmitRequest,
};
use bisa_harness::mock::MockAdapter;
use bisa_harness::{HarnessAdapter, HarnessCatalog, LifecycleEvent, ProgressEvent, SessionEvent};
use common::*;
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;

/// A workflow that waits for a person to release it: a run that stays open.
fn held(name: &str) -> bisa_store::NewWorkflow {
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

/// A held workflow that declares `who` and reads it after the hold.
fn held_with_who(name: &str) -> bisa_store::NewWorkflow {
    let mut wf = new_workflow(
        name,
        chain(vec![
            step(
                "hold",
                StepKind::Wait {
                    until: WaitFor::Release,
                },
            ),
            step(
                "tell",
                StepKind::Notify {
                    scope: None,
                    template: "hello {inputs.who}".into(),
                    mentions: vec![],
                    author: None,
                },
            ),
        ]),
    );
    wf.inputs = vec![bisa_core::InputDef {
        name: bisa_core::InputName::new("who").unwrap(),
        label: "Who".into(),
        kind: bisa_core::InputKind::Text,
        default: None,
        required: true,
    }];
    wf
}

/// An engine on one mock harness, and the handle a test reads its launches
/// and prompts back from.
fn engine_watching(dir: &tempfile::TempDir, adapter: MockAdapter) -> (Engine, Arc<MockAdapter>) {
    let adapter = Arc::new(adapter);
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<dyn HarnessAdapter>);
    let engine = Engine::start(workspace(dir), catalog, design_off_config()).unwrap();
    (engine, adapter)
}

fn goal_count(engine: &Engine) -> usize {
    engine.workspace().list_goals(None).unwrap().len()
}

fn inputs(who: &str) -> BTreeMap<String, serde_json::Value> {
    BTreeMap::from([("who".to_string(), json!(who))])
}

// ---------------------------------------------------------------------------
// Starting
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn a_workspace_run_starts_at_once_runs_its_steps_and_captures_no_goal() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"summary": "ok"}))]);
    let mut rx = engine.events();
    let (wf, run) = workspace_run(
        &engine,
        new_workflow("nightly", vec![agent_step("build", "mock")]),
    );
    assert!(run.scope.is_workspace(), "{:?}", run.scope);
    assert_ne!(
        run.status(),
        RunStatus::Queued,
        "a run of the workspace never queues"
    );
    assert_eq!(run.home(), Home::Run { run: run.id });

    let done = run_finished(&engine, run.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done), "{done:?}");
    assert_eq!(goal_count(&engine), 0, "no goal was captured for it");

    // Its work item is filed at the run, and its result on the run's journal.
    let items = items_at(&engine, &run.home());
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].home, run.home());
    assert_eq!(items[0].run, Some(run.id));
    let journal = engine.workspace().journal(&run.home()).unwrap();
    assert!(journal
        .iter()
        .any(|e| matches!(&e.payload, JournalPayload::Result { work_item, .. } if *work_item == items[0].id)));

    // Everything the run emits names its workflow and no goal.
    let finished = wait_for(
        &mut rx,
        "the run to finish",
        |e| matches!(&e.payload, EnginePayload::RunFinished { run: r, .. } if *r == run.id),
    )
    .await;
    assert_eq!(finished.goal, None);
    assert_eq!(finished.workflow, Some(wf.id));
    let EnginePayload::RunFinished { workflow, .. } = finished.payload else {
        unreachable!()
    };
    assert_eq!(workflow, wf.id);
    assert_eq!(engine.workflow_runs(wf.id).unwrap().len(), 1);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn two_runs_of_one_workflow_go_at_once_and_one_ends_alone() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let (wf, first) = workspace_run(&engine, held("held"));
    let second = engine.start_workspace_run(wf.id, BTreeMap::new()).unwrap();
    for run in [&first, &second] {
        run_step_in_state(&engine, run.id, "hold", "waiting").await;
    }
    let live = engine.workspace().live_workspace_runs(Some(wf.id)).unwrap();
    assert_eq!(live.len(), 2, "both go; neither waits for the other");

    engine.release_step(first.id, &sid("hold"), None).unwrap();
    assert_eq!(
        run_finished(&engine, first.id).await.outcome,
        Some(RunOutcome::Done)
    );
    assert_eq!(run_of(&engine, second.id).status(), RunStatus::Waiting);
    // A step is released by its run: a step the run lacks is refused.
    assert!(engine.release_step(second.id, &sid("nope"), None).is_err());
    engine.shutdown().await;
}

/// A workflow's history in the workspace is bounded, and the bound is
/// applied the moment a run of it ends: with `workflow.runs.keep` at two, a
/// fourth run's end puts the oldest finished one away — folder and record —
/// while a run still going is neither counted nor touched. A goal's run
/// ending applies no bound: a goal's runs are the goal's.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_of_the_workspace_ending_puts_the_oldest_finished_ones_beyond_the_bound_away() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    engine
        .workspace()
        .set_setting(
            SettingScope::Workspace,
            None,
            bisa_core::WorkflowRun::SETTING_KEPT,
            json!(2),
        )
        .unwrap();
    let (wf, first) = workspace_run(&engine, held("bounded"));
    engine.stop_run(first.id, None).await.unwrap();
    let second = engine.start_workspace_run(wf.id, BTreeMap::new()).unwrap();
    engine.stop_run(second.id, None).await.unwrap();
    let live = engine.start_workspace_run(wf.id, BTreeMap::new()).unwrap();
    run_step_in_state(&engine, live.id, "hold", "waiting").await;
    assert_eq!(
        engine.workspace().list_workflow_runs(wf.id).unwrap().len(),
        3,
        "two finished, one going: within the bound, nothing went"
    );

    // The fourth's end: three finished, the oldest goes.
    let fourth = engine.start_workspace_run(wf.id, BTreeMap::new()).unwrap();
    engine.stop_run(fourth.id, None).await.unwrap();
    let kept: Vec<_> = engine
        .workspace()
        .list_workflow_runs(wf.id)
        .unwrap()
        .iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(kept, vec![second.id, live.id, fourth.id]);
    assert!(
        !engine
            .workspace()
            .paths()
            .home(&first.home())
            .dir()
            .exists(),
        "the oldest finished run's folder went"
    );
    assert!(engine.workspace().get_run(first.id).is_err());
    assert_eq!(run_of(&engine, live.id).status(), RunStatus::Waiting);

    // A goal on the same workflow: its run's end leaves the workspace's
    // history as it is, and its own run is nobody's to put away.
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            workflow: Some(wf.id),
            ..SubmitRequest::captured("a goal on it")
        })
        .unwrap();
    engine
        .workspace()
        .set_setting(
            SettingScope::Workspace,
            None,
            bisa_core::WorkflowRun::SETTING_KEPT,
            json!(1),
        )
        .unwrap();
    let goals_run = engine.start_run(goal.id, BTreeMap::new()).unwrap();
    run_step_in_state(&engine, goals_run.id, "hold", "waiting").await;
    engine.stop_goal(goal.id, None).await.unwrap();
    run_finished(&engine, goals_run.id).await;
    let after: Vec<_> = engine
        .workspace()
        .list_workflow_runs(wf.id)
        .unwrap()
        .iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(after, kept, "a goal's run ending bounds nothing");
    assert!(engine.workspace().get_run(goals_run.id).is_ok());
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_input_is_refused_before_any_run_is_made() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let wf = engine.create_workflow(held_with_who("asks who")).unwrap();
    let err = engine
        .start_workspace_run(wf.id, BTreeMap::new())
        .unwrap_err();
    assert!(err.is_refusal(), "{err}");
    assert!(engine.workflow_runs(wf.id).unwrap().is_empty());
    assert_eq!(goal_count(&engine), 0);
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// Stop and restart
// ---------------------------------------------------------------------------

/// A run of the workspace's worker is named by its run in the roster — no
/// goal holds it — and a stop of the run ends that session and no other.
#[tokio::test(flavor = "multi_thread")]
async fn a_workspace_runs_worker_is_named_by_its_run_and_stops_with_it() {
    let dir = tempfile::tempdir().unwrap();
    // A session that starts and never ends: a worker at work.
    let at_work = MockAdapter {
        script: Some(vec![]),
        ..Default::default()
    };
    let engine = engine_with(&dir, vec![at_work]);
    let wf = engine
        .create_workflow(new_workflow("works", vec![agent_step("work", "mock")]))
        .unwrap();
    let one = engine.start_workspace_run(wf.id, BTreeMap::new()).unwrap();
    let other = engine.start_workspace_run(wf.id, BTreeMap::new()).unwrap();

    let live = |engine: &Engine| {
        engine
            .inner()
            .presence
            .snapshot()
            .into_iter()
            .filter(|row| !row.state.is_ended())
            .collect::<Vec<_>>()
    };
    let rows = until("both workers in the roster", || {
        let rows = live(&engine);
        (rows.len() == 2).then_some(rows)
    })
    .await;
    for run in [&one, &other] {
        let row = rows
            .iter()
            .find(|row| row.run == Some(run.id))
            .unwrap_or_else(|| panic!("no row names the run {}: {rows:?}", run.id));
        assert_eq!(row.goal, None, "no goal holds it");
        assert_eq!(
            row.work_item,
            run_of(&engine, run.id).steps[&sid("work")].work_item,
            "the step's own item"
        );
    }

    engine.stop_run(one.id, None).await.unwrap();
    let left = until("the stopped run's worker to be gone", || {
        let rows = live(&engine);
        (rows.len() == 1).then_some(rows)
    })
    .await;
    assert_eq!(left[0].run, Some(other.id), "the run beside it goes on");
    assert_eq!(
        run_of(&engine, other.id).steps[&sid("work")].state,
        StepState::Running
    );
    engine.shutdown().await;
}

/// A stop or a restart that finds its run over — it ended by itself before
/// the cancel reached it — is no fault and changes nothing of how it ended:
/// the run's own refusal is the answer, never a look before the cancel.
#[tokio::test(flavor = "multi_thread")]
async fn a_stop_that_finds_its_run_over_leaves_it_as_it_ended() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let wf = engine.create_workflow(held("holds")).unwrap();
    let ended = engine.start_workspace_run(wf.id, BTreeMap::new()).unwrap();
    let going = engine.start_workspace_run(wf.id, BTreeMap::new()).unwrap();
    for run in [&ended, &going] {
        run_step_in_state(&engine, run.id, "hold", "waiting").await;
    }
    engine.release_step(ended.id, &sid("hold"), None).unwrap();
    let done = run_finished(&engine, ended.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));

    let asked = engine.stop_run(ended.id, None).await.unwrap();
    assert_eq!(asked.outcome, Some(RunOutcome::Done));
    assert_eq!(asked.cancelled, None, "it was not stopped: it had ended");

    // A stop of every run of the workflow names the one it stopped.
    assert_eq!(engine.stop_workflow(wf.id).await.unwrap(), vec![going.id]);
    assert_eq!(engine.stop_workflow(wf.id).await.unwrap(), vec![]);

    // A restart of the run that ended starts another, and the one that
    // ended is as it ended.
    let fresh = engine.restart_run(ended.id).await.unwrap();
    assert_ne!(fresh.id, ended.id);
    let kept = run_of(&engine, ended.id);
    assert_eq!(kept.outcome, Some(RunOutcome::Done));
    assert_eq!(kept.cancelled, None);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn stopping_a_run_cancels_it_and_restarting_keeps_its_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let wf = engine.create_workflow(held_with_who("greets")).unwrap();
    let run = engine.start_workspace_run(wf.id, inputs("Ada")).unwrap();
    run_step_in_state(&engine, run.id, "hold", "waiting").await;

    let stopped = engine
        .stop_run(run.id, Some("not tonight".into()))
        .await
        .unwrap();
    assert_eq!(
        stopped.cancelled,
        Some(CancelCause::Stopped {
            rationale: Some("not tonight".into())
        })
    );
    assert!(stopped.is_finished());
    // Stopping a run that is over answers it as it is.
    let again = engine.stop_run(run.id, None).await.unwrap();
    assert_eq!(again.cancelled, stopped.cancelled);

    // Restarting a finished run starts a new one with the same inputs.
    let fresh = engine.restart_run(run.id).await.unwrap();
    assert_ne!(fresh.id, run.id);
    assert!(fresh.scope.is_workspace());
    assert_eq!(fresh.inputs["who"], json!("Ada"));
    run_step_in_state(&engine, fresh.id, "hold", "waiting").await;

    // Restarting a live one cancels it first, as restarted.
    let newer = engine.restart_run(fresh.id).await.unwrap();
    assert_eq!(
        run_of(&engine, fresh.id).cancelled,
        Some(CancelCause::Restarted)
    );
    assert_eq!(newer.inputs["who"], json!("Ada"));
    assert_eq!(engine.workflow_runs(wf.id).unwrap().len(), 3);
    assert_eq!(goal_count(&engine), 0);
    engine.shutdown().await;
}

/// A held workflow with two ways in — by hand, and on a call — or, without
/// its hook, the one.
fn held_on_call(name: &str, with_hook: bool) -> bisa_store::NewWorkflow {
    let way_in = |id: &str, on: StartOn| {
        let mut start = step(
            id,
            StepKind::Start {
                on,
                inputs: BTreeMap::new(),
                guard: Default::default(),
            },
        );
        start.then = vec![Flow::to(sid("hold"))];
        start
    };
    let mut steps = vec![way_in("by-hand", StartOn::Manual)];
    if with_hook {
        steps.push(way_in("ticket", StartOn::Hook { public: false }));
    }
    steps.push(step(
        "hold",
        StepKind::Wait {
            until: WaitFor::Release,
        },
    ));
    new_workflow(name, steps)
}

/// A restart begins where the run began, on the event that began it, under
/// the ceiling it carried — a run of its own, which the signal did not make.
/// When the start is gone from the workflow as it stands there is nowhere to
/// begin again: refused, and the run it would have replaced is left alone.
#[tokio::test(flavor = "multi_thread")]
async fn restarting_a_run_an_event_began_keeps_its_start_its_event_and_its_ceiling() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    let wf = engine
        .create_workflow(held_on_call("on call", true))
        .unwrap();
    let host = ListenerHost::Workspace { workflow: wf.id };
    let ceiling = Budget {
        max_tokens: Some(5_000),
        ..Budget::default()
    };
    engine
        .set_listening(host, BTreeMap::new(), Some(ceiling.clone()))
        .unwrap();
    let ticket = ListenerKey {
        host,
        step: sid("ticket"),
    };
    let signal = engine
        .call_hook(&ticket, json!({"why": "a test"}), None, HookDoor::Local)
        .unwrap();
    assert_eq!(engine.drain_signals().await, 1);
    let run = ws.live_workspace_runs(Some(wf.id)).unwrap().remove(0);
    assert_eq!(run.scope.budget(), Some(&ceiling), "the listening ceiling");
    assert_eq!(run.dispatched, Some(signal.clone()));

    let again = engine.restart_run(run.id).await.unwrap();
    assert_eq!(again.start, Some(sid("ticket")), "the same start");
    assert_eq!(again.event, run.event, "the same event");
    assert_eq!(
        again.event.as_ref().map(|e| e.id.as_str()),
        Some(signal.as_str())
    );
    assert_eq!(again.dispatched, None, "the signal made one run: the first");
    assert_eq!(again.scope.budget(), Some(&ceiling), "the same ceiling");
    assert_eq!(
        run_of(&engine, run.id).cancelled,
        Some(CancelCause::Restarted)
    );

    // The hook start goes from the definition: nowhere to begin again.
    engine.stop_listening(host).unwrap();
    let (_, problems) = engine
        .save_workflow(wf.id, held_on_call("on call", false), wf.revision)
        .unwrap();
    assert!(problems.is_empty(), "{problems:?}");
    let refused = engine.restart_run(again.id).await.unwrap_err();
    assert!(matches!(&refused, EngineError::Conflict(_)), "{refused:?}");
    assert!(refused.to_string().contains("ticket"), "{refused}");
    assert!(
        run_of(&engine, again.id).is_live(),
        "a refused restart leaves the run as it was"
    );
    engine.shutdown().await;
}

/// A test run begins at an event start as if its event had happened with a
/// sample payload — the mapping read over it — and says it is a test.
#[tokio::test(flavor = "multi_thread")]
async fn a_test_run_begins_at_an_event_start_on_a_sample() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let mut draft = held_on_call("tickets", true);
    if let StepKind::Start { inputs, .. } = &mut draft.steps[1].kind {
        inputs.insert("subject".into(), "{event.payload.subject}".into());
    }
    draft.inputs = vec![bisa_core::InputDef {
        name: bisa_core::InputName::new("subject").unwrap(),
        label: "Subject".into(),
        kind: bisa_core::InputKind::Text,
        default: None,
        required: true,
    }];
    let wf = engine.create_workflow(draft).unwrap();

    // Off, and never turned on: a test needs no listening.
    let run = engine
        .test_run_workflow(
            wf.id,
            &sid("ticket"),
            json!({"subject": "printer on fire"}),
            BTreeMap::new(),
        )
        .unwrap();
    assert_eq!(run.start, Some(sid("ticket")));
    assert_eq!(run.inputs["subject"], json!("printer on fire"));
    assert_eq!(run.dispatched, None, "no signal was queued for it");
    let sample = run.event.as_ref().expect("the sample it began on");
    assert_eq!(sample.source, SignalSource::Test);
    assert_eq!(sample.listener, None);
    assert_eq!(sample.payload, json!({"subject": "printer on fire"}));
    assert!(engine
        .workspace()
        .list_signals(None, 10)
        .unwrap()
        .is_empty());

    // A sample the mapping cannot read, and a step that is no start.
    let unmapped = engine
        .test_run_workflow(wf.id, &sid("ticket"), json!({}), BTreeMap::new())
        .unwrap_err();
    assert!(unmapped.to_string().contains("subject"), "{unmapped}");
    let no_start = engine
        .test_run_workflow(wf.id, &sid("hold"), json!({}), BTreeMap::new())
        .unwrap_err();
    assert!(no_start.is_refusal(), "{no_start:?}");
    // By hand it needs the input the event would have mapped.
    let by_hand = engine
        .start_workspace_run(wf.id, BTreeMap::new())
        .unwrap_err();
    assert!(by_hand.to_string().contains("subject"), "{by_hand}");
    let by_hand = engine
        .start_workspace_run(
            wf.id,
            BTreeMap::from([("subject".to_string(), json!("typed in"))]),
        )
        .unwrap();
    assert_eq!(by_hand.start, Some(sid("by-hand")));
    assert_eq!(by_hand.event, None);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_goals_run_is_the_goals_and_the_workflows_verbs_leave_it_alone() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let wf = engine.create_workflow(held("shared")).unwrap();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            workflow: Some(wf.id),
            ..SubmitRequest::captured("a goal on it")
        })
        .unwrap();
    let goals_run = engine.start_run(goal.id, BTreeMap::new()).unwrap();
    let mut ours = vec![
        engine
            .start_workspace_run(wf.id, BTreeMap::new())
            .unwrap()
            .id,
        engine
            .start_workspace_run(wf.id, BTreeMap::new())
            .unwrap()
            .id,
    ];
    // Another workflow's run of the workspace is not this workflow's.
    let (_, elsewhere) = workspace_run(&engine, held("other"));
    // A finished run of it is not going.
    let over = engine.start_workspace_run(wf.id, BTreeMap::new()).unwrap();
    engine.stop_run(over.id, None).await.unwrap();

    // A goal's run is stopped and restarted from its goal.
    let err = engine.stop_run(goals_run.id, None).await.unwrap_err();
    assert!(err.is_refusal(), "{err}");
    assert!(err.to_string().contains("from its goal"), "{err}");
    let err = engine.restart_run(goals_run.id).await.unwrap_err();
    assert!(err.to_string().contains("from its goal"), "{err}");

    let mut stopped = engine.stop_workflow(wf.id).await.unwrap();
    stopped.sort();
    ours.sort();
    assert_eq!(
        stopped, ours,
        "its live runs of the workspace, and those only"
    );
    for run in &ours {
        assert_eq!(
            run_of(&engine, *run).cancelled,
            Some(CancelCause::Stopped { rationale: None })
        );
    }
    assert_eq!(
        run_of(&engine, goals_run.id).status(),
        RunStatus::Waiting,
        "the goal's run of it is untouched"
    );
    assert_eq!(run_of(&engine, elsewhere.id).status(), RunStatus::Waiting);
    assert!(engine.stop_workflow(wf.id).await.unwrap().is_empty());

    // Restart: every live run of it again, as new runs.
    let a = engine.start_workspace_run(wf.id, BTreeMap::new()).unwrap();
    let b = engine.start_workspace_run(wf.id, BTreeMap::new()).unwrap();
    let restarted = engine.restart_workflow(wf.id).await.unwrap();
    assert_eq!(restarted.len(), 2);
    for old in [a.id, b.id] {
        assert_eq!(run_of(&engine, old).cancelled, Some(CancelCause::Restarted));
        assert!(!restarted.contains(&old));
    }
    for new in &restarted {
        assert_eq!(run_of(&engine, *new).status(), RunStatus::Waiting);
    }
    assert_eq!(run_of(&engine, goals_run.id).status(), RunStatus::Waiting);
    assert_eq!(goal_count(&engine), 1, "only the person's goal");

    let ghost = bisa_core::WorkflowId::from_ulid(ulid::Ulid::from_parts(9, 9));
    assert!(engine.stop_workflow(ghost).await.is_err());
    assert!(engine.restart_workflow(ghost).await.is_err());
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// A person's part, through the run
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn a_human_step_is_answered_through_its_run_and_journaled_on_it() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let which = step(
        "which",
        StepKind::Human {
            prompt: "Which region?".into(),
            options: vec![
                AskOption {
                    id: "eu".into(),
                    label: "Europe".into(),
                    detail: None,
                    recommended: true,
                },
                AskOption {
                    id: "us".into(),
                    label: "America".into(),
                    detail: None,
                    recommended: false,
                },
            ],
            multi: false,
            assignee: None,
        },
    );
    let (_, run) = workspace_run(&engine, new_workflow("asks", vec![which]));
    run_step_in_state(&engine, run.id, "which", "waiting").await;
    let gate = until("the question", || {
        engine.inbox().into_iter().find(|g| g.home == run.home())
    })
    .await;
    assert_eq!(gate.home.goal(), None, "a run of the workspace's own ask");

    let answered = engine
        .answer_step(run.id, &sid("which"), &Answer::selecting(["eu"]))
        .unwrap();
    let done = run_finished(&engine, answered.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert_eq!(
        done.steps[&sid("which")].answer.as_ref().unwrap().selected,
        vec!["eu".to_string()]
    );
    let journal = engine.workspace().journal(&run.home()).unwrap();
    assert!(
        journal
            .iter()
            .any(|e| matches!(&e.payload, JournalPayload::Decision { .. })),
        "the answer is signed on the run's own journal"
    );
    assert!(engine.inbox().iter().all(|g| g.home != run.home()));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_approval_is_decided_through_the_runs_home() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let gate = step(
        "ok",
        StepKind::Approval {
            prompt: "Ship it?".into(),
        },
    );
    let (_, run) = workspace_run(&engine, new_workflow("gated", vec![gate]));
    until("the approval gate", || {
        engine
            .inbox()
            .into_iter()
            .find(|g| g.home == run.home() && g.gate == Gate::Approval)
    })
    .await;

    let out = engine
        .decide_durable(&run.home(), true, Some("go"), None, None, None, None)
        .unwrap();
    assert_eq!(out.home, run.home());
    assert_eq!(out.gate, Gate::Approval);
    assert!(out.approve);
    assert!(
        matches!(out.status, HomeStatus::Run { .. }),
        "{:?}",
        out.status
    );
    let done = run_finished(&engine, run.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert!(matches!(
        done.steps[&sid("ok")].state,
        StepState::Done { .. }
    ));
    // Nothing more is owed.
    let err = engine
        .decide_durable(&run.home(), true, None, None, None, None, None)
        .unwrap_err();
    assert!(err.is_refusal(), "{err}");
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// What a run of the workspace does in the world
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn a_notify_with_no_conversation_speaks_in_general() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let say = step(
        "say",
        StepKind::Notify {
            scope: None,
            template: "the nightly report is out".into(),
            mentions: vec![],
            author: None,
        },
    );
    let (_, run) = workspace_run(&engine, new_workflow("announces", vec![say]));
    let done = run_finished(&engine, run.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done), "{done:?}");
    let general = bisa_core::ChannelId::general().to_string();
    let said: Vec<String> = engine
        .workspace()
        .messages(&general, None, 50)
        .unwrap()
        .into_iter()
        .map(|m| m.content)
        .collect();
    assert!(
        said.iter().any(|m| m == "the nightly report is out"),
        "{said:?}"
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_spawn_step_captures_a_goal_born_of_the_run_and_waits_for_it() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let child_wf = engine
        .create_workflow(new_workflow("child work", vec![agent_step("do", "mock")]))
        .unwrap();
    let spawn = step(
        "delegate",
        StepKind::Spawn {
            statement_template: "follow the release up".into(),
            workflow: Some(child_wf.id),
            assignees: vec![],
            inputs: Default::default(),
            wait: true,
        },
    );
    let (_, run) = workspace_run(&engine, new_workflow("parent", vec![spawn]));
    let done = run_finished(&engine, run.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done), "{done:?}");
    let output = done.steps[&sid("delegate")].output.clone().unwrap();
    let child: bisa_core::GoalId = output["child"].as_str().unwrap().parse().unwrap();
    assert_eq!(output["outcome"], json!("done"));
    let child_goal = engine.workspace().get_goal(child).unwrap();
    assert_eq!(
        child_goal.origin,
        GoalOrigin::Run {
            run: run.id,
            step: sid("delegate")
        }
    );
    assert_eq!(child_goal.origin.parent(), None);
    assert!(
        engine.workspace().edges_from(child).unwrap().is_empty(),
        "a goal of its own: it refines nothing"
    );
    assert_eq!(child_goal.mode, engine.default_goal_mode());
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_workspace_run_spends_against_its_own_ceiling_and_is_stopped_by_it() {
    let dir = tempfile::tempdir().unwrap();
    let scripted = MockAdapter {
        script: Some(vec![
            SessionEvent::Lifecycle(LifecycleEvent::Started),
            SessionEvent::Progress(ProgressEvent::CostDelta {
                input_tokens: 150,
                output_tokens: 100,
                usd_cents: 0,
            }),
            // No terminal end: the engine must abort on the ceiling.
        ]),
        ..Default::default()
    };
    let engine = engine_with(&dir, vec![scripted]);
    engine
        .workspace()
        .set_setting(
            SettingScope::Workspace,
            None,
            Budget::SETTING_TOKENS,
            json!(100),
        )
        .unwrap();
    let mut rx = engine.events();
    let (_, run) = workspace_run(
        &engine,
        new_workflow("expensive", vec![agent_step("spend", "mock")]),
    );
    assert_eq!(
        run.scope.budget().and_then(|b| b.max_tokens),
        Some(100),
        "the workspace default, frozen on the run"
    );
    wait_for(&mut rx, "the budget abort", |e| {
        matches!(
            &e.payload,
            EnginePayload::ExecutionEnded {
                outcome: ExecutionOutcome::BudgetExhausted
            }
        )
    })
    .await;
    let failed = run_finished(&engine, run.id).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed));
    assert!(failed.steps[&sid("spend")]
        .error
        .as_deref()
        .unwrap()
        .contains("budget"));
    let spent = engine.workspace().spent(&run.home()).unwrap();
    assert!(spent.tokens >= 250, "{spent:?}");
    assert_eq!(goal_count(&engine), 0);
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// What a session is told
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn a_goals_first_prompt_names_its_goal_and_a_workspace_runs_names_none() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_watching(&dir, yielding("mock", json!({"ok": true})));
    let draft = || new_workflow("one step", vec![agent_step("build", "mock")]);

    let wf = engine.create_workflow(draft()).unwrap();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            title: Some("Checkout".into()),
            workflow: Some(wf.id),
            ..SubmitRequest::captured("Ship the new checkout by Friday.")
        })
        .unwrap();
    engine.start_run(goal.id, BTreeMap::new()).unwrap();
    finished_run(&engine, goal.id).await;
    let on_the_goal = adapter.prompts().remove(0);
    assert!(
        on_the_goal.contains("## The goal this work serves"),
        "{on_the_goal}"
    );
    assert!(on_the_goal.contains("Checkout"), "{on_the_goal}");
    assert!(
        on_the_goal.contains("Ship the new checkout by Friday."),
        "{on_the_goal}"
    );

    let run = engine.start_workspace_run(wf.id, BTreeMap::new()).unwrap();
    run_finished(&engine, run.id).await;
    let prompts = adapter.prompts();
    let in_the_workspace = prompts.last().unwrap();
    assert!(
        !in_the_workspace.contains("The goal this work serves"),
        "{in_the_workspace}"
    );
    assert!(
        !in_the_workspace.contains("Ship the new checkout"),
        "{in_the_workspace}"
    );
    engine.shutdown().await;
}
