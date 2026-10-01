//! Boundary events: an event on a live step. A **divert** stops the step —
//! its session ended, its question withdrawn, its wait disarmed — and the run
//! takes only the flows labelled with the boundary's name; an **act** posts
//! or raises a signal beside the step, which goes on.
//!
//! The clock is an argument (`tick_waits_at`) and every timer counts from the
//! moment its step was entered, so a timeout of an hour is tested in
//! milliseconds. The one harness that never finishes its turn stands in for
//! work that takes too long.

use crate::common;

use bisa_core::{
    Boundary, BoundaryAct, BoundaryOn, Branch, Flow, Gate, GoalOrigin, MessageBody, MessageFilter,
    MessageFrom, RunOutcome, RunStatus, SignalFilter, SignalScope, Step, StepKind, StepState,
    ValueRef, WaitFor, WorkItemState,
};
use bisa_engine::{Engine, EngineEvent, EnginePayload};
use bisa_harness::mock::MockAdapter;
use bisa_harness::{LifecycleEvent, ProgressEvent, SessionEvent};
use bisa_store::PostOrigin;
use common::*;
use serde_json::json;
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn branch(name: &str) -> Branch {
    Branch::new(name).unwrap()
}

/// A timeout that diverts: `secs` after the step was entered.
fn timeout(name: &str, secs: u64) -> Boundary {
    Boundary {
        name: branch(name),
        on: BoundaryOn::After {
            secs: ValueRef::Fixed(secs),
        },
        act: BoundaryAct::Divert,
    }
}

/// A reminder: a post beside the step every `secs`, at most `max` times.
fn reminder(name: &str, secs: u64, max: u32, text: &str) -> Boundary {
    Boundary {
        name: branch(name),
        on: BoundaryOn::Every {
            secs: ValueRef::Fixed(secs),
            max,
        },
        act: BoundaryAct::Notify {
            scope: None,
            template: text.into(),
            mentions: vec![],
            author: None,
        },
    }
}

/// A boundary on the named signal `signal`.
fn on_signal(name: &str, signal: &str, act: BoundaryAct) -> Boundary {
    Boundary {
        name: branch(name),
        on: BoundaryOn::Signal {
            filter: SignalFilter {
                name: signal.into(),
                fields: BTreeMap::new(),
            },
        },
        act,
    }
}

/// A harness whose session starts a turn and never ends it: work that is
/// live until something stops it.
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

/// A step with its boundaries, its normal flow to `then`, and one flow per
/// divert: `(boundary, target)`.
fn bounded(
    mut step: Step,
    boundaries: Vec<Boundary>,
    then: &str,
    diverts: &[(&str, &str)],
) -> Step {
    step.boundaries = boundaries;
    step.then = std::iter::once(Flow::to(sid(then)))
        .chain(
            diverts
                .iter()
                .map(|(name, to)| Flow::branch(sid(to), branch(name))),
        )
        .collect();
    step
}

/// When a step was entered: what every boundary clock counts from.
fn entered_at(engine: &Engine, run: bisa_core::RunId, step: &str) -> u64 {
    run_of(engine, run).steps[&sid(step)]
        .started_at
        .expect("the step was entered")
}

/// What was said in a conversation, oldest first.
fn said(engine: &Engine, scope: &str) -> Vec<String> {
    let mut messages = engine.workspace().messages(scope, None, 50).unwrap();
    messages.sort_by_key(|m| m.created_at);
    messages.into_iter().map(|m| m.content).collect()
}

/// Wait until `text` was said `n` times in a conversation — and never more:
/// a count that went past `n` is said at once, with what was said.
async fn said_times(engine: &Engine, scope: &str, text: &str, n: usize) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let all = said(engine, scope);
        let count = all.iter().filter(|m| *m == text).count();
        if count == n {
            return;
        }
        assert!(
            count < n,
            "`{text}` was said {count} times, past the {n} awaited: {all:?}"
        );
        assert!(
            std::time::Instant::now() < deadline,
            "timed out: `{text}` to be said {n} time(s); said so far: {all:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
}

fn boundary_fires(events: &[EngineEvent]) -> Vec<(String, String, bool)> {
    events
        .iter()
        .filter_map(|e| match &e.payload {
            EnginePayload::BoundaryFired {
                step,
                boundary,
                diverts,
                ..
            } => Some((step.to_string(), boundary.to_string(), *diverts)),
            _ => None,
        })
        .collect()
}

