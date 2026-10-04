//! The guided cycle, proven without an LLM.
//!
//! A live run shows guided mode works; these tests show it keeps working.
//! Mock sessions speak the real intake wire protocol (they find the socket in
//! their `SessionSpec` exactly as a harness does), so the engine is exercised
//! through the same door a real harness uses — only the judgement is
//! scripted. The driver is the Workflow Agent: it proposes, the person
//! adopts, and the run does the rest.

use crate::common;

use bisa_core::event::JournalPayload;
use bisa_core::{
    AgentId, Answer, Gate, GoalStatus, GuidancePhase, GuidanceStatus, MemberRole, PrincipalId,
    RunOutcome, StepKind, WorkItemState,
};
use bisa_engine::{Engine, EngineConfig, EnginePayload, SubmitRequest};
use bisa_harness::mock::{IntakeScript, MockAdapter};
use bisa_harness::HarnessCatalog;
use bisa_store::{CatalogKind, MemoryKeyStore, Workspace};
use common::*;
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;

/// The Workflow Agent, as the wire spells it.
const DRIVER: &str = AgentId::WORKFLOW;

/// A catalog whose `guided` and `worker` harnesses run scripted intake
/// conversations.
fn catalog(guided: Option<IntakeScript>, worker: Option<IntakeScript>) -> HarnessCatalog {
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::new(MockAdapter {
        id: "guided-harness".into(),
        intake_script: guided,
        ..Default::default()
    }));
    catalog.register(Arc::new(MockAdapter {
        id: "worker-harness".into(),
        intake_script: worker,
        ..Default::default()
    }));
    // A harness that is here — the validator admits a step on it — and
    // whose every session dies at once: the shape of a run failing outright.
    catalog.register(Arc::new(MockAdapter {
        id: "doomed-harness".into(),
        script: Some(vec![bisa_harness::SessionEvent::Lifecycle(
            bisa_harness::LifecycleEvent::Ended {
                outcome: bisa_harness::Outcome::Failed {
                    error: "the harness died at launch".into(),
                },
                is_terminal: true,
            },
        )]),
        ..Default::default()
    }));
    catalog
}

fn guided_config() -> EngineConfig {
    EngineConfig {
        design_enabled: true,
        events_enabled: false,
        ..Default::default()
    }
}

/// A worker that yields a result — the shape of a step finishing.
fn worker_yields() -> IntakeScript {
    IntakeScript::new(vec![json!({
        "op": "result_submit", "work_item": "{{work_item}}", "output": {"ok": true}
    })])
}

/// A one-step workflow on the worker harness, as the Workflow Agent's tool
/// would send it.
fn proposal(name: &str) -> serde_json::Value {
    json!({
        "name": name,
        "description": "one step, proposed by the driver",
        "steps": [{"id": "do", "name": "Do it", "kind": "agent",
                   "instructions": "do {goal.statement}", "harness": ["worker-harness"]}]
    })
}

/// The whole guided loop: capture → the Workflow Agent reads, checks the
/// templates, validates and proposes → the person adopts → the run walks →
/// done. No LLM, no real harness, no manual commands.
#[tokio::test(flavor = "multi_thread")]
async fn a_guided_capture_wakes_the_workflow_agent_which_proposes_and_a_person_adopts() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "guided-harness");

    let script = IntakeScript::new(vec![
        json!({"op": "get_goal", "goal": "{{goal}}"}),
        json!({"op": "list_workflow_templates", "agent": DRIVER}),
        json!({"op": "validate_workflow", "agent": DRIVER, "workflow": proposal("Sharpened")}),
        json!({"op": "propose_workflow", "agent": DRIVER, "goal": "{{goal}}",
               "workflow": proposal("Sharpened")}),
        json!({"op": "add_note", "goal": "{{goal}}", "text": "proposed one step; adopt to run"}),
    ]);
    let replies = script.replies.clone();
    let engine = Engine::start(
        ws,
        catalog(Some(script), Some(worker_yields())),
        guided_config(),
    )
    .unwrap();
    let mut rx = engine.events();

    let goal = engine.submit_goal(guided("write a sales report")).unwrap();
    assert_eq!(goal.mode, bisa_core::GoalMode::Guided);
    assert_eq!(goal.status(None), GoalStatus::Draft);

    // The proposal lands: the goal points at it and an Adopt gate is open.
    let proposed = wait_for(&mut rx, "the proposal", |e| {
        matches!(&e.payload, EnginePayload::WorkflowProposed { .. })
    })
    .await;
    let EnginePayload::WorkflowProposed {
        workflow,
        gate_id,
        revision,
    } = proposed.payload
    else {
        unreachable!()
    };
    assert_eq!(revision, 1);
    let gate_id = gate_id.expect("a guided goal's proposal opens the Adopt gate");
    let g = engine.workspace().get_goal(goal.id).unwrap();
    assert_eq!(g.workflow, Some(workflow));
    assert!(g.run.is_none(), "a guided proposal starts nothing");
    let gate = engine.gate(&gate_id).unwrap();
    assert_eq!(gate.gate, Gate::Approval);
    assert!(gate.subject.starts_with("adopt:"), "{}", gate.subject);
    assert!(gate.step.is_none());
    let wf = engine.workspace().get_workflow(workflow).unwrap();
    assert_eq!(wf.name, "Sharpened");
    assert_eq!(
        wf.author,
        engine
            .workspace()
            .get_agent(&AgentId::workflow())
            .unwrap()
            .pubkey,
        "the proposal is the Workflow Agent's, in the record"
    );

    // Every reply the driver received said yes; the templates were there.
    until("the script to finish", || {
        (replies.lock().unwrap().len() == 5).then_some(())
    })
    .await;
    let replies = replies.lock().unwrap().clone();
    assert!(
        replies.iter().all(|r| r["ok"] == json!(true)),
        "{replies:?}"
    );
    assert_eq!(replies[1]["templates"].as_array().unwrap().len(), 13);
    assert_eq!(replies[2]["problems"], json!([]));

    // The person adopts; the run walks to done on the worker.
    engine
        .decide(&gate_id, true, Some("looks right"), None, None)
        .unwrap();
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert_eq!(done.workflow.id, workflow);
    assert_eq!(
        engine
            .workspace()
            .get_goal(goal.id)
            .unwrap()
            .status(Some(&done)),
        GoalStatus::Done
    );
    // The General Agent never woke: every guided session identified itself
    // as the Workflow Agent.
    let mut phases = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        if let EnginePayload::Guided { phase, .. } = ev.payload {
            phases.push(phase);
        }
    }
    assert!(
        phases.iter().all(|p| *p == GuidancePhase::Design),
        "{phases:?}"
    );
    engine.shutdown().await;
}

/// The facts a guided goal's journal holds about the Workflow Agent's
/// standing, in order.
fn guidance_facts(
    engine: &Engine,
    goal: bisa_core::GoalId,
) -> Vec<(GuidancePhase, GuidanceStatus, Option<String>)> {
    engine
        .workspace()
        .journal(&bisa_core::Home::from(goal))
        .unwrap()
        .iter()
        .filter_map(|e| match &e.payload {
            JournalPayload::Guidance {
                phase,
                status,
                detail,
                ..
            } => Some((*phase, *status, detail.clone())),
            _ => None,
        })
        .collect()
}

/// The statuses alone.
fn statuses(engine: &Engine, goal: bisa_core::GoalId) -> Vec<GuidanceStatus> {
    guidance_facts(engine, goal)
        .into_iter()
        .map(|(_, s, _)| s)
        .collect()
}

/// What the Workflow Agent said in the goal's thread, oldest first.
fn said(engine: &Engine, goal: bisa_core::GoalId) -> Vec<String> {
    let mut rows = engine
        .workspace()
        .messages(&goal.to_string(), None, 50)
        .unwrap();
    rows.sort_by_key(|m| m.created_at);
    rows.into_iter().map(|m| m.content).collect()
}

/// A design wake is never silent: every transition is a journal fact signed
/// by the Workflow Agent and a `Guided` event, the agent's presence lights up
/// while it works, and it speaks in the goal's thread when it starts and when
/// it proposes.
#[tokio::test(flavor = "multi_thread")]
async fn a_design_wake_records_the_lifecycle_and_speaks_in_the_goal_thread() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "guided-harness");
    let script = IntakeScript::new(vec![
        json!({"op": "get_goal", "goal": "{{goal}}"}),
        json!({"op": "propose_workflow", "agent": DRIVER, "goal": "{{goal}}",
               "workflow": proposal("Spoken")}),
    ]);
    let engine = Engine::start(
        ws,
        catalog(Some(script), Some(worker_yields())),
        guided_config(),
    )
    .unwrap();
    let mut rx = engine.events();
    let goal = engine.submit_goal(guided("write a sales report")).unwrap();

    // The bus, in order: scheduled, working (with a session), thinking, proposed, replied.
    let scheduled = wait_for(&mut rx, "scheduled", |e| {
        matches!(
            &e.payload,
            EnginePayload::Guided {
                status: GuidanceStatus::Scheduled,
                ..
            }
        )
    })
    .await;
    assert_eq!(scheduled.goal, Some(goal.id));
    let working = wait_for(&mut rx, "working", |e| {
        matches!(
            &e.payload,
            EnginePayload::Guided {
                status: GuidanceStatus::Working,
                ..
            }
        )
    })
    .await;
    let EnginePayload::Guided { session, phase, .. } = working.payload else {
        unreachable!()
    };
    assert_eq!(phase, GuidancePhase::Design);
    assert!(session.is_some(), "working names the session");
    wait_for(&mut rx, "the agent thinking", |e| {
        matches!(&e.payload, EnginePayload::AgentThinking { scope, agent } if *scope == goal.id.to_string() && agent == DRIVER)
    })
    .await;
    wait_for(&mut rx, "proposed", |e| {
        matches!(
            &e.payload,
            EnginePayload::Guided {
                status: GuidanceStatus::Proposed,
                ..
            }
        )
    })
    .await;
    wait_for(&mut rx, "the agent done", |e| {
        matches!(&e.payload, EnginePayload::AgentReplied { agent, posted: true, .. } if agent == DRIVER)
    })
    .await;

    // The journal holds the same, signed by the Workflow Agent.
    until("the facts to land", || {
        (statuses(&engine, goal.id).len() >= 3).then_some(())
    })
    .await;
    assert_eq!(
        statuses(&engine, goal.id),
        vec![
            GuidanceStatus::Scheduled,
            GuidanceStatus::Working,
            GuidanceStatus::Proposed
        ]
    );
    let agent_key = engine
        .workspace()
        .get_agent(&AgentId::workflow())
        .unwrap()
        .pubkey;
    for e in engine
        .workspace()
        .journal(&bisa_core::Home::from(goal.id))
        .unwrap()
    {
        if matches!(e.payload, JournalPayload::Guidance { .. }) {
            assert_eq!(
                e.author, agent_key,
                "a guidance fact is the Workflow Agent's"
            );
        }
    }
    // And the goal's thread heard the proposal — one line by its fate; the
    // working state is silent.
    let words = said(&engine, goal.id);
    assert_eq!(words.len(), 1, "{words:?}");
    assert!(words[0].contains("Spoken"), "{}", words[0]);
    assert!(words[0].contains("Adopt"), "{}", words[0]);
    engine.shutdown().await;
}

