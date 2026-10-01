//! Gateways and end events, end to end: a `parallel` gateway splits and the
//! step its branches flow into joins them; a `decide` that picks *every* rule
//! that holds takes as many paths at once; an `end` ends its path, or the run
//! — done or failed — now, stopping what is still live.

use crate::common;

use bisa_core::{
    Branch, Condition, Finish, Flow, InputDef, InputKind, InputName, Pick, Rule, RunOutcome,
    RunStatus, Step, StepKind, StepState, WaitFor, WorkItemState, ENDED_FAILED,
};
use bisa_engine::Engine;
use bisa_harness::mock::MockAdapter;
use bisa_harness::{LifecycleEvent, ProgressEvent, SessionEvent};
use common::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn branch(name: &str) -> Branch {
    Branch::new(name).unwrap()
}

/// A step that says `text` in the run's conversation.
fn tell(id: &str, text: &str) -> Step {
    step(
        id,
        StepKind::Notify {
            scope: None,
            template: text.into(),
            mentions: vec![],
            author: None,
        },
    )
}

fn hold(id: &str) -> Step {
    step(
        id,
        StepKind::Wait {
            until: WaitFor::Release,
        },
    )
}

fn end(id: &str, finish: Finish) -> Step {
    step(id, StepKind::End { finish })
}

/// `step` flowing, unlabelled, to each of `then`.
fn to(mut step: Step, then: &[&str]) -> Step {
    step.then = then.iter().map(|id| Flow::to(sid(id))).collect();
    step
}

/// A harness whose session starts a turn and never ends it.
fn never_done(id: &str) -> MockAdapter {
    MockAdapter {
        id: id.into(),
        script: Some(vec![
            SessionEvent::Lifecycle(LifecycleEvent::Started),
            SessionEvent::Progress(ProgressEvent::TurnStarted),
        ]),
        ..Default::default()
    }
}

/// What was said in a goal's thread, oldest first.
fn said(engine: &Engine, goal: bisa_core::GoalId) -> Vec<String> {
    let mut messages = engine
        .workspace()
        .messages(&goal.to_string(), None, 50)
        .unwrap();
    messages.sort_by_key(|m| m.created_at);
    messages.into_iter().map(|m| m.content).collect()
}

fn state(run: &bisa_core::WorkflowRun, step: &str) -> StepState {
    run.steps[&sid(step)].state.clone()
}

// ---------------------------------------------------------------------------
// Parallel
// ---------------------------------------------------------------------------

/// Two approvals waiting at once are two questions: with neither named the
/// door refuses to guess, and a step named decides that step's gate and no
/// other — from the command line's `approve <goal> --step <id>`.
#[tokio::test(flavor = "multi_thread")]
async fn a_step_named_picks_its_gate_among_several_waiting() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let approval = |id: &str| {
        step(
            id,
            StepKind::Approval {
                prompt: format!("{id}?"),
            },
        )
    };
    let (goal, run) = run_on(
        &engine,
        "two questions at once",
        new_workflow(
            "asked twice",
            vec![
                to(step("fork", StepKind::Parallel), &["go", "publish"]),
                to(approval("go"), &["after"]),
                to(approval("publish"), &["after"]),
                tell("after", "both said yes"),
            ],
        ),
    );
    let home = bisa_core::Home::from(goal.id);
    let waiting = until("both gates to open", || {
        let open: Vec<_> = engine
            .inbox()
            .into_iter()
            .filter(|g| g.home == home)
            .collect();
        (open.len() == 2).then_some(open)
    })
    .await;

    // Nobody named: two are a guess the door will not make.
    let refused = engine
        .decide_durable(&home, true, None, None, None, None, None)
        .expect_err("two gates, none named");
    assert!(refused.to_string().contains("2 pending gates"), "{refused}");
    assert!(engine.inbox().iter().filter(|g| g.home == home).count() == 2);

    // The step named: its gate, and the other still waits.
    let decided = engine
        .decide_durable(&home, true, None, None, None, None, Some(&sid("go")))
        .expect("the step's gate is decided");
    let _ = decided;
    let left: Vec<_> = engine
        .inbox()
        .into_iter()
        .filter(|g| g.home == home && g.resolution.is_none())
        .collect();
    assert_eq!(left.len(), 1, "{left:?}");
    assert_eq!(left[0].step, Some(sid("publish")));
    assert_eq!(left[0].subject, format!("approval:{}/publish", run.id));
    let go = waiting.iter().find(|g| g.step == Some(sid("go"))).unwrap();
    assert_eq!(engine.gate(&go.id).unwrap().decided(), Some(true));

    // A step that waits on nothing is said so, and decides nothing.
    let none = engine
        .decide_durable(&home, true, None, None, None, None, Some(&sid("after")))
        .expect_err("no gate of that step");
    assert!(!none.to_string().is_empty());

    engine
        .decide_durable(&home, true, None, None, None, None, Some(&sid("publish")))
        .expect("the other, by its step");
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done), "{:?}", done.steps);
    assert!(said(&engine, goal.id).contains(&"both said yes".to_string()));
    engine.shutdown().await;
}

