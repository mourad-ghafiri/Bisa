//! Engine integration tests, driven end-to-end against the mock harness: the
//! executor's launch walk, budgets, permissions, the intake socket, gates
//! and their durable mirror, the guided hook.

use crate::common;

use bisa_core::event::JournalPayload;
use bisa_core::{
    AgentId, Answer, AskKind, Gate, GoalId, GoalOrigin, GoalStatus, Home, RunOutcome, StepKind,
    StepState, ToolTier, WorkItemState,
};
use bisa_engine::{
    Engine, EngineConfig, EnginePayload, ExecutionOutcome, LiveRunId, ScheduleRejection,
    SubmitRequest,
};
use bisa_harness::mock::MockAdapter;
use bisa_harness::{
    InputAnswer, InputRequest, LifecycleEvent, Outcome, ProgressEvent, SessionEvent,
};
use common::*;
use serde_json::json;
use std::collections::BTreeMap;
use std::time::Duration;

fn ulid() -> ulid::Ulid {
    ulid::Ulid::from_datetime(std::time::SystemTime::now())
}

#[tokio::test(flavor = "multi_thread")]
async fn fallback_chain_on_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let broken = MockAdapter {
        id: "mock-a".into(),
        available: false,
        ..Default::default()
    };
    let working = yielding("mock-b", json!({"ok": true}));
    let engine = engine_with(&dir, vec![broken, working]);
    let mut rx = engine.events();

    let mut s = agent_step("build", "mock-a");
    if let StepKind::Agent { harness, .. } = &mut s.kind {
        harness.push("mock-b".into());
    }
    let (goal, _) = run_on(&engine, "fallback test", new_workflow("fallback", vec![s]));
    let ev = wait_for(&mut rx, "scheduled", |e| {
        matches!(e.payload, EnginePayload::Scheduled { .. })
    })
    .await;
    let EnginePayload::Scheduled { harness } = ev.payload else {
        unreachable!()
    };
    assert_eq!(harness, "mock-b");
    assert_eq!(
        finished_run(&engine, goal.id).await.outcome,
        Some(RunOutcome::Done)
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn budget_exhaustion_aborts_mid_run_and_fails_the_step() {
    let dir = tempfile::tempdir().unwrap();
    let scripted = MockAdapter {
        script: Some(vec![
            SessionEvent::Lifecycle(LifecycleEvent::Started),
            SessionEvent::Progress(ProgressEvent::CostDelta {
                input_tokens: 150,
                output_tokens: 100,
                usd_cents: 0,
            }),
            // No terminal end: the engine must abort on budget.
        ]),
        ..Default::default()
    };
    let engine = engine_with(&dir, vec![scripted]);
    let mut rx = engine.events();
    let wf = engine
        .create_workflow(new_workflow("expensive", vec![agent_step("spend", "mock")]))
        .unwrap();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            workflow: Some(wf.id),
            budget: Some(bisa_core::Budget {
                max_tokens: Some(100),
                ..Default::default()
            }),
            ..guided("expensive")
        })
        .unwrap();
    engine.start_run(goal.id, BTreeMap::new()).unwrap();

    wait_for(&mut rx, "budget abort", |e| {
        matches!(
            &e.payload,
            EnginePayload::ExecutionEnded {
                outcome: ExecutionOutcome::BudgetExhausted
            }
        )
    })
    .await;
    let failed = finished_run(&engine, goal.id).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed));
    assert!(failed.steps[&sid("spend")]
        .error
        .as_deref()
        .unwrap()
        .contains("budget"));
    let items = items_of(&engine, goal.id);
    assert!(matches!(items[0].state, WorkItemState::Blocked { .. }));
    engine.shutdown().await;
}