fn heard(rx: &mut tokio::sync::broadcast::Receiver<EngineEvent>) -> Vec<EngineEvent> {
    let mut out = Vec::new();
    loop {
        match rx.try_recv() {
            Ok(event) => out.push(event),
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
            Err(_) => return out,
        }
    }
}

// ---------------------------------------------------------------------------
// A divert
// ---------------------------------------------------------------------------

/// A timeout on an agent step: the session is ended, its work item
/// cancelled, the step reads *diverted* and the run takes the timeout's path
/// — never the normal one.
#[tokio::test(flavor = "multi_thread")]
async fn a_timeout_diverts_an_agent_step_and_its_work_is_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![never_done("slow"), MockAdapter::default()]);
    let (goal, run) = run_on(
        &engine,
        "work that takes too long",
        new_workflow(
            "timed",
            vec![
                bounded(
                    agent_step("work", "slow"),
                    vec![timeout("late", 3600)],
                    "publish",
                    &[("late", "escalate")],
                ),
                tell("publish", "published"),
                tell("escalate", "it took too long"),
            ],
        ),
    );
    let live = step_in_state(&engine, goal.id, "work", "running").await;
    let item = live.steps[&sid("work")]
        .work_item
        .expect("the step runs a work item");
    assert_eq!(
        engine.armed_boundaries(run.id, &sid("work")),
        vec![branch("late")]
    );
    let t0 = entered_at(&engine, run.id, "work");
    let mut rx = engine.events();

    engine.tick_waits_at(t0 + 3599);
    assert_eq!(
        run_of(&engine, run.id).steps[&sid("work")].state,
        StepState::Running,
        "not yet"
    );
    engine.tick_waits_at(t0 + 3600);
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done), "{done:?}");
    let work = &done.steps[&sid("work")];
    assert_eq!(work.state, StepState::Diverted { by: branch("late") });
    assert_eq!(work.fired[&branch("late")].count, 1);
    assert_eq!(done.steps[&sid("escalate")].state, StepState::done());
    assert_eq!(
        done.steps[&sid("publish")].state,
        StepState::Skipped,
        "only the boundary's flows are taken"
    );
    assert_eq!(
        said(&engine, &goal.id.to_string()),
        vec!["it took too long"]
    );
    assert_eq!(
        boundary_fires(&heard(&mut rx)),
        vec![("work".to_string(), "late".to_string(), true)]
    );

    // The work is stopped, not abandoned to run on.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let state = items_of(&engine, goal.id)
            .into_iter()
            .find(|i| i.id == item)
            .map(|i| i.state);
        if matches!(state, Some(WorkItemState::Cancelled)) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the step's work item was never cancelled: it reads {state:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    assert!(engine.armed_boundaries(run.id, &sid("work")).is_empty());
    engine.shutdown().await;
}

/// A message diverts a `human` step: the question is withdrawn and the run
/// takes the boundary's path.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_diverts_a_human_step_and_its_question_is_withdrawn() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    let mut rx = engine.events();
    let ask = step(
        "ask",
        StepKind::Human {
            prompt: "Which colour?".into(),
            options: vec![],
            multi: false,
            assignee: None,
        },
    );
    let never_mind = Boundary {
        name: branch("dropped"),
        on: BoundaryOn::Message {
            filter: MessageFilter {
                r#in: None,
                from: MessageFrom::You,
                mentions: None,
                contains: Some("never mind".into()),
            },
        },
        act: BoundaryAct::Divert,
    };
    let (goal, run) = run_on(
        &engine,
        "ask, unless told not to",
        new_workflow(
            "asked",
            vec![
                bounded(ask, vec![never_mind], "paint", &[("dropped", "skip")]),
                tell("paint", "painting"),
                tell("skip", "skipped the question"),
            ],
        ),
    );
    wait_for(&mut rx, "the question", |e| {
        matches!(&e.payload, EnginePayload::GateOpened { .. })
    })
    .await;
    assert_eq!(
        engine
            .inbox()
            .iter()
            .filter(|g| g.run == Some(run.id) && g.step == Some(sid("ask")))
            .count(),
        1,
        "the step's question is open"
    );

    let general = bisa_core::ChannelId::general().to_string();
    let say = |text: &str| {
        ws.post_message(
            &general,
            MessageBody::post(text.to_string()),
            None,
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap()
    };
    say("what a day");
    let message = say("Never mind, I chose already");
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done), "{done:?}");
    assert_eq!(
        done.steps[&sid("ask")].state,
        StepState::Diverted {
            by: branch("dropped")
        }
    );
    assert_eq!(done.steps[&sid("skip")].state, StepState::done());
    assert_eq!(done.steps[&sid("paint")].state, StepState::Skipped);
    assert!(
        engine
            .inbox()
            .iter()
            .all(|g| g.run != Some(run.id) || g.resolution.is_some()),
        "the question is withdrawn: {:?}",
        engine.inbox()
    );
    // Said on the bus once the run took it — a hop after the run's own
    // facts, so it is awaited, never read off what has arrived so far.
    wait_for(
        &mut rx,
        &format!("the message {message} to be said to have diverted the step"),
        |e| {
            matches!(
                &e.payload,
                EnginePayload::BoundaryFired { boundary, diverts: true, .. }
                    if *boundary == branch("dropped")
            )
        },
    )
    .await;
    engine.shutdown().await;
}

