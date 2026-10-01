//! `wait` steps — catch events: the run holds for a delay, a moment, a
//! schedule, a named signal, a message, a run's end, a platform topic or a
//! person, and moves when the outside world does. The clock is an argument
//! (`tick_waits_at`) and the ear is a call (`hear`, `emit_signal`), so
//! nothing here waits for a timer or races a bus.

use crate::common;

use bisa_core::{
    MessageBody, MessageFilter, MessageFrom, PlatformFilter, RunEnd, RunFilter, RunOutcome,
    RunStatus, Signal, SignalFilter, SignalScope, SignalSource, StartOn, StepKind, StepState,
    ValueRef, WaitFor,
};
use bisa_engine::{Engine, EnginePayload};
use bisa_harness::mock::MockAdapter;
use common::*;
use serde_json::json;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn wait(id: &str, until: WaitFor) -> bisa_core::Step {
    step(id, StepKind::Wait { until })
}

/// A wait for the named signal `name` whose payload carries `fields`.
fn signal(name: &str, fields: &[(&str, &str)]) -> WaitFor {
    WaitFor::Signal {
        filter: SignalFilter {
            name: name.into(),
            fields: fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        },
    }
}

/// Raise a named signal as a person would, in the workspace or on a goal.
fn raise(engine: &Engine, name: &str, payload: serde_json::Value, scope: SignalScope) {
    engine.emit_signal(name, payload, scope).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_delay_wait_elapses_on_the_ticker_clock() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let (goal, run) = run_on(
        &engine,
        "wait a bit",
        new_workflow(
            "delayed",
            vec![wait(
                "nap",
                WaitFor::Delay {
                    secs: ValueRef::Fixed(3600),
                },
            )],
        ),
    );
    assert_eq!(run.status(), RunStatus::Waiting);
    assert!(engine
        .armed_waits()
        .iter()
        .any(|(r, s, _)| *r == run.id && s == &sid("nap")));

    // Not due yet.
    engine.tick_waits_at(now() + 10);
    assert_eq!(current_run(&engine, goal.id).status(), RunStatus::Waiting);
    // Due.
    engine.tick_waits_at(now() + 3601);
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert!(engine.armed_waits().is_empty());
    engine.shutdown().await;
}