/// A mock whose turn stops on a permission request above the step's ceiling
/// (Write), then — once answered — ends its turn and the session.
fn asking_mock(tool: &str) -> MockAdapter {
    MockAdapter {
        input_request: Some(InputRequest::permission(
            "p1",
            tool,
            ToolTier::Exec,
            "cargo build",
            serde_json::json!({ "command": "cargo build" }),
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

#[tokio::test(flavor = "multi_thread")]
async fn permission_above_ceiling_refused_answers_deny_and_the_session_goes_on() {
    let dir = tempfile::tempdir().unwrap();
    let scripted = asking_mock("bash");
    let answered = std::sync::Arc::clone(&scripted.answered);
    let engine = engine_with(&dir, vec![scripted]);
    let mut rx = engine.events();
    // The step's ceiling is Write, so an Exec request must gate.
    let (goal, _) = run_on(
        &engine,
        "guarded",
        new_workflow("guarded", vec![agent_step("run", "mock")]),
    );

    let opened = wait_for(&mut rx, "escalation gate", |e| {
        matches!(
            &e.payload,
            EnginePayload::GateOpened {
                gate: Gate::Escalation,
                ..
            }
        )
    })
    .await;
    let EnginePayload::GateOpened { gate_id, .. } = opened.payload else {
        unreachable!()
    };
    assert!(
        engine.gate(&gate_id).unwrap().step.is_none(),
        "a permission is not a step's gate"
    );
    engine
        .decide(&gate_id, false, Some("no"), None, None)
        .unwrap();

    // A refusal is an answer, not an abort: the harness hears `deny` and the
    // turn ends on its own — without a result, so the step fails honestly.
    wait_for(&mut rx, "the item ends", |e| {
        matches!(&e.payload, EnginePayload::ExecutionEnded { .. })
    })
    .await;
    let answers = answered.lock().unwrap().clone();
    assert_eq!(answers.len(), 1);
    assert_eq!(answers[0].0, "p1");
    assert!(matches!(answers[0].1, InputAnswer::Deny { .. }));
    let run = finished_run(&engine, goal.id).await;
    assert_eq!(run.outcome, Some(RunOutcome::Failed));
    assert!(run.steps[&sid("run")]
        .error
        .as_deref()
        .unwrap()
        .contains("without yielding"));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn permission_approved_answers_allow_and_completes() {
    let dir = tempfile::tempdir().unwrap();
    let scripted = asking_mock("bash");
    let answered = std::sync::Arc::clone(&scripted.answered);
    let engine = engine_with(&dir, vec![scripted]);
    let mut rx = engine.events();
    let (goal, _) = run_on(
        &engine,
        "approved",
        new_workflow("approved", vec![agent_step("run", "mock")]),
    );

    let opened = wait_for(&mut rx, "escalation gate", |e| {
        matches!(
            &e.payload,
            EnginePayload::GateOpened {
                gate: Gate::Escalation,
                ..
            }
        )
    })
    .await;
    let EnginePayload::GateOpened { gate_id, .. } = opened.payload else {
        unreachable!()
    };
    engine.decide(&gate_id, true, None, None, None).unwrap();

    wait_for(&mut rx, "completed", |e| {
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
    // Completed without a result is still a failed step: the work is judged
    // by what it yields, not by the session ending politely.
    let run = finished_run(&engine, goal.id).await;
    assert_eq!(run.outcome, Some(RunOutcome::Failed));
    assert!(run.steps[&sid("run")]
        .error
        .as_deref()
        .unwrap()
        .contains("without yielding"));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn intake_schema_rejects_then_accepts() {
    let dir = tempfile::tempdir().unwrap();
    // A session that stays open so the test can submit results by hand.
    let engine = engine_with(
        &dir,
        vec![MockAdapter {
            script: Some(vec![SessionEvent::Lifecycle(LifecycleEvent::Started)]),
            ..Default::default()
        }],
    );
    let mut rx = engine.events();
    let mut s = agent_step("answer", "mock");
    if let StepKind::Agent { output_schema, .. } = &mut s.kind {
        *output_schema = Some(json!({
            "type": "object",
            "properties": {"answer": {"type": "string"}},
            "required": ["answer"]
        }));
    }
    let (goal, _) = run_on(&engine, "structured", new_workflow("structured", vec![s]));
    wait_for(&mut rx, "scheduled", |e| {
        matches!(e.payload, EnginePayload::Scheduled { .. })
    })
    .await;
    let item = until("the item to be in progress", || {
        items_of(&engine, goal.id)
            .into_iter()
            .find(|i| matches!(i.state, WorkItemState::InProgress { .. }))
    })
    .await;

    // Wrong shape -> rejected with attempts accounting.
    let bad = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "result_submit", "work_item": item.id.to_string(), "output": {"answer": 42}}),
    )
    .await;
    assert_eq!(bad["ok"], false);
    assert_eq!(bad["attempts_left"], 2);
    assert!(!bad["errors"].as_array().unwrap().is_empty());

    // Right shape -> accepted, and the step is done with it.
    let good = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "result_submit", "work_item": item.id.to_string(), "output": {"answer": "42"}}),
    )
    .await;
    assert_eq!(good["ok"], true, "reply: {good}");
    wait_for(&mut rx, "accepted", |e| {
        matches!(e.payload, EnginePayload::ResultAccepted)
    })
    .await;
    let run = finished_run(&engine, goal.id).await;
    assert_eq!(run.outcome, Some(RunOutcome::Done));
    assert_eq!(
        run.steps[&sid("answer")].output,
        Some(json!({"answer": "42"}))
    );
    engine.shutdown().await;
}

/// The last refused result fails the step there and then, with the schema's
/// own words — not later, when the session gives up, with a vaguer reason.
#[tokio::test(flavor = "multi_thread")]
async fn intake_schema_exhausts_then_fails_the_step() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(
        &dir,
        vec![MockAdapter {
            script: Some(vec![SessionEvent::Lifecycle(LifecycleEvent::Started)]),
            ..Default::default()
        }],
    );
    let mut rx = engine.events();
    let mut s = agent_step("answer", "mock");
    if let StepKind::Agent { output_schema, .. } = &mut s.kind {
        *output_schema = Some(json!({
            "type": "object",
            "properties": {"answer": {"type": "string"}},
            "required": ["answer"]
        }));
    }
    let (goal, _) = run_on(&engine, "structured", new_workflow("structured", vec![s]));
    wait_for(&mut rx, "scheduled", |e| {
        matches!(e.payload, EnginePayload::Scheduled { .. })
    })
    .await;
    let item = until("the item to be in progress", || {
        items_of(&engine, goal.id)
            .into_iter()
            .find(|i| matches!(i.state, WorkItemState::InProgress { .. }))
    })
    .await;

    let submit = |output: serde_json::Value| {
        intake_roundtrip(
            engine.socket_path(),
            json!({"op": "result_submit", "work_item": item.id.to_string(), "output": output}),
        )
    };
    for left in [2, 1] {
        let reply = submit(json!({"answer": 42})).await;
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["attempts_left"], left);
        assert!(reply.get("failed").is_none(), "{reply}");
    }
    let last = submit(json!({"answer": 42})).await;
    assert_eq!(last["ok"], false);
    assert_eq!(last["attempts_left"], 0);
    assert_eq!(last["failed"], true, "{last}");
    assert!(!last["errors"].as_array().unwrap().is_empty());

    let run = finished_run(&engine, goal.id).await;
    assert_eq!(run.outcome, Some(RunOutcome::Failed));
    let why = run.steps[&sid("answer")].error.clone().unwrap_or_default();
    assert!(why.contains("output schema"), "{why}");
    assert!(why.contains("3 attempts"), "{why}");
    assert!(run.steps[&sid("answer")].output.is_none());
    let blocked = items_of(&engine, goal.id)
        .into_iter()
        .find(|i| i.id == item.id)
        .unwrap();
    let WorkItemState::Blocked { reason, .. } = &blocked.state else {
        panic!(
            "expected the item blocked with the schema's reason, got {:?}",
            blocked.state
        );
    };
    assert!(reason.contains("output schema"), "{reason}");
    assert!(!reason.contains("session aborted"), "{reason}");

    // A conforming answer after the fact moves nothing: the item is settled.
    let late = submit(json!({"answer": "42"})).await;
    assert_eq!(late["ok"], false, "{late}");
    wait_for(&mut rx, "the session to end", |e| {
        matches!(e.payload, EnginePayload::ExecutionEnded { .. })
    })
    .await;
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn pause_gate_holds_scheduling() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({}))]);
    let mut rx = engine.events();
    engine.pause();
    let (goal, _) = run_on(
        &engine,
        "paused work",
        new_workflow("paused", vec![agent_step("go", "mock")]),
    );
    // Nothing schedules while paused.
    let raced = tokio::time::timeout(Duration::from_millis(300), async {
        loop {
            if let Ok(ev) = rx.recv().await {
                if matches!(ev.payload, EnginePayload::Scheduled { .. }) {
                    return;
                }
            }
        }
    })
    .await;
    assert!(raced.is_err(), "scheduled while paused");
    assert_eq!(
        current_run(&engine, goal.id).steps[&sid("go")].state,
        StepState::Running
    );

    engine.resume();
    wait_for(&mut rx, "scheduled after resume", |e| {
        matches!(e.payload, EnginePayload::Scheduled { .. })
    })
    .await;
    finished_run(&engine, goal.id).await;
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn preflight_rejections_are_typed() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    let config = design_off_config();
    let (goal, wf) = goal_on(
        &engine,
        "not yet running",
        new_workflow("w", vec![agent_step("go", "mock")]),
    );
    let item = |run, step| bisa_core::WorkItemSpec {
        id: bisa_core::WorkItemId::from_ulid(ulid()),
        home: bisa_core::Home::Goal { goal: goal.id },
        run,
        step,
        instructions: "x".into(),
        state: WorkItemState::Open,
        project: None,
        harness_candidates: vec!["mock".into()],
        model: None,
        effort: None,
        output_schema: None,
        budget: Default::default(),
        assignees: vec![],
        tier_ceiling: ToolTier::Write,
        agent: None,
        spawn_allowlist: vec![],
        depth_budget: 0,
        result_attempts: 0,
        interruptions: 0,
    };

    // An item bound to no step: nothing schedules work a workflow did not ask for.
    assert_eq!(
        bisa_engine::scheduler::preflight(
            ws,
            &config,
            &item(None, None),
            bisa_engine::scheduler::Launch::Fresh
        ),
        Err(ScheduleRejection::NoStep)
    );
    // A step that is not running this item.
    let run = engine.start_run(goal.id, BTreeMap::new()).unwrap();
    let stray = item(Some(run.id), Some(sid("go")));
    assert!(matches!(
        bisa_engine::scheduler::preflight(
            ws,
            &config,
            &stray,
            bisa_engine::scheduler::Launch::Fresh
        ),
        Err(ScheduleRejection::StepNotRunning { .. })
    ));
    let _ = wf;
    finished_run(&engine, goal.id).await;
    // A closed goal.
    engine
        .close_goal(
            goal.id,
            bisa_core::ClosureReason::Abandoned { rationale: None },
        )
        .unwrap();
    assert!(matches!(
        bisa_engine::scheduler::preflight(
            ws,
            &config,
            &stray,
            bisa_engine::scheduler::Launch::Fresh
        ),
        Err(ScheduleRejection::GoalClosed(_))
    ));

    // Spawn policy: depth exhausted, then allowlist.
    let mut parent = item(None, None);
    parent.depth_budget = 0;
    let child = item(None, None);
    assert_eq!(
        bisa_engine::scheduler::preflight_spawn(ws, &config, &parent, "scout", &child),
        Err(ScheduleRejection::DepthExhausted)
    );
    parent.depth_budget = 1;
    parent.spawn_allowlist = vec!["reviewer".into()];
    assert!(matches!(
        bisa_engine::scheduler::preflight_spawn(ws, &config, &parent, "scout", &child),
        Err(ScheduleRejection::SpawnNotAllowed { .. })
    ));

    // Disabled harness.
    let cfg2 = EngineConfig {
        disabled_harnesses: vec!["mock".into()],
        ..design_off_config()
    };
    assert!(matches!(
        bisa_engine::scheduler::preflight(ws, &cfg2, &stray, bisa_engine::scheduler::Launch::Fresh),
        Err(ScheduleRejection::HarnessDisabled(_))
    ));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn park_then_nothing_revives_and_tombstone() {
    use bisa_engine::registry::{AgentRef, AgentStatus, SessionKind};
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let inner = engine.inner();

    // Launch a session by hand and adopt it with a tiny TTL.
    let adapter = inner.catalog.get("mock").unwrap();
    let spec = bisa_harness::SessionSpec {
        skills: vec![],
        work_item: None,
        cwd: dir.path().to_path_buf(),
        prompt: "hi".into(),
        model: None,
        effort: None,
        mcp_servers: vec![],
        env: Default::default(),
        env_remove: Vec::new(),
        tier_ceiling: ToolTier::Read,
        output_schema: None,
    };
    let session = adapter.launch(spec).await.unwrap();
    let token = session.resume_token().unwrap();
    let transcript = dir.path().join("transcript.jsonl");
    std::fs::write(&transcript, "x").unwrap();

    let agent_id = LiveRunId::mint();
    inner
        .registry
        .register_if(
            AgentRef {
                id: agent_id,
                kind: SessionKind::Worker,
                status: AgentStatus::Idle,
                generation: 1,
                session_id: None,
                work_item: None,
                conversation: None,
                goal: None,
                workstream: None,
                transcript_path: Some(transcript.clone()),
                last_activity: 0,
            },
            None,
        )
        .unwrap();
    inner
        .ws
        .record_session(&bisa_store::SessionRow {
            id: "sess-1".into(),
            adapter: "mock".into(),
            kind: SessionKind::Worker,
            conversation: None,
            work_item: None,
            agent_id: None,
            transcript_path: Some(transcript.display().to_string()),
            resume_token_json: Some(serde_json::to_string(&token).unwrap()),
            workstream: None,
            status: bisa_store::SessionStatus::Live,
            parked_at: None,
            pid: None,
            pid_seen_at: None,
            ended_at: None,
        })
        .unwrap();

    inner.lifecycle.adopt(
        inner,
        agent_id,
        session,
        "sess-1".into(),
        Duration::from_millis(50),
    );
    assert!(inner.lifecycle.is_live(agent_id).await);

    // TTL parks it.
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            tokio::time::sleep(Duration::from_millis(25)).await;
            if inner.registry.get(agent_id).unwrap().status == AgentStatus::Parked {
                break;
            }
        }
    })
    .await
    .expect("agent never parked");
    assert!(!inner.lifecycle.is_live(agent_id).await);
    assert_eq!(
        inner.ws.session_by_id("sess-1").unwrap().unwrap().status,
        bisa_store::SessionStatus::Parked
    );

    // Nothing revives it: the next wake launches afresh. A follow-up finds
    // no live session to take it.
    assert!(!inner.lifecycle.follow_up(inner, agent_id, "continue").await);
    // Tombstone: the hard-kill survives.
    inner.registry.abort(agent_id).unwrap();
    assert_eq!(
        inner.registry.get(agent_id).unwrap().status,
        AgentStatus::Aborted
    );
    // The live handle round-trips as text and is not a workflow run id.
    let text = agent_id.to_string();
    assert_eq!(text.parse::<LiveRunId>().unwrap(), agent_id);
    engine.shutdown().await;
}