/// A race — the first event wins — is a `wait` with divert boundaries: the
/// signal it holds for, another that diverts it, a timeout that diverts it.
/// Whichever comes first takes its path, and the others no longer apply.
#[tokio::test(flavor = "multi_thread")]
async fn the_first_event_of_a_race_wins() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let race = || {
        let verdict = step(
            "verdict",
            StepKind::Wait {
                until: WaitFor::Signal {
                    filter: SignalFilter {
                        name: "review.approved".into(),
                        fields: BTreeMap::new(),
                    },
                },
            },
        );
        new_workflow(
            "a race",
            vec![
                bounded(
                    verdict,
                    vec![
                        on_signal("rejected", "review.rejected", BoundaryAct::Divert),
                        timeout("silence", 3600),
                    ],
                    "ship",
                    &[("rejected", "rework"), ("silence", "chase")],
                ),
                tell("ship", "shipping"),
                tell("rework", "back to the desk"),
                tell("chase", "nobody answered"),
            ],
        )
    };
    let raise = |name: &str, goal: bisa_core::GoalId| {
        engine
            .emit_signal(name, json!({}), SignalScope::Goal { goal })
            .unwrap();
    };

    // The catch wins: the normal path, and the boundaries are over.
    let (approved, run) = run_on(&engine, "approved first", race());
    step_in_state(&engine, approved.id, "verdict", "waiting").await;
    let t0 = entered_at(&engine, run.id, "verdict");
    raise("review.approved", approved.id);
    let done = finished_run(&engine, approved.id).await;
    assert_eq!(done.steps[&sid("verdict")].state, StepState::done());
    assert_eq!(said(&engine, &approved.id.to_string()), vec!["shipping"]);
    raise("review.rejected", approved.id);
    engine.tick_waits_at(t0 + 3600);
    assert_eq!(
        run_of(&engine, run.id).steps[&sid("verdict")].state,
        StepState::done(),
        "what lost the race moves nothing"
    );
    assert!(engine.armed_boundaries(run.id, &sid("verdict")).is_empty());

    // The other signal wins: its path, and the catch is disarmed.
    let (rejected, run) = run_on(&engine, "rejected first", race());
    step_in_state(&engine, rejected.id, "verdict", "waiting").await;
    raise("review.rejected", rejected.id);
    let done = finished_run(&engine, rejected.id).await;
    assert_eq!(
        done.steps[&sid("verdict")].state,
        StepState::Diverted {
            by: branch("rejected")
        }
    );
    assert_eq!(
        said(&engine, &rejected.id.to_string()),
        vec!["back to the desk"]
    );
    assert!(
        engine.armed_waits().iter().all(|(r, _, _)| *r != run.id),
        "a diverted wait holds for nothing"
    );

    // Nobody answers: the timeout wins.
    let (silent, run) = run_on(&engine, "silence first", race());
    step_in_state(&engine, silent.id, "verdict", "waiting").await;
    let t0 = entered_at(&engine, run.id, "verdict");
    engine.tick_waits_at(t0 + 3600);
    let done = finished_run(&engine, silent.id).await;
    assert_eq!(
        done.steps[&sid("verdict")].state,
        StepState::Diverted {
            by: branch("silence")
        }
    );
    assert_eq!(
        said(&engine, &silent.id.to_string()),
        vec!["nobody answered"]
    );
    engine.shutdown().await;
}

