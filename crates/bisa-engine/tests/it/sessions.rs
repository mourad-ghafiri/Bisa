//! Stopping one session by its row reaches the harness that runs it.
//!
//! A person's *Terminate* in the roster, `bisa sessions abort` and a goal's
//! stop all end a row — and the harness behind it, whatever drives it: a
//! worker on a step, an agent's turn in a conversation, the Workflow Agent's
//! design wake. A row that reads *aborted* beside a harness still at work is
//! the fault these tests hold off: each reads what the harness itself was
//! told, never the roster alone.

use crate::common;

use bisa_core::event::{GuardJudge, JournalPayload};
use bisa_core::{
    AgentId, ClosureReason, GuidanceStatus, MessageBody, RosterPolicy, SessionOrigin, ToolTier,
    WorkItemState,
};
use bisa_engine::registry::SessionKind;
use bisa_engine::{
    AgentStatus, Engine, EngineConfig, EnginePayload, SessionPresence, SessionState,
};
use bisa_harness::mock::{Close, MockAdapter};
use bisa_harness::{
    InputAnswer, InputRequest, LifecycleEvent, Outcome, ProgressEvent, SessionEvent,
};
use bisa_store::PostOrigin;
use common::*;
use serde_json::json;
use std::sync::{Arc, Mutex};

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

/// The one live row of a kind, once the roster has it.
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

fn state_of(engine: &Engine, row: &SessionPresence) -> Option<SessionState> {
    engine.inner().presence.get(row.id).map(|row| row.state)
}

/// A worker terminated from the roster: its harness is told to stop, its
/// item settles failed in words, and the row reads *aborted* — the person's
/// decision, which what the driver says on its way out does not rewrite.
#[tokio::test(flavor = "multi_thread")]
async fn a_worker_stopped_by_its_row_is_told_to_stop_and_its_item_settles_failed() {
    let dir = tempfile::tempdir().unwrap();
    let (adapter, closes) = at_work("mock");
    let engine = engine_with(&dir, vec![adapter]);
    let (goal, _) = run_on(
        &engine,
        "work that is stopped",
        new_workflow("works", vec![agent_step("work", "mock")]),
    );
    let row = live_row(&engine, SessionKind::Worker).await;
    assert!(row.work_item.is_some(), "a worker's row names its item");
    let mut bus = engine.events();
    // The roster as `GET /sessions` reads it, memoized for a minute here:
    // read once before the stop, it holds the row as it was.
    let roster = |engine: &Engine| {
        engine
            .inner()
            .presence
            .snapshot_cached(std::time::Duration::from_secs(60))
            .into_iter()
            .find(|held| held.id == row.id)
            .map(|held| held.state)
    };
    assert!(roster(&engine).is_some_and(|state| state.is_live()));

    bisa_engine::sessions::stop_one(engine.inner(), row.id).unwrap();

    assert_eq!(
        roster(&engine),
        Some(SessionState::Aborted),
        "the roster read right after the stop says it, whatever it memoized"
    );
    until("the harness to be told to stop", || {
        closed(&closes).contains(&Close::Aborted).then_some(())
    })
    .await;
    let reason = until("the item to settle failed", || {
        items_of(&engine, goal.id)
            .into_iter()
            .find_map(|item| match item.state {
                WorkItemState::Blocked { reason, .. } => Some(reason),
                _ => None,
            })
    })
    .await;
    assert!(reason.contains("aborted"), "{reason}");
    // The driver's last word on the roster comes before this frame: read
    // after it, the row says what stands.
    wait_for(&mut bus, "the execution to end", |e| {
        matches!(e.payload, EnginePayload::ExecutionEnded { .. })
    })
    .await;
    assert_eq!(state_of(&engine, &row), Some(SessionState::Aborted));
    assert_eq!(
        engine.inner().registry.get(row.id).map(|run| run.status),
        Some(AgentStatus::Aborted)
    );
    // Asked twice, it is the same answer; a row nobody has is said so.
    bisa_engine::sessions::stop_one(engine.inner(), row.id).unwrap();
    assert!(
        bisa_engine::sessions::stop_one(engine.inner(), bisa_engine::LiveRunId::mint()).is_err()
    );
    engine.shutdown().await;
}