/// A delivered follow-up is a touch: the idle clock starts again from it,
/// and the session still parks once nobody talks to it — it is not kept for
/// good by the one follow-up that invalidated the clock armed at adoption.
#[tokio::test(flavor = "multi_thread")]
async fn a_follow_up_re_arms_the_idle_clock_and_the_session_still_parks() {
    use bisa_engine::registry::{AgentRef, AgentStatus, SessionKind};
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let inner = engine.inner();
    let adapter = inner.catalog.get("mock").unwrap();
    let spec = bisa_harness::SessionSpec {
        skills: vec![],
        work_item: None,
        cwd: dir.path().to_path_buf(),
        prompt: "hi".into(),
        model: None,
        effort: None,
        mcp_servers: vec![],
        env: Default::default(),
        env_remove: Vec::new(),
        tier_ceiling: ToolTier::Read,
        output_schema: None,
    };
    let session = adapter.launch(spec).await.unwrap();
    let agent_id = LiveRunId::mint();
    inner
        .registry
        .register_if(
            AgentRef {
                id: agent_id,
                kind: SessionKind::Worker,
                status: AgentStatus::Idle,
                generation: 1,
                session_id: None,
                work_item: None,
                conversation: None,
                goal: None,
                workstream: None,
                transcript_path: None,
                last_activity: 0,
            },
            None,
        )
        .unwrap();
    inner
        .ws
        .record_session(&bisa_store::SessionRow {
            id: "sess-2".into(),
            adapter: "mock".into(),
            kind: SessionKind::Worker,
            status: bisa_store::SessionStatus::Live,
            ..Default::default()
        })
        .unwrap();
    inner.lifecycle.adopt(
        inner,
        agent_id,
        session,
        "sess-2".into(),
        Duration::from_millis(200),
    );
    assert!(
        inner.lifecycle.follow_up(inner, agent_id, "go on").await,
        "delivered into the live session"
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            tokio::time::sleep(Duration::from_millis(25)).await;
            if inner.registry.get(agent_id).unwrap().status == AgentStatus::Parked {
                break;
            }
        }
    })
    .await
    .expect("the session parks after the follow-up's own idle time");
    assert!(!inner.lifecycle.is_live(agent_id).await);
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// Questions: text answers, options, "not sure", refusals
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn a_free_text_question_round_trips_to_the_waiting_session() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let mut rx = engine.events();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..guided("needs a decision")
        })
        .unwrap();

    let asked = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "ask_human", "goal": goal.id.to_string(),
               "question": "Which database?", "expects": "answer"}),
    )
    .await;
    assert_eq!(asked["ok"], true, "{asked}");
    let gate = asked["gate"].as_str().unwrap().to_string();

    wait_for(&mut rx, "question asked", |e| {
        matches!(&e.payload, EnginePayload::QuestionAsked { expects, .. } if expects.is_answer())
    })
    .await;
    let journal = engine
        .workspace()
        .journal(&bisa_core::Home::from(goal.id))
        .unwrap();
    assert!(journal.iter().any(|j| matches!(
        &j.payload,
        JournalPayload::Question { text, expects, .. }
            if text == "Which database?" && expects.is_answer()
    )));
    let entry = engine.inbox().into_iter().find(|g| g.id == gate).unwrap();
    assert_eq!(entry.expects, AskKind::free_text());
    assert!(
        entry.step.is_none(),
        "an agent's own question completes no step"
    );

    let socket = engine.socket_path().to_path_buf();
    let gate2 = gate.clone();
    let waiter = tokio::spawn(async move {
        intake_roundtrip(&socket, json!({"op": "await_decision", "gate": gate2})).await
    });
    tokio::time::sleep(Duration::from_millis(150)).await;
    engine
        .decide(
            &gate,
            true,
            None,
            Some(&Answer::text("sqlite, bundled")),
            None,
        )
        .unwrap();
    let got = waiter.await.unwrap();
    assert_eq!(got["ok"], true);
    assert_eq!(got["approve"], true);
    assert_eq!(got["answer"]["text"], "sqlite, bundled");
    assert!(
        got["clarify_rounds_left"].is_null(),
        "a plain answer spends no clarify round"
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_question_with_options_accepts_a_multi_select_and_free_text_together() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..guided("pick the stack")
        })
        .unwrap();

    let asked = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "ask_human", "goal": goal.id.to_string(),
        "question": "Which stores should it write to?",
        "expects": "answer", "multi": true,
        "options": [
            {"id": "sqlite", "label": "SQLite", "detail": "bundled", "recommended": true},
            {"id": "postgres", "label": "Postgres"},
            {"id": "s3", "label": "Object storage"}
        ]}),
    )
    .await;
    assert_eq!(asked["ok"], true, "{asked}");
    let gate = asked["gate"].as_str().unwrap().to_string();

    let entry = engine.inbox().into_iter().find(|g| g.id == gate).unwrap();
    let AskKind::Answer { options, multi } = &entry.expects else {
        panic!("expected an answer gate, got {:?}", entry.expects);
    };
    assert!(multi);
    assert_eq!(options.len(), 3);
    assert_eq!(options.iter().filter(|o| o.recommended).count(), 1);
    assert_eq!(options[0].detail.as_deref(), Some("bundled"));
    let journal = engine
        .workspace()
        .journal(&bisa_core::Home::from(goal.id))
        .unwrap();
    assert!(journal.iter().any(|j| matches!(
        &j.payload,
        JournalPayload::Question { expects, .. } if expects.options().len() == 3
    )));

    let answer = Answer::selecting(["sqlite", "s3"]).with_text("s3 only for the exports");
    engine
        .decide(&gate, true, None, Some(&answer), None)
        .unwrap();
    let journal = engine
        .workspace()
        .journal(&bisa_core::Home::from(goal.id))
        .unwrap();
    let recorded = journal
        .iter()
        .rev()
        .find_map(|j| match &j.payload {
            JournalPayload::Decision { answer, .. } => answer.clone(),
            _ => None,
        })
        .expect("the decision journalled the answer");
    assert_eq!(recorded.selected, vec!["sqlite", "s3"]);
    assert_eq!(recorded.text.as_deref(), Some("s3 only for the exports"));
    assert!(!recorded.unsure);
    engine.shutdown().await;
}