/// A timeout on a `spawn` that waits: the parent stops waiting and takes the
/// boundary's path; the child goes on.
#[tokio::test(flavor = "multi_thread")]
async fn a_timeout_on_a_spawn_stops_the_waiting_and_the_child_goes_on() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    let child = engine
        .create_workflow(new_workflow("the rest", vec![hold("hold")]))
        .unwrap();
    let hand_on = step(
        "hand-on",
        StepKind::Spawn {
            statement_template: "do the rest".into(),
            workflow: Some(child.id),
            assignees: vec![],
            inputs: Default::default(),
            wait: true,
        },
    );
    let (goal, run) = run_on(
        &engine,
        "hand work on, for an hour",
        new_workflow(
            "spawns",
            vec![
                bounded(
                    hand_on,
                    vec![timeout("late", 3600)],
                    "thank",
                    &[("late", "move-on")],
                ),
                tell("thank", "thank you"),
                tell("move-on", "moving on without it"),
            ],
        ),
    );
    step_in_state(&engine, goal.id, "hand-on", "waiting").await;
    let sub_goal = until("the child goal", || {
        ws.list_goals(None)
            .ok()?
            .into_iter()
            .find(|g| g.origin == GoalOrigin::Spawned { parent: goal.id })
    })
    .await;
    step_in_state(&engine, sub_goal.id, "hold", "waiting").await;
    assert!(engine.inner().waits.awaits(sub_goal.id));

    let t0 = entered_at(&engine, run.id, "hand-on");
    engine.tick_waits_at(t0 + 3600);
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done), "{done:?}");
    assert_eq!(
        done.steps[&sid("hand-on")].state,
        StepState::Diverted { by: branch("late") }
    );
    assert_eq!(
        said(&engine, &goal.id.to_string()),
        vec!["moving on without it"]
    );
    assert!(
        !engine.inner().waits.awaits(sub_goal.id),
        "nobody waits on the child any more"
    );
    assert_eq!(
        current_run(&engine, sub_goal.id).status(),
        RunStatus::Waiting,
        "the child goes on"
    );
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// An act beside the step
// ---------------------------------------------------------------------------

/// A reminder posts beside an approval at its cadence, never more than its
/// `max`, and the approval waits all the while; decided, the run goes on.
#[tokio::test(flavor = "multi_thread")]
async fn a_reminder_posts_beside_an_approval_until_its_max() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let mut rx = engine.events();
    let ship = step(
        "ship",
        StepKind::Approval {
            prompt: "Ship it?".into(),
        },
    );
    let (goal, run) = run_on(
        &engine,
        "ship, with a nudge",
        new_workflow(
            "gated",
            vec![
                bounded(
                    ship,
                    vec![reminder("nudge", 60, 2, "still waiting: ship it?")],
                    "publish",
                    &[],
                ),
                tell("publish", "shipped"),
            ],
        ),
    );
    let thread = goal.id.to_string();
    let opened = wait_for(&mut rx, "the approval gate", |e| {
        matches!(
            &e.payload,
            EnginePayload::GateOpened {
                gate: Gate::Approval,
                ..
            }
        )
    })
    .await;
    let EnginePayload::GateOpened { gate_id, .. } = opened.payload else {
        unreachable!("matched above");
    };
    let t0 = entered_at(&engine, run.id, "ship");

    engine.tick_waits_at(t0 + 59);
    assert!(said(&engine, &thread).is_empty(), "not due yet");
    engine.tick_waits_at(t0 + 60);
    said_times(&engine, &thread, "still waiting: ship it?", 1).await;
    engine.tick_waits_at(t0 + 61);
    let waiting = run_of(&engine, run.id);
    assert_eq!(waiting.steps[&sid("ship")].state, StepState::Waiting);
    assert_eq!(waiting.steps[&sid("ship")].fired[&branch("nudge")].count, 1);
    assert_eq!(
        engine.armed_boundaries(run.id, &sid("ship")),
        vec![branch("nudge")],
        "armed for its next time"
    );

    engine.tick_waits_at(t0 + 120);
    said_times(&engine, &thread, "still waiting: ship it?", 2).await;
    assert!(
        engine.armed_boundaries(run.id, &sid("ship")).is_empty(),
        "spent"
    );
    engine.tick_waits_at(t0 + 180);
    engine.tick_waits_at(t0 + 240);
    assert_eq!(said(&engine, &thread).len(), 2, "never more than its max");
    assert_eq!(
        boundary_fires(&heard(&mut rx)),
        vec![
            ("ship".to_string(), "nudge".to_string(), false),
            ("ship".to_string(), "nudge".to_string(), false),
        ]
    );
    assert!(
        engine.gate(&gate_id).unwrap().resolution.is_none(),
        "the approval waited all the while"
    );

    engine.decide(&gate_id, true, None, None, None).unwrap();
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert_eq!(done.steps[&sid("ship")].state, StepState::done());
    assert_eq!(
        said(&engine, &thread).last().map(String::as_str),
        Some("shipped")
    );
    engine.shutdown().await;
}