/// A turn that ends with no proposal is a stall, said out loud; asking again
/// wakes the agent again.
#[tokio::test(flavor = "multi_thread")]
async fn a_turn_that_ends_without_a_proposal_records_stalled_and_redesign_rewakes() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "guided-harness");
    let script = IntakeScript::new(vec![json!({"op": "get_goal", "goal": "{{goal}}"})]);
    let engine = Engine::start(ws, catalog(Some(script), None), guided_config()).unwrap();
    let mut rx = engine.events();
    let goal = engine.submit_goal(guided("stall me")).unwrap();
    let stalled = wait_for(&mut rx, "stalled", |e| {
        matches!(
            &e.payload,
            EnginePayload::Guided {
                status: GuidanceStatus::Stalled,
                ..
            }
        )
    })
    .await;
    let EnginePayload::Guided { detail, .. } = stalled.payload else {
        unreachable!()
    };
    assert!(
        detail.unwrap_or_default().contains("without proposing"),
        "the reason is named"
    );
    until("the stall to be journaled", || {
        (statuses(&engine, goal.id).last() == Some(&GuidanceStatus::Stalled)).then_some(())
    })
    .await;
    // The fact lands first, the words in the thread after it: polled.
    let words = until("the stall to be said", || {
        let words = said(&engine, goal.id);
        (!words.is_empty()).then_some(words)
    })
    .await;
    assert!(
        words.last().unwrap().contains("Retry the design"),
        "{words:?}"
    );
    assert!(engine
        .workspace()
        .get_goal(goal.id)
        .unwrap()
        .workflow
        .is_none());

    // Ask again: a fresh wake is scheduled and runs.
    engine.redesign(goal.id).unwrap();
    wait_for(&mut rx, "scheduled again", |e| {
        matches!(
            &e.payload,
            EnginePayload::Guided {
                status: GuidanceStatus::Scheduled,
                ..
            }
        )
    })
    .await;
    wait_for(&mut rx, "working again", |e| {
        matches!(
            &e.payload,
            EnginePayload::Guided {
                status: GuidanceStatus::Working,
                ..
            }
        )
    })
    .await;
    engine.shutdown().await;
}

/// A wake that never ends its turn is cut at the timeout and recorded stalled.
#[tokio::test(flavor = "multi_thread")]
async fn a_timed_out_wake_records_stalled() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "hanging-harness");
    let mut catalog = HarnessCatalog::new();
    // A script with no terminal end leaves the session running.
    catalog.register(Arc::new(MockAdapter {
        id: "hanging-harness".into(),
        script: Some(vec![]),
        ..Default::default()
    }));
    let engine = Engine::start(
        ws,
        catalog,
        EngineConfig {
            guided_wake_timeout_secs: 1,
            ..guided_config()
        },
    )
    .unwrap();
    let mut rx = engine.events();
    let goal = engine.submit_goal(guided("hang")).unwrap();
    let stalled = wait_for(&mut rx, "stalled", |e| {
        matches!(
            &e.payload,
            EnginePayload::Guided {
                status: GuidanceStatus::Stalled,
                ..
            }
        )
    })
    .await;
    let EnginePayload::Guided { detail, .. } = stalled.payload else {
        unreachable!()
    };
    assert!(detail.unwrap_or_default().contains("no proposal after 1s"));
    until("the stall to be journaled", || {
        (statuses(&engine, goal.id).last() == Some(&GuidanceStatus::Stalled)).then_some(())
    })
    .await;
    engine.shutdown().await;
}

/// A question is not a stall: the wake records `asking`, and the answer
/// brings it back to `working`.
#[tokio::test(flavor = "multi_thread")]
async fn a_design_question_records_asking_and_the_answer_resumes() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "guided-harness");
    let script = IntakeScript::new(vec![json!({"op": "ask_human", "goal": "{{goal}}",
        "question": "Which tone?", "expects": "answer"})]);
    let engine = Engine::start(
        ws,
        catalog(Some(script), None),
        EngineConfig {
            guided_wake_timeout_secs: 1,
            ..guided_config()
        },
    )
    .unwrap();
    let mut rx = engine.events();
    let goal = engine.submit_goal(guided("ask me")).unwrap();
    let asking = wait_for(&mut rx, "asking", |e| {
        matches!(
            &e.payload,
            EnginePayload::Guided {
                status: GuidanceStatus::Asking,
                ..
            }
        )
    })
    .await;
    let EnginePayload::Guided { detail, .. } = asking.payload else {
        unreachable!()
    };
    assert_eq!(detail.as_deref(), Some("Which tone?"));
    // The turn ends (or times out) while asking: no stall is recorded.
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    assert!(
        !statuses(&engine, goal.id).contains(&GuidanceStatus::Stalled),
        "{:?}",
        statuses(&engine, goal.id)
    );
    // The person answers: the agent is working again.
    let gate = until("the question", || {
        engine
            .inbox()
            .into_iter()
            .find(|g| g.home.goal() == Some(goal.id))
    })
    .await;
    engine
        .decide(&gate.id, true, None, Some(&Answer::text("warm")), None)
        .unwrap();
    until("working again", || {
        let s = statuses(&engine, goal.id);
        (s.iter().position(|x| *x == GuidanceStatus::Asking)
            < s.iter().rposition(|x| *x == GuidanceStatus::Working))
        .then_some(())
    })
    .await;
    engine.shutdown().await;
}

/// A question the agent shaped as a decision is still answerable — Approve
/// carries a verdict — and the verdict resumes the design like an answer
/// would. This goal would otherwise stay pending forever: the
/// default `expects` was "decision", and only an answer woke the agent.
#[tokio::test(flavor = "multi_thread")]
async fn a_decision_shaped_design_question_is_answered_by_its_verdict_and_the_design_resumes() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "guided-harness");
    let script = IntakeScript::new(vec![json!({"op": "ask_human", "goal": "{{goal}}",
        "question": "Ship without a review step?", "expects": "decision"})]);
    let engine = Engine::start(
        ws,
        catalog(Some(script), None),
        EngineConfig {
            guided_wake_timeout_secs: 1,
            ..guided_config()
        },
    )
    .unwrap();
    let mut rx = engine.events();
    let goal = engine.submit_goal(guided("decide for me")).unwrap();
    wait_for(&mut rx, "asking", |e| {
        matches!(
            &e.payload,
            EnginePayload::Guided {
                status: GuidanceStatus::Asking,
                ..
            }
        )
    })
    .await;
    let gate = until("the question", || {
        engine
            .inbox()
            .into_iter()
            .find(|g| g.home.goal() == Some(goal.id))
    })
    .await;
    assert_eq!(gate.expects, bisa_core::AskKind::Decision);
    // The journal names the question by its stable subject, not the gate's id.
    let journal = engine
        .workspace()
        .journal(&bisa_core::Home::from(goal.id))
        .unwrap();
    assert!(
        journal.iter().any(|e| matches!(&e.payload, bisa_core::event::JournalPayload::Question { gate: g, .. } if *g == format!("ask_human:{}", goal.id))),
        "the question is journaled under ask_human:<goal>"
    );
    engine
        .decide(&gate.id, true, Some("yes, ship it"), None, None)
        .unwrap();
    until("working again", || {
        let s = statuses(&engine, goal.id);
        (s.iter().position(|x| *x == GuidanceStatus::Asking)
            < s.iter().rposition(|x| *x == GuidanceStatus::Working))
        .then_some(())
    })
    .await;
    engine.shutdown().await;
}

/// A question with no `expects` is an answer: the person types, never
/// approves. The default used to be a decision.
#[tokio::test(flavor = "multi_thread")]
async fn a_design_question_without_expects_is_an_answer() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "guided-harness");
    let script = IntakeScript::new(vec![
        json!({"op": "ask_human", "goal": "{{goal}}", "question": "Which tone?"}),
    ]);
    let engine = Engine::start(
        ws,
        catalog(Some(script), None),
        EngineConfig {
            guided_wake_timeout_secs: 1,
            ..guided_config()
        },
    )
    .unwrap();
    let goal = engine.submit_goal(guided("tone")).unwrap();
    let gate = until("the question", || {
        engine
            .inbox()
            .into_iter()
            .find(|g| g.home.goal() == Some(goal.id))
    })
    .await;
    assert!(gate.expects.is_answer(), "{:?}", gate.expects);
    engine.shutdown().await;
}

/// With guided mode off, a guided capture says so on the goal and a redesign
/// is refused.
#[tokio::test(flavor = "multi_thread")]
async fn guided_off_records_off_and_redesign_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    let engine = Engine::start(
        ws,
        HarnessCatalog::new(),
        EngineConfig {
            design_enabled: false,
            events_enabled: false,
            ..Default::default()
        },
    )
    .unwrap();
    let goal = engine.submit_goal(guided("nobody designs")).unwrap();
    assert_eq!(statuses(&engine, goal.id), vec![GuidanceStatus::Off]);
    let words = said(&engine, goal.id);
    assert!(words[0].contains("Designing is off"), "{words:?}");
    let err = engine.redesign(goal.id).unwrap_err();
    assert!(err.is_refusal());
    assert!(err.to_string().contains("designing is off"), "{err}");
    engine.shutdown().await;
}

