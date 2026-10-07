//! A restart resumes what it can, re-runs what is safe, fails the rest once
//! without charging the author's retries, ends every session the last
//! process was driving — and the child it left behind — and says so on the
//! goal. Every scenario stages the crash the only way a test can: a session
//! on the endless mock, an engine shut down without settling, or a run
//! written straight into the store as a dead process would have left it;
//! then a second engine on the same directory. Fakes only: the mock harness,
//! a `sleep` this test starts and ends, a temporary directory.

use crate::common;

use bisa_core::event::JournalPayload;
use bisa_core::{
    CheckKind, RunEvent, RunOutcome, StepKind, StepState, WorkItemId, WorkItemSpec, WorkItemState,
};
use bisa_engine::{EnginePayload, INTERRUPTED, ORPHANED};
use bisa_harness::mock::MockAdapter;
use bisa_store::{FileKeyStore, Paths, SessionRow, SessionStatus, Workspace};
use common::*;
use serde_json::json;
use std::collections::BTreeMap;

/// A session that starts and never ends — the shape of a harness the last
/// process was driving when it stopped.
fn endless(id: &str) -> MockAdapter {
    MockAdapter {
        id: id.into(),
        script: Some(vec![]),
        ..Default::default()
    }
}

/// The store alone, the way a dead process left it: for staging a run
/// between two engines without an engine in the way.
fn store(dir: &tempfile::TempDir) -> Workspace {
    Workspace::open_with_keystore(
        dir.path(),
        Box::new(FileKeyStore::new(Paths::new(dir.path()).identity_dir())),
    )
    .unwrap()
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// The usual crash: the session had started, so the item was already in
/// progress. The launch preflight admits it as a resume — the session is not
/// the work — and the run finishes on the same item, no retry charged.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_step_in_progress_at_the_crash_resumes_on_its_item() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![endless("mock")]);
    let (goal, _) = run_on(
        &engine,
        "resume mid-session",
        new_workflow("A", vec![agent_step("work", "mock")]),
    );
    step_in_state(&engine, goal.id, "work", "running").await;
    until("the item to be in progress", || {
        items_of(&engine, goal.id)
            .into_iter()
            .find(|i| matches!(i.state, WorkItemState::InProgress { .. }))
            .map(|i| i.id)
    })
    .await;
    let item = items_of(&engine, goal.id)[0].id;
    engine.shutdown().await;

    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done), "{done:?}");
    let work = &done.steps[&sid("work")];
    assert_eq!(work.attempts, 0, "a restart is not the step's failure");
    assert_eq!(work.work_item, Some(item), "the same item, not a fresh one");
    let items = items_of(&engine, goal.id);
    assert_eq!(items.len(), 1, "no second item was minted: {items:?}");
    assert_eq!(items[0].interruptions, 1);
    assert_eq!(items[0].state, WorkItemState::Accepted);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_interrupted_agent_step_resumes_on_its_item_in_its_checkout_and_no_retry_is_charged() {
    let dir = tempfile::tempdir().unwrap();
    let first = endless("mock");
    let first_launches = std::sync::Arc::clone(&first.launches);
    let engine = engine_with(&dir, vec![first]);
    let (goal, _) = run_on(
        &engine,
        "resume after a restart",
        new_workflow("A", vec![agent_step("work", "mock")]),
    );
    step_in_state(&engine, goal.id, "work", "running").await;
    until("the item to be launched", || {
        (!items_of(&engine, goal.id).is_empty()).then_some(())
    })
    .await;
    let item = items_of(&engine, goal.id)[0].id;
    // The old session idles on the endless mock and dies with the runtime,
    // as it would with the process.
    engine.shutdown().await;

    let second = yielding("mock", json!({"ok": true}));
    let second_launches = std::sync::Arc::clone(&second.launches);
    let second_prompts = std::sync::Arc::clone(&second.prompts);
    let engine = engine_with(&dir, vec![second]);
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done), "{done:?}");
    let work = &done.steps[&sid("work")];
    assert_eq!(work.attempts, 0, "a restart is not the step's failure");
    assert_eq!(work.work_item, Some(item), "the same item, not a fresh one");

    let items = items_of(&engine, goal.id);
    assert_eq!(items.len(), 1, "no second item was minted: {items:?}");
    assert_eq!(items[0].id, item);
    assert_eq!(items[0].interruptions, 1);
    assert_eq!(items[0].state, WorkItemState::Accepted);

    {
        let before = first_launches.lock().unwrap();
        let after = second_launches.lock().unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(
            after[0].cwd, before[0].cwd,
            "the resumed session lands in the checkout the first one had"
        );
    }
    let prompt = second_prompts
        .lock()
        .unwrap()
        .last()
        .cloned()
        .unwrap_or_default();
    assert!(
        prompt.contains("interrupted by a restart") && prompt.contains("continue from there"),
        "the new session is told what happened: {prompt}"
    );
    assert!(
        notes(&engine, goal.id)
            .iter()
            .any(|n| n.contains("a restart interrupted 1 running step")
                && n.contains("`work` resumed on its work item")),
        "the goal says what the restart did: {:?}",
        notes(&engine, goal.id)
    );
    engine.shutdown().await;
}

