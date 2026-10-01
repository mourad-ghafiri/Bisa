//! Session presence, end to end: what `GET /sessions` and the `session_state`
//! frames say while a worker runs, waits on a person, spawns a sub-agent and
//! finishes — and that nothing is said per token.

use crate::common;

use bisa_core::{Gate, ToolTier};
use bisa_engine::{Engine, EngineConfig, EnginePayload, ExecutionOutcome, SessionState, WaitingOn};
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
            }),
            SessionEvent::Progress(ProgressEvent::TextDelta { text: "a".into() }),
            SessionEvent::Progress(ProgressEvent::TextDelta { text: "b".into() }),
            SessionEvent::Progress(ProgressEvent::TextDelta { text: "c".into() }),
            SessionEvent::Progress(ProgressEvent::ToolEnded {
                name: "Read".into(),
                ok: true,
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