/// Every refusal of a redesign, by name.
#[tokio::test(flavor = "multi_thread")]
async fn redesign_is_refused_when_there_is_nothing_to_design_or_someone_is_at_it() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "hanging-harness");
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::new(MockAdapter {
        id: "hanging-harness".into(),
        script: Some(vec![]),
        ..Default::default()
    }));
    catalog.register(Arc::new(MockAdapter {
        id: "worker-harness".into(),
        intake_script: Some(worker_yields()),
        ..Default::default()
    }));
    let engine = Engine::start(
        ws,
        catalog,
        EngineConfig {
            guided_wake_timeout_secs: 30,
            ..guided_config()
        },
    )
    .unwrap();
    let mut rx = engine.events();

    let manual = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..guided("by hand")
        })
        .unwrap();
    assert!(engine
        .redesign(manual.id)
        .unwrap_err()
        .to_string()
        .contains("is manual"));

    let wf = engine
        .create_workflow(new_workflow("w", vec![agent_step("do", "worker-harness")]))
        .unwrap();
    let pointed = engine
        .submit_goal(SubmitRequest {
            workflow: Some(wf.id),
            ..guided("has a workflow")
        })
        .unwrap();
    assert!(engine
        .redesign(pointed.id)
        .unwrap_err()
        .to_string()
        .contains("already has a workflow"));
    engine.start_run(pointed.id, BTreeMap::new()).unwrap();
    assert!(engine
        .redesign(pointed.id)
        .unwrap_err()
        .to_string()
        .contains("has a run"));

    let closed = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..guided("closed")
        })
        .unwrap();
    engine
        .close_goal(
            closed.id,
            bisa_core::ClosureReason::Abandoned { rationale: None },
        )
        .unwrap();
    assert!(engine
        .redesign(closed.id)
        .unwrap_err()
        .to_string()
        .contains("closed"));

    // A wake in flight: busy.
    let busy = engine.submit_goal(guided("being designed")).unwrap();
    wait_for(&mut rx, "working", |e| {
        e.goal == Some(busy.id)
            && matches!(
                &e.payload,
                EnginePayload::Guided {
                    status: GuidanceStatus::Working,
                    ..
                }
            )
    })
    .await;
    assert!(engine
        .redesign(busy.id)
        .unwrap_err()
        .to_string()
        .contains("already working"));
    engine.shutdown().await;
}

/// The guided session is registered against its goal, so the sessions roster
/// can name it.
#[tokio::test(flavor = "multi_thread")]
async fn a_guided_wake_registers_its_goal() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "hanging-harness");
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::new(MockAdapter {
        id: "hanging-harness".into(),
        script: Some(vec![]),
        ..Default::default()
    }));
    let engine = Engine::start(
        ws,
        catalog,
        EngineConfig {
            guided_wake_timeout_secs: 30,
            ..guided_config()
        },
    )
    .unwrap();
    let mut rx = engine.events();
    let goal = engine.submit_goal(guided("who am I for")).unwrap();
    wait_for(&mut rx, "working", |e| {
        matches!(
            &e.payload,
            EnginePayload::Guided {
                status: GuidanceStatus::Working,
                ..
            }
        )
    })
    .await;
    let entry = engine
        .inner()
        .registry
        .list()
        .into_iter()
        .find(|a| a.kind == bisa_engine::registry::SessionKind::Guided)
        .expect("a guided entry");
    assert_eq!(entry.goal, Some(goal.id));
    assert!(entry.work_item.is_none());
    engine.shutdown().await;
}

/// A fact that claims work in progress with no wake behind it reads as
/// stalled; at boot the engine says so and wakes again.
#[tokio::test(flavor = "multi_thread")]
async fn a_dangling_working_fact_reads_as_stalled_and_a_restart_rewakes() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "guided-harness");
    // A guided draft whose last fact says "working" — the previous process
    // died mid-design.
    let goal = ws
        .create_goal(bisa_store::NewGoal::captured("interrupted").mode(bisa_core::GoalMode::Guided))
        .unwrap();
    assert!(goal.mode.designs());
    let owner = ws.owner_keys().clone();
    ws.append_journal(
        &bisa_core::Home::from(goal.id),
        JournalPayload::Guidance {
            phase: GuidancePhase::Design,
            status: GuidanceStatus::Working,
            detail: None,
            session: None,
        },
        &owner,
        None,
    )
    .unwrap();
    let journal = ws.journal(&bisa_core::Home::from(goal.id)).unwrap();
    let status =
        bisa_engine::guided::design_status_from_journal(&goal, None, &journal, false).unwrap();
    assert_eq!(
        status.status,
        GuidanceStatus::Stalled,
        "nothing live behind it"
    );
    assert!(!status.live);

    let script = IntakeScript::new(vec![
        json!({"op": "propose_workflow", "agent": DRIVER, "goal": "{{goal}}",
        "workflow": proposal("Resumed")}),
    ]);
    let engine = Engine::start(ws, catalog(Some(script), None), guided_config()).unwrap();
    let mut rx = engine.events();
    wait_for(&mut rx, "the resumed proposal", |e| {
        matches!(&e.payload, EnginePayload::WorkflowProposed { .. })
    })
    .await;
    let facts = guidance_facts(&engine, goal.id);
    assert!(
        facts.iter().any(|(_, s, d)| *s == GuidanceStatus::Stalled
            && d.as_deref().unwrap_or("").contains("restart")),
        "{facts:?}"
    );
    assert_eq!(
        facts.last().map(|(_, s, _)| *s),
        Some(GuidanceStatus::Proposed)
    );
    engine.shutdown().await;
}

/// The Workflow Agent is told who it may name: every enabled agent and team,
/// with what each does and who is on each team — and nobody disabled, and
/// never a core agent.
#[tokio::test(flavor = "multi_thread")]
async fn a_guided_wake_names_every_enabled_agent_and_team_and_no_disabled_one() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "mock-guided");
    ws.install(CatalogKind::Team, "engineering").unwrap();
    ws.install(CatalogKind::Team, "design").unwrap();
    ws.set_agent_enabled(&AgentId::new("qa-engineer").unwrap(), false)
        .unwrap();
    ws.set_team_enabled(&bisa_core::TeamId::new("design").unwrap(), false)
        .unwrap();
    let adapter = Arc::new(MockAdapter {
        id: "mock-guided".into(),
        ..Default::default()
    });
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<dyn bisa_harness::HarnessAdapter>);
    let engine = Engine::start(ws, catalog, guided_config()).unwrap();
    engine.submit_goal(guided("staff me")).unwrap();
    let prompt = until("the prompt", || adapter.prompts().first().cloned()).await;
    // The directive names the block before the block itself appears: the
    // roster is the block that starts a paragraph with the word.
    let staff = prompt.split("\n\nSTAFF").nth(1).expect("a STAFF block");
    assert!(staff.contains("- developer "), "{staff}");
    assert!(staff.contains("- architect "), "{staff}");
    assert!(staff.contains("- engineering "), "{staff}");
    assert!(
        !staff.contains("qa-engineer"),
        "a disabled agent is not offered: {staff}"
    );
    assert!(
        !staff.contains("- design "),
        "a disabled team is not offered: {staff}"
    );
    assert!(!staff.contains(AgentId::GENERAL), "{staff}");
    assert!(!staff.contains(AgentId::WORKFLOW), "{staff}");
    assert!(
        prompt.contains("Phase: DESIGN"),
        "the directive rides the same prompt"
    );
    engine.shutdown().await;
}

/// A proposal with problems is refused **with** them, by step and kind, and
/// writes nothing: no workflow, no gate, no goal pointer.
#[tokio::test(flavor = "multi_thread")]
async fn a_proposal_with_problems_is_refused_with_them_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "guided-harness");
    let bad = json!({
        "name": "Broken",
        "steps": [{"id": "do", "name": "Do", "kind": "agent", "instructions": "x",
                   "assignee": {"agent": "nobody-here"}, "harness": ["worker-harness"],
                   "then": ["nowhere"]}]
    });
    let script = IntakeScript::new(vec![json!({
        "op": "propose_workflow", "agent": DRIVER, "goal": "{{goal}}", "workflow": bad
    })]);
    let replies = script.replies.clone();
    let engine = Engine::start(ws, catalog(Some(script), None), guided_config()).unwrap();
    let goal = engine.submit_goal(guided("needs a real agent")).unwrap();

    until("the refusal", || {
        (replies.lock().unwrap().len() == 1).then_some(())
    })
    .await;
    let reply = replies.lock().unwrap()[0].clone();
    assert_eq!(reply["ok"], json!(false), "{reply}");
    let kinds: Vec<&str> = reply["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["kind"].as_str().unwrap())
        .collect();
    assert!(kinds.contains(&"unknown_assignee"), "{kinds:?}");
    assert!(kinds.contains(&"unknown_step"), "{kinds:?}");
    assert!(reply["problems"][0]["step"].is_string());
    assert!(
        engine.workspace().list_workflows().unwrap().is_empty(),
        "nothing was recorded"
    );
    assert!(engine
        .workspace()
        .get_goal(goal.id)
        .unwrap()
        .workflow
        .is_none());
    assert!(engine.inbox().is_empty());
    engine.shutdown().await;
}