/// A clock that steps back — a machine whose time was corrected, a laptop
/// off the network for a night — fires nothing early and loses nothing: a
/// tick before the wait was armed is a tick on which nothing is due, the wait
/// stays armed, and it elapses once the clock reaches its due time again. A
/// due time is compared, never subtracted from: no wait comes due by
/// underflow.
#[tokio::test(flavor = "multi_thread")]
async fn a_clock_that_steps_back_fires_nothing_early_and_the_wait_still_elapses_in_its_time() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let (goal, run) = run_on(
        &engine,
        "wait through a clock that steps back",
        new_workflow(
            "delayed",
            vec![wait(
                "nap",
                WaitFor::Delay {
                    secs: ValueRef::Fixed(3600),
                },
            )],
        ),
    );
    let is_ours = |(r, s, _): &(bisa_core::RunId, bisa_core::StepId, WaitFor)| {
        *r == run.id && *s == sid("nap")
    };
    assert!(
        engine.armed_waits().iter().any(is_ours),
        "the wait is armed"
    );
    // One clock: the step's entry, and the delay counted from it.
    let entered = run_of(&engine, run.id).steps[&sid("nap")]
        .started_at
        .expect("the step was entered");
    let due = entered + 3600;
    assert_eq!(
        engine.workspace().list_due_waits(due).unwrap(),
        vec![(run.id, sid("nap"))]
    );

    // The clock steps back a day, then to the epoch itself: nothing is due.
    engine.tick_waits_at(entered.saturating_sub(86_400));
    engine.tick_waits_at(0);
    assert_eq!(current_run(&engine, goal.id).status(), RunStatus::Waiting);
    assert!(
        engine.armed_waits().iter().any(is_ours),
        "the wait stays armed"
    );
    assert_eq!(
        engine.workspace().list_due_waits(due - 1).unwrap(),
        vec![],
        "and is not due a second before its time"
    );
    // The clock reaches the due time: the wait elapses as it would have.
    engine.tick_waits_at(due);
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert!(engine.armed_waits().is_empty());
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_schedule_wait_comes_due_by_cron() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let (goal, _) = run_on(
        &engine,
        "wait for nine",
        new_workflow(
            "scheduled",
            vec![wait(
                "nine",
                WaitFor::Schedule {
                    cron: ValueRef::Fixed("0 9 * * *".into()),
                    tz: Some("UTC".into()),
                },
            )],
        ),
    );
    // The next 09:00 UTC from now is at most a day away; a tick a day later
    // is past it, a tick now is not.
    engine.tick_waits_at(now());
    assert_eq!(current_run(&engine, goal.id).status(), RunStatus::Waiting);
    engine.tick_waits_at(now() + 86_400 + 1);
    assert_eq!(
        finished_run(&engine, goal.id).await.outcome,
        Some(RunOutcome::Done)
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_signal_wait_matches_a_name_with_its_fields_and_only_in_its_scope() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let (goal, run) = run_on(
        &engine,
        "wait for green",
        new_workflow(
            "signalled",
            vec![wait(
                "deploy",
                signal("deploy.finished", &[("status", "green")]),
            )],
        ),
    );
    let (other, _) = run_on(
        &engine,
        "someone else's wait",
        new_workflow(
            "signalled",
            vec![wait("deploy", signal("deploy.finished", &[]))],
        ),
    );

    // A wrong field does not move it.
    raise(
        &engine,
        "deploy.finished",
        json!({"status": "red"}),
        SignalScope::Goal { goal: goal.id },
    );
    assert_eq!(current_run(&engine, goal.id).status(), RunStatus::Waiting);
    assert_eq!(current_run(&engine, other.id).status(), RunStatus::Waiting);
    // A matching one, raised on a different goal, does not reach this one…
    raise(
        &engine,
        "deploy.finished",
        json!({"status": "green"}),
        SignalScope::Goal { goal: other.id },
    );
    assert_eq!(current_run(&engine, goal.id).status(), RunStatus::Waiting);
    assert_eq!(
        finished_run(&engine, other.id).await.outcome,
        Some(RunOutcome::Done)
    );
    // …and one for this goal does, carrying the payload as the step's output.
    raise(
        &engine,
        "deploy.finished",
        json!({"status": "green", "sha": "abc"}),
        SignalScope::Goal { goal: goal.id },
    );
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.id, run.id);
    assert_eq!(
        done.steps[&sid("deploy")].output.as_ref().unwrap()["sha"],
        json!("abc")
    );
    engine.shutdown().await;
}

/// A signal nobody in particular raised reaches a goal's run and a run of
/// the workspace alike; a goal's signal never reaches a run of the workspace.
#[tokio::test(flavor = "multi_thread")]
async fn a_workspace_run_never_hears_a_goals_signal() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let (goal, _) = run_on(
        &engine,
        "a goal that raises",
        new_workflow("held", vec![wait("hold", WaitFor::Release)]),
    );
    let (_, run) = workspace_run(
        &engine,
        new_workflow("listening", vec![wait("green", signal("lights", &[]))]),
    );
    raise(
        &engine,
        "lights",
        json!({}),
        SignalScope::Goal { goal: goal.id },
    );
    assert_eq!(run_of(&engine, run.id).status(), RunStatus::Waiting);
    raise(
        &engine,
        "lights",
        json!({"by": "anyone"}),
        SignalScope::Workspace,
    );
    let done = run_finished(&engine, run.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert_eq!(
        done.steps[&sid("green")].output.as_ref().unwrap()["by"],
        json!("anyone")
    );
    engine.shutdown().await;
}

