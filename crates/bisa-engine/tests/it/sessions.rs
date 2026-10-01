//! Stopping one session by its row reaches the harness that runs it.
//!
//! A person's *Terminate* in the roster, `bisa sessions abort` and a goal's
//! stop all end a row — and the harness behind it, whatever drives it: a
//! worker on a step, an agent's turn in a conversation, the Workflow Agent's
//! design wake. A row that reads *aborted* beside a harness still at work is
//! the fault these tests hold off: each reads what the harness itself was
//! told, never the roster alone.

use crate::common;

use bisa_core::event::JournalPayload;
use bisa_core::{AgentId, GuidanceStatus, MessageBody, RosterPolicy, WorkItemState};
use bisa_engine::registry::SessionKind;
use bisa_engine::{
    AgentStatus, Engine, EngineConfig, EnginePayload, SessionPresence, SessionState,
};
use bisa_harness::mock::{Close, MockAdapter};
use bisa_store::PostOrigin;
use common::*;
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
async fn a_turn_stopped_by_its_row_lets_go_of_its_session_and_the_next_message_starts_afresh() {
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

    until("the session to be let go of", || {
        closed(&closes).contains(&Close::Disposed).then_some(())
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