/// An `emit` act raises a named signal beside its step every time its event
/// is heard — a side process is another workflow's `signal` start — and the
/// step goes on.
#[tokio::test(flavor = "multi_thread")]
async fn an_emit_act_raises_a_signal_beside_its_step() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let page = on_signal(
        "page",
        "escalate.now",
        BoundaryAct::Emit {
            signal: "pager.ring".into(),
            payload: BTreeMap::from([("why".to_string(), "{inputs.why}".to_string())]),
        },
    );
    let mut draft = new_workflow(
        "held, and paging",
        vec![
            bounded(hold("hold"), vec![page], "thank", &[]),
            tell("thank", "thanks"),
        ],
    );
    draft.inputs = vec![bisa_core::InputDef {
        name: bisa_core::InputName::new("why").unwrap(),
        label: "Why".into(),
        kind: bisa_core::InputKind::Text,
        default: None,
        required: true,
    }];
    let (goal, _) = goal_on(&engine, "page when asked", draft);
    let run = engine
        .start_run(
            goal.id,
            BTreeMap::from([("why".to_string(), json!("the build is red"))]),
        )
        .unwrap();
    step_in_state(&engine, goal.id, "hold", "waiting").await;

    let rung = |engine: &Engine| {
        engine
            .workspace()
            .named_signals_since("pager.ring", 0, 50)
            .unwrap()
    };
    for n in 1..=2 {
        engine
            .emit_signal(
                "escalate.now",
                json!({}),
                SignalScope::Goal { goal: goal.id },
            )
            .unwrap();
        let raised = until("the act's signal", || {
            let all = rung(&engine);
            (all.len() == n).then_some(all)
        })
        .await;
        let last = raised.last().unwrap();
        assert_eq!(last.payload, json!({"why": "the build is red"}));
        assert_eq!(last.scope, SignalScope::Goal { goal: goal.id });
        assert_eq!(
            last.dedupe_key,
            Some(format!(
                "boundary:{}:hold:{}:page:{n}",
                run.id,
                run_of(&engine, run.id).steps[&sid("hold")].entered
            ))
        );
    }
    let live = run_of(&engine, run.id);
    assert_eq!(live.steps[&sid("hold")].state, StepState::Waiting);
    assert_eq!(live.steps[&sid("hold")].fired[&branch("page")].count, 2);

    // Somebody else's signal is not this goal's to hear.
    engine
        .emit_signal(
            "escalate.now",
            json!({}),
            SignalScope::Goal {
                goal: bisa_core::GoalId::from_ulid(ulid::Ulid::from_parts(1, 1)),
            },
        )
        .unwrap();
    assert_eq!(rung(&engine).len(), 2);
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// Order, and a restart
// ---------------------------------------------------------------------------