/// A `run` wait holds for a run's end — of one workflow, with one outcome —
/// and a `platform` wait for one of the engine's own topics with exact
/// fields. The ear hears the bus by itself; the event is handed over by hand
/// as well, so the test does not depend on which was armed first — a wait
/// that caught it holds for nothing more. A run of the workspace ends for
/// everybody; a goal's run ends for its goal alone.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_wait_and_a_platform_wait_hear_a_run_that_ended() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    // A run of the workspace does the work; three goals wait on its end.
    let (worker, worker_run) =
        workspace_run(&engine, new_workflow("w", vec![agent_step("go", "mock")]));
    let watching = |statement: &str, until: WaitFor| {
        run_on(
            &engine,
            statement,
            new_workflow("watching", vec![wait("for-it", until)]),
        )
        .0
    };
    let by_run = watching(
        "watch the workflow",
        WaitFor::Run {
            filter: RunFilter {
                workflow: Some(worker.id),
                outcome: Some(RunEnd::Done),
            },
        },
    );
    let by_topic = watching(
        "watch the topic",
        WaitFor::Platform {
            filter: PlatformFilter {
                topic: "run.finished".into(),
                fields: BTreeMap::from([
                    ("run".to_string(), worker_run.id.to_string()),
                    ("outcome".to_string(), "done".to_string()),
                ]),
            },
        },
    );
    let for_a_failure = watching(
        "watch for a failure",
        WaitFor::Run {
            filter: RunFilter {
                workflow: Some(worker.id),
                outcome: Some(RunEnd::Failed),
            },
        },
    );
    let worker_done = run_finished(&engine, worker_run.id).await;
    let ended = |run: &bisa_core::WorkflowRun| {
        bisa_engine::EngineEvent::of_run(
            run,
            None,
            EnginePayload::RunFinished {
                run: run.id,
                workflow: run.workflow.id,
                outcome: RunOutcome::Done,
            },
        )
    };
    engine.hear(&ended(&worker_done));
    let heard = finished_run(&engine, by_run.id).await;
    assert_eq!(heard.outcome, Some(RunOutcome::Done));
    let output = heard.steps[&sid("for-it")].output.clone().unwrap();
    assert_eq!(output["run"], json!(worker_done.id.to_string()));
    assert_eq!(output["workflow"], json!(worker.id.to_string()));
    assert_eq!(output["outcome"], json!("done"));
    assert_eq!(
        finished_run(&engine, by_topic.id).await.outcome,
        Some(RunOutcome::Done)
    );
    assert_eq!(
        current_run(&engine, for_a_failure.id).status(),
        RunStatus::Waiting,
        "a run that ended done is not the failure it holds for"
    );

    // A goal's run that ended is its goal's news: another goal's wait for
    // any run's end does not hear it.
    let any_end = watching(
        "watch anything end",
        WaitFor::Run {
            filter: RunFilter::default(),
        },
    );
    engine.hear(&ended(&heard));
    assert_eq!(
        current_run(&engine, any_end.id).status(),
        RunStatus::Waiting,
        "another goal's run is not this goal's to hear"
    );
    engine.shutdown().await;
}