/// A turn of a conversation terminated from the roster: the session is let
/// go of — not only its row ended — and the next message is answered by a
/// fresh one.
#[tokio::test(flavor = "multi_thread")]
async fn a_turn_stopped_by_its_row_is_aborted_then_let_go_of_and_the_next_message_starts_afresh() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::general(), "endless");
    let channel = ws
        .create_channel(
            "fleeting",
            None,
            RosterPolicy::default(),
            Default::default(),
        )
        .unwrap();
    let (adapter, closes) = at_work("endless");
    let launches = Arc::clone(&adapter.launches);
    let engine = Engine::start(ws, catalog_with(vec![adapter]), design_off_config()).unwrap();
    let say = |words: &str| {
        engine
            .workspace()
            .post_message(
                channel.id.as_str(),
                MessageBody::post(words),
                None,
                &[],
                &[],
                None,
                PostOrigin::Asked,
            )
            .unwrap();
    };
    say("anyone there?");
    let row = live_row(&engine, SessionKind::Conversation).await;

    bisa_engine::sessions::stop_one(engine.inner(), row.id).unwrap();

    // Aborted — the turn ended where it stood — then let go of.
    until("the session to be aborted, then let go of", || {
        (closed(&closes) == vec![Close::Aborted, Close::Disposed]).then_some(())
    })
    .await;
    assert_eq!(state_of(&engine, &row), Some(SessionState::Aborted));
    assert_eq!(
        engine.inner().registry.get(row.id).map(|run| run.status),
        Some(AgentStatus::Aborted),
        "aborted is terminal: nothing parks or revives it"
    );

    say("and now?");
    until("the next message to start a session of its own", || {
        (launches.lock().unwrap().len() == 2).then_some(())
    })
    .await;
    let next = live_row(&engine, SessionKind::Conversation).await;
    assert_ne!(next.id, row.id);
    engine.shutdown().await;
}

fn designing() -> EngineConfig {
    EngineConfig {
        design_enabled: true,
        ..design_off_config()
    }
}

fn guidance(engine: &Engine, goal: bisa_core::GoalId) -> Vec<(GuidanceStatus, Option<String>)> {
    engine
        .workspace()
        .journal(&bisa_core::Home::from(goal))
        .unwrap()
        .iter()
        .filter_map(|e| match &e.payload {
            JournalPayload::Guidance { status, detail, .. } => Some((*status, detail.clone())),
            _ => None,
        })
        .collect()
}

/// The Workflow Agent's wake terminated from the roster: the harness is told
/// to stop, the goal says the design failed and why, and the session is not
/// kept for a follow-up it could never take.
#[tokio::test(flavor = "multi_thread")]
async fn a_design_wake_stopped_by_its_row_is_told_to_stop_and_the_goal_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "hanging");
    let (adapter, closes) = at_work("hanging");
    let engine = Engine::start(ws, catalog_with(vec![adapter]), designing()).unwrap();
    let goal = engine
        .submit_goal(guided("design that is stopped"))
        .unwrap();
    let row = live_row(&engine, SessionKind::Guided).await;
    assert_eq!(row.goal, Some(goal.id));

    bisa_engine::sessions::stop_one(engine.inner(), row.id).unwrap();

    until("the harness to be told to stop", || {
        closed(&closes).contains(&Close::Aborted).then_some(())
    })
    .await;
    let detail = until("the goal to say its design failed", || {
        guidance(&engine, goal.id)
            .into_iter()
            .find_map(|(status, detail)| (status == GuidanceStatus::Failed).then_some(detail))
    })
    .await;
    assert!(
        detail.unwrap_or_default().contains("it was stopped"),
        "the words say what happened"
    );
    until("the session to be let go of", || {
        closed(&closes).contains(&Close::Disposed).then_some(())
    })
    .await;
    assert!(
        !engine.inner().lifecycle.is_live(row.id).await,
        "a stopped session is kept for no follow-up"
    );
    assert_eq!(state_of(&engine, &row), Some(SessionState::Aborted));
    engine.shutdown().await;
}