/// What one tick wakes fires in order: the step's own catch first, then the
/// diverts, then the acts — each in the order it was declared.
#[tokio::test(flavor = "multi_thread")]
async fn one_tick_fires_the_catch_then_diverts_then_acts() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);

    // A delay and a timeout due at the same moment: the step's own catch
    // completes it, and the timeout no longer applies.
    let nap = step(
        "nap",
        StepKind::Wait {
            until: WaitFor::Delay {
                secs: ValueRef::Fixed(60),
            },
        },
    );
    let (napping, run) = run_on(
        &engine,
        "a delay and its timeout",
        new_workflow(
            "tied",
            vec![
                bounded(
                    nap,
                    vec![timeout("late", 60)],
                    "awake",
                    &[("late", "gave-up")],
                ),
                tell("awake", "awake"),
                tell("gave-up", "gave up"),
            ],
        ),
    );
    let t0 = entered_at(&engine, run.id, "nap");
    engine.tick_waits_at(t0 + 60);
    let done = finished_run(&engine, napping.id).await;
    assert_eq!(done.steps[&sid("nap")].state, StepState::done());
    assert_eq!(said(&engine, &napping.id.to_string()), vec!["awake"]);

    // A reminder declared first and two timeouts, all due at once: the first
    // divert declared wins, and nothing acts beside a step that is over.
    let (held, run) = run_on(
        &engine,
        "three at once",
        new_workflow(
            "crowded",
            vec![
                bounded(
                    hold("hold"),
                    vec![
                        reminder("nudge", 60, 3, "still holding"),
                        timeout("first", 60),
                        timeout("second", 60),
                    ],
                    "released",
                    &[("first", "took-first"), ("second", "took-second")],
                ),
                tell("released", "released"),
                tell("took-first", "the first timeout"),
                tell("took-second", "the second timeout"),
            ],
        ),
    );
    let t0 = entered_at(&engine, run.id, "hold");
    engine.tick_waits_at(t0 + 60);
    let done = finished_run(&engine, held.id).await;
    let hold = &done.steps[&sid("hold")];
    assert_eq!(
        hold.state,
        StepState::Diverted {
            by: branch("first")
        }
    );
    assert_eq!(
        hold.fired.keys().cloned().collect::<Vec<_>>(),
        vec![branch("first")],
        "what came after the divert fired nothing"
    );
    assert_eq!(
        said(&engine, &held.id.to_string()),
        vec!["the first timeout"]
    );
    engine.shutdown().await;
}

/// The run's snapshot is the truth: after a restart every live step's
/// boundaries are armed again from it — a timer from the moment its step was
/// entered, a reminder from how often it already fired.
#[tokio::test(flavor = "multi_thread")]
async fn boundary_timers_are_armed_again_from_the_snapshot_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let (goal, run) = run_on(
        &engine,
        "held across a restart",
        new_workflow(
            "durable",
            vec![
                bounded(
                    hold("hold"),
                    vec![
                        reminder("nudge", 60, 3, "still holding"),
                        timeout("late", 200),
                    ],
                    "released",
                    &[("late", "gave-up")],
                ),
                tell("released", "released"),
                tell("gave-up", "gave up"),
            ],
        ),
    );
    let thread = goal.id.to_string();
    let t0 = entered_at(&engine, run.id, "hold");
    engine.tick_waits_at(t0 + 60);
    said_times(&engine, &thread, "still holding", 1).await;
    engine.shutdown().await;

    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    assert_eq!(
        engine.armed_boundaries(run.id, &sid("hold")),
        vec![branch("late"), branch("nudge")],
        "armed again from the snapshot"
    );
    // The reminder goes on from its second time, not from its first again.
    engine.tick_waits_at(t0 + 61);
    assert_eq!(said(&engine, &thread).len(), 1, "it fired at sixty already");
    engine.tick_waits_at(t0 + 120);
    assert_eq!(
        run_of(&engine, run.id).steps[&sid("hold")].fired[&branch("nudge")].count,
        2,
        "fired for the second time, on the run the restart read"
    );
    let troubles: Vec<String> = notes(&engine, goal.id)
        .into_iter()
        .filter(|n| n.contains("could not act"))
        .collect();
    assert!(troubles.is_empty(), "the reminder acted: {troubles:?}");
    said_times(&engine, &thread, "still holding", 2).await;
    engine.tick_waits_at(t0 + 180);
    said_times(&engine, &thread, "still holding", 3).await;
    assert_eq!(
        engine.armed_boundaries(run.id, &sid("hold")),
        vec![branch("late")],
        "the reminder is spent"
    );

    // The timeout counts from the step's entry, not from the restart.
    engine.tick_waits_at(t0 + 200);
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(
        done.steps[&sid("hold")].state,
        StepState::Diverted { by: branch("late") }
    );
    assert_eq!(done.steps[&sid("hold")].fired[&branch("nudge")].count, 3);
    assert_eq!(
        said(&engine, &thread).last().map(String::as_str),
        Some("gave up")
    );
    engine.shutdown().await;
}