/// A `message` wait holds for a message in one conversation, from the
/// person, saying something: it is heard where conversation dispatch hears
/// it, and the message is the step's output. An announcement — a workflow's
/// own post — is never heard.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_wait_hears_what_the_person_says_and_never_an_announcement() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    let general = bisa_core::ChannelId::general().to_string();
    let (goal, _) = run_on(
        &engine,
        "wait for the word",
        new_workflow(
            "spoken to",
            vec![wait(
                "word",
                WaitFor::Message {
                    filter: MessageFilter {
                        r#in: Some(general.clone()),
                        from: MessageFrom::You,
                        mentions: None,
                        contains: Some("Go Ahead".into()),
                    },
                },
            )],
        ),
    );
    step_in_state(&engine, goal.id, "word", "waiting").await;
    let say = |text: &str, origin: bisa_store::PostOrigin| {
        ws.post_message(
            &general,
            MessageBody::post(text.to_string()),
            None,
            &[],
            &[],
            None,
            origin,
        )
        .unwrap()
    };
    say("please go ahead", bisa_store::PostOrigin::Announced);
    say("not yet", bisa_store::PostOrigin::Asked);
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(current_run(&engine, goal.id).status(), RunStatus::Waiting);
    let message = say("you may go ahead now", bisa_store::PostOrigin::Asked);
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    let output = done.steps[&sid("word")].output.clone().unwrap();
    assert_eq!(output["message"], json!(message));
    assert_eq!(output["author_kind"], json!("you"));
    assert_eq!(output["scope"], json!(general));
    assert_eq!(output["text"], json!("you may go ahead now"));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_release_wait_moves_only_when_a_person_releases_it() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let (goal, _) = run_on(
        &engine,
        "hold",
        new_workflow("held", vec![wait("hold", WaitFor::Release)]),
    );
    engine.tick_waits_at(now() + 1_000_000);
    raise(&engine, "anything", json!({}), SignalScope::Workspace);
    assert_eq!(
        current_run(&engine, goal.id).steps[&sid("hold")].state,
        StepState::Waiting
    );
    // Releasing a step that is not waiting is refused.
    assert!(engine
        .release_step(current_run(&engine, goal.id).id, &sid("nope"), None)
        .is_err());
    assert!(
        engine
            .armed_waits()
            .iter()
            .any(|(_, s, _)| s == &sid("hold")),
        "a refused release leaves what is armed, armed"
    );
    engine
        .release_step(current_run(&engine, goal.id).id, &sid("hold"), None)
        .unwrap();
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert!(engine.armed_waits().is_empty());
    engine.shutdown().await;
}

/// A call that releases a wait may carry a payload: it is the step's output,
/// what a later step reads.
#[tokio::test(flavor = "multi_thread")]
async fn a_release_with_a_payload_is_the_steps_output() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let mut hold = wait("hold", WaitFor::Release);
    hold.then = vec![bisa_core::Flow::to(sid("tell"))];
    let tell = step(
        "tell",
        StepKind::Notify {
            scope: None,
            template: "released by {steps.hold.output.by}".into(),
            mentions: vec![],
            author: None,
        },
    );
    let (goal, run) = run_on(
        &engine,
        "held for a call",
        new_workflow("held", vec![hold, tell]),
    );
    engine
        .release_step(run.id, &sid("hold"), Some(json!({"by": "the deploy bot"})))
        .unwrap();
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert_eq!(
        done.steps[&sid("hold")].output,
        Some(json!({"by": "the deploy bot"}))
    );
    let messages = engine
        .workspace()
        .messages(&goal.id.to_string(), None, 10)
        .unwrap();
    assert_eq!(messages[0].content, "released by the deploy bot");
    engine.shutdown().await;
}