/// Open a question on `goal` and return its gate id.
async fn ask(engine: &Engine, goal: GoalId, question: &str) -> String {
    let asked = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "ask_human", "goal": goal.to_string(),
               "question": question, "expects": "answer"}),
    )
    .await;
    assert_eq!(asked["ok"], true, "{asked}");
    asked["gate"].as_str().unwrap().to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn not_sure_steers_the_asker_instead_of_denying_it() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..guided("needs a narrower question")
        })
        .unwrap();
    let gate = ask(&engine, goal.id, "Which currency?").await;

    let socket = engine.socket_path().to_path_buf();
    let gate2 = gate.clone();
    let waiter = tokio::spawn(async move {
        intake_roundtrip(&socket, json!({"op": "await_decision", "gate": gate2})).await
    });
    tokio::time::sleep(Duration::from_millis(150)).await;
    engine
        .decide(&gate, true, None, Some(&Answer::unsure()), None)
        .unwrap();

    let got = waiter.await.unwrap();
    assert_eq!(got["answer"]["unsure"], true);
    assert_eq!(got["clarify_rounds_left"], 2);
    let entry = engine.gate(&gate).unwrap();
    assert!(entry.resolution.is_some(), "the wait is over");
    assert_eq!(entry.decided(), None, "\"not sure\" decides nothing");
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_clarify_bound_runs_out_and_stops_the_re_asking() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = design_off_config();
    config.max_clarify_rounds = 2;
    let engine = Engine::start(
        workspace(&dir),
        catalog_with(vec![MockAdapter::default()]),
        config,
    )
    .unwrap();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..guided("a question that keeps coming back")
        })
        .unwrap();

    let mut left = vec![];
    for _ in 0..3 {
        let gate = ask(&engine, goal.id, "Which currency?").await;
        engine
            .decide(&gate, true, None, Some(&Answer::unsure()), None)
            .unwrap();
        let entry = engine.gate(&gate).unwrap();
        left.push(entry.resolution.unwrap().clarify_rounds_left.unwrap());
    }
    assert_eq!(left, vec![1, 0, 0]);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_that_does_not_fit_its_question_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..guided("validation")
        })
        .unwrap();

    let gate = ask(&engine, goal.id, "Which currency?").await;
    let err = engine
        .decide(&gate, true, None, Some(&Answer::text("   ")), None)
        .unwrap_err();
    assert!(err.to_string().contains("carried nothing"), "{err}");
    assert!(engine.decide(&gate, true, None, None, None).is_err());

    let offered = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "ask_human", "goal": goal.id.to_string(),
               "question": "Which database?", "expects": "answer",
               "options": [{"id": "sqlite", "label": "SQLite"}]}),
    )
    .await;
    let offered_gate = offered["gate"].as_str().unwrap().to_string();
    let err = engine
        .decide(
            &offered_gate,
            true,
            None,
            Some(&Answer::selecting(["postgres"])),
            None,
        )
        .unwrap_err();
    assert!(err.to_string().contains("never offered"), "{err}");
    let err = engine
        .decide(
            &offered_gate,
            true,
            None,
            Some(&Answer::selecting(["sqlite", "sqlite"])),
            None,
        )
        .unwrap_err();
    assert!(err.to_string().contains("one answer"), "{err}");

    let decision_gate = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "ask_human", "goal": goal.id.to_string(),
               "question": "Ship it?", "expects": "decision"}),
    )
    .await;
    let decision_gate = decision_gate["gate"].as_str().unwrap().to_string();
    let err = engine
        .decide(
            &decision_gate,
            true,
            None,
            Some(&Answer::selecting(["yes"])),
            None,
        )
        .unwrap_err();
    assert!(err.to_string().contains("no options"), "{err}");

    let two_recs = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "ask_human", "goal": goal.id.to_string(),
               "question": "Which?", "expects": "answer",
               "options": [{"id": "a", "label": "A", "recommended": true},
                           {"id": "b", "label": "B", "recommended": true}]}),
    )
    .await;
    assert_eq!(two_recs["ok"], false, "{two_recs}");
    assert!(two_recs["errors"][0]
        .as_str()
        .unwrap()
        .contains("at most one"));
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// Intake goal ops
// ---------------------------------------------------------------------------

