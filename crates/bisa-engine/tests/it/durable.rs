//! The durable mirror of what a goal owes — what the Inbox rebuilds and the
//! durable decide reads when no live gate stands: after a restart, and
//! whenever a question was taken back. A question withdrawn in memory is
//! withdrawn in the journal too, so nothing resurrects it; a decision is
//! taken once however many hands reach for it; an answer with no live gate
//! goes through the rules a live one does; a stop that fails half-way leaves
//! no mark behind; a second close is a conflict.

use crate::common;
use bisa_core::event::JournalPayload;
use bisa_core::{AgentId, Answer, AskKind, AskOption, GoalId, Home, RunOutcome, StepKind, WaitFor};
use bisa_engine::{Engine, EngineConfig, EngineError};
use common::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

const DRIVER: &str = AgentId::WORKFLOW;

fn engine_on(dir: &tempfile::TempDir) -> Engine {
    engine_with(dir, vec![yielding("mock", json!({"ok": true}))])
}

fn draft(name: &str) -> Value {
    json!({
        "name": name,
        "description": "from the driver",
        "steps": [{"id": "do", "name": "Do it", "kind": "agent",
                   "instructions": "do {goal.statement}", "harness": ["mock"]}]
    })
}

fn plain(name: &str) -> bisa_store::NewWorkflow {
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

fn ask(id: &str) -> bisa_core::Step {
    step(
        id,
        StepKind::Human {
            prompt: format!("{id}?"),
            options: vec![AskOption::new("yes", "Yes"), AskOption::new("no", "No")],
            multi: false,
            assignee: None,
        },
    )
}

/// The goal's facts on `subject`, oldest first, one letter each: a
/// Q(uestion), a D(ecision), a W(ithdrawal).
fn facts_on(engine: &Engine, goal: GoalId, subject: &str) -> String {
    engine
        .workspace()
        .journal(&Home::Goal { goal })
        .unwrap()
        .iter()
        .filter_map(|e| match &e.payload {
            JournalPayload::Question { gate, .. } if gate == subject => Some('Q'),
            JournalPayload::Decision { subject: s, .. } if s == subject => Some('D'),
            JournalPayload::Withdrawn { subject: s, .. } if s == subject => Some('W'),
            _ => None,
        })
        .collect()
}

fn decisions_of(engine: &Engine, goal: GoalId) -> usize {
    engine
        .workspace()
        .journal(&Home::Goal { goal })
        .unwrap()
        .iter()
        .filter(|e| matches!(e.payload, JournalPayload::Decision { .. }))
        .count()
}

fn decide_durably(
    engine: &Engine,
    goal: GoalId,
    answer: Option<&Answer>,
) -> Result<bisa_engine::DecideOutcome, EngineError> {
    engine.decide_durable(&Home::Goal { goal }, true, None, answer, None, None, None)
}

/// The Workflow Agent proposes for `goal`; the adopt gate's subject.
async fn proposed(engine: &Engine, goal: GoalId) -> String {
    let reply = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "propose_workflow", "agent": DRIVER, "goal": goal.to_string(),
               "workflow": draft("Socks")}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    let gate = reply["gate"].as_str().unwrap();
    engine.gate(gate).expect("the adopt gate is open").subject
}