/// A `time` wait holds until a moment an input names — Unix seconds or an
/// RFC 3339 time — whatever the clock said when it was armed.
#[tokio::test(flavor = "multi_thread")]
async fn a_time_wait_comes_due_at_the_moment_an_input_names() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let mut draft = new_workflow(
        "until the deadline",
        vec![wait(
            "deadline",
            WaitFor::Time {
                at: "{inputs.deadline}".into(),
            },
        )],
    );
    draft.inputs = vec![text_input("deadline")];
    // 2100-01-01T00:00:00Z.
    let moment = 4_102_444_800u64;
    let (goal, run) = start_with_inputs(
        &engine,
        "hold until 2100",
        draft,
        BTreeMap::from([("deadline".to_string(), json!("2100-01-01T00:00:00Z"))]),
    );
    assert_eq!(run.status(), RunStatus::Waiting);
    engine.tick_waits_at(moment - 1);
    assert_eq!(current_run(&engine, goal.id).status(), RunStatus::Waiting);
    engine.tick_waits_at(moment);
    assert_eq!(
        finished_run(&engine, goal.id).await.outcome,
        Some(RunOutcome::Done)
    );

    // A moment nobody can read fails the step, in words.
    let mut unreadable = new_workflow(
        "until whenever",
        vec![wait(
            "deadline",
            WaitFor::Time {
                at: "{inputs.deadline}".into(),
            },
        )],
    );
    unreadable.inputs = vec![text_input("deadline")];
    let (goal, _) = start_with_inputs(
        &engine,
        "hold until next tuesday",
        unreadable,
        BTreeMap::from([("deadline".to_string(), json!("next tuesday"))]),
    );
    let failed = finished_run(&engine, goal.id).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed));
    let why = failed.steps[&sid("deadline")]
        .error
        .clone()
        .unwrap_or_default();
    assert!(why.contains("next tuesday"), "{why}");
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn waits_are_rearmed_from_the_run_snapshot_on_start() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let (goal, run) = run_on(
        &engine,
        "survive a restart",
        new_workflow(
            "durable",
            vec![
                {
                    let mut s = wait(
                        "nap",
                        WaitFor::Delay {
                            secs: ValueRef::Fixed(60),
                        },
                    );
                    s.then = vec![bisa_core::Flow::to(sid("green"))];
                    s
                },
                wait("green", signal("lights", &[])),
            ],
        ),
    );
    let armed_at = now();
    engine.shutdown().await;

    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    assert!(
        engine
            .armed_waits()
            .iter()
            .any(|(r, s, _)| *r == run.id && s == &sid("nap")),
        "the delay is armed again from the snapshot"
    );
    // The delay counts from when the step started, not from the restart.
    engine.tick_waits_at(armed_at + 61);
    step_in_state(&engine, goal.id, "green", "waiting").await;
    raise(&engine, "lights", json!({}), SignalScope::Workspace);
    assert_eq!(
        finished_run(&engine, goal.id).await.outcome,
        Some(RunOutcome::Done)
    );
    engine.shutdown().await;
}

/// A start and a wait can hold for the same signal; both hear it. The wait
/// moves its run directly — no signal is queued for it — and the start's
/// listener gets a signal of its own, which begins its run.
#[tokio::test(flavor = "multi_thread")]
async fn a_wait_and_a_start_both_hear_one_signal() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let mut on_lights = step(
        "lights",
        StepKind::Start {
            on: StartOn::Signal {
                filter: SignalFilter {
                    name: "lights".into(),
                    fields: BTreeMap::new(),
                },
            },
            inputs: BTreeMap::new(),
            guard: Default::default(),
        },
    );
    on_lights.then = vec![bisa_core::Flow::to(sid("hold"))];
    let listening = engine
        .create_workflow(new_workflow(
            "starts on lights",
            vec![on_lights, wait("hold", WaitFor::Release)],
        ))
        .unwrap();
    let host = bisa_core::ListenerHost::Workspace {
        workflow: listening.id,
    };
    engine.set_listening(host, BTreeMap::new(), None).unwrap();
    let (goal, _) = run_on(
        &engine,
        "also holding for it",
        new_workflow("waiting", vec![wait("green", signal("lights", &[]))]),
    );
    let emitted = engine
        .emit_signal("lights", json!({}), SignalScope::Workspace)
        .unwrap();
    assert_eq!(
        emitted.listeners.len(),
        1,
        "one start listened, one signal queued"
    );
    assert_eq!(
        finished_run(&engine, goal.id).await.outcome,
        Some(RunOutcome::Done)
    );
    assert_eq!(engine.drain_signals().await, 1);
    let started = engine.workflow_runs(listening.id).unwrap();
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].start, Some(sid("lights")));
    engine.shutdown().await;
}

fn text_input(name: &str) -> bisa_core::InputDef {
    bisa_core::InputDef {
        name: bisa_core::InputName::new(name).unwrap(),
        label: name.to_uppercase(),
        kind: bisa_core::InputKind::Text,
        default: None,
        required: true,
    }
}