/// A run that fails on a step whose `on_fail` is `fail` wakes the Workflow
/// Agent in the repair phase; its corrected proposal is gated, and adopting
/// it starts a fresh run that succeeds.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_run_wakes_repair_and_the_corrected_proposal_is_gated() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "guided-harness");
    // The first run fails: its step runs on the harness that dies at launch.
    // The repair proposes the same step on the worker harness.
    let repair = IntakeScript::new(vec![
        json!({"op": "get_goal", "goal": "{{goal}}"}),
        json!({"op": "propose_workflow", "agent": DRIVER, "goal": "{{goal}}",
               "workflow": proposal("Repaired")}),
    ]);
    let replies = repair.replies.clone();
    let engine = Engine::start(
        ws,
        catalog(Some(repair), Some(worker_yields())),
        guided_config(),
    )
    .unwrap();
    let mut rx = engine.events();

    let wf = engine
        .create_workflow(new_workflow(
            "Doomed",
            vec![agent_step("do", "doomed-harness")],
        ))
        .unwrap();
    let goal = engine
        .submit_goal(SubmitRequest {
            workflow: Some(wf.id),
            ..guided("fragile")
        })
        .unwrap();
    engine.start_run(goal.id, BTreeMap::new()).unwrap();
    let failed = finished_run(&engine, goal.id).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed));

    wait_for(&mut rx, "the repair phase", |e| {
        matches!(
            &e.payload,
            EnginePayload::Guided {
                phase: GuidancePhase::Repair,
                status: GuidanceStatus::Working,
                ..
            }
        )
    })
    .await;
    let proposed = wait_for(&mut rx, "the repaired proposal", |e| {
        matches!(&e.payload, EnginePayload::WorkflowProposed { .. })
    })
    .await;
    let EnginePayload::WorkflowProposed { gate_id, .. } = proposed.payload else {
        unreachable!()
    };
    let gate_id = gate_id.expect("a guided goal's repair is gated");
    // The repair read the failure before proposing.
    until("the script to finish", || {
        (replies.lock().unwrap().len() == 2).then_some(())
    })
    .await;
    let seen = replies.lock().unwrap()[0].clone();
    assert_eq!(seen["run"]["status"], json!("failed"));
    assert_eq!(seen["run"]["steps"][0]["state"], json!("failed"));

    engine.decide(&gate_id, true, None, None, None).unwrap();
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert_ne!(done.id, failed.id, "a fresh run, not the failed one");
    assert_eq!(engine.workspace().get_goal(goal.id).unwrap().runs.len(), 2);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_guided_wake_walks_the_plan_when_a_model_hits_a_wall() {
    use bisa_harness::mock::{DeadModel, ModelFailure};
    use bisa_harness::{ModelChoice, ModelPlan, ModelStrategy};

    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "guided-harness");
    let mut core = ws.get_agent(&AgentId::workflow()).unwrap();
    core.models = ModelPlan {
        strategy: ModelStrategy::Fallback,
        effort: None,
        models: vec![ModelChoice::new("fable-5"), ModelChoice::new("opus-5")],
    };
    ws.update_agent(core).unwrap();

    let script = IntakeScript::new(vec![
        json!({"op": "get_goal", "goal": "{{goal}}"}),
        json!({"op": "revise_statement", "goal": "{{goal}}",
               "statement": "Write a sales report into ./reports",
               "why": "the scope was vague"}),
    ]);
    let adapter = Arc::new(MockAdapter {
        id: "guided-harness".into(),
        intake_script: Some(script),
        dead_models: vec![ModelFailure::new("fable-5", DeadModel::NoProgress)
            .reason("rate limited")
            .retry_after(300)],
        ..Default::default()
    });
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<dyn bisa_harness::HarnessAdapter>);
    let engine = Engine::start(ws, catalog, guided_config()).unwrap();

    let goal = engine.submit_goal(guided("write a sales report")).unwrap();
    until("the driver to sharpen the statement", || {
        let g = engine.workspace().get_goal(goal.id).unwrap();
        g.statement.contains("./reports").then_some(())
    })
    .await;
    assert_eq!(
        adapter.launched_models(),
        vec![Some("fable-5".to_string()), Some("opus-5".to_string())]
    );
    let switch = engine
        .workspace()
        .journal(&bisa_core::Home::from(goal.id))
        .unwrap()
        .into_iter()
        .find_map(|e| match e.payload {
            JournalPayload::Note { text } if text.contains("fable-5") => Some(text),
            _ => None,
        })
        .expect("the driver's switch must be journaled");
    assert!(switch.contains("retrying on opus-5"), "{switch}");
    engine.shutdown().await;
}

/// The driver is the Workflow Agent's own definition — its plan leads with
/// `claude-opus-5-5[1m]`, and an unresolved agent has no plan at all.
#[tokio::test(flavor = "multi_thread")]
async fn a_guided_wake_runs_the_workflow_agents_own_definition() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "mock-guided");
    let adapter = Arc::new(MockAdapter {
        id: "mock-guided".into(),
        ..Default::default()
    });
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<dyn bisa_harness::HarnessAdapter>);
    let engine = Engine::start(ws, catalog, guided_config()).unwrap();
    engine.submit_goal(guided("sell more socks")).unwrap();
    until("the guided wake to launch", || {
        adapter.launched_models().first().cloned()
    })
    .await;
    assert_eq!(
        adapter.launched_models().first().cloned().flatten(),
        Some("claude-opus-5-5[1m]".to_string())
    );
    engine.shutdown().await;
}

/// The Workflow Agent signs its own wake, with the owner's attestation.
#[tokio::test(flavor = "multi_thread")]
async fn a_guided_wakes_journal_event_is_signed_by_the_workflow_agent() {
    use bisa_harness::mock::{DeadModel, ModelFailure};
    use bisa_harness::{ModelChoice, ModelPlan, ModelStrategy};

    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "guided-harness");
    let mut core = ws.get_agent(&AgentId::workflow()).unwrap();
    core.models = ModelPlan {
        strategy: ModelStrategy::Fallback,
        effort: None,
        models: vec![ModelChoice::new("fable-5"), ModelChoice::new("opus-5")],
    };
    ws.update_agent(core).unwrap();
    let core_key = ws.get_agent(&AgentId::workflow()).unwrap().pubkey;
    let owner = ws.owner_principal();
    assert_ne!(core_key, owner);

    let adapter = Arc::new(MockAdapter {
        id: "guided-harness".into(),
        dead_models: vec![ModelFailure::new("fable-5", DeadModel::NoProgress)
            .reason("rate limited")
            .retry_after(300)],
        ..Default::default()
    });
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<dyn bisa_harness::HarnessAdapter>);
    let engine = Engine::start(ws, catalog, guided_config()).unwrap();
    let goal = engine.submit_goal(guided("sell more socks")).unwrap();

    let journal_path = engine.workspace().paths().goal(goal.id).journal();
    let switch = until("the wake's model-switch note", || {
        let body = std::fs::read_to_string(&journal_path).ok()?;
        body.lines()
            .filter_map(|l| nostr::event::Event::from_json(l).ok())
            .find(|e| e.content.contains("fable-5"))
    })
    .await;
    assert_eq!(switch.pubkey.to_hex(), core_key.as_hex());
    let attested = bisa_store::verify_attestation(&switch).expect("attested");
    assert_eq!(PrincipalId::new(attested).unwrap(), owner);
    engine.shutdown().await;
}

/// The wake's session says who it is: `--agent workflow-agent`.
#[tokio::test(flavor = "multi_thread")]
async fn a_guided_wake_identifies_itself_as_the_workflow_agent() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "mock-guided");
    let adapter = Arc::new(MockAdapter {
        id: "mock-guided".into(),
        ..Default::default()
    });
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<dyn bisa_harness::HarnessAdapter>);
    let engine = Engine::start(ws, catalog, guided_config()).unwrap();
    let goal = engine.submit_goal(guided("sell more socks")).unwrap();

    let args = until("the guided wake's bisa MCP server", || {
        adapter.launches().into_iter().find_map(|spec| {
            spec.mcp_servers.into_iter().find_map(|m| match m.config {
                bisa_harness::McpServerConfig::Stdio { name, args, .. } if name == "bisa" => {
                    Some(args)
                }
                _ => None,
            })
        })
    })
    .await;
    assert!(
        args.windows(2).any(|w| w[0] == "--agent" && w[1] == DRIVER),
        "{args:?}"
    );
    assert!(
        args.windows(2)
            .any(|w| w[0] == "--goal" && w[1] == goal.id.to_string()),
        "{args:?}"
    );
    engine.shutdown().await;
}

/// What the Workflow Agent says in the thread is redacted as what is
/// journaled about it is. The words of a wake that failed carry the
/// harness's own, and a harness that dies printing a token must not put the
/// token in the goal's thread — which goes to whoever the goal reaches.
#[tokio::test(flavor = "multi_thread")]
async fn what_the_workflow_agent_says_of_a_failure_carries_no_secret() {
    // Synthetic: the shape the redactor knows, and nobody's token.
    const TOKEN: &str = "ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "leaky-harness");
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::new(MockAdapter {
        id: "leaky-harness".into(),
        script: Some(vec![bisa_harness::SessionEvent::Lifecycle(
            bisa_harness::LifecycleEvent::Ended {
                outcome: bisa_harness::Outcome::Failed {
                    error: format!("the provider refused the key {TOKEN}"),
                },
                is_terminal: true,
            },
        )]),
        ..Default::default()
    }));
    let engine = Engine::start(ws, catalog, guided_config()).unwrap();
    let goal = engine.submit_goal(guided("sell more socks")).unwrap();

    let words = until("the failure to be said", || {
        let words = said(&engine, goal.id);
        words
            .iter()
            .any(|w| w.contains("the provider refused the key"))
            .then_some(words)
    })
    .await;
    assert!(
        words.iter().all(|w| !w.contains(TOKEN)),
        "the thread carries the token: {words:?}"
    );
    assert!(
        words.iter().any(|w| w.contains("«secret:")),
        "it carries the placeholder in the token's place: {words:?}"
    );
    // The sentence a reader in another language renders is the same words.
    let rows = engine
        .workspace()
        .messages(&goal.id.to_string(), None, 50)
        .unwrap();
    let as_said = serde_json::to_string(
        &rows
            .iter()
            .filter_map(|m| m.said.clone())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert!(as_said.contains("guided-say-failed"), "{as_said}");
    assert!(!as_said.contains(TOKEN), "{as_said}");
    // And the journal's fact, as it always was.
    let facts = guidance_facts(&engine, goal.id);
    assert!(
        facts
            .iter()
            .all(|(_, _, d)| !d.as_deref().unwrap_or("").contains(TOKEN)),
        "{facts:?}"
    );
    engine.shutdown().await;
}