/// A post through the intake is an agent's words, never the person's: the
/// session's agent when it has one, the work item's when the session runs
/// one, and the General Agent's when it has neither — an unstaffed step's
/// session, or a goal session driven by hand.
#[tokio::test(flavor = "multi_thread")]
async fn an_agents_post_without_an_identity_is_the_general_agents_never_the_persons() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let ws = engine.workspace();
    let sock = engine.socket_path().to_path_buf();
    // Nobody is installed, so the step's item names no agent.
    let (goal, _) = run_on(
        &engine,
        "nameless",
        new_workflow("w", vec![agent_step("do", "mock")]),
    );
    finished_run(&engine, goal.id).await;
    let item = items_of(&engine, goal.id).remove(0);
    assert_eq!(item.agent, None, "an unstaffed step");
    let gid = goal.id.to_string();

    let r = intake_roundtrip(
        &sock,
        json!({"op": "post_message", "scope": gid, "content": "from the item",
               "work_item": item.id.to_string()}),
    )
    .await;
    assert_eq!(r["ok"], true, "{r}");
    let r = intake_roundtrip(
        &sock,
        json!({"op": "post_message", "scope": gid, "content": "from nowhere"}),
    )
    .await;
    assert_eq!(r["ok"], true, "{r}");
    ws.install(bisa_store::CatalogKind::Agent, "developer")
        .unwrap();
    let r = intake_roundtrip(
        &sock,
        json!({"op": "post_message", "scope": gid, "content": "from the developer",
               "agent": "developer"}),
    )
    .await;
    assert_eq!(r["ok"], true, "{r}");

    let general = ws.get_agent(&AgentId::general()).unwrap().pubkey;
    let developer = ws
        .get_agent(&AgentId::new("developer").unwrap())
        .unwrap()
        .pubkey;
    let posts: Vec<(String, String)> = ws
        .messages(&gid, None, 10)
        .unwrap()
        .into_iter()
        .map(|m| (m.content, m.author))
        .collect();
    let by = |text: &str| {
        posts
            .iter()
            .find(|(t, _)| t == text)
            .map(|(_, a)| a.clone())
            .unwrap_or_else(|| panic!("{text} was posted: {posts:?}"))
    };
    assert_eq!(by("from the item"), general.as_hex());
    assert_eq!(by("from nowhere"), general.as_hex());
    assert_eq!(by("from the developer"), developer.as_hex());
    assert!(
        posts
            .iter()
            .all(|(_, a)| a != ws.owner_principal().as_hex()),
        "nothing an agent posts is the person's"
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn intake_goal_ops_read_and_revise_until_a_run_is_live() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let (goal, wf) = goal_on(
        &engine,
        "vague idea",
        new_workflow(
            "held",
            vec![step(
                "hold",
                StepKind::Wait {
                    until: bisa_core::WaitFor::Release,
                },
            )],
        ),
    );
    let sock = engine.socket_path().to_path_buf();
    let gid = goal.id.to_string();

    let got = intake_roundtrip(&sock, json!({"op": "get_goal", "goal": gid})).await;
    assert_eq!(got["ok"], true);
    assert_eq!(got["goal"]["statement"], "vague idea");
    assert_eq!(got["goal"]["status"], "draft");
    assert_eq!(got["goal"]["workflow"], json!(wf.id.to_string()));
    assert!(got["run"].is_null());

    let r = intake_roundtrip(
        &sock,
        json!({"op": "revise_statement", "goal": gid,
               "statement": "sharp idea", "why": "clarified with the human"}),
    )
    .await;
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        engine.workspace().get_goal(goal.id).unwrap().statement,
        "sharp idea"
    );

    engine.start_run(goal.id, BTreeMap::new()).unwrap();
    let got = intake_roundtrip(&sock, json!({"op": "get_goal", "goal": gid})).await;
    assert_eq!(got["goal"]["status"], "waiting");
    assert_eq!(got["run"]["status"], "waiting");
    assert_eq!(got["run"]["steps"][0]["id"], "hold");
    assert_eq!(got["run"]["steps"][0]["state"], "waiting");

    // While a run is live, the statement is frozen.
    let r = intake_roundtrip(
        &sock,
        json!({"op": "revise_statement", "goal": gid, "statement": "scope creep"}),
    )
    .await;
    assert_eq!(r["ok"], false);
    assert!(r["errors"][0].as_str().unwrap().contains("live"));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn spawn_sub_goal_policy() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(
        &dir,
        vec![MockAdapter {
            script: Some(vec![SessionEvent::Lifecycle(LifecycleEvent::Started)]),
            ..Default::default()
        }],
    );
    let (goal, _) = run_on(
        &engine,
        "parent",
        new_workflow("p", vec![agent_step("go", "mock")]),
    );
    let sock = engine.socket_path().to_path_buf();

    // Work-item scope: an item has no spawn budget, so it is refused.
    let wi = until("the item", || items_of(&engine, goal.id).into_iter().next()).await;
    let r = intake_roundtrip(
        &sock,
        json!({"op": "spawn_sub_goal", "work_item": wi.id.to_string(), "statement": "child"}),
    )
    .await;
    assert_eq!(r["ok"], false);
    assert!(r["errors"][0].as_str().unwrap().contains("depth"));

    // Goal scope (a designing session) spawns with a refines edge and a note.
    let r = intake_roundtrip(
        &sock,
        json!({"op": "spawn_sub_goal", "goal": goal.id.to_string(),
               "statement": "research the options", "title": "Research"}),
    )
    .await;
    assert_eq!(r["ok"], true, "{r}");
    let child: GoalId = r["child"].as_str().unwrap().parse().unwrap();
    let child_goal = engine.workspace().get_goal(child).unwrap();
    assert_eq!(child_goal.origin, GoalOrigin::Spawned { parent: goal.id });
    assert_eq!(child_goal.title.as_deref(), Some("Research"));
    assert_eq!(
        engine.workspace().edges_from(child).unwrap(),
        vec![(goal.id, bisa_core::GoalEdgeKind::Refines)]
    );
    engine.shutdown().await;
}