/// A goal stopped while the Workflow Agent is designing it: the wake's
/// harness is told to stop with the goal.
#[tokio::test(flavor = "multi_thread")]
async fn a_goal_stopped_while_it_is_designed_stops_the_workflow_agents_harness() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "hanging");
    let (adapter, closes) = at_work("hanging");
    let engine = Engine::start(ws, catalog_with(vec![adapter]), designing()).unwrap();
    let goal = engine.submit_goal(guided("a goal stopped early")).unwrap();
    let row = live_row(&engine, SessionKind::Guided).await;

    engine.stop_goal(goal.id, None).await.unwrap();

    until("the harness to be told to stop", || {
        closed(&closes).contains(&Close::Aborted).then_some(())
    })
    .await;
    assert_eq!(state_of(&engine, &row), Some(SessionState::Aborted));
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// What a row carries, and what a stop reaches
// ---------------------------------------------------------------------------

/// A worker whose turn stops on one permission and, once answered, ends.
fn asking_worker(id: &str) -> MockAdapter {
    MockAdapter {
        id: id.into(),
        input_request: Some(InputRequest::permission(
            "p1",
            "Bash",
            ToolTier::Exec,
            "fake-tool build",
            json!({ "command": "fake-tool build" }),
        )),
        script: Some(vec![
            SessionEvent::Progress(ProgressEvent::TurnEnded),
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::Completed,
                is_terminal: true,
            }),
        ]),
        ..Default::default()
    }
}

fn journal_of(engine: &Engine, goal: bisa_core::GoalId) -> Vec<JournalPayload> {
    engine
        .workspace()
        .journal(&bisa_core::Home::from(goal))
        .unwrap()
        .into_iter()
        .map(|e| e.payload)
        .collect()
}

/// A long-lived harness announces its child from its own task at launch,
/// before the driver listens: the announcement is still heard, and the pid
/// lands on the row and on the record a boot after a crash reads.
#[tokio::test(flavor = "multi_thread")]
async fn a_worker_whose_harness_announced_its_process_at_launch_has_its_pid_on_the_row_and_the_record(
) {
    let dir = tempfile::tempdir().unwrap();
    let (adapter, _closes) = at_work("mock");
    let adapter = MockAdapter {
        pid: Some(4242),
        ..adapter
    };
    let engine = engine_with(&dir, vec![adapter]);
    run_on(
        &engine,
        "announced",
        new_workflow("announced", vec![agent_step("work", "mock")]),
    );
    let row = until("the row to carry the pid", || {
        engine
            .inner()
            .presence
            .snapshot()
            .into_iter()
            .find(|r| r.kind == SessionKind::Worker && r.pid == Some(4242))
    })
    .await;
    let session_id = row
        .session_id
        .expect("a worker's row names its session")
        .to_string();
    // The row moves a moment before the record is written: read the record
    // as the boot would, once it says so.
    let record = until("the record to carry the pid a boot would end", || {
        engine
            .workspace()
            .session_by_id(&session_id)
            .unwrap()
            .filter(|r| r.pid == Some(4242))
    })
    .await;
    assert!(record.pid_seen_at.is_some());
    engine.shutdown().await;
}