/// When the Workflow Agent cannot be resolved, the goal records a failure
/// that names it, the thread hears why, and nothing runs.
#[tokio::test(flavor = "multi_thread")]
async fn an_unresolvable_workflow_agent_records_failed_and_starts_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "mock-guided");
    std::fs::write(
        ws.paths().agent_file(&AgentId::workflow()),
        "this is not json",
    )
    .unwrap();
    let adapter = Arc::new(MockAdapter {
        id: "mock-guided".into(),
        ..Default::default()
    });
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<dyn bisa_harness::HarnessAdapter>);
    let engine = Engine::start(ws, catalog, guided_config()).unwrap();
    let goal = engine.submit_goal(guided("sell more socks")).unwrap();

    until("the failure to be journaled", || {
        guidance_facts(&engine, goal.id)
            .iter()
            .any(|(_, s, d)| {
                *s == GuidanceStatus::Failed && d.as_deref().unwrap_or("").contains(DRIVER)
            })
            .then_some(())
    })
    .await;
    // The fact lands first, the words in the thread after it: polled.
    let words = until("the failure to be said", || {
        let words = said(&engine, goal.id);
        (!words.is_empty()).then_some(words)
    })
    .await;
    assert!(
        words
            .iter()
            .any(|w| w.contains(DRIVER) && w.contains("pick a workflow by hand")),
        "{words:?}"
    );
    assert!(adapter.launched_models().is_empty());
    engine.shutdown().await;
}

/// A live guided wake never starves the workers it exists to help: with a
/// cap of one, a run's agent step still finishes while the driver is
/// mid-turn waiting on a person.
#[tokio::test(flavor = "multi_thread")]
async fn a_guided_wake_does_not_hold_the_concurrency_cap() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "guided-harness");
    let guide = IntakeScript::new(vec![
        json!({"op": "ask_human", "goal": "{{goal}}",
               "question": "Which directory?", "expects": "answer"}),
        json!({"op": "await_decision", "gate": "{{gate}}"}),
    ]);
    let engine = Engine::start(
        ws,
        catalog(Some(guide), Some(worker_yields())),
        EngineConfig {
            design_enabled: true,
            events_enabled: false,
            global_concurrency: 1,
            per_adapter_concurrency: 1,
            ..Default::default()
        },
    )
    .unwrap();

    let guided = engine.submit_goal(guided("guided one")).unwrap();
    let question = until("the wake to be live and waiting on a human", || {
        engine
            .inbox()
            .into_iter()
            .find(|g| g.gate == Gate::Escalation && g.home.goal() == Some(guided.id))
    })
    .await;

    let (work, _) = run_on(
        &engine,
        "worker one",
        new_workflow("w", vec![agent_step("go", "worker-harness")]),
    );
    let done = finished_run(&engine, work.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert!(items_of(&engine, work.id)
        .iter()
        .all(|i| matches!(i.state, WorkItemState::Accepted)));
    assert!(
        engine.inbox().iter().any(|g| g.id == question.id),
        "the guided wake must still be waiting"
    );
    engine
        .decide(
            &question.id,
            true,
            None,
            Some(&Answer::text("./reports")),
            None,
        )
        .unwrap();
    engine.shutdown().await;
}

/// Spawning from a work item is gated twice over — a depth budget AND an
/// allowlist — so a runaway agent needs two mistakes, not one.
#[tokio::test(flavor = "multi_thread")]
async fn sub_goal_spawning_respects_depth_budget() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::start(workspace(&dir), catalog(None, None), design_off_config()).unwrap();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..guided("parent")
        })
        .unwrap();
    let item = |depth: u8, allow: Vec<String>| bisa_core::WorkItemSpec {
        id: bisa_core::WorkItemId::from_ulid(ulid::Ulid::from_datetime(
            std::time::SystemTime::now(),
        )),
        home: bisa_core::Home::Goal { goal: goal.id },
        run: None,
        step: None,
        instructions: "do the work".into(),
        state: WorkItemState::Open,
        project: None,
        harness_candidates: vec!["worker-harness".into()],
        model: None,
        effort: None,
        output_schema: None,
        budget: Default::default(),
        assignees: vec![],
        tier_ceiling: bisa_core::ToolTier::Write,
        agent: None,
        spawn_allowlist: allow,
        depth_budget: depth,
        result_attempts: 0,
        interruptions: 0,
    };

    let leaf = item(0, vec!["*".into()]);
    engine.workspace().put_work_item(&leaf).unwrap();
    let refused = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "spawn_sub_goal", "work_item": leaf.id.to_string(), "statement": "child work"}),
    )
    .await;
    assert_eq!(refused["ok"], json!(false), "leaf may not spawn: {refused}");

    let unlisted = item(2, vec![]);
    engine.workspace().put_work_item(&unlisted).unwrap();
    let reply = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "spawn_sub_goal", "work_item": unlisted.id.to_string(), "statement": "child work"}),
    )
    .await;
    assert_eq!(
        reply["ok"],
        json!(false),
        "an empty allowlist refuses: {reply}"
    );

    let deep = item(2, vec!["*".into()]);
    engine.workspace().put_work_item(&deep).unwrap();
    let allowed = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "spawn_sub_goal", "work_item": deep.id.to_string(), "statement": "child work"}),
    )
    .await;
    assert_eq!(allowed["ok"], json!(true), "budgeted spawn: {allowed}");
    let child: bisa_core::GoalId = allowed["child"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        engine.workspace().get_goal(child).unwrap().origin,
        bisa_core::GoalOrigin::Spawned { parent: goal.id }
    );
    engine.shutdown().await;
}