/// A `parallel` gateway is done the moment it is entered and takes every
/// flow out of it at once; the step its branches flow into waits for them
/// all, and is entered once.
#[tokio::test(flavor = "multi_thread")]
async fn a_parallel_gateway_splits_and_the_step_after_joins() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let (goal, run) = run_on(
        &engine,
        "two things at once",
        new_workflow(
            "forked",
            vec![
                to(step("fork", StepKind::Parallel), &["build", "hold"]),
                to(agent_step("build", "mock"), &["merge"]),
                to(hold("hold"), &["merge"]),
                tell("merge", "both are in"),
            ],
        ),
    );
    // Both branches are live at once: one working, one held.
    let live = step_in_state(&engine, goal.id, "hold", "waiting").await;
    assert_eq!(state(&live, "fork"), StepState::done());
    let built = step_in_state(&engine, goal.id, "build", "done").await;
    assert_eq!(
        state(&built, "merge"),
        StepState::Pending,
        "the join waits for every branch that can still arrive"
    );
    assert!(said(&engine, goal.id).is_empty());

    engine.release_step(run.id, &sid("hold"), None).unwrap();
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done), "{done:?}");
    assert_eq!(state(&done, "merge"), StepState::done());
    assert_eq!(done.steps[&sid("merge")].visits, 1, "entered once");
    assert_eq!(said(&engine, goal.id), vec!["both are in"]);
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// Choose every
// ---------------------------------------------------------------------------

fn equals(input: &str, value: Value) -> Condition {
    Condition::InputEquals {
        input: InputName::new(input).unwrap(),
        value,
    }
}

/// A workflow that routes a parcel: heavy ones are lifted, urgent ones are
/// rushed, one that is both gets both at once, and one that is neither is
/// walked — then every path meets at the door.
fn routed() -> bisa_store::NewWorkflow {
    let mut route = step(
        "route",
        StepKind::Decide {
            rules: vec![
                Rule {
                    when: equals("size", json!("big")),
                    branch: branch("heavy"),
                },
                Rule {
                    when: equals("urgent", json!(true)),
                    branch: branch("rush"),
                },
            ],
            otherwise: branch("plain"),
            pick: Pick::Every,
        },
    );
    route.then = vec![
        Flow::branch(sid("lift"), branch("heavy")),
        Flow::branch(sid("hurry"), branch("rush")),
        Flow::branch(sid("stroll"), branch("plain")),
    ];
    let mut draft = new_workflow(
        "routes a parcel",
        vec![
            route,
            to(tell("lift", "lifting"), &["door"]),
            to(tell("hurry", "hurrying"), &["door"]),
            to(tell("stroll", "strolling"), &["door"]),
            tell("door", "at the door"),
        ],
    );
    let input = |name: &str, kind: InputKind| InputDef {
        name: InputName::new(name).unwrap(),
        label: name.to_uppercase(),
        kind,
        default: None,
        required: true,
    };
    draft.inputs = vec![
        input("size", InputKind::Text),
        input("urgent", InputKind::Bool),
    ];
    draft
}