/// A worker's permission question reaches the Inbox; a stop while it waits
/// takes the question back — a `withdrawn` fact under its subject, never a
/// guard fact in the person's name — the harness hears a refusal and then
/// the stop, and the row reads *aborted*.
#[tokio::test(flavor = "multi_thread")]
async fn a_worker_stopped_while_it_waits_on_a_permission_is_aborted_and_its_question_withdrawn_and_not_remembered(
) {
    let dir = tempfile::tempdir().unwrap();
    let adapter = asking_worker("mock");
    let closes = Arc::clone(&adapter.closes);
    let answered = Arc::clone(&adapter.answered);
    let engine = engine_with(&dir, vec![adapter]);
    let (goal, _) = run_on(
        &engine,
        "asks, then is stopped",
        new_workflow("asks", vec![agent_step("work", "mock")]),
    );
    let gate = until("the permission's gate", || {
        engine
            .inner()
            .gates
            .pending()
            .into_iter()
            .find(|g| g.subject == "permission:Bash")
    })
    .await;
    let row = live_row(&engine, SessionKind::Worker).await;
    assert_eq!(
        gate.session,
        Some(row.id),
        "the gate names the session that asked"
    );
    assert!(
        matches!(row.state, SessionState::Waiting { .. }),
        "{:?}",
        row.state
    );

    bisa_engine::sessions::stop_one(engine.inner(), row.id).unwrap();

    assert!(
        engine.inner().gates.get(&gate.id).is_none(),
        "the gate is gone, not decided"
    );
    until("the harness to be told to stop", || {
        closed(&closes).contains(&Close::Aborted).then_some(())
    })
    .await;
    let answers = answered.lock().unwrap().clone();
    assert!(
        answers.iter().any(|(_, a)| matches!(
            a,
            InputAnswer::Deny { reason } if reason == bisa_engine::inputs::WITHDRAWN
        )),
        "the harness heard the withdrawal: {answers:?}"
    );
    let journal = journal_of(&engine, goal.id);
    assert!(
        journal.iter().any(|p| matches!(
            p,
            JournalPayload::Withdrawn { subject, reason }
                if subject == "permission:Bash" && reason == bisa_engine::sessions::STOPPED
        )),
        "a withdrawn fact under the question's subject: {journal:?}"
    );
    assert!(
        !journal.iter().any(|p| matches!(
            p,
            JournalPayload::Guard {
                by: GuardJudge::Person,
                ..
            }
        )),
        "nothing was remembered as the person's answer: {journal:?}"
    );
    until("the item to settle", || {
        items_of(&engine, goal.id)
            .into_iter()
            .find(|i| matches!(i.state, WorkItemState::Blocked { .. }))
            .map(|_| ())
    })
    .await;
    assert_eq!(state_of(&engine, &row), Some(SessionState::Aborted));
    engine.shutdown().await;
}

/// A goal stopped while its worker waits on a permission: the step's cancel
/// reaches the question too — withdrawn, the driver hears it and the stop.
#[tokio::test(flavor = "multi_thread")]
async fn stopping_a_goal_releases_a_worker_waiting_on_a_permission() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = asking_worker("mock");
    let closes = Arc::clone(&adapter.closes);
    let engine = engine_with(&dir, vec![adapter]);
    let (goal, _) = run_on(
        &engine,
        "asks, then its goal is stopped",
        new_workflow("asks-stopped", vec![agent_step("work", "mock")]),
    );
    let gate = until("the permission's gate", || {
        engine
            .inner()
            .gates
            .pending()
            .into_iter()
            .find(|g| g.subject == "permission:Bash")
    })
    .await;
    let row = live_row(&engine, SessionKind::Worker).await;

    engine.stop_goal(goal.id, None).await.unwrap();

    assert!(
        engine.inner().gates.get(&gate.id).is_none(),
        "the gate went"
    );
    until("the harness to be told to stop", || {
        closed(&closes).contains(&Close::Aborted).then_some(())
    })
    .await;
    assert!(
        journal_of(&engine, goal.id).iter().any(|p| matches!(
            p,
            JournalPayload::Withdrawn { subject, .. } if subject == "permission:Bash"
        )),
        "the question is withdrawn in the journal"
    );
    until("the row to end", || {
        state_of(&engine, &row).filter(|s| s.is_ended()).map(|_| ())
    })
    .await;
    engine.shutdown().await;
}