/// A run of the workspace is walked like a goal's: found by its folder, its
/// step resumed on the item it had, and what the restart did written on the
/// run's own journal — no goal holds one.
#[tokio::test(flavor = "multi_thread")]
async fn a_workspace_runs_interrupted_step_resumes_and_the_run_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![endless("mock")]);
    let (_, run) = workspace_run(&engine, new_workflow("A", vec![agent_step("work", "mock")]));
    let home = run.home();
    run_step_in_state(&engine, run.id, "work", "running").await;
    let item = until("the item to be in progress", || {
        items_at(&engine, &home)
            .into_iter()
            .find(|i| matches!(i.state, WorkItemState::InProgress { .. }))
            .map(|i| i.id)
    })
    .await;
    engine.shutdown().await;

    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let done = run_finished(&engine, run.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done), "{done:?}");
    let work = &done.steps[&sid("work")];
    assert_eq!(work.attempts, 0, "a restart is not the step's failure");
    assert_eq!(work.work_item, Some(item), "the same item, not a fresh one");
    let items = items_at(&engine, &home);
    assert_eq!(items.len(), 1, "no second item was minted: {items:?}");
    assert_eq!(items[0].interruptions, 1);
    assert_eq!(items[0].state, WorkItemState::Accepted);
    assert!(
        notes_at(&engine, &home)
            .iter()
            .any(|n| n.contains("a restart interrupted 1 running step")
                && n.contains("`work` resumed on its work item")),
        "the run says what the restart did: {:?}",
        notes_at(&engine, &home)
    );
    assert_eq!(
        engine.workspace().list_goals(None).unwrap().len(),
        0,
        "and no goal was made to say it on"
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_restart_resumes_the_resumed_item_again() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![endless("mock")]);
    let (goal, _) = run_on(
        &engine,
        "twice",
        new_workflow("A", vec![agent_step("work", "mock")]),
    );
    step_in_state(&engine, goal.id, "work", "running").await;
    until("the item", || {
        (!items_of(&engine, goal.id).is_empty()).then_some(())
    })
    .await;
    engine.shutdown().await;

    let engine = engine_with(&dir, vec![endless("mock")]);
    until("the item to be resumed", || {
        (items_of(&engine, goal.id)
            .first()
            .is_some_and(|i| i.interruptions == 1))
        .then_some(())
    })
    .await;
    engine.shutdown().await;

    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    let items = items_of(&engine, goal.id);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].interruptions, 2);
    assert_eq!(done.steps[&sid("work")].attempts, 0);
    engine.shutdown().await;
}

/// A run written the way a dead process leaves one: started, its first step
/// `Running`, nothing behind it. `create_run` applies `Start` and hands the
/// effects back without running them — exactly the crash's shape.
fn staged_run(
    ws: &Workspace,
    name: &str,
    steps: Vec<bisa_core::Step>,
) -> (bisa_core::GoalId, bisa_core::RunId) {
    let goal = ws
        .create_goal(bisa_store::NewGoal::captured(name))
        .unwrap()
        .id;
    let wf = ws
        .create_workflow(
            new_workflow(name, steps),
            bisa_core::WorkflowOrigin::Workspace,
        )
        .unwrap();
    ws.set_goal_workflow(goal, Some(wf.id)).unwrap();
    let (run, _) = ws
        .create_run(
            bisa_core::RunScope::Goal { goal },
            wf.id,
            BTreeMap::new(),
            bisa_core::RunEntry::by_hand(),
            None,
        )
        .unwrap();
    (goal, run.id)
}