/// A Team is agents and humans together: work runs on the team's agent, and
/// the team's human is the one who may decide its approval steps.
#[tokio::test(flavor = "multi_thread")]
async fn team_routes_work_to_agents_and_gates_to_humans() {
    use bisa_core::Assignee;
    use bisa_store::GatePolicy;

    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    ws.install(CatalogKind::Agent, "developer").unwrap();
    let teammate = PrincipalId::new("cd".repeat(32)).unwrap();
    ws.add_member(
        teammate.clone(),
        MemberRole::Member,
        bisa_store::Admission {
            label: Some("teammate".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let stranger = PrincipalId::new("ab".repeat(32)).unwrap();
    ws.add_member(
        stranger.clone(),
        MemberRole::Member,
        bisa_store::Admission::default(),
    )
    .unwrap();
    let team = ws
        .create_team(
            "delivery",
            Some("ship it"),
            vec![
                Assignee::Human(teammate.clone()),
                Assignee::Agent("developer".into()),
            ],
            Default::default(),
        )
        .unwrap();
    ws.set_gate_policy(
        Gate::Approval,
        GatePolicy::Listed(vec![format!("team:{}", team.id)]),
    )
    .unwrap();
    assert!(ws.gate_policy_allows(Gate::Approval, &teammate).unwrap());
    assert!(!ws.gate_policy_allows(Gate::Approval, &stranger).unwrap());
    let developer = ws.team_agents(&team.id).unwrap();
    assert_eq!(developer.len(), 1);
    let mut dev = ws.get_agent(&developer[0]).unwrap();
    dev.harness = "worker-harness".into();
    ws.update_agent(dev).unwrap();
    let root = ws.root().to_path_buf();

    let engine = Engine::start(
        ws,
        catalog(None, Some(worker_yields())),
        design_off_config(),
    )
    .unwrap();
    // The goal is the team's: its agent step routes to the team's agent, and
    // its approval step is the team's human's to decide.
    let wf = engine
        .create_workflow(new_workflow(
            "team work",
            chain(vec![
                agent_step("build", "worker-harness"),
                step(
                    "ship",
                    StepKind::Approval {
                        prompt: "Ship?".into(),
                    },
                ),
            ]),
        ))
        .unwrap();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            workflow: Some(wf.id),
            assignees: vec![Assignee::Team(team.id.to_string())],
            ..guided("team work")
        })
        .unwrap();
    engine.start_run(goal.id, BTreeMap::new()).unwrap();
    let gate = until("the approval step's gate", || {
        engine
            .inbox()
            .into_iter()
            .find(|g| g.home.goal() == Some(goal.id))
    })
    .await;
    // The owner is not on the team, so the owner cannot sign this gate.
    assert!(engine.decide(&gate.id, true, None, None, None).is_err());
    engine.shutdown().await;

    let ws2 = Workspace::open_with_keystore(&root, Box::new(MemoryKeyStore::default())).unwrap();
    let items = ws2
        .list_work_items(&bisa_core::Home::from(goal.id))
        .unwrap();
    assert_eq!(
        items[0].agent.as_deref(),
        Some("developer"),
        "the team's agent took the step"
    );
}

/// The Workflow Agent is told the rules in so many words: placement (every
/// agent step runs in a project, the scratch folder is for designing),
/// staffing (name an agent or a team from STAFF, never a core agent, an
/// `assignee` input when nobody fits) and topology (wire every `then`, end in
/// an `end`, a loop's target keeps `join: all`; a stall means rewire).
#[test]
fn the_directives_say_only_the_phase() {
    use bisa_engine::guided::{
        AUTO_ADDENDUM, DESIGN_DIRECTIVE, DESIGN_PLACEMENT, GUIDED_ADDENDUM, REPAIR_DIRECTIVE,
    };
    // The design directive is the phase and what the prompt carries; the
    // method — staff, topology, outputs, shape, placement — is the agent's
    // definition, said once (`library/core/workflow-agent.toml`).
    for word in [
        "Phase: DESIGN",
        "GOAL",
        "STAFF",
        "CONNECTORS",
        "TEMPLATES",
        "propose_workflow",
    ] {
        assert!(
            DESIGN_DIRECTIVE.contains(word),
            "{word}: {DESIGN_DIRECTIVE}"
        );
    }
    for restated in [
        "Topology:",
        "`output_schema`",
        "every agent step runs in a project",
        "at most one round",
        "{ \"team\": id }",
    ] {
        assert!(
            !DESIGN_DIRECTIVE.contains(restated),
            "the directive restates the method: {restated}"
        );
    }
    // How the work begins is the one thing a design decides that the method
    // cannot: by hand always, on an event when the goal says so — and the
    // event is read in that start's mapping and nowhere else.
    for word in ["`start` step", "event start", "`inputs` mapping"] {
        assert!(
            DESIGN_DIRECTIVE.contains(word),
            "{word}: {DESIGN_DIRECTIVE}"
        );
    }
    // An auto goal arms alone only what nobody needs to see.
    for word in [
        "listens for the events",
        "a hook",
        "waits for a person's Adopt",
    ] {
        assert!(AUTO_ADDENDUM.contains(word), "{word}: {AUTO_ADDENDUM}");
    }
    // The mode addendum owns the ask rule: guided teaches asking once, auto
    // says ask nothing and nothing before it said otherwise.
    assert!(GUIDED_ADDENDUM.contains("one round"), "{GUIDED_ADDENDUM}");
    assert!(AUTO_ADDENDUM.contains("Ask nothing"), "{AUTO_ADDENDUM}");
    // A repair is a new proposal; a finished run is not amended.
    for word in [
        "Phase: REPAIR",
        "propose_workflow",
        "stalled:",
        "names no project",
    ] {
        assert!(
            REPAIR_DIRECTIVE.contains(word),
            "{word}: {REPAIR_DIRECTIVE}"
        );
    }
    assert!(
        !REPAIR_DIRECTIVE.contains("amend_workflow"),
        "{REPAIR_DIRECTIVE}"
    );
    // Where the designer stands, and the one way a project is made.
    assert!(DESIGN_PLACEMENT.contains("scratch folder"));
    assert!(DESIGN_PLACEMENT.contains("create_project"));
}

/// The wake's prompt carries what the agent used to fetch: the goal — its
/// title, statement, mode and projects — the staff, the connectors and the
/// templates, and where the session stands. Attach a project and ask again:
/// the next prompt lists it with its path.
#[tokio::test(flavor = "multi_thread")]
async fn a_design_wake_carries_the_goal_the_staff_the_connectors_and_the_templates() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "guided-harness");
    let script = IntakeScript::new(vec![
        json!({"op": "add_note", "goal": "{{goal}}", "text": "read the prompt, nothing else"}),
    ]);
    let adapter = Arc::new(MockAdapter {
        id: "guided-harness".into(),
        intake_script: Some(script),
        ..Default::default()
    });
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<dyn bisa_harness::HarnessAdapter>);
    let engine = Engine::start(
        ws,
        catalog,
        EngineConfig {
            guided_wake_timeout_secs: 1,
            ..guided_config()
        },
    )
    .unwrap();

    let goal = engine
        .submit_goal(SubmitRequest {
            title: Some("Quarterly sales report".into()),
            ..guided("write a sales report for the quarter")
        })
        .unwrap();
    let prompt = until("the prompt", || adapter.prompts().first().cloned()).await;
    for word in [
        "Phase: DESIGN",
        "GOAL",
        "Title: Quarterly sales report",
        "Statement: write a sales report for the quarter",
        "Mode: guided",
        "Projects: none",
        "create_project",
        "STAFF",
        "CONNECTORS",
        "TEMPLATES",
        "catalog software-feature",
        "scratch folder",
    ] {
        assert!(prompt.contains(word), "{word} missing from:\n{prompt}");
    }
    assert!(
        prompt.find("GOAL").unwrap() < prompt.find("STAFF").unwrap(),
        "the goal comes before the staff"
    );

    // The turn ends without a proposal; a project appears; the next design
    // wake lists it with its path.
    until("the stall", || {
        statuses(&engine, goal.id)
            .contains(&GuidanceStatus::Stalled)
            .then_some(())
    })
    .await;
    let project = engine
        .workspace()
        .create_project(bisa_store::NewProject::managed("reports").unwrap())
        .unwrap();
    engine.workspace().attach(goal.id, project.id).unwrap();
    engine.redesign(goal.id).unwrap();
    let second = until("the second prompt", || adapter.prompts().get(1).cloned()).await;
    let path = engine.workspace().project_root_path(&project);
    assert!(
        second.contains(&format!("- reports ({})", path.display())),
        "{second}"
    );
    assert!(!second.contains("Projects: none"), "{second}");
    engine.shutdown().await;
}

/// A goal captured with the agents and teams that carry it is designed with
/// them alone: the wake's `STAFF` block is the goal's — the team, its member
/// and the named agent — and says nobody else may be named, while an
/// installed agent the goal does not name is left out of it.
#[tokio::test(flavor = "multi_thread")]
async fn a_design_wake_for_a_goal_that_names_who_carries_it_lists_them_alone() {
    use bisa_core::{Assignee, Tags};
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "guided-harness");
    for slug in ["developer", "mobile-developer", "code-reviewer"] {
        ws.install(CatalogKind::Agent, slug).unwrap();
    }
    let team = ws
        .create_team(
            "mobile",
            None,
            vec![Assignee::Agent("mobile-developer".into())],
            Tags::default(),
        )
        .unwrap();
    let script = IntakeScript::new(vec![
        json!({"op": "add_note", "goal": "{{goal}}", "text": "read the prompt, nothing else"}),
    ]);
    let adapter = Arc::new(MockAdapter {
        id: "guided-harness".into(),
        intake_script: Some(script),
        ..Default::default()
    });
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<dyn bisa_harness::HarnessAdapter>);
    let engine = Engine::start(
        ws,
        catalog,
        EngineConfig {
            guided_wake_timeout_secs: 1,
            ..guided_config()
        },
    )
    .unwrap();

    engine
        .submit_goal(SubmitRequest {
            assignees: vec![
                Assignee::Team(team.id.to_string()),
                Assignee::Agent("code-reviewer".into()),
            ],
            ..guided("ship the mobile app")
        })
        .unwrap();
    let prompt = until("the prompt", || adapter.prompts().first().cloned()).await;
    // The block, not the directive's mention of it: the block opens on its own header.
    let staff = &prompt[prompt.find("STAFF — ").expect("a STAFF block")..];
    let staff = &staff[..staff.find("CONNECTORS").unwrap_or(staff.len())];
    assert!(
        staff.starts_with("STAFF — the person named who carries this goal"),
        "{staff}"
    );
    assert!(staff.contains("Nobody else may be named"), "{staff}");
    for named in [
        "- code-reviewer ",
        "- mobile-developer ",
        &format!("- {} ", team.id),
    ] {
        assert!(
            staff.lines().any(|l| l.starts_with(named)),
            "{named} missing from:\n{staff}"
        );
    }
    assert!(
        !staff.lines().any(|l| l.starts_with("- developer ")),
        "an installed agent the goal does not name is left out:\n{staff}"
    );
    engine.shutdown().await;
}

/// The prompt is built once per wake: a relaunch after a model wall sends
/// the same words, byte for byte.
#[tokio::test(flavor = "multi_thread")]
async fn a_design_prompt_is_built_once_across_a_model_wall() {
    use bisa_harness::mock::{DeadModel, ModelFailure};
    use bisa_harness::{ModelChoice, ModelPlan, ModelStrategy};

    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "guided-harness");
    let mut core = ws.get_agent(&AgentId::workflow()).unwrap();
    core.models = ModelPlan {
        strategy: ModelStrategy::Fallback,
        effort: None,
        models: vec![ModelChoice::new("fable-5"), ModelChoice::new("opus-5")],
    };
    ws.update_agent(core).unwrap();
    let script = IntakeScript::new(vec![json!({"op": "revise_statement", "goal": "{{goal}}",
               "statement": "Write a sales report into ./reports",
               "why": "the scope was vague"})]);
    let adapter = Arc::new(MockAdapter {
        id: "guided-harness".into(),
        intake_script: Some(script),
        dead_models: vec![ModelFailure::new("fable-5", DeadModel::NoProgress)
            .reason("rate limited")
            .retry_after(300)],
        ..Default::default()
    });
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<dyn bisa_harness::HarnessAdapter>);
    let engine = Engine::start(ws, catalog, guided_config()).unwrap();

    let goal = engine.submit_goal(guided("write a sales report")).unwrap();
    until("the driver to sharpen the statement", || {
        let g = engine.workspace().get_goal(goal.id).unwrap();
        g.statement.contains("./reports").then_some(())
    })
    .await;
    let prompts = adapter.prompts();
    assert_eq!(prompts.len(), 2, "one prompt per model attempt");
    assert_eq!(prompts[0], prompts[1], "the same words on the relaunch");
    engine.shutdown().await;
}

/// A question asked during a repair, answered once the session that asked it
/// is gone, resumes the repair — never a design the goal has moved past.
#[tokio::test(flavor = "multi_thread")]
async fn an_answer_during_repair_resumes_repair() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "guided-harness");
    let repair = IntakeScript::new(vec![json!({"op": "ask_human", "goal": "{{goal}}",
        "question": "Retry on which harness?", "expects": "answer"})]);
    let engine = Engine::start(
        ws,
        catalog(Some(repair), Some(worker_yields())),
        EngineConfig {
            guided_wake_timeout_secs: 1,
            ..guided_config()
        },
    )
    .unwrap();
    let mut rx = engine.events();
    let wf = engine
        .create_workflow(new_workflow(
            "Doomed",
            vec![agent_step("do", "doomed-harness")],
        ))
        .unwrap();
    let goal = engine
        .submit_goal(SubmitRequest {
            workflow: Some(wf.id),
            ..guided("fragile")
        })
        .unwrap();
    engine.start_run(goal.id, BTreeMap::new()).unwrap();
    let failed = finished_run(&engine, goal.id).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed));
    wait_for(&mut rx, "the repair question", |e| {
        matches!(
            &e.payload,
            EnginePayload::Guided {
                phase: GuidancePhase::Repair,
                status: GuidanceStatus::Asking,
                ..
            }
        )
    })
    .await;
    // The turn times out while asking; the session that asked is gone.
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    let gate = until("the question", || {
        engine
            .inbox()
            .into_iter()
            .find(|g| g.home.goal() == Some(goal.id))
    })
    .await;
    engine
        .decide(
            &gate.id,
            true,
            None,
            Some(&Answer::text("the worker harness")),
            None,
        )
        .unwrap();
    wait_for(&mut rx, "the repair working again", |e| {
        matches!(
            &e.payload,
            EnginePayload::Guided {
                phase: GuidancePhase::Repair,
                status: GuidanceStatus::Working,
                ..
            }
        )
    })
    .await;
    let phases: Vec<GuidancePhase> = engine
        .workspace()
        .journal(&bisa_core::Home::from(goal.id))
        .unwrap()
        .iter()
        .filter_map(|e| match &e.payload {
            JournalPayload::Guidance { phase, .. } => Some(*phase),
            _ => None,
        })
        .collect();
    assert!(
        phases.iter().all(|p| *p == GuidancePhase::Repair),
        "an answer never turned a repair into a design: {phases:?}"
    );
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// The goal's mode: auto adopts alone, manual wakes nobody
// ---------------------------------------------------------------------------