/// A proposal withdrawn by the person's hand — the design edited, another
/// workflow chosen, the goal started — is withdrawn in the journal as well:
/// the durable decide finds nothing to adopt and starts nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_withdrawn_adoption_is_withdrawn_in_the_journal_and_the_durable_decide_finds_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);

    // Edited by hand.
    let edited = engine.submit_goal(guided("edit it")).unwrap();
    let subject = proposed(&engine, edited.id).await;
    engine
        .design_workflow(
            edited.id,
            new_workflow("Socks, edited", vec![agent_step("go", "mock")]),
            Some(1),
        )
        .unwrap();
    assert_eq!(facts_on(&engine, edited.id, &subject), "QW");
    let err = decide_durably(&engine, edited.id, None).unwrap_err();
    assert!(matches!(err, EngineError::Invalid(_)), "{err:?}");
    assert!(err.to_string().contains("no pending gate"), "{err}");
    assert!(engine
        .workspace()
        .get_goal(edited.id)
        .unwrap()
        .run
        .is_none());
    assert_eq!(decisions_of(&engine, edited.id), 0);

    // Another workflow chosen by hand.
    let chosen = engine.submit_goal(guided("choose instead")).unwrap();
    let subject = proposed(&engine, chosen.id).await;
    let other = engine.create_workflow(plain("chosen")).unwrap();
    engine.set_workflow(chosen.id, Some(other.id)).unwrap();
    assert_eq!(facts_on(&engine, chosen.id, &subject), "QW");
    let err = decide_durably(&engine, chosen.id, None).unwrap_err();
    assert!(err.to_string().contains("no pending gate"), "{err}");
    assert!(engine
        .workspace()
        .get_goal(chosen.id)
        .unwrap()
        .run
        .is_none());

    // Started by hand on the proposed design itself.
    let started = engine.submit_goal(guided("start it")).unwrap();
    let subject = proposed(&engine, started.id).await;
    engine.start_run(started.id, BTreeMap::new()).unwrap();
    assert_eq!(facts_on(&engine, started.id, &subject), "QW");
    let done = finished_run(&engine, started.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    let err = decide_durably(&engine, started.id, None).unwrap_err();
    assert!(err.to_string().contains("no pending gate"), "{err}");
    assert_eq!(engine.runs(started.id).unwrap().len(), 1, "no second run");
    assert_eq!(decisions_of(&engine, started.id), 0);
    engine.shutdown().await;
}

/// A step's question answered live and its run over is nobody's to answer
/// again: the durable decide refuses, records nothing, and resumes nobody.
#[tokio::test(flavor = "multi_thread")]
async fn a_finished_steps_question_is_not_the_durable_decides_to_answer() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let (goal, run) = run_on(
        &engine,
        "ask once",
        new_workflow(
            "asked",
            chain(vec![ask("which"), agent_step("build", "mock")]),
        ),
    );
    let gate = until("the question", || {
        engine
            .inbox()
            .into_iter()
            .find(|g| g.run == Some(run.id) && g.step == Some(sid("which")))
    })
    .await;
    engine
        .decide(
            &gate.id,
            true,
            None,
            Some(&Answer::selecting(["yes"])),
            None,
        )
        .unwrap();
    assert_eq!(
        finished_run(&engine, goal.id).await.outcome,
        Some(RunOutcome::Done)
    );
    assert_eq!(decisions_of(&engine, goal.id), 1);
    let err = decide_durably(&engine, goal.id, Some(&Answer::text("again"))).unwrap_err();
    assert!(err.to_string().contains("no pending gate"), "{err}");
    assert_eq!(decisions_of(&engine, goal.id), 1, "nothing was recorded");
    engine.shutdown().await;
}