/// A guided submit wakes the Workflow Agent (mock session) in the design
/// phase, and `guided: false` does not.
#[tokio::test(flavor = "multi_thread")]
async fn the_workflow_agent_wakes_on_guided_submit_only() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "mock-guided");
    let engine = Engine::start(
        ws,
        catalog_with(vec![MockAdapter {
            id: "mock-guided".into(),
            ..Default::default()
        }]),
        EngineConfig {
            design_enabled: true,
            events_enabled: false,
            ..Default::default()
        },
    )
    .unwrap();
    let mut rx = engine.events();

    let _quiet = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..guided("manual one")
        })
        .unwrap();
    let raced = tokio::time::timeout(Duration::from_millis(400), async {
        loop {
            if let Ok(ev) = rx.recv().await {
                if matches!(ev.payload, EnginePayload::Guided { .. }) {
                    return;
                }
            }
        }
    })
    .await;
    assert!(raced.is_err(), "a design wake happened on a manual goal");

    let guided = engine.submit_goal(guided("guided one")).unwrap();
    let ev = wait_for(&mut rx, "guided phase", |e| {
        matches!(
            &e.payload,
            EnginePayload::Guided {
                phase: bisa_core::GuidancePhase::Design,
                status: bisa_core::GuidanceStatus::Working,
                ..
            }
        )
    })
    .await;
    assert_eq!(ev.goal, Some(guided.id));
    engine.shutdown().await;
}