/// A one-step proposal whose one input has no default: it cannot start with
/// nobody present, so an auto goal falls back to asking.
fn proposal_needing_an_input(name: &str) -> serde_json::Value {
    json!({
        "name": name,
        "description": "one step over an input a person must give",
        "inputs": [{"name": "audience", "label": "Audience", "kind": "text", "required": true}],
        "steps": [{"id": "do", "name": "Do it", "kind": "agent",
                   "instructions": "do {goal.statement} for {inputs.audience}", "harness": ["worker-harness"]}]
    })
}

/// Auto: the capture wakes the Workflow Agent, whose proposal the platform
/// adopts by a note and starts at once — no Adopt gate, nothing in the inbox,
/// the run walks to done on the worker, and the thread says so in one line.
#[tokio::test(flavor = "multi_thread")]
async fn an_auto_capture_is_adopted_and_started_without_a_gate() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "guided-harness");
    let script = IntakeScript::new(vec![
        json!({"op": "get_goal", "goal": "{{goal}}"}),
        json!({"op": "propose_workflow", "agent": DRIVER, "goal": "{{goal}}",
               "workflow": proposal("Unattended")}),
    ]);
    let replies = script.replies.clone();
    let engine = Engine::start(
        ws,
        catalog(Some(script), Some(worker_yields())),
        guided_config(),
    )
    .unwrap();
    let mut rx = engine.events();

    let goal = engine
        .submit_goal(SubmitRequest::captured("write a sales report"))
        .unwrap();
    assert_eq!(goal.mode, bisa_core::GoalMode::Auto, "a capture is auto");

    let proposed = wait_for(&mut rx, "the proposal", |e| {
        matches!(&e.payload, EnginePayload::WorkflowProposed { .. })
    })
    .await;
    let EnginePayload::WorkflowProposed {
        workflow, gate_id, ..
    } = proposed.payload
    else {
        unreachable!()
    };
    assert_eq!(gate_id, None, "an auto goal opens no Adopt gate");
    assert!(
        engine
            .inbox()
            .iter()
            .all(|g| g.home.goal() != Some(goal.id)),
        "nothing waits on a person"
    );
    // The agent read its goal's mode, so its directive could say nobody adopts.
    until("the script to finish", || {
        (replies.lock().unwrap().len() == 2).then_some(())
    })
    .await;
    assert_eq!(replies.lock().unwrap()[0]["goal"]["mode"], json!("auto"));
    assert!(replies.lock().unwrap()[1]["gate"].is_null());

    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert_eq!(done.workflow.id, workflow);
    let g = engine.workspace().get_goal(goal.id).unwrap();
    assert_eq!(g.workflow, Some(workflow));
    assert_eq!(g.runs.len(), 1);
    // Adopted by a note, never by a forged decision.
    let notes = notes(&engine, goal.id);
    assert!(
        notes
            .iter()
            .any(|n| n.contains("adopted") && n.contains("auto mode")),
        "{notes:?}"
    );
    assert!(
        !engine
            .workspace()
            .journal(&bisa_core::Home::from(goal.id))
            .unwrap()
            .iter()
            .any(|e| matches!(e.payload, JournalPayload::Decision { .. })),
        "no decision is signed for the platform's own adoption"
    );
    let words = said(&engine, goal.id);
    assert_eq!(
        words.len(),
        1,
        "one line, not a running commentary: {words:?}"
    );
    assert!(words[0].contains("started it"), "{words:?}");
    engine.shutdown().await;
}

/// Auto, the adoption judged, and the judgement breaks: the proposal is put
/// to the person with what happened — an auto goal never hangs on a task
/// that is gone, proposed and adopted by nobody.
#[tokio::test(flavor = "multi_thread")]
async fn an_adoption_whose_judgement_panics_is_put_to_the_person() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "guided-harness");
    let script = IntakeScript::new(vec![
        json!({"op": "propose_workflow", "agent": DRIVER, "goal": "{{goal}}",
               "workflow": proposal("Judged")}),
    ]);
    let engine = Engine::start(
        ws,
        catalog(Some(script), Some(worker_yields())),
        guided_config(),
    )
    .unwrap();
    engine
        .set_setting(
            bisa_core::SettingScope::Workspace,
            None,
            "decisions.enabled",
            json!(true),
        )
        .unwrap();
    engine.stand_in_decision_provider(Some(Arc::new(PanickingProvider)));

    let goal = engine
        .submit_goal(SubmitRequest::captured("write a sales report"))
        .unwrap();
    assert_eq!(goal.mode, bisa_core::GoalMode::Auto);
    let asked = until("the adoption to be put to the person", || {
        engine
            .inbox()
            .into_iter()
            .find(|g| g.home.goal() == Some(goal.id) && g.subject.starts_with("adopt:"))
    })
    .await;
    assert!(
        asked.question.contains("panicked"),
        "the question says why it is asked after all: {}",
        asked.question
    );
    assert!(
        engine
            .workspace()
            .get_goal(goal.id)
            .unwrap()
            .runs
            .is_empty(),
        "nothing ran alone after the judgement broke"
    );
    engine.shutdown().await;
}

/// Auto, but the design declares a required input with no default: the
/// platform cannot start it unattended, so the Adopt gate opens after all,
/// its question saying why, and the person's decision carries the input.
#[tokio::test(flavor = "multi_thread")]
async fn an_auto_proposal_that_cannot_start_unattended_opens_the_adopt_gate() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "guided-harness");
    let script = IntakeScript::new(vec![
        json!({"op": "propose_workflow", "agent": DRIVER, "goal": "{{goal}}",
               "workflow": proposal_needing_an_input("Needs an audience")}),
    ]);
    let engine = Engine::start(
        ws,
        catalog(Some(script), Some(worker_yields())),
        guided_config(),
    )
    .unwrap();
    let mut rx = engine.events();
    let goal = engine
        .submit_goal(SubmitRequest::captured("write a pitch"))
        .unwrap();
    let proposed = wait_for(&mut rx, "the proposal", |e| {
        matches!(&e.payload, EnginePayload::WorkflowProposed { .. })
    })
    .await;
    let EnginePayload::WorkflowProposed { gate_id, .. } = proposed.payload else {
        unreachable!()
    };
    let gate_id = gate_id.expect("an input nobody can default falls back to the person");
    let gate = engine.gate(&gate_id).unwrap();
    assert_eq!(gate.gate, Gate::Approval);
    assert!(gate.subject.starts_with("adopt:"), "{}", gate.subject);
    assert!(gate.question.contains("It needs you"), "{}", gate.question);
    assert!(gate.question.contains("audience"), "{}", gate.question);
    assert!(
        engine.workspace().get_goal(goal.id).unwrap().run.is_none(),
        "nothing started"
    );
    let words = said(&engine, goal.id);
    assert!(words[0].contains("needs you"), "{words:?}");

    // The person adopts with the input, and the run walks.
    engine
        .decide(
            &gate_id,
            true,
            None,
            None,
            Some(BTreeMap::from([(
                "audience".to_string(),
                json!("founders"),
            )])),
        )
        .unwrap();
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    engine.shutdown().await;
}

/// A proposal that begins on `on` — one event start, then one step on the
/// worker.
fn proposal_starting_on(name: &str, id: &str, on: serde_json::Value) -> serde_json::Value {
    json!({
        "name": name,
        "description": "begins on an event, proposed by the driver",
        "steps": [
            {"id": id, "name": "When it happens", "kind": "start", "on": on, "then": ["do"]},
            {"id": "do", "name": "Do it", "kind": "agent",
             "instructions": "do {goal.statement}", "harness": ["worker-harness"]}
        ]
    })
}

/// Auto, and the design begins on a schedule: nobody needs to see a clock
/// armed, so the platform adopts it alone and the goal **listens** — nothing
/// runs until the schedule comes due, and the thread says so.
#[tokio::test(flavor = "multi_thread")]
async fn an_auto_design_that_begins_on_a_schedule_is_adopted_and_listens() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "guided-harness");
    let script = IntakeScript::new(vec![
        json!({"op": "propose_workflow", "agent": DRIVER, "goal": "{{goal}}",
               "workflow": proposal_starting_on(
                   "Weekly digest", "weekly", json!({"event": "schedule", "every": 604_800}))}),
    ]);
    let engine = Engine::start(
        ws,
        catalog(Some(script), Some(worker_yields())),
        guided_config(),
    )
    .unwrap();
    let mut rx = engine.events();
    let goal = engine
        .submit_goal(SubmitRequest::captured(
            "every week, post a digest of the work",
        ))
        .unwrap();
    let proposed = wait_for(&mut rx, "the proposal", |e| {
        matches!(&e.payload, EnginePayload::WorkflowProposed { .. })
    })
    .await;
    let EnginePayload::WorkflowProposed { gate_id, .. } = proposed.payload else {
        unreachable!()
    };
    assert_eq!(gate_id, None, "a schedule is armed with nobody asked");
    let listening = until("the goal to listen", || {
        let g = engine.workspace().get_goal(goal.id).ok()?;
        g.is_listening().then_some(g)
    })
    .await;
    assert_eq!(listening.run, None, "listening is not a run");
    assert_eq!(listening.status(None), bisa_core::GoalStatus::Waiting);
    let words = said(&engine, goal.id);
    assert_eq!(words.len(), 1, "{words:?}");
    assert!(words[0].contains("it is listening"), "{words:?}");

    // The schedule comes due: the run the goal was captured for.
    let host = bisa_core::ListenerHost::Goal { goal: goal.id };
    let t0 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    engine.tick_listeners_at(t0).await;
    engine.tick_listeners_at(t0 + 604_800).await;
    assert_eq!(engine.drain_signals().await, 1);
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert_eq!(done.start, Some(sid("weekly")));
    assert!(
        engine
            .workspace()
            .listening(&host)
            .unwrap()
            .is_some_and(|l| !l.is_paused()),
        "and it goes on listening"
    );
    engine.shutdown().await;
}