fn number_input(name: &str) -> bisa_core::InputDef {
    bisa_core::InputDef {
        kind: bisa_core::InputKind::Number,
        ..text_input(name)
    }
}

fn start_with_inputs(
    engine: &Engine,
    statement: &str,
    draft: bisa_store::NewWorkflow,
    inputs: BTreeMap<String, serde_json::Value>,
) -> (bisa_core::Goal, bisa_core::WorkflowRun) {
    let (goal, _) = goal_on(engine, statement, draft);
    let run = engine.start_run(goal.id, inputs).unwrap();
    (goal, run)
}

/// A signal wait's fields are templates: they are rendered against the run
/// before they are matched, so `env = "{inputs.env}"` waits for the value.
#[tokio::test(flavor = "multi_thread")]
async fn signal_fields_are_rendered_before_matching() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let mut draft = new_workflow(
        "env gated",
        vec![wait(
            "hold",
            signal("deploy.finished", &[("env", "{inputs.env}")]),
        )],
    );
    draft.inputs = vec![text_input("env")];
    let (goal, run) = start_with_inputs(
        &engine,
        "wait for prod",
        draft,
        BTreeMap::from([("env".to_string(), json!("prod"))]),
    );
    assert_eq!(run.status(), RunStatus::Waiting);
    // What is armed is the rendered value, not the template.
    let armed = engine.armed_waits();
    let (_, _, until) = armed.iter().find(|(r, _, _)| *r == run.id).unwrap();
    assert_eq!(*until, signal("deploy.finished", &[("env", "prod")]));
    // The literal template text does not match…
    raise(
        &engine,
        "deploy.finished",
        json!({"env": "{inputs.env}"}),
        SignalScope::Workspace,
    );
    assert_eq!(current_run(&engine, goal.id).status(), RunStatus::Waiting);
    // …a staging deploy does not match…
    raise(
        &engine,
        "deploy.finished",
        json!({"env": "staging"}),
        SignalScope::Workspace,
    );
    assert_eq!(current_run(&engine, goal.id).status(), RunStatus::Waiting);
    // …the rendered value does.
    raise(
        &engine,
        "deploy.finished",
        json!({"env": "prod"}),
        SignalScope::Workspace,
    );
    assert_eq!(
        finished_run(&engine, goal.id).await.outcome,
        Some(RunOutcome::Done)
    );
    engine.shutdown().await;
}

/// The name itself is a template too.
#[tokio::test(flavor = "multi_thread")]
async fn signal_name_is_templatable() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let mut draft = new_workflow(
        "topic from input",
        vec![wait("hold", signal("{inputs.name}", &[]))],
    );
    draft.inputs = vec![text_input("name")];
    let (goal, _) = start_with_inputs(
        &engine,
        "wait for a name",
        draft,
        BTreeMap::from([("name".to_string(), json!("build.green"))]),
    );
    raise(&engine, "build.red", json!({}), SignalScope::Workspace);
    assert_eq!(current_run(&engine, goal.id).status(), RunStatus::Waiting);
    raise(&engine, "build.green", json!({}), SignalScope::Workspace);
    assert_eq!(
        finished_run(&engine, goal.id).await.outcome,
        Some(RunOutcome::Done)
    );
    engine.shutdown().await;
}

/// A delay read from a number input is due that many seconds after arming.
#[tokio::test(flavor = "multi_thread")]
async fn delay_from_input() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let mut draft = new_workflow(
        "hold for a while",
        vec![wait(
            "nap",
            WaitFor::Delay {
                secs: ValueRef::Input {
                    input: bisa_core::InputName::new("hold").unwrap(),
                },
            },
        )],
    );
    draft.inputs = vec![number_input("hold")];
    let (goal, run) = start_with_inputs(
        &engine,
        "nap a while",
        draft,
        BTreeMap::from([("hold".to_string(), json!(600))]),
    );
    let armed = engine.armed_waits();
    let (_, _, until) = armed.iter().find(|(r, _, _)| *r == run.id).unwrap();
    assert_eq!(
        *until,
        WaitFor::Delay {
            secs: ValueRef::Fixed(600)
        }
    );
    engine.tick_waits_at(now() + 10);
    assert_eq!(current_run(&engine, goal.id).status(), RunStatus::Waiting);
    engine.tick_waits_at(now() + 601);
    assert_eq!(
        finished_run(&engine, goal.id).await.outcome,
        Some(RunOutcome::Done)
    );
    engine.shutdown().await;
}