/// Closing a goal stops what was running for it beyond its workers: the
/// turn in its thread is let go of, and its row says so.
#[tokio::test(flavor = "multi_thread")]
async fn closing_a_goal_aborts_the_turn_in_its_thread() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::general(), "endless");
    let goal = ws
        .create_goal(bisa_store::NewGoal::captured("ship the demo"))
        .unwrap();
    let (adapter, closes) = at_work("endless");
    let engine = Engine::start(ws, catalog_with(vec![adapter]), design_off_config()).unwrap();
    engine
        .workspace()
        .post_message(
            &goal.id.to_string(),
            MessageBody::post("how is it going?"),
            None,
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();
    let row = live_row(&engine, SessionKind::Conversation).await;
    assert_eq!(row.goal, Some(goal.id));
    assert!(
        matches!(&row.origin, SessionOrigin::Turn { scope, on_behalf_of: None } if scope == &goal.id.to_string()),
        "the turn's row names the thread it is a turn of: {:?}",
        row.origin
    );

    engine
        .close_goal(goal.id, ClosureReason::Abandoned { rationale: None })
        .unwrap();

    until("the turn to be aborted", || {
        closed(&closes).contains(&Close::Aborted).then_some(())
    })
    .await;
    assert_eq!(state_of(&engine, &row), Some(SessionState::Aborted));
    engine.shutdown().await;
}

/// A driver that panics after the row was registered — an adapter bug on
/// the first prompt — ends the row as failed and the record with it: no row
/// reads *starting* for good with nothing behind it.
#[tokio::test(flavor = "multi_thread")]
async fn a_panicking_driver_ends_its_row() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = MockAdapter {
        id: "mock".into(),
        panic_on_prompt: true,
        ..Default::default()
    };
    let engine = engine_with(&dir, vec![adapter]);
    let (goal, _) = run_on(
        &engine,
        "panics",
        new_workflow("panics", vec![agent_step("work", "mock")]),
    );
    let row = until("the row to end as its driver went", || {
        engine
            .inner()
            .presence
            .snapshot()
            .into_iter()
            .find(|r| {
                r.kind == SessionKind::Worker
                    && matches!(&r.state, SessionState::Failed { reason } if reason == bisa_engine::sessions::DRIVER_GONE)
            })
    })
    .await;
    let record = engine
        .workspace()
        .session_by_id(&row.session_id.unwrap().to_string())
        .unwrap()
        .unwrap();
    assert_eq!(record.status, bisa_store::SessionStatus::Ended);
    let run = finished_run(&engine, goal.id).await;
    assert_eq!(run.outcome, Some(bisa_core::RunOutcome::Failed));
    engine.shutdown().await;
}

/// A turn whose harness refuses its first prompt: the wake returns early,
/// and the row it registered ends as failed rather than reading *starting*.
#[tokio::test(flavor = "multi_thread")]
async fn a_turn_whose_prompt_is_refused_leaves_no_row_in_starting() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::general(), "refusing");
    let channel = ws
        .create_channel("quiet", None, RosterPolicy::default(), Default::default())
        .unwrap();
    let adapter = MockAdapter {
        id: "refusing".into(),
        refuse_prompt: true,
        ..Default::default()
    };
    let engine = Engine::start(ws, catalog_with(vec![adapter]), design_off_config()).unwrap();
    engine
        .workspace()
        .post_message(
            channel.id.as_str(),
            MessageBody::post("anyone there?"),
            None,
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();
    let row = until("the turn's row to end", || {
        engine
            .inner()
            .presence
            .snapshot()
            .into_iter()
            .find(|r| r.kind == SessionKind::Conversation && r.state.is_ended())
    })
    .await;
    assert!(
        matches!(&row.state, SessionState::Failed { reason } if reason == bisa_engine::sessions::DRIVER_GONE),
        "{:?}",
        row.state
    );
    let session_id = row.session_id.unwrap().to_string();
    until("the record to be ended", || {
        engine
            .workspace()
            .session_by_id(&session_id)
            .unwrap()
            .filter(|r| r.status == bisa_store::SessionStatus::Ended)
            .map(|_| ())
    })
    .await;
    engine.shutdown().await;
}

/// The executing set is what is executing now: an item that settled is not
/// in it.
#[tokio::test(flavor = "multi_thread")]
async fn the_executing_set_is_empty_once_an_item_settled() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({ "ok": true }))]);
    let (goal, _) = run_on(
        &engine,
        "settles",
        new_workflow("settles", vec![agent_step("work", "mock")]),
    );
    finished_run(&engine, goal.id).await;
    until("the executing set to be empty", || {
        engine.inner().active_items.is_empty().then_some(())
    })
    .await;
    engine.shutdown().await;
}