#[tokio::test(flavor = "multi_thread")]
async fn an_interrupted_check_runs_again_and_a_notify_fails_once_with_no_retry_charged() {
    let dir = tempfile::tempdir().unwrap();
    let (checked, notified) = {
        let ws = store(&dir);
        let checked = staged_run(
            &ws,
            "check again",
            vec![step(
                "verify",
                StepKind::Check {
                    check: CheckKind::Command {
                        command: "true".into(),
                    },
                },
            )],
        );
        let notified = staged_run(
            &ws,
            "notify once",
            vec![step(
                "tell",
                StepKind::Notify {
                    scope: Some("nowhere".into()),
                    template: "hello".into(),
                    mentions: vec![],
                    author: None,
                },
            )],
        );
        assert_eq!(
            ws.get_run(checked.1).unwrap().steps[&sid("verify")].state,
            StepState::Running,
            "staged as the crash left it"
        );
        (checked, notified)
    };

    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let done = finished_run(&engine, checked.0).await;
    assert_eq!(
        done.outcome,
        Some(RunOutcome::Done),
        "the check ran again: {done:?}"
    );
    assert_eq!(done.steps[&sid("verify")].attempts, 0);
    assert!(
        notes(&engine, checked.0)
            .iter()
            .any(|n| n.contains("`verify` will run again")),
        "{:?}",
        notes(&engine, checked.0)
    );

    let failed = finished_run(&engine, notified.0).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed), "{failed:?}");
    let tell = &failed.steps[&sid("tell")];
    assert_eq!(tell.state, StepState::Failed);
    assert_eq!(tell.error.as_deref(), Some(INTERRUPTED));
    assert_eq!(tell.attempts, 0, "the interruption charged nothing");
    assert!(
        notes(&engine, notified.0)
            .iter()
            .any(|n| n.contains("`tell` failed — no retries left")),
        "{:?}",
        notes(&engine, notified.0)
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_item_orphaned_between_its_creation_and_its_step_is_cancelled_and_the_step_starts_afresh(
) {
    let dir = tempfile::tempdir().unwrap();
    let (goal, run, orphan) = {
        let ws = store(&dir);
        let (goal, run) = staged_run(&ws, "orphan", vec![agent_step("work", "mock")]);
        // `put_work_item` landed, `StepStarted` never did: the item names the
        // step, the step's record names no item.
        let orphan = WorkItemId::from_ulid(ulid::Ulid::from_datetime(std::time::SystemTime::now()));
        ws.put_work_item(&WorkItemSpec {
            id: orphan,
            home: bisa_core::Home::Goal { goal },
            run: Some(run),
            step: Some(sid("work")),
            instructions: "orphaned".into(),
            state: WorkItemState::Open,
            project: None,
            harness_candidates: vec!["mock".into()],
            model: None,
            effort: None,
            output_schema: None,
            budget: Default::default(),
            assignees: vec![],
            tier_ceiling: bisa_core::ToolTier::Write,
            agent: None,
            spawn_allowlist: vec![],
            depth_budget: 0,
            result_attempts: 0,
            interruptions: 0,
        })
        .unwrap();
        (goal, run, orphan)
    };
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let done = finished_run(&engine, goal).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done), "{done:?}");
    assert_eq!(done.id, run);
    let items = items_of(&engine, goal);
    let orphaned = items
        .iter()
        .find(|i| i.id == orphan)
        .expect("the orphan is still a record");
    assert_eq!(orphaned.state, WorkItemState::Cancelled, "{orphaned:?}");
    assert!(
        items
            .iter()
            .any(|i| i.id != orphan && i.state == WorkItemState::Accepted),
        "the step started a fresh item: {items:?}"
    );
    assert!(
        notes(&engine, goal)
            .iter()
            .any(|n| n.contains(ORPHANED) || n.contains("named by no step, cancelled")),
        "{:?}",
        notes(&engine, goal)
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn every_live_session_row_is_ended_at_boot_and_only_the_child_that_is_still_ours_is_terminated(
) {
    let dir = tempfile::tempdir().unwrap();
    // Two children this test starts: one recorded as seen when it started
    // — ours — and one recorded as seen long before it existed, the shape of
    // a recycled pid. Both are `sleep`, nobody's but this test's.
    let mut ours = std::process::Command::new("sleep")
        .arg("300")
        .spawn()
        .unwrap();
    let mut stranger = std::process::Command::new("sleep")
        .arg("300")
        .spawn()
        .unwrap();
    // A chat turn's child, recorded as a worker's is now: ended at boot the
    // same way.
    let mut turn = std::process::Command::new("sleep")
        .arg("300")
        .spawn()
        .unwrap();
    {
        let ws = store(&dir);
        ws.record_session(&SessionRow {
            id: "sess-ours".into(),
            adapter: "mock".into(),
            status: SessionStatus::Live,
            pid: Some(ours.id()),
            pid_seen_at: Some(now()),
            ..Default::default()
        })
        .unwrap();
        ws.record_session(&SessionRow {
            id: "sess-turn".into(),
            adapter: "mock".into(),
            kind: bisa_store::SessionKind::Conversation,
            status: SessionStatus::Live,
            pid: Some(turn.id()),
            pid_seen_at: Some(now()),
            ..Default::default()
        })
        .unwrap();
        ws.record_session(&SessionRow {
            id: "sess-stranger".into(),
            adapter: "mock".into(),
            status: SessionStatus::Live,
            pid: Some(stranger.id()),
            pid_seen_at: Some(now().saturating_sub(100_000)),
            ..Default::default()
        })
        .unwrap();
        ws.record_session(&SessionRow {
            id: "sess-parked".into(),
            adapter: "mock".into(),
            status: SessionStatus::Parked,
            parked_at: Some(1),
            ..Default::default()
        })
        .unwrap();
    }
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    assert!(
        ws.list_live_sessions().unwrap().is_empty(),
        "every live row was ended"
    );
    let row = ws.session_by_id("sess-ours").unwrap().unwrap();
    assert_eq!(row.status, SessionStatus::Ended);
    assert!(row.ended_at.is_some());
    assert_eq!(row.pid, None);
    assert_eq!(
        ws.session_by_id("sess-parked").unwrap().unwrap().status,
        SessionStatus::Parked,
        "a parked session is not a dead one"
    );
    until("our child to be terminated", || {
        ours.try_wait().ok().flatten().map(|_| ())
    })
    .await;
    until("the turn's child to be terminated", || {
        turn.try_wait().ok().flatten().map(|_| ())
    })
    .await;
    assert!(
        stranger.try_wait().unwrap().is_none(),
        "a process that merely inherited a pid is nobody's to terminate"
    );
    let _terminated = stranger.kill();
    let _reaped = stranger.wait();
    engine.shutdown().await;
}

/// A second start on a workspace with nothing running does nothing — the
/// walk is idempotent and costs no fact.
/// The last process ended a run and stopped before its queue advanced: at
/// boot the goal has nothing live and a queued run, which starts.
#[tokio::test(flavor = "multi_thread")]
async fn a_queued_run_starts_at_boot_when_nothing_is_live() {
    let dir = tempfile::tempdir().unwrap();
    let (goal, queued) = {
        let ws = store(&dir);
        let (goal, live) = staged_run(
            &ws,
            "queued at boot",
            vec![step(
                "hold",
                StepKind::Wait {
                    until: bisa_core::WaitFor::Release,
                },
            )],
        );
        let wf = ws.get_goal(goal).unwrap().workflow.unwrap();
        let (queued, _) = ws
            .create_run(
                bisa_core::RunScope::Goal { goal },
                wf,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        assert_eq!(queued.status(), bisa_core::RunStatus::Queued);
        // The live run ends as a dead process would have left it: cancelled
        // in the store, with nothing to advance the queue.
        ws.record_run_event(
            live,
            RunEvent::Cancel {
                cause: bisa_core::CancelCause::Stopped { rationale: None },
            },
        )
        .unwrap();
        assert!(ws.live_run(goal).unwrap().is_none());
        (goal, queued.id)
    };

    let engine = engine_with(&dir, vec![endless("mock")]);
    let started = until("the queued run to start", || {
        let run = engine.workspace().get_run(queued).ok()?;
        run.is_live().then_some(run)
    })
    .await;
    assert_eq!(started.status(), bisa_core::RunStatus::Waiting);
    assert_eq!(engine.workspace().get_goal(goal).unwrap().run, Some(queued));
    assert!(engine.workspace().queued_runs(goal).unwrap().is_empty());
    let facts: Vec<String> = engine
        .workspace()
        .journal(&bisa_core::Home::from(goal))
        .unwrap()
        .into_iter()
        .filter_map(|je| match je.payload {
            JournalPayload::Run { run, event } => Some(format!("{run}:{}", event.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(
        facts.last().map(String::as_str),
        Some(format!("{queued}:started").as_str()),
        "{facts:?}"
    );
    engine.shutdown().await;
}

/// A held wait and the queue behind it come back as they were: the wait
/// still held and still a person's to release, the queued runs in the order
/// they were asked for, each starting when the one before it ends.
#[tokio::test(flavor = "multi_thread")]
async fn a_held_wait_and_the_queue_behind_it_survive_a_restart_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let held = || {
        vec![step(
            "hold",
            StepKind::Wait {
                until: bisa_core::WaitFor::Release,
            },
        )]
    };
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let (goal, first) = run_on(
        &engine,
        "held across a restart",
        new_workflow("held", held()),
    );
    let second = engine.start_run(goal.id, BTreeMap::new()).unwrap();
    let third = engine.start_run(goal.id, BTreeMap::new()).unwrap();
    assert!(second.is_queued() && third.is_queued());
    engine.shutdown().await;

    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    assert_eq!(ws.live_run(goal.id).unwrap().map(|r| r.id), Some(first.id));
    assert_eq!(
        current_run(&engine, goal.id).steps[&sid("hold")].state,
        StepState::Waiting,
        "a restart releases nothing"
    );
    let queue = |ws: &Workspace| -> Vec<bisa_core::RunId> {
        ws.queued_runs(goal.id)
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect()
    };
    assert_eq!(
        queue(ws),
        vec![second.id, third.id],
        "the order they were asked in"
    );
    // A clock that runs far ahead moves no `release` wait.
    engine.tick_waits_at(now() + 1_000_000);
    assert_eq!(ws.live_run(goal.id).unwrap().map(|r| r.id), Some(first.id));

    // Released: the first ends, the second takes its place — and waits in turn.
    engine
        .release_step(current_run(&engine, goal.id).id, &sid("hold"), None)
        .unwrap();
    until("the second run to be live", || {
        (ws.live_run(goal.id).ok()?.map(|r| r.id) == Some(second.id)).then_some(())
    })
    .await;
    assert_eq!(
        ws.get_run(first.id).unwrap().outcome,
        Some(RunOutcome::Done)
    );
    assert_eq!(queue(ws), vec![third.id]);
    engine
        .release_step(current_run(&engine, goal.id).id, &sid("hold"), None)
        .unwrap();
    until("the third run to be live", || {
        (ws.live_run(goal.id).ok()?.map(|r| r.id) == Some(third.id)).then_some(())
    })
    .await;
    assert!(queue(ws).is_empty());
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_restart_with_nothing_running_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let mut rx = engine.events();
    let (goal, _) = run_on(
        &engine,
        "finish first",
        new_workflow("done", vec![agent_step("work", "mock")]),
    );
    assert_eq!(
        finished_run(&engine, goal.id).await.outcome,
        Some(RunOutcome::Done)
    );
    // The run finishes on the result; the session's own teardown lands after
    // it, so the journal is counted once the execution has ended.
    wait_for(&mut rx, "the execution to end", |e| {
        matches!(e.payload, EnginePayload::ExecutionEnded { .. })
    })
    .await;
    let before = notes(&engine, goal.id).len();
    let journal_before = engine
        .workspace()
        .journal(&bisa_core::Home::from(goal.id))
        .unwrap()
        .len();
    engine.shutdown().await;
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    assert_eq!(
        notes(&engine, goal.id).len(),
        before,
        "no note on a finished run"
    );
    assert_eq!(
        engine
            .workspace()
            .journal(&bisa_core::Home::from(goal.id))
            .unwrap()
            .len(),
        journal_before,
        "no fact of any kind"
    );
    assert!(!engine
        .workspace()
        .journal(&bisa_core::Home::from(goal.id))
        .unwrap()
        .iter()
        .any(|e| matches!(e.payload, JournalPayload::Withdrawn { .. })),);
    // The machine refuses an interruption of a finished run like any event.
    assert!(engine
        .workspace()
        .record_run_event(
            engine
                .workspace()
                .get_current_run(goal.id)
                .unwrap()
                .unwrap()
                .id,
            RunEvent::StepInterrupted { step: sid("work") }
        )
        .is_err());
    engine.shutdown().await;
}