/// Auto, and the design begins on a hook: what a caller outside may start is
/// a person's to arm, so the Adopt gate opens after all, saying why; adopted,
/// the goal listens and the decision shows its public hook's secret.
#[tokio::test(flavor = "multi_thread")]
async fn an_auto_design_that_begins_on_a_hook_waits_for_a_person_to_arm_it() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "guided-harness");
    let script = IntakeScript::new(vec![
        json!({"op": "propose_workflow", "agent": DRIVER, "goal": "{{goal}}",
               "workflow": proposal_starting_on(
                   "On a ticket", "ticket", json!({"event": "hook", "public": true}))}),
    ]);
    let engine = Engine::start(
        ws,
        catalog(Some(script), Some(worker_yields())),
        guided_config(),
    )
    .unwrap();
    let mut rx = engine.events();
    let goal = engine
        .submit_goal(SubmitRequest::captured(
            "whenever the help desk calls, triage the ticket",
        ))
        .unwrap();
    let proposed = wait_for(&mut rx, "the proposal", |e| {
        matches!(&e.payload, EnginePayload::WorkflowProposed { .. })
    })
    .await;
    let EnginePayload::WorkflowProposed { gate_id, .. } = proposed.payload else {
        unreachable!()
    };
    let gate_id = gate_id.expect("a hook is a person's to arm");
    let gate = engine.gate(&gate_id).unwrap();
    assert!(gate.subject.starts_with("adopt:"), "{}", gate.subject);
    assert!(gate.question.contains("It needs you"), "{}", gate.question);
    assert!(
        gate.question.contains("`ticket` starts on a hook"),
        "{}",
        gate.question
    );
    let host = bisa_core::ListenerHost::Goal { goal: goal.id };
    assert_eq!(engine.workspace().listening(&host).unwrap(), None);
    assert!(
        engine.armed_listeners().armed.is_empty(),
        "nothing is armed"
    );

    // Adopted: the goal listens, and the decision's answer shows the secret
    // its public hook was minted — once.
    let outcome = engine
        .decide_durable(
            &bisa_core::Home::Goal { goal: goal.id },
            true,
            None,
            None,
            None,
            Some(&gate_id),
            None,
        )
        .unwrap();
    let adopted = engine.workspace().get_goal(goal.id).unwrap();
    assert!(adopted.is_listening(), "adopted, it listens");
    assert_eq!(adopted.run, None);
    let ticket = bisa_core::ListenerKey {
        host,
        step: sid("ticket"),
    };
    assert_eq!(outcome.secrets.len(), 1, "{:?}", outcome.secrets);
    let shown = &outcome.secrets[0];
    assert_eq!(shown.step, sid("ticket"));
    assert_eq!(shown.path, format!("/hooks/{host}/ticket"));
    assert_eq!(
        hex::decode(&shown.secret).unwrap(),
        engine.workspace().hook_secret(&ticket).unwrap()
    );
    // Later it is read by rotating it: a new one, shown once.
    let rotated = engine.rotate_hook_secret(&ticket).unwrap();
    assert_ne!(rotated.secret, shown.secret);
    engine.shutdown().await;
}

/// Auto, after a failure: the Workflow Agent repairs and the platform
/// restarts on the corrected workflow — within `goals.auto.repair_limit`.
/// Past it, the corrected proposal waits for the person, so a goal that
/// keeps failing cannot loop unattended.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_auto_run_is_repaired_and_restarted_within_its_budget() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "guided-harness");
    let repair = IntakeScript::new(vec![
        json!({"op": "get_goal", "goal": "{{goal}}"}),
        json!({"op": "propose_workflow", "agent": DRIVER, "goal": "{{goal}}",
               "workflow": proposal("Repaired")}),
    ]);
    let engine = Engine::start(
        ws,
        catalog(Some(repair), Some(worker_yields())),
        guided_config(),
    )
    .unwrap();
    let mut rx = engine.events();

    let wf = engine
        .create_workflow(new_workflow(
            "Doomed",
            vec![agent_step("do", "doomed-harness")],
        ))
        .unwrap();
    let goal = engine
        .submit_goal(SubmitRequest {
            workflow: Some(wf.id),
            ..SubmitRequest::captured("fragile")
        })
        .unwrap();
    assert_eq!(goal.mode, bisa_core::GoalMode::Auto);
    engine.start_run(goal.id, BTreeMap::new()).unwrap();
    let failed = finished_run(&engine, goal.id).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed));

    let proposed = wait_for(&mut rx, "the repaired proposal", |e| {
        matches!(&e.payload, EnginePayload::WorkflowProposed { .. })
    })
    .await;
    let EnginePayload::WorkflowProposed { gate_id, .. } = proposed.payload else {
        unreachable!()
    };
    assert_eq!(gate_id, None, "within the budget the repair restarts alone");
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert_ne!(done.id, failed.id, "a fresh run, not the failed one");
    assert_eq!(engine.workspace().get_goal(goal.id).unwrap().runs.len(), 2);
    let words = said(&engine, goal.id);
    assert!(
        words.iter().any(|w| w.contains("running again")),
        "{words:?}"
    );
    engine.shutdown().await;

    // The budget spent: with no repairs allowed, the first failure's repair
    // is gated, and its question says why.
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "guided-harness");
    ws.set_setting(
        bisa_core::settings::Scope::Workspace,
        None,
        bisa_engine::ops::AUTO_REPAIR_LIMIT_KEY,
        json!(0),
    )
    .unwrap();
    let repair = IntakeScript::new(vec![
        json!({"op": "propose_workflow", "agent": DRIVER, "goal": "{{goal}}",
               "workflow": proposal("Repaired")}),
    ]);
    let engine = Engine::start(
        ws,
        catalog(Some(repair), Some(worker_yields())),
        guided_config(),
    )
    .unwrap();
    let mut rx = engine.events();
    let wf = engine
        .create_workflow(new_workflow(
            "Doomed",
            vec![agent_step("do", "doomed-harness")],
        ))
        .unwrap();
    let goal = engine
        .submit_goal(SubmitRequest {
            workflow: Some(wf.id),
            ..SubmitRequest::captured("fragile, no budget")
        })
        .unwrap();
    engine.start_run(goal.id, BTreeMap::new()).unwrap();
    finished_run(&engine, goal.id).await;
    let proposed = wait_for(&mut rx, "the gated repair", |e| {
        matches!(&e.payload, EnginePayload::WorkflowProposed { .. })
    })
    .await;
    let EnginePayload::WorkflowProposed { gate_id, .. } = proposed.payload else {
        unreachable!()
    };
    let gate = engine
        .gate(&gate_id.expect("past the budget, a person decides"))
        .unwrap();
    assert!(gate.question.contains("failed 1 time"), "{}", gate.question);
    assert!(gate.question.contains("0 repairs"), "{}", gate.question);
    assert_eq!(
        engine.workspace().get_goal(goal.id).unwrap().runs.len(),
        1,
        "no second run without the person"
    );
    engine.shutdown().await;
}

/// Manual: the capture wakes nobody, and the Workflow Agent's proposal —
/// asked for in the conversation — becomes the goal's own draft with no
/// gate: the person edits it on the Workflow tab and starts it.
#[tokio::test(flavor = "multi_thread")]
async fn a_manual_capture_wakes_nobody_and_a_proposal_is_its_draft() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    drive_on(&ws, &AgentId::workflow(), "guided-harness");
    let engine = Engine::start(
        ws,
        catalog(Some(IntakeScript::new(vec![])), Some(worker_yields())),
        guided_config(),
    )
    .unwrap();
    let mut rx = engine.events();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..SubmitRequest::captured("by hand")
        })
        .unwrap();
    let raced = tokio::time::timeout(std::time::Duration::from_millis(400), async {
        loop {
            if let Ok(ev) = rx.recv().await {
                if matches!(ev.payload, EnginePayload::Guided { .. }) {
                    return;
                }
            }
        }
    })
    .await;
    assert!(raced.is_err(), "a manual capture wakes no designer");
    assert!(
        statuses(&engine, goal.id).is_empty(),
        "no guidance fact either"
    );

    // Asked in the conversation, the agent proposes: a draft, no gate.
    let reply = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "propose_workflow", "agent": DRIVER, "goal": goal.id.to_string(),
               "workflow": proposal("Drafted for you")}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    assert!(reply["gate"].is_null(), "{reply}");
    let g = engine.workspace().get_goal(goal.id).unwrap();
    assert!(g.workflow.is_some());
    assert!(g.run.is_none(), "a manual goal's person starts it");
    assert!(engine
        .inbox()
        .iter()
        .all(|x| x.home.goal() != Some(goal.id)));
    let notes = notes(&engine, goal.id);
    assert!(notes.iter().any(|n| n.contains("drafted")), "{notes:?}");
    // A redesign is refused by name: the design is the person's.
    let err = engine.redesign(goal.id).unwrap_err();
    assert!(err.to_string().contains("is manual"), "{err}");
    engine.shutdown().await;
}