/// `pick = "every"`: every rule that holds names a branch and their flows are
/// all taken; `otherwise` only when none holds. The join after them waits
/// only for the paths that were taken.
#[tokio::test(flavor = "multi_thread")]
async fn a_decide_that_picks_every_rule_takes_every_path_that_holds() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let parcel = |statement: &str, size: &str, urgent: bool| {
        let (goal, _) = goal_on(&engine, statement, routed());
        engine
            .start_run(
                goal.id,
                BTreeMap::from([
                    ("size".to_string(), json!(size)),
                    ("urgent".to_string(), json!(urgent)),
                ]),
            )
            .unwrap();
        goal
    };

    let both = parcel("big and urgent", "big", true);
    let done = finished_run(&engine, both.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done), "{done:?}");
    assert_eq!(
        state(&done, "route"),
        StepState::Done {
            branches: vec![branch("heavy"), branch("rush")]
        }
    );
    assert_eq!(state(&done, "lift"), StepState::done());
    assert_eq!(state(&done, "hurry"), StepState::done());
    assert_eq!(state(&done, "stroll"), StepState::Skipped);
    assert_eq!(done.steps[&sid("door")].visits, 1, "the paths meet once");
    let mut heard = said(&engine, both.id);
    assert_eq!(heard.pop().as_deref(), Some("at the door"), "after both");
    heard.sort();
    assert_eq!(heard, vec!["hurrying", "lifting"]);

    let one = parcel("small and urgent", "small", true);
    let done = finished_run(&engine, one.id).await;
    assert_eq!(state(&done, "route"), StepState::chose(branch("rush")));
    assert_eq!(state(&done, "lift"), StepState::Skipped);
    assert_eq!(said(&engine, one.id), vec!["hurrying", "at the door"]);

    let neither = parcel("small and in no hurry", "small", false);
    let done = finished_run(&engine, neither.id).await;
    assert_eq!(
        state(&done, "route"),
        StepState::chose(branch("plain")),
        "`otherwise`, when no rule holds"
    );
    assert_eq!(state(&done, "lift"), StepState::Skipped);
    assert_eq!(state(&done, "hurry"), StepState::Skipped);
    assert_eq!(said(&engine, neither.id), vec!["strolling", "at the door"]);
    engine.shutdown().await;
}

/// `pick = "first"` — the default — is what a `decide` always was: the first
/// rule that holds, one branch, whatever else would hold.
#[tokio::test(flavor = "multi_thread")]
async fn a_decide_that_picks_the_first_rule_takes_one_path() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let mut draft = routed();
    if let StepKind::Decide { pick, .. } = &mut draft.steps[0].kind {
        *pick = Pick::First;
    }
    let (goal, _) = goal_on(&engine, "big and urgent, one path", draft);
    engine
        .start_run(
            goal.id,
            BTreeMap::from([
                ("size".to_string(), json!("big")),
                ("urgent".to_string(), json!(true)),
            ]),
        )
        .unwrap();
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(state(&done, "route"), StepState::chose(branch("heavy")));
    assert_eq!(state(&done, "hurry"), StepState::Skipped);
    assert_eq!(said(&engine, goal.id), vec!["lifting", "at the door"]);
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// End events
// ---------------------------------------------------------------------------

/// A fork of three branches: one held, one working, and one that reaches
/// `quick` — an end with `finish` — once `go` is released.
fn ends_beside_live_work(finish: Finish) -> bisa_store::NewWorkflow {
    new_workflow(
        "ends beside live work",
        vec![
            to(step("fork", StepKind::Parallel), &["hold", "work", "go"]),
            to(hold("hold"), &["after"]),
            to(agent_step("work", "slow"), &["after"]),
            to(hold("go"), &["quick"]),
            end("quick", finish),
            tell("after", "everything came in"),
        ],
    )
}