/// A number input that arrived as text — a CLI `--input hold=0` — is a whole
/// number once coerced, and the delay arms.
#[tokio::test(flavor = "multi_thread")]
async fn a_delay_read_from_a_text_sourced_input_arms() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let mut draft = new_workflow(
        "hold for no time",
        vec![wait(
            "nap",
            WaitFor::Delay {
                secs: ValueRef::Input {
                    input: bisa_core::InputName::new("hold").unwrap(),
                },
            },
        )],
    );
    draft.inputs = vec![number_input("hold")];
    let hold = bisa_engine::effects::coerce_input(&bisa_core::InputKind::Number, "0").unwrap();
    assert_eq!(hold, json!(0), "whole text is a whole number");
    let (goal, _) = start_with_inputs(
        &engine,
        "nap for no time",
        draft,
        BTreeMap::from([("hold".to_string(), hold)]),
    );
    engine.tick_waits_at(now() + 1);
    assert_eq!(
        finished_run(&engine, goal.id).await.outcome,
        Some(RunOutcome::Done)
    );
    engine.shutdown().await;
}

/// A wait whose values cannot be resolved fails its step with the reason,
/// rather than holding the run on nothing.
#[tokio::test(flavor = "multi_thread")]
async fn unresolvable_wait_fails_the_step() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    // The field names an upstream output that does not exist at arm time.
    let (goal, _) = run_on(
        &engine,
        "wait on nothing",
        new_workflow(
            "unresolvable",
            vec![wait("hold", signal("x", &[("k", "{goal.title}")]))],
        ),
    );
    let failed = finished_run(&engine, goal.id).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed));
    assert_eq!(failed.steps[&sid("hold")].state, StepState::Failed);
    let why = failed.steps[&sid("hold")].error.clone().unwrap_or_default();
    assert!(why.contains("cannot be armed"), "{why}");
    assert!(why.contains("goal.title"), "{why}");
    assert!(engine.armed_waits().is_empty());
    engine.shutdown().await;
}

/// The wait clock is the engine's own: a delay comes due on a node whose
/// listening tasks are off, with nobody ticking anything by hand.
#[tokio::test(flavor = "multi_thread")]
async fn a_delay_wait_comes_due_with_listening_off() {
    let dir = tempfile::tempdir().unwrap();
    let config = bisa_engine::EngineConfig {
        wait_tick_secs: 1,
        ..design_off_config()
    };
    let engine = Engine::start(
        workspace(&dir),
        catalog_with(vec![MockAdapter::default()]),
        config,
    )
    .unwrap();
    let (goal, run) = run_on(
        &engine,
        "wait a second",
        new_workflow(
            "short",
            vec![wait(
                "nap",
                WaitFor::Delay {
                    secs: ValueRef::Fixed(1),
                },
            )],
        ),
    );
    assert_eq!(run.status(), RunStatus::Waiting);
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    engine.shutdown().await;
}