/// A durable adoption — the gate rebuilt from the journal, after a restart
/// — is decided once: of two hands reaching for it together, one adopts and
/// starts the run, the other is told it was decided already, and the goal
/// has one run.
#[tokio::test(flavor = "multi_thread")]
async fn a_durable_adoption_is_decided_once_whoever_else_reaches_for_it() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(engine_on(&dir));
    let goal = engine.submit_goal(guided("adopt me durably")).unwrap();
    // The design on the goal, and the question an earlier process journaled
    // for it — what a restart leaves: no live gate, a durable one.
    let (wf, _) = engine
        .design_workflow(goal.id, plain("Proposed"), None)
        .unwrap();
    let subject = format!("adopt:{}@{}", wf.id, wf.revision);
    let owner = engine.workspace().owner_keys().clone();
    engine
        .workspace()
        .append_journal(
            &Home::Goal { goal: goal.id },
            JournalPayload::Question {
                work_item: None,
                gate: subject.clone(),
                text: "Adopt \"Proposed\"?".into(),
                expects: AskKind::Decision,
            },
            &owner,
            None,
        )
        .unwrap();
    assert!(engine.inbox().is_empty(), "no live gate stands");

    let decide = |engine: Arc<Engine>| {
        tokio::task::spawn_blocking(move || decide_durably(&engine, goal.id, None))
    };
    let (a, b) = tokio::join!(decide(Arc::clone(&engine)), decide(Arc::clone(&engine)));
    let outcomes = [a.unwrap(), b.unwrap()];
    assert_eq!(
        outcomes.iter().filter(|o| o.is_ok()).count(),
        1,
        "one adopts: {outcomes:?}"
    );
    let refused = outcomes.iter().find_map(|o| o.as_ref().err()).unwrap();
    assert!(
        matches!(refused, EngineError::GateAlreadyDecided(_)),
        "the other is told it was decided: {refused:?}"
    );
    assert_eq!(facts_on(&engine, goal.id, &subject), "QD");
    let engine = Arc::try_unwrap(engine).ok().expect("the tasks are done");
    assert_eq!(
        finished_run(&engine, goal.id).await.outcome,
        Some(RunOutcome::Done)
    );
    assert_eq!(engine.runs(goal.id).unwrap().len(), 1, "one run");
    engine.shutdown().await;
}

/// An answer with no live gate — after a restart, from the command line
/// offline — goes through the rules a live one does: "not sure" is re-asked
/// within the clarify budget, and past it the step fails with the words;
/// every answer is a signed decision.
#[tokio::test(flavor = "multi_thread")]
async fn an_unsure_answer_with_no_live_gate_is_re_asked_and_then_fails_the_step() {
    let dir = tempfile::tempdir().unwrap();
    let config = || EngineConfig {
        max_clarify_rounds: 1,
        ..design_off_config()
    };
    let catalog = || catalog_with(vec![yielding("mock", json!({"ok": true}))]);
    let engine = Engine::start(workspace(&dir), catalog(), config()).unwrap();
    let (goal, run) = run_on(
        &engine,
        "ask across a restart",
        new_workflow(
            "asked",
            chain(vec![ask("which"), agent_step("build", "mock")]),
        ),
    );
    until("the question", || {
        engine.inbox().into_iter().find(|g| g.run == Some(run.id))
    })
    .await;
    engine.shutdown().await;

    let engine = Engine::start(workspace(&dir), catalog(), config()).unwrap();
    assert!(engine.inbox().is_empty(), "the gate died with the process");
    engine
        .answer_step(run.id, &sid("which"), &Answer::unsure())
        .unwrap();
    let again = until("the question asked again", || {
        engine
            .inbox()
            .into_iter()
            .find(|g| g.run == Some(run.id) && g.step == Some(sid("which")))
    })
    .await;
    assert_eq!(
        current_run(&engine, goal.id).steps[&sid("which")]
            .state
            .as_str(),
        "waiting",
        "not sure is re-asked, never done"
    );
    assert_eq!(
        decisions_of(&engine, goal.id),
        1,
        "the answer is a signed decision"
    );
    // Past the budget, through the re-asked gate: the step fails with the words.
    engine
        .decide(
            &again.id,
            true,
            None,
            Some(&Answer::unsure().with_text("no idea")),
            None,
        )
        .unwrap();
    let failed = finished_run(&engine, goal.id).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed));
    let error = failed.steps[&sid("which")]
        .error
        .clone()
        .unwrap_or_default();
    assert!(
        error.contains("not sure") && error.contains("no idea"),
        "{error}"
    );
    engine.shutdown().await;
}