/// A gate is decided once, as itself: a second decision is refused and
/// journals nothing, and a gate named on another goal's route never decides
/// what that goal owes.
#[tokio::test(flavor = "multi_thread")]
async fn a_gate_is_decided_once_and_only_as_its_own_goals() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let ws = engine.workspace();
    let gated = |name: &str| {
        run_on(
            &engine,
            name,
            new_workflow(
                name,
                chain(vec![step(
                    "ok",
                    StepKind::Approval {
                        prompt: "Go?".into(),
                    },
                )]),
            ),
        )
    };
    let (first, _) = gated("first");
    let (second, _) = gated("second");
    let gate_of = |goal: GoalId| {
        engine
            .inbox()
            .into_iter()
            .find(|g| g.home.goal() == Some(goal))
    };
    let mine = until("the first goal's gate", || gate_of(first.id)).await;
    let theirs = until("the second goal's gate", || gate_of(second.id)).await;
    let decisions = |goal: GoalId| {
        ws.journal(&bisa_core::Home::from(goal))
            .unwrap()
            .iter()
            .filter(|j| matches!(j.payload, JournalPayload::Decision { .. }))
            .count()
    };

    // Named on the wrong goal's route: refused, and neither goal moves.
    let refused = engine
        .decide_durable(
            &bisa_core::Home::from(first.id),
            true,
            None,
            None,
            None,
            Some(&theirs.id),
            None,
        )
        .unwrap_err();
    assert!(
        refused.to_string().contains("is not one of goal"),
        "{refused}"
    );
    assert_eq!((decisions(first.id), decisions(second.id)), (0, 0));
    assert!(gate_of(first.id).is_some() && gate_of(second.id).is_some());

    // Decided, then decided again — through either door.
    engine.decide(&mine.id, true, None, None, None).unwrap();
    let again = engine
        .decide(&mine.id, false, None, None, None)
        .unwrap_err();
    assert!(
        matches!(again, bisa_engine::EngineError::GateAlreadyDecided(_)),
        "{again}"
    );
    let named_again = engine
        .decide_durable(
            &bisa_core::Home::from(first.id),
            false,
            None,
            None,
            None,
            Some(&mine.id),
            None,
        )
        .unwrap_err();
    assert!(
        matches!(named_again, bisa_engine::EngineError::GateAlreadyDecided(_)),
        "a decided gate named again is not the mirror's to decide: {named_again}"
    );
    assert_eq!(
        decisions(first.id),
        1,
        "one signed decision, whatever was tried after"
    );
    assert!(matches!(
        engine
            .decide("01J0NOSUCHGATE", true, None, None, None)
            .unwrap_err(),
        bisa_engine::EngineError::UnknownGate(_)
    ));
    let done = finished_run(&engine, first.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done), "the first word stood");
    assert!(gate_of(second.id).is_some(), "the other goal still waits");
    engine.shutdown().await;
}

/// What a run's verbs refuse, and what a refusal leaves behind: nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_runs_verbs_refuse_what_cannot_be_and_leave_the_run_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let ws = engine.workspace();
    let (goal, first) = run_on(
        &engine,
        "refusals",
        new_workflow(
            "asks",
            chain(vec![step(
                "which",
                StepKind::Human {
                    prompt: "Which?".into(),
                    options: vec![
                        bisa_core::AskOption::new("a", "A"),
                        bisa_core::AskOption::new("b", "B"),
                    ],
                    multi: false,
                    assignee: None,
                },
            )]),
        ),
    );
    until("the question", || {
        engine
            .inbox()
            .into_iter()
            .find(|g| g.home.goal() == Some(goal.id))
    })
    .await;

    // A second start while one is live is a run in the queue, not a second live run.
    let second = engine.start_run(goal.id, BTreeMap::new()).unwrap();
    assert!(second.is_queued(), "queued behind the live run");
    assert_eq!(ws.live_run(goal.id).unwrap().map(|r| r.id), Some(first.id));
    assert_eq!(ws.queued_runs(goal.id).unwrap().len(), 1);

    // An option nobody offered: refused, the step still waiting, nothing signed.
    let bogus = Answer::selecting(["c"]);
    assert!(engine
        .answer_step(current_run(&engine, goal.id).id, &sid("which"), &bogus)
        .is_err());
    assert_eq!(
        current_run(&engine, goal.id).steps[&sid("which")].state,
        StepState::Waiting
    );
    assert!(
        !ws.journal(&bisa_core::Home::from(goal.id))
            .unwrap()
            .iter()
            .any(|j| matches!(j.payload, JournalPayload::Decision { .. })),
        "a refused answer is not a decision"
    );
    // A step that is not a wait, and one that does not exist, release nothing.
    assert!(engine
        .release_step(current_run(&engine, goal.id).id, &sid("which"), None)
        .is_err());
    assert!(engine
        .release_step(current_run(&engine, goal.id).id, &sid("nope"), None)
        .is_err());
    // A step that does not exist cannot be answered either.
    assert!(engine
        .answer_step(
            current_run(&engine, goal.id).id,
            &sid("nope"),
            &Answer::selecting(["a"])
        )
        .is_err());

    // Stop: the live run cancelled, the queue withdrawn, in one word.
    let stopped = engine
        .stop_goal(goal.id, Some("enough".into()))
        .await
        .unwrap();
    assert_eq!(stopped.run, Some(first.id));
    assert_eq!(stopped.withdrawn, vec![second.id]);
    // Stopped again: nothing live, nothing queued — nothing done, no error.
    let again = engine.stop_goal(goal.id, None).await.unwrap();
    assert_eq!((again.run, again.withdrawn.len()), (None, 0));
    // The cancelled run takes no more words.
    assert!(engine
        .answer_step(
            current_run(&engine, goal.id).id,
            &sid("which"),
            &Answer::selecting(["a"])
        )
        .is_err());
    assert!(
        engine.withdraw_run(goal.id, second.id).is_err(),
        "withdrawn once"
    );

    // A closed goal starts, stops and restarts nothing.
    engine
        .close_goal(
            goal.id,
            bisa_core::ClosureReason::Abandoned { rationale: None },
        )
        .unwrap();
    assert!(engine.start_run(goal.id, BTreeMap::new()).is_err());
    assert!(engine.stop_goal(goal.id, None).await.is_err());
    assert!(engine.restart_goal(goal.id).await.is_err());
    assert!(ws.live_run(goal.id).unwrap().is_none());
    engine.shutdown().await;
}

