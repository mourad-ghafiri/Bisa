//! Session presence, end to end: what `GET /sessions` and the `session_state`
//! frames say while a worker runs, waits on a person, spawns a sub-agent and
//! finishes — and that nothing is said per token.

use crate::common;

use bisa_core::{Gate, GuidancePhase, SessionOrigin, ToolTier};
use bisa_engine::presence::SessionMeta;
use bisa_engine::registry::SessionKind;
use bisa_engine::{
    Engine, EngineConfig, EnginePayload, ExecutionOutcome, LiveRunId, SessionState, WaitingOn,
};
use bisa_harness::mock::{subagent_script, MockAdapter};
use bisa_harness::{
    InputAnswer, InputRequest, LifecycleEvent, Outcome, ProgressEvent, SessionEvent,
};
use common::*;
use std::time::Duration;

/// An engine whose finished sessions leave the roster after one second.
fn engine_retaining_briefly(dir: &tempfile::TempDir, adapters: Vec<MockAdapter>) -> Engine {
    Engine::start(
        workspace(dir),
        catalog_with(adapters),
        EngineConfig {
            retain_ended_secs: 1,
            ..design_off_config()
        },
    )
    .unwrap()
}

fn state_words(events: &[bisa_engine::EngineEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match &e.payload {
            EnginePayload::SessionState { presence, .. } => {
                Some(presence.state.as_str().to_string())
            }
            _ => None,
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_worker_reports_its_states_never_a_token_and_leaves_after_retention() {
    let dir = tempfile::tempdir().unwrap();
    let scripted = MockAdapter {
        script: Some(vec![
            SessionEvent::Lifecycle(LifecycleEvent::Started),
            SessionEvent::Progress(ProgressEvent::TurnStarted),
            SessionEvent::Progress(ProgressEvent::ToolStarted {
                name: "Read".into(),
                args_summary: "README.md".into(),
                tier: ToolTier::Read,
                id: None,
            }),
            SessionEvent::Progress(ProgressEvent::TextDelta { text: "a".into() }),
            SessionEvent::Progress(ProgressEvent::TextDelta { text: "b".into() }),
            SessionEvent::Progress(ProgressEvent::TextDelta { text: "c".into() }),
            SessionEvent::Progress(ProgressEvent::ToolEnded {
                name: "Read".into(),
                ok: true,
                id: None,
            }),
            SessionEvent::Progress(ProgressEvent::TurnEnded),
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::Completed,
                is_terminal: true,
            }),
        ]),
        ..Default::default()
    };
    let engine = engine_retaining_briefly(&dir, vec![scripted]);
    let mut rx = engine.events();
    run_on(
        &engine,
        "watched",
        new_workflow("watched", vec![agent_step("run", "mock")]),
    );

    let mut seen = Vec::new();
    let gone = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    let gone = matches!(ev.payload, EnginePayload::SessionGone { .. });
                    seen.push(ev);
                    if gone {
                        return true;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return false,
            }
        }
    })
    .await
    .expect("the session leaves the roster within the retention window");
    assert!(gone);

    let words = state_words(&seen);
    assert_eq!(words[0], "starting", "registered before the harness spoke");
    assert!(words.contains(&"idle".into()) && words.contains(&"thinking".into()));
    assert!(words.contains(&"running".into()), "{words:?}");
    let ended = words.last().unwrap();
    assert_eq!(
        ended, "failed",
        "completed without a result is a failed item: {words:?}"
    );
    // Three text deltas produced no frame: every frame is a state change.
    let text_frames = seen
        .iter()
        .filter(|e| {
            matches!(
                &e.payload,
                EnginePayload::Session {
                    event: SessionEvent::Progress(ProgressEvent::TextDelta { .. })
                }
            )
        })
        .count();
    assert_eq!(text_frames, 3, "the raw session stream still carries them");
    let running = seen.iter().find_map(|e| match &e.payload {
        EnginePayload::SessionState { presence, .. }
            if matches!(&presence.state, SessionState::Running { .. }) =>
        {
            Some(presence.clone())
        }
        _ => None,
    });
    let running = running.expect("a running frame");
    assert!(matches!(&running.state, SessionState::Running { tool, .. } if tool == "Read"));
    assert_eq!(running.harness, "mock");
    assert!(running.work_item.is_some() && running.goal.is_some());
    assert!(
        running.workstream.is_none(),
        "a step with no project runs in the goal's scratch: no workstream"
    );
    assert!(
        engine.inner().presence.snapshot().is_empty(),
        "gone from the roster too"
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_permission_above_the_ceiling_waits_before_its_gate_opens_and_resumes_on_the_decision() {
    let dir = tempfile::tempdir().unwrap();
    let scripted = MockAdapter {
        input_request: Some(InputRequest::permission(
            "p1",
            "bash",
            ToolTier::Exec,
            "cargo test",
            serde_json::json!({ "command": "cargo test" }),
        )),
        script: Some(vec![
            SessionEvent::Progress(ProgressEvent::TurnEnded),
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::Completed,
                is_terminal: true,
            }),
        ]),
        ..Default::default()
    };
    let answered = std::sync::Arc::clone(&scripted.answered);
    let engine = engine_with(&dir, vec![scripted]);
    let mut rx = engine.events();
    run_on(
        &engine,
        "asking",
        new_workflow("asking", vec![agent_step("run", "mock")]),
    );

    let mut seen = Vec::new();
    let opened = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    let opened = matches!(
                        &ev.payload,
                        EnginePayload::GateOpened {
                            gate: Gate::Escalation,
                            ..
                        }
                    );
                    seen.push(ev);
                    if opened {
                        return;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(e) => panic!("bus closed: {e}"),
            }
        }
    })
    .await;
    opened.expect("the gate opens");
    let EnginePayload::GateOpened { gate_id, .. } = &seen.last().unwrap().payload else {
        unreachable!()
    };
    // The row read `waiting` on this very gate *before* the gate frame.
    let waiting = seen.iter().position(|e| matches!(&e.payload,
        EnginePayload::SessionState { presence, .. }
            if matches!(&presence.state, SessionState::Waiting { on: WaitingOn::Permission { tool, gate_id: Some(g) } } if tool == "bash" && g == gate_id)));
    assert!(
        waiting.is_some(),
        "a waiting frame naming the gate: {:?}",
        state_words(&seen)
    );
    assert!(waiting.unwrap() < seen.len() - 1);
    assert!(matches!(
        engine
            .inner()
            .presence
            .get(engine.inner().presence.by_gate(gate_id).unwrap())
            .unwrap()
            .state,
        SessionState::Waiting { .. }
    ));

    engine.decide(gate_id, true, None, None, None).unwrap();
    wait_for(&mut rx, "the session resumes", |e| {
        matches!(&e.payload, EnginePayload::SessionState { presence, .. } if !matches!(presence.state, SessionState::Waiting { .. }))
    })
    .await;
    wait_for(&mut rx, "the item ends", |e| {
        matches!(
            &e.payload,
            EnginePayload::ExecutionEnded {
                outcome: ExecutionOutcome::Completed
            }
        )
    })
    .await;
    assert!(
        matches!(answered.lock().unwrap().as_slice(), [(id, InputAnswer::Allow { .. })] if id == "p1")
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sub_agent_nests_under_its_parent_and_the_parent_delegates_until_it_leaves() {
    let dir = tempfile::tempdir().unwrap();
    let scripted = MockAdapter {
        script: Some(subagent_script("t1", "explore")),
        ..Default::default()
    };
    let engine = engine_with(&dir, vec![scripted]);
    let mut rx = engine.events();
    run_on(
        &engine,
        "nested",
        new_workflow("nested", vec![agent_step("run", "mock")]),
    );

    let with_child = wait_for(&mut rx, "a frame with the sub-agent running", |e| {
        matches!(&e.payload, EnginePayload::SessionState { presence, .. }
            if presence.children.iter().any(|c| c.name == "explore" && matches!(c.state, SessionState::Running { .. })))
    })
    .await;
    let EnginePayload::SessionState { presence, .. } = with_child.payload else {
        unreachable!()
    };
    assert!(
        matches!(&presence.state, SessionState::Running { tool, args, .. } if tool == "sub-agent" && args == "explore"),
        "the parent is not running the child's tool: it is delegating — {:?}",
        presence.state
    );
    assert_eq!(presence.children.len(), 1);
    wait_for(&mut rx, "the sub-agent left", |e| {
        matches!(&e.payload, EnginePayload::SessionState { presence, .. }
            if presence.children.is_empty())
    })
    .await;
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// Why a row exists, and where it stands
// ---------------------------------------------------------------------------

/// A row that stands for nothing but the test — a design wake's shape.
fn design_meta(phase: GuidancePhase) -> SessionMeta {
    SessionMeta {
        kind: SessionKind::Guided,
        origin: SessionOrigin::Design { phase },
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
        cwd: Some("/tmp/scratch".into()),
        transcript_path: None,
    }
}

/// A worker's row says which step it is for — by id and by name — that it
/// was not resumed, its goal and its run, and the folder it stands in; the
/// wire spells each the same way.
#[tokio::test(flavor = "multi_thread")]
async fn a_workers_row_says_its_step_its_goal_and_its_folder() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = MockAdapter {
        id: "mock".into(),
        script: Some(vec![]),
        ..Default::default()
    };
    let engine = engine_with(&dir, vec![adapter]);
    let (goal, run) = run_on(
        &engine,
        "placed",
        new_workflow("placed", vec![agent_step("build-it", "mock")]),
    );
    let row = until("the worker's row", || {
        engine
            .inner()
            .presence
            .snapshot()
            .into_iter()
            .find(|r| r.kind == SessionKind::Worker && r.state.is_live())
    })
    .await;
    let named = run.workflow.step(&sid("build-it")).unwrap().name.clone();
    assert_eq!(
        row.origin,
        SessionOrigin::Step {
            step: Some(sid("build-it")),
            name: Some(named.clone()),
            resumed: false,
        }
    );
    assert_eq!(row.goal, Some(goal.id));
    assert_eq!(row.run, Some(run.id));
    let cwd = row.cwd.clone().expect("a worker stands somewhere");
    assert!(std::path::Path::new(&cwd).is_absolute(), "{cwd}");
    let wire = serde_json::to_value(&row).unwrap();
    assert_eq!(wire["origin"]["origin"], serde_json::json!("step"));
    assert_eq!(wire["origin"]["step"], serde_json::json!("build-it"));
    assert_eq!(wire["origin"]["name"], serde_json::json!(named));
    assert_eq!(wire["origin"]["resumed"], serde_json::json!(false));
    assert_eq!(wire["cwd"], serde_json::json!(cwd));
    engine.shutdown().await;
}

/// A row's origin said again with a new fact bumps its revision and goes
/// out as a frame; the same fact twice says nothing.
#[tokio::test(flavor = "multi_thread")]
async fn an_origin_re_said_bumps_the_rows_revision_and_the_same_one_says_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let inner = engine.inner();
    let mut rx = engine.events();
    let id = LiveRunId::mint();
    inner
        .presence
        .register(inner, id, design_meta(GuidancePhase::Design));
    let before = inner.presence.get(id).unwrap().revision;

    inner.presence.origin(
        inner,
        id,
        SessionOrigin::Design {
            phase: GuidancePhase::Repair,
        },
    );
    let said = wait_for(&mut rx, "the row said again", |e| {
        matches!(&e.payload, EnginePayload::SessionState { presence, .. } if presence.id == id && presence.revision > before)
    })
    .await;
    let EnginePayload::SessionState { presence, .. } = said.payload else {
        unreachable!()
    };
    assert_eq!(
        presence.origin,
        SessionOrigin::Design {
            phase: GuidancePhase::Repair
        }
    );
    assert_eq!(presence.revision, before + 1);

    inner.presence.origin(
        inner,
        id,
        SessionOrigin::Design {
            phase: GuidancePhase::Repair,
        },
    );
    assert_eq!(
        inner.presence.get(id).unwrap().revision,
        before + 1,
        "the same fact twice is no change"
    );
    engine.shutdown().await;
}

/// A parked row loses its pid — the process went with the park — and leaves
/// the roster after the retention window, like an ended one.
#[tokio::test(flavor = "multi_thread")]
async fn a_parked_row_loses_its_pid_and_leaves_after_retention() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_retaining_briefly(&dir, vec![MockAdapter::default()]);
    let inner = engine.inner();
    let mut rx = engine.events();
    let id = LiveRunId::mint();
    inner
        .presence
        .register(inner, id, design_meta(GuidancePhase::Design));
    inner.presence.apply(
        inner,
        id,
        &SessionEvent::Lifecycle(LifecycleEvent::ProcessStarted { pid: Some(31337) }),
    );
    assert_eq!(inner.presence.get(id).unwrap().pid, Some(31337));

    inner.presence.parked(inner, id);

    let row = inner.presence.get(id).unwrap();
    assert_eq!(row.state, SessionState::Parked);
    assert_eq!(row.pid, None, "the process went with the park");
    wait_for(
        &mut rx,
        "the parked row to leave",
        |e| matches!(&e.payload, EnginePayload::SessionGone { live_run } if *live_run == id),
    )
    .await;
    assert!(inner.presence.get(id).is_none());
    engine.shutdown().await;
}