/// Two human steps waiting side by side keep their own clarify budgets: one
/// person's "not sure" on the first spends nothing of the second's.
#[tokio::test(flavor = "multi_thread")]
async fn parallel_human_steps_keep_their_own_clarify_rounds() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::start(
        workspace(&dir),
        catalog_with(vec![yielding("mock", json!({"ok": true}))]),
        EngineConfig {
            max_clarify_rounds: 1,
            ..design_off_config()
        },
    )
    .unwrap();
    let mut fan = step("fan", StepKind::Parallel);
    fan.then = vec![bisa_core::Flow::to(sid("a")), bisa_core::Flow::to(sid("b"))];
    let (goal, run) = run_on(
        &engine,
        "two questions",
        new_workflow("twice", vec![fan, ask("a"), ask("b")]),
    );
    let gate_of = |step: &str| {
        let step = sid(step);
        let engine = &engine;
        async move {
            until(&format!("the question of {step}"), || {
                engine
                    .inbox()
                    .into_iter()
                    .find(|g| g.run == Some(run.id) && g.step.as_ref() == Some(&step))
            })
            .await
        }
    };
    let a = gate_of("a").await;
    let unsure = Answer::unsure();
    engine
        .decide(&a.id, true, None, Some(&unsure), None)
        .unwrap();
    let a_again = gate_of("a").await;
    assert_ne!(a_again.id, a.id, "a is re-asked");
    let b = gate_of("b").await;
    engine
        .decide(&b.id, true, None, Some(&unsure), None)
        .unwrap();
    let b_again = gate_of("b").await;
    assert_ne!(
        b_again.id, b.id,
        "b has its own first round, and is re-asked"
    );
    let run = current_run(&engine, goal.id);
    assert_eq!(run.steps[&sid("a")].state.as_str(), "waiting");
    assert_eq!(run.steps[&sid("b")].state.as_str(), "waiting");
    engine.shutdown().await;
}

/// A stop that fails half-way — its run's snapshot unreadable when the stop
/// reaches for it — leaves no mark that would refuse every later wake of the
/// goal; repaired, the goal is stopped and woken as any other.
#[tokio::test(flavor = "multi_thread")]
async fn a_stop_that_fails_half_way_leaves_no_ending_mark_behind() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let (goal, run) = run_on(
        &engine,
        "held, then stopped",
        new_workflow(
            "held",
            vec![step(
                "hold",
                StepKind::Wait {
                    until: WaitFor::Release,
                },
            )],
        ),
    );
    let snapshot = bisa_store::Paths::new(dir.path())
        .state_dir(&bisa_store::Paths::ns_goal(goal.id))
        .join(format!(
            "{}-{}.json",
            bisa_core::kind::KIND_WORKFLOW_RUN,
            run.id
        ));
    let kept = std::fs::read(&snapshot).unwrap();
    std::fs::write(&snapshot, "{not an event this build can read").unwrap();
    let err = engine.stop_goal(goal.id, None).await.unwrap_err();
    assert!(!matches!(err, EngineError::Conflict(_)), "{err:?}");
    assert!(
        !engine.inner().guided.is_stopping(goal.id),
        "a stop that failed is not a stop in progress"
    );
    std::fs::write(&snapshot, kept).unwrap();
    let stopped = engine.stop_goal(goal.id, None).await.unwrap();
    assert_eq!(stopped.run, Some(run.id));
    engine.shutdown().await;
}

/// Closing a goal twice is a conflict — the state the second close meets,
/// never a fault of the request — and the first close stands.
#[tokio::test(flavor = "multi_thread")]
async fn closing_a_goal_twice_is_a_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let goal = engine.submit_goal(guided("close me twice")).unwrap();
    let reason = || bisa_core::ClosureReason::Abandoned { rationale: None };
    let (closed, _) = engine.close_goal_settled(goal.id, reason()).await.unwrap();
    assert!(closed.is_closed());
    let err = engine
        .close_goal_settled(goal.id, reason())
        .await
        .unwrap_err();
    assert!(matches!(err, EngineError::Conflict(_)), "{err:?}");
    let err = engine.close_goal(goal.id, reason()).unwrap_err();
    assert!(matches!(err, EngineError::Conflict(_)), "{err:?}");
    assert!(engine.workspace().get_goal(goal.id).unwrap().is_closed());
    engine.shutdown().await;
}