/// decide_durable resolves live gates and falls back to durable state: an
/// approval step, an escalation step, and an adoption.
#[tokio::test(flavor = "multi_thread")]
async fn decide_durable_live_and_mirror() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let ws = engine.workspace();

    // 1. Live path: the approval step's gate is pending in memory.
    let (goal, run) = run_on(
        &engine,
        "durable",
        new_workflow(
            "gated",
            chain(vec![
                step(
                    "ok",
                    StepKind::Approval {
                        prompt: "Go?".into(),
                    },
                ),
                step(
                    "which",
                    StepKind::Human {
                        prompt: "Which?".into(),
                        options: vec![],
                        multi: false,
                        assignee: None,
                    },
                ),
            ]),
        ),
    );
    until("the approval gate", || {
        engine
            .inbox()
            .into_iter()
            .find(|g| g.home.goal() == Some(goal.id))
    })
    .await;
    let out = engine
        .decide_durable(
            &bisa_core::Home::from(goal.id),
            true,
            Some("ok"),
            None,
            None,
            None,
            None,
        )
        .unwrap();
    assert_eq!(out.gate, Gate::Approval);
    assert_eq!(
        out.status,
        bisa_engine::HomeStatus::Goal {
            status: GoalStatus::Waiting
        }
    );
    assert!(matches!(
        current_run(&engine, goal.id).steps[&sid("ok")].state,
        StepState::Done { .. }
    ));

    // 2. Mirror path: drop the live gate for the human step (a restarted
    // process) and answer from the run alone.
    let question = until("the question", || {
        engine
            .inbox()
            .into_iter()
            .find(|g| g.home.goal() == Some(goal.id))
    })
    .await;
    engine.inner().gates.withdraw(&question.id);
    let out = bisa_engine::ops::decide_without_engine(
        engine.inner(),
        &bisa_core::Home::from(goal.id),
        true,
        None,
        Some(&Answer::text("this one")),
        None,
        None,
    )
    .unwrap();
    assert_eq!(out.gate, Gate::Escalation);
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert_eq!(
        done.steps[&sid("which")]
            .answer
            .as_ref()
            .unwrap()
            .text
            .as_deref(),
        Some("this one")
    );
    let journal = ws.journal(&bisa_core::Home::from(goal.id)).unwrap();
    assert!(journal.iter().any(|j| matches!(
        &j.payload,
        JournalPayload::Decision { subject, .. } if *subject == format!("step:{}/which", run.id)
    )));

    // 3. An adoption, decided durably: the proposal is a journaled question
    // under its subject, and approving starts the run with the inputs.
    // Guided: a proposal on it opens the Adopt gate (a manual goal's is a
    // draft with no gate); the design wake itself is off in this config.
    let goal2 = engine.submit_goal(guided("adopt me")).unwrap();
    let mut draft = new_workflow("proposed", vec![agent_step("go", "mock")]);
    // A declared input is read by a step, or the workflow is invalid.
    if let bisa_core::StepKind::Agent { instructions, .. } = &mut draft.steps[0].kind {
        instructions.push_str(" in a {inputs.tone} tone");
    }
    draft.inputs = vec![bisa_core::InputDef {
        name: bisa_core::InputName::new("tone").unwrap(),
        label: "Tone".into(),
        kind: bisa_core::InputKind::Text,
        default: None,
        required: true,
    }];
    let (wf, gate) =
        bisa_engine::ops::propose_workflow(engine.inner(), goal2.id, draft, None).unwrap();
    engine
        .inner()
        .gates
        .withdraw(&gate.expect("a guided goal's proposal opens the Adopt gate"));
    let err = bisa_engine::ops::decide_without_engine(
        engine.inner(),
        &bisa_core::Home::from(goal2.id),
        true,
        None,
        None,
        None,
        None,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("required"),
        "starting needs the input: {err}"
    );
    let out = bisa_engine::ops::decide_without_engine(
        engine.inner(),
        &bisa_core::Home::from(goal2.id),
        true,
        None,
        None,
        Some(BTreeMap::from([("tone".to_string(), json!("warm"))])),
        None,
    )
    .unwrap();
    assert_eq!(out.gate, Gate::Approval);
    let run2 = current_run(&engine, goal2.id);
    assert_eq!(run2.workflow.id, wf.id);
    assert_eq!(run2.inputs["tone"], json!("warm"));
    finished_run(&engine, goal2.id).await;
    engine.shutdown().await;
}

/// A work item run under an assigned Agent journals its claim signed by the
/// agent's own key (attested).
#[tokio::test(flavor = "multi_thread")]
async fn an_assigned_agent_signs_its_claim() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let agent = engine
        .workspace()
        .add_agent(bisa_store::NewAgent {
            name: "Builder".into(),
            harness: "mock".into(),
            ..Default::default()
        })
        .unwrap();
    let (goal, _) = run_on(
        &engine,
        "attributed",
        new_workflow(
            "attributed",
            vec![assigned_agent_step("build", "mock", agent.id.as_str())],
        ),
    );
    finished_run(&engine, goal.id).await;
    let journal = engine
        .workspace()
        .journal(&bisa_core::Home::from(goal.id))
        .unwrap();
    let claim_author = journal
        .iter()
        .find_map(|j| match &j.payload {
            JournalPayload::Claim { .. } => Some(j.author.clone()),
            _ => None,
        })
        .expect("claim journaled");
    assert_eq!(claim_author, agent.pubkey, "claim signed by the agent");
    assert_ne!(claim_author, engine.workspace().owner_principal());
    assert_eq!(
        items_of(&engine, goal.id)[0].agent.as_deref(),
        Some(agent.id.as_str())
    );
    engine.shutdown().await;
}

/// A panic inside the executor costs the step, not the mark and not the
/// engine: the item is blocked with the panic's sentence, the step fails
/// through the funnel, the in-flight mark is released, and another goal on
/// a healthy harness still finishes.
#[tokio::test(flavor = "multi_thread")]
async fn a_panic_in_a_work_item_releases_its_in_flight_mark() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(
        &dir,
        vec![
            MockAdapter {
                id: "broken".into(),
                panic_on_launch: true,
                ..Default::default()
            },
            yielding("good", json!({"ok": true})),
        ],
    );
    let (goal, _) = run_on(
        &engine,
        "hit the bug",
        new_workflow("panics", vec![agent_step("work", "broken")]),
    );
    let failed = finished_run(&engine, goal.id).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed), "{failed:?}");
    let error = failed.steps[&sid("work")].error.clone().unwrap_or_default();
    assert!(error.contains("panicked"), "{error}");
    until("the in-flight mark to be released", || {
        (engine.active_item_count(&Home::Goal { goal: goal.id }) == 0).then_some(())
    })
    .await;
    let items = items_of(&engine, goal.id);
    assert!(
        items.iter().any(|i| matches!(&i.state, WorkItemState::Blocked { reason, .. } if reason.contains("panicked"))),
        "the item is blocked with the reason: {items:?}"
    );

    let (other, _) = run_on(
        &engine,
        "carry on",
        new_workflow("fine", vec![agent_step("work", "good")]),
    );
    assert_eq!(
        finished_run(&engine, other.id).await.outcome,
        Some(RunOutcome::Done),
        "the engine lives"
    );
    engine.shutdown().await;
}