/// Start a run of [`ends_beside_live_work`], wait until every branch is
/// live, and let the one that ends go.
async fn end_beside_live_work(
    engine: &Engine,
    statement: &str,
    finish: Finish,
) -> (bisa_core::Goal, bisa_core::WorkflowRun) {
    let (goal, run) = run_on(engine, statement, ends_beside_live_work(finish));
    step_in_state(engine, goal.id, "hold", "waiting").await;
    step_in_state(engine, goal.id, "go", "waiting").await;
    let live = step_in_state(engine, goal.id, "work", "running").await;
    assert!(live.steps[&sid("work")].work_item.is_some());
    engine.release_step(run.id, &sid("go"), None).unwrap();
    (goal, run)
}

/// `finish = "path"` — the default — ends its path and nothing else: the run
/// is done once every path has drained.
#[tokio::test(flavor = "multi_thread")]
async fn an_end_that_ends_its_path_leaves_the_run_to_its_other_paths() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let (goal, run) = run_on(
        &engine,
        "one path ends early",
        new_workflow(
            "two paths",
            vec![
                to(step("fork", StepKind::Parallel), &["quick", "hold"]),
                end("quick", Finish::Path),
                to(hold("hold"), &["after"]),
                tell("after", "the other path came in"),
            ],
        ),
    );
    let live = step_in_state(&engine, goal.id, "hold", "waiting").await;
    assert_eq!(state(&live, "quick"), StepState::done());
    assert_eq!(live.status(), RunStatus::Waiting, "the run goes on");
    assert_eq!(live.outcome, None);

    engine.release_step(run.id, &sid("hold"), None).unwrap();
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert_eq!(said(&engine, goal.id), vec!["the other path came in"]);
    engine.shutdown().await;
}

/// `finish = "done"` finishes the run now: what is still live is cancelled —
/// the wait disarmed, the session ended and its work item with it — and
/// nothing after it runs.
#[tokio::test(flavor = "multi_thread")]
async fn an_end_that_finishes_the_run_stops_what_is_still_live() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![never_done("slow")]);
    let (goal, run) =
        end_beside_live_work(&engine, "done as soon as one path is", Finish::Done).await;
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done), "{done:?}");
    assert_eq!(state(&done, "quick"), StepState::done());
    assert_eq!(state(&done, "hold"), StepState::Cancelled);
    assert_eq!(state(&done, "work"), StepState::Cancelled);
    assert_ne!(
        state(&done, "after"),
        StepState::done(),
        "nothing after the end runs"
    );
    assert!(said(&engine, goal.id).is_empty());
    assert!(
        engine.armed_waits().iter().all(|(r, _, _)| *r != run.id),
        "the wait is disarmed"
    );
    until("the live step's work item to be cancelled", || {
        let items = items_of(&engine, goal.id);
        (!items.is_empty()
            && items
                .iter()
                .all(|i| matches!(i.state, WorkItemState::Cancelled)))
        .then_some(())
    })
    .await;
    assert!(
        engine.release_step(run.id, &sid("hold"), None).is_err(),
        "a finished run moves no more"
    );
    engine.shutdown().await;
}

/// `finish = "failed"` fails the run now: the end step is the step that
/// failed, saying so, and what is still live is cancelled.
#[tokio::test(flavor = "multi_thread")]
async fn an_end_that_fails_the_run_fails_it_now_and_names_itself() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![never_done("slow")]);
    let (goal, _) = end_beside_live_work(
        &engine,
        "failed as soon as one path says so",
        Finish::Failed,
    )
    .await;
    let failed = finished_run(&engine, goal.id).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed), "{failed:?}");
    assert_eq!(state(&failed, "quick"), StepState::Failed);
    assert_eq!(
        failed.steps[&sid("quick")].error.as_deref(),
        Some(ENDED_FAILED)
    );
    assert_eq!(state(&failed, "hold"), StepState::Cancelled);
    assert_eq!(state(&failed, "work"), StepState::Cancelled);
    assert!(said(&engine, goal.id).is_empty());
    engine.shutdown().await;
}