/// A schedule counts from when the step was entered, like a delay: an
/// occurrence that fell while the node was down is due on the first tick —
/// once — and one that has not come yet is not.
#[tokio::test(flavor = "multi_thread")]
async fn a_schedule_wait_missed_during_downtime_fires_once_on_the_first_tick() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let (goal, run) = run_on(
        &engine,
        "every minute",
        new_workflow(
            "minutely",
            vec![wait(
                "tick",
                WaitFor::Schedule {
                    cron: ValueRef::Fixed("* * * * *".into()),
                    tz: Some("UTC".into()),
                },
            )],
        ),
    );
    step_in_state(&engine, goal.id, "tick", "waiting").await;
    let started_at = engine.workspace().get_run(run.id).unwrap().steps[&sid("tick")]
        .started_at
        .expect("entered");
    engine.shutdown().await;

    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    assert!(
        engine
            .armed_waits()
            .iter()
            .any(|(r, s, _)| *r == run.id && s == &sid("tick")),
        "the schedule is armed again from the snapshot"
    );
    // Not yet: the next minute after the step started has not come. Ticked
    // at the start itself — a second later may already be that minute when
    // the step was entered at :59.
    engine.tick_waits_at(started_at);
    assert_eq!(
        engine.workspace().get_run(run.id).unwrap().steps[&sid("tick")].state,
        StepState::Waiting
    );
    // The downtime passed it: due at once, and once.
    engine.tick_waits_at(started_at + 130);
    assert_eq!(
        finished_run(&engine, goal.id).await.outcome,
        Some(RunOutcome::Done)
    );
    engine.shutdown().await;
}

/// A named signal raised while the node was down is one the wait would have
/// heard: every emit is recorded once, listener or not, and on re-arm the
/// record is read back and the wait completes.
#[tokio::test(flavor = "multi_thread")]
async fn a_signal_raised_while_the_node_was_down_completes_a_signal_wait_on_rearm() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let (goal, _) = run_on(
        &engine,
        "wait for the lights",
        new_workflow(
            "listening",
            vec![wait("green", signal("lights", &[("colour", "green")]))],
        ),
    );
    step_in_state(&engine, goal.id, "green", "waiting").await;
    engine.shutdown().await;

    // The store alone, as the emit door would have written before the node
    // stopped: two named signals, recorded with no listener.
    {
        let ws = bisa_store::Workspace::open_with_keystore(
            dir.path(),
            Box::new(bisa_store::FileKeyStore::new(
                bisa_store::Paths::new(dir.path()).identity_dir(),
            )),
        )
        .unwrap();
        // A red light first — the fields do not match — then the green.
        for (id, colour) in [("sig-red", "red"), ("sig-green", "green")] {
            ws.enqueue_signal(&Signal {
                id: id.into(),
                listener: None,
                source: SignalSource::Signal,
                name: Some("lights".into()),
                at: now(),
                payload: json!({"colour": colour}),
                scope: SignalScope::Workspace,
                chain: Default::default(),
                dedupe_key: None,
            })
            .unwrap();
        }
    }

    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done), "{done:?}");
    assert_eq!(
        done.steps[&sid("green")]
            .output
            .as_ref()
            .and_then(|o| o.get("colour")),
        Some(&json!("green")),
        "the matching signal's payload is the step's answer"
    );
    engine.shutdown().await;
}

/// A wait that hears a signal with a causal chain behind it widens its run's
/// chain: what the run raises next still names the listeners that led here.
#[tokio::test(flavor = "multi_thread")]
async fn a_wait_that_hears_a_chained_signal_widens_its_runs_chain() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let mut green = wait("green", signal("lights", &[]));
    green.then = vec![bisa_core::Flow::to(sid("hold"))];
    let (goal, run) = run_on(
        &engine,
        "hears a chained signal",
        new_workflow("chained", vec![green, wait("hold", WaitFor::Release)]),
    );
    assert!(run.chain.is_empty());
    let behind: bisa_core::ListenerKey = format!(
        "workspace:{}/upstream",
        bisa_core::WorkflowId::from_ulid(ulid::Ulid::from_parts(1, 1))
    )
    .parse()
    .unwrap();
    let chain = bisa_core::Chain::default().extend(&behind);
    bisa_engine::listen::emit::emit(
        engine.inner(),
        "lights",
        json!({}),
        SignalScope::Workspace,
        chain.clone(),
        None,
    )
    .unwrap();
    let after = step_in_state(&engine, goal.id, "hold", "waiting").await;
    assert_eq!(after.chain, chain, "the run carries the line it heard");
    engine.shutdown().await;
}
