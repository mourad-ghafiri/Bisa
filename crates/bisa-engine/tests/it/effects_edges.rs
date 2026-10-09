//! What the interpreter does when a step's effect cannot be carried out —
//! each kind failing in its own words, with the run saying so — and the
//! refusals the engine's doors give before anything is written. Fakes only:
//! the mock harness, templates that cannot render, inputs of the wrong kind,
//! a command the guard never runs.

use crate::common;

use bisa_core::{
    AnswerError, Assignee, CheckKind, ClosureReason, Home, InputDef, InputKind, InputName,
    RunOutcome, RunStatus, StepKind, StepState, ValueRef, WaitFor,
};
use bisa_engine::scheduler::{preflight, Launch, ScheduleRejection};
use bisa_engine::{Engine, EngineConfig, EngineError};
use bisa_harness::mock::MockAdapter;
use common::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn text_input(name: &str) -> InputDef {
    InputDef {
        name: InputName::new(name).unwrap(),
        label: name.to_uppercase(),
        kind: InputKind::Text,
        default: None,
        required: true,
    }
}

fn number_input(name: &str) -> InputDef {
    InputDef {
        kind: InputKind::Number,
        ..text_input(name)
    }
}

fn assignee_input(name: &str) -> InputDef {
    InputDef {
        kind: InputKind::Assignee,
        ..text_input(name)
    }
}

/// An input a run may leave unbound: not required, no default.
fn optional(mut def: InputDef) -> InputDef {
    def.required = false;
    def
}

fn with_inputs(
    mut draft: bisa_store::NewWorkflow,
    inputs: Vec<InputDef>,
) -> bisa_store::NewWorkflow {
    draft.inputs = inputs;
    draft
}

fn notify(id: &str, scope: Option<&str>, template: &str) -> bisa_core::Step {
    step(
        id,
        StepKind::Notify {
            scope: scope.map(str::to_string),
            template: template.into(),
            mentions: vec![],
            author: None,
        },
    )
}

fn input_ref(name: &str) -> ValueRef<Assignee> {
    ValueRef::Input {
        input: InputName::new(name).unwrap(),
    }
}

/// Start the goal's run with `inputs` and wait for it to end.
async fn ran_with(
    engine: &Engine,
    statement: &str,
    draft: bisa_store::NewWorkflow,
    inputs: BTreeMap<String, Value>,
) -> bisa_core::WorkflowRun {
    let (goal, _) = goal_on(engine, statement, draft);
    engine.start_run(goal.id, inputs).unwrap();
    finished_run(engine, goal.id).await
}

fn failed_step<'a>(run: &'a bisa_core::WorkflowRun, step: &str) -> &'a str {
    let record = &run.steps[&sid(step)];
    assert_eq!(record.state, StepState::Failed, "{run:?}");
    record.error.as_deref().unwrap_or_default()
}

/// A schema check reads the upstream step's output: it passes when the
/// output fits and fails naming what does not. (A check of a step that
/// writes nothing, or one that may have failed, never validates:
/// `NoSuchOutput`, `NotAssured`.)
#[tokio::test(flavor = "multi_thread")]
async fn a_schema_check_reads_the_step_it_names() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let check = |id: &str, of: &str, schema: Value| {
        step(
            id,
            StepKind::Check {
                check: CheckKind::Schema {
                    schema,
                    of: Some(sid(of)),
                },
            },
        )
    };
    let fits = ran_with(
        &engine,
        "fits",
        new_workflow(
            "fits",
            chain(vec![
                notify("say", None, "hello"),
                check(
                    "shape",
                    "say",
                    json!({ "type": "object", "required": ["message"] }),
                ),
            ]),
        ),
        BTreeMap::new(),
    )
    .await;
    assert_eq!(fits.outcome, Some(RunOutcome::Done), "{fits:?}");
    let evidence = fits.steps[&sid("shape")].output.clone().unwrap_or_default();
    assert!(evidence.to_string().contains("schema ok"), "{evidence}");

    let misfit = ran_with(
        &engine,
        "misfit",
        new_workflow(
            "misfit",
            chain(vec![
                notify("say", None, "hello"),
                check(
                    "shape",
                    "say",
                    json!({ "type": "object", "required": ["number"] }),
                ),
            ]),
        ),
        BTreeMap::new(),
    )
    .await;
    assert_eq!(misfit.outcome, Some(RunOutcome::Failed), "{misfit:?}");
    assert!(
        failed_step(&misfit, "shape").contains("number"),
        "{}",
        failed_step(&misfit, "shape")
    );

    engine.shutdown().await;
}

/// A command check that outstays the node's deadline fails the step and
/// says how long it had.
#[tokio::test(flavor = "multi_thread")]
async fn a_command_check_that_outstays_its_deadline_fails_the_step() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::start(
        workspace(&dir),
        catalog_with(vec![MockAdapter::default()]),
        EngineConfig {
            check_timeout_secs: 1,
            ..design_off_config()
        },
    )
    .unwrap();
    let run = ran_with(
        &engine,
        "slow check",
        new_workflow(
            "slow",
            vec![step(
                "verify",
                StepKind::Check {
                    check: CheckKind::Command {
                        command: "sleep 5".into(),
                    },
                },
            )],
        ),
        BTreeMap::new(),
    )
    .await;
    assert_eq!(run.outcome, Some(RunOutcome::Failed), "{run:?}");
    assert!(
        failed_step(&run, "verify").contains("timed out after 1s"),
        "{}",
        failed_step(&run, "verify")
    );
    engine.shutdown().await;
}

/// A notify into a conversation that does not exist, one whose author is
/// read from an input that names a team, an emit whose rendered signal is
/// no name, and a spawn of an archived workflow: each fails its step with
/// the reason, and the run says so.
#[tokio::test(flavor = "multi_thread")]
async fn a_notify_an_emit_and_a_spawn_that_cannot_act_fail_their_steps() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let nowhere = ran_with(
        &engine,
        "nowhere",
        new_workflow("nowhere", vec![notify("say", Some("nowhere"), "hello")]),
        BTreeMap::new(),
    )
    .await;
    assert_eq!(nowhere.outcome, Some(RunOutcome::Failed), "{nowhere:?}");
    assert!(!failed_step(&nowhere, "say").is_empty());

    // The author is read from an input that names a team: the door holds
    // it — only an agent speaks for a workflow.
    let mut by_team = notify("say", None, "hello");
    if let StepKind::Notify { author, .. } = &mut by_team.kind {
        *author = Some(input_ref("who"));
    }
    let (goal, _) = goal_on(
        &engine,
        "a team speaks",
        with_inputs(
            new_workflow("team", vec![by_team]),
            vec![assignee_input("who")],
        ),
    );
    let refused = engine
        .start_run(
            goal.id,
            BTreeMap::from([("who".to_string(), json!("team:ops"))]),
        )
        .err()
        .unwrap();
    assert!(refused.to_string().contains("agent"), "{refused}");

    // A mention read from an input the run left unbound mentions nobody.
    let mut by_nobody = notify("say", None, "hello");
    if let StepKind::Notify { mentions, .. } = &mut by_nobody.kind {
        *mentions = vec![input_ref("who")];
    }
    let nobody = ran_with(
        &engine,
        "nobody is mentioned",
        with_inputs(
            new_workflow("nobody", vec![by_nobody.clone()]),
            vec![optional(assignee_input("who"))],
        ),
        BTreeMap::new(),
    )
    .await;
    assert_eq!(nobody.outcome, Some(RunOutcome::Done), "{nobody:?}");

    // A value that is not an assignee never binds: the door refuses it.
    let (goal, _) = goal_on(
        &engine,
        "a number is mentioned",
        with_inputs(
            new_workflow("number", vec![by_nobody]),
            vec![assignee_input("who")],
        ),
    );
    let refused = engine
        .start_run(goal.id, BTreeMap::from([("who".to_string(), json!(7))]))
        .err()
        .unwrap();
    assert!(refused.to_string().contains("assignee"), "{refused}");

    let emit = ran_with(
        &engine,
        "no name",
        with_inputs(
            new_workflow(
                "emit",
                vec![step(
                    "raise",
                    StepKind::Emit {
                        signal: "{inputs.name}".into(),
                        payload: BTreeMap::new(),
                    },
                )],
            ),
            vec![text_input("name")],
        ),
        BTreeMap::from([("name".to_string(), json!("not a signal name!"))]),
    )
    .await;
    assert_eq!(emit.outcome, Some(RunOutcome::Failed), "{emit:?}");
    assert!(!failed_step(&emit, "raise").is_empty());

    // The child was put away while the parent held, before its step came.
    let child = engine
        .create_workflow(new_workflow("child", vec![notify("say", None, "child")]))
        .unwrap();
    let (goal, run) = run_on(
        &engine,
        "spawns a put-away workflow",
        new_workflow(
            "spawn",
            chain(vec![
                step(
                    "hold",
                    StepKind::Wait {
                        until: WaitFor::Release,
                    },
                ),
                step(
                    "child",
                    StepKind::Spawn {
                        statement_template: "do the child's part".into(),
                        workflow: Some(child.id),
                        assignees: vec![],
                        inputs: BTreeMap::new(),
                        wait: true,
                    },
                ),
            ]),
        ),
    );
    step_in_state(&engine, goal.id, "hold", "waiting").await;
    engine.archive_workflow(child.id, true).unwrap();
    engine.release_step(run.id, &sid("hold"), None).unwrap();
    let spawn = finished_run(&engine, goal.id).await;
    assert_eq!(spawn.outcome, Some(RunOutcome::Failed), "{spawn:?}");
    assert!(
        failed_step(&spawn, "child").contains("archived"),
        "{}",
        failed_step(&spawn, "child")
    );
    engine.shutdown().await;
}

/// Release `hold` with a payload that lacks `foo`, and wait for the end.
async fn released(
    engine: &Engine,
    goal: bisa_core::GoalId,
    run: bisa_core::RunId,
) -> bisa_core::WorkflowRun {
    step_in_state(engine, goal, "hold", "waiting").await;
    engine
        .release_step(run, &sid("hold"), Some(json!({ "bar": 1 })))
        .unwrap();
    finished_run(engine, goal).await
}

/// A human step's or an approval's prompt that reads a field the world
/// never sent — a released wait's payload lacks it — cannot be asked: the
/// step fails with the reason instead of asking a question nobody can read.
#[tokio::test(flavor = "multi_thread")]
async fn a_prompt_that_cannot_render_fails_the_step_instead_of_asking() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let hold = || {
        step(
            "hold",
            StepKind::Wait {
                until: WaitFor::Release,
            },
        )
    };
    let (goal, run) = run_on(
        &engine,
        "unreadable question",
        new_workflow(
            "human",
            chain(vec![
                hold(),
                step(
                    "ask",
                    StepKind::Human {
                        prompt: "Is {steps.hold.output.foo} right?".into(),
                        options: vec![],
                        multi: false,
                        assignee: None,
                    },
                ),
            ]),
        ),
    );
    let human = released(&engine, goal.id, run.id).await;
    assert_eq!(human.outcome, Some(RunOutcome::Failed), "{human:?}");
    assert!(
        failed_step(&human, "ask").contains("foo"),
        "{}",
        failed_step(&human, "ask")
    );
    let (goal, run) = run_on(
        &engine,
        "unreadable approval",
        new_workflow(
            "approval",
            chain(vec![
                hold(),
                step(
                    "ok",
                    StepKind::Approval {
                        prompt: "Ship {steps.hold.output.foo}?".into(),
                    },
                ),
            ]),
        ),
    );
    let approval = released(&engine, goal.id, run.id).await;
    assert_eq!(approval.outcome, Some(RunOutcome::Failed), "{approval:?}");
    assert!(
        failed_step(&approval, "ok").contains("foo"),
        "{}",
        failed_step(&approval, "ok")
    );
    engine.shutdown().await;
}

/// A wait whose delay is read from an input the run left unbound, or
/// from a value that is no number, never starts: the door refuses both. A
/// schedule with no occurrence ahead fails its step rather than holding
/// the run forever.
#[tokio::test(flavor = "multi_thread")]
async fn a_wait_that_cannot_be_read_is_refused_at_the_door_or_fails_its_step() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let wait = |id: &str, until: WaitFor| step(id, StepKind::Wait { until });
    let delay = || {
        wait(
            "hold",
            WaitFor::Delay {
                secs: ValueRef::Input {
                    input: InputName::new("n").unwrap(),
                },
            },
        )
    };
    let (goal, _) = goal_on(
        &engine,
        "unbound delay",
        with_inputs(
            new_workflow("unbound", vec![delay()]),
            vec![optional(number_input("n"))],
        ),
    );
    let refused = engine.start_run(goal.id, BTreeMap::new()).err().unwrap();
    assert!(refused.to_string().contains("nothing"), "{refused}");

    let (goal, _) = goal_on(
        &engine,
        "a word for a delay",
        with_inputs(new_workflow("word", vec![delay()]), vec![number_input("n")]),
    );
    let refused = engine
        .start_run(goal.id, BTreeMap::from([("n".to_string(), json!("soon"))]))
        .err()
        .unwrap();
    assert!(refused.to_string().contains("number"), "{refused}");

    let never = ran_with(
        &engine,
        "never due",
        new_workflow(
            "never",
            vec![wait(
                "hold",
                WaitFor::Schedule {
                    cron: ValueRef::Fixed("0 0 31 2 *".into()),
                    tz: None,
                },
            )],
        ),
        BTreeMap::new(),
    )
    .await;
    assert_eq!(never.outcome, Some(RunOutcome::Failed), "{never:?}");
    assert!(
        failed_step(&never, "hold").contains("no future occurrence"),
        "{}",
        failed_step(&never, "hold")
    );
    engine.shutdown().await;
}

/// The doors refuse before anything is written: a run of an archived
/// workflow, a design for a closed goal, an amendment of a run that is
/// over, a decision on a home with no question, a human step answered with
/// nothing.
#[tokio::test(flavor = "multi_thread")]
async fn the_doors_refuse_what_cannot_be_before_anything_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let archived = engine
        .create_workflow(new_workflow("old", vec![notify("say", None, "old")]))
        .unwrap();
    engine.archive_workflow(archived.id, true).unwrap();
    let refused = engine.submit_goal(bisa_engine::SubmitRequest {
        workflow: Some(archived.id),
        start: true,
        ..bisa_engine::SubmitRequest::captured("runs a put-away workflow")
    });
    assert!(
        matches!(&refused, Err(EngineError::Invalid(_))),
        "{refused:?}"
    );
    assert!(refused.unwrap_err().to_string().contains("archived"));

    let (goal, _) = goal_on(
        &engine,
        "to be closed",
        new_workflow("done", vec![notify("say", None, "hello")]),
    );
    let closed = engine
        .close_goal(goal.id, ClosureReason::Abandoned { rationale: None })
        .unwrap();
    assert!(closed.is_closed());
    let designed = engine.design_workflow(
        goal.id,
        new_workflow("another", vec![notify("say", None, "again")]),
        None,
    );
    assert!(
        matches!(&designed, Err(EngineError::Invalid(_)))
            && designed.unwrap_err().to_string().contains("closed"),
    );

    let (goal, _) = goal_on(
        &engine,
        "ran already",
        new_workflow("quick", vec![notify("say", None, "hello")]),
    );
    engine.start_run(goal.id, BTreeMap::new()).unwrap();
    let over = finished_run(&engine, goal.id).await;
    assert_eq!(over.status(), RunStatus::Done);
    let amended = engine.amend_run(
        goal.id,
        new_workflow("quick", vec![notify("say", None, "late")]),
    );
    assert!(
        matches!(&amended, Err(EngineError::Invalid(_))),
        "{amended:?}"
    );
    let nothing = engine.decide("gate:nowhere", true, None, None, None);
    assert!(
        matches!(&nothing, Err(EngineError::UnknownGate(_))),
        "{nothing:?}"
    );

    let mut rx = engine.events();
    let (goal, _) = run_on(
        &engine,
        "asks",
        new_workflow(
            "asks",
            vec![step(
                "ask",
                StepKind::Human {
                    prompt: "Which?".into(),
                    options: vec![bisa_core::AskOption::new("a", "A")],
                    multi: false,
                    assignee: None,
                },
            )],
        ),
    );
    let opened = wait_for(&mut rx, "the question", |e| {
        matches!(&e.payload, bisa_engine::EnginePayload::GateOpened { .. })
    })
    .await;
    let bisa_engine::EnginePayload::GateOpened { gate_id, .. } = opened.payload else {
        unreachable!()
    };
    let unanswered = engine.decide(&gate_id, true, None, None, None);
    assert!(
        matches!(&unanswered, Err(EngineError::Answer(AnswerError::Empty))),
        "{unanswered:?}"
    );
    assert_eq!(current_run(&engine, goal.id).status(), RunStatus::Waiting);
    engine.shutdown().await;
}

/// The scheduler's preflight, refusal by refusal, over a hand-built item.
#[tokio::test(flavor = "multi_thread")]
async fn the_preflight_refuses_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    let config = design_off_config();
    let (goal, run) = run_on(
        &engine,
        "preflight",
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
    let mut spec = bisa_core::WorkItemSpec {
        id: bisa_core::WorkItemId::from_ulid(ulid::Ulid::from_parts(9, 9)),
        home: bisa_core::Home::Goal { goal: goal.id },
        run: Some(run.id),
        step: Some(sid("hold")),
        instructions: "work".into(),
        state: bisa_core::WorkItemState::Open,
        project: None,
        harness_candidates: vec!["mock".into()],
        model: None,
        effort: None,
        output_schema: None,
        budget: Default::default(),
        assignees: vec![],
        tier_ceiling: bisa_core::ToolTier::Write,
        agent: None,
        spawn_allowlist: vec![],
        depth_budget: 0,
        result_attempts: 0,
        interruptions: 0,
    };
    // The step is waiting, not running an item.
    assert!(matches!(
        preflight(ws, &config, &spec, Launch::Fresh),
        Err(ScheduleRejection::StepNotRunning { .. })
    ));
    spec.step = Some(sid("nowhere"));
    assert!(matches!(
        preflight(ws, &config, &spec, Launch::Fresh),
        Err(ScheduleRejection::StepNotRunning { state, .. }) if state == "not in the run"
    ));
    spec.step = None;
    assert!(matches!(
        preflight(ws, &config, &spec, Launch::Fresh),
        Err(ScheduleRejection::NoStep)
    ));
    spec.state = bisa_core::WorkItemState::Cancelled;
    assert!(matches!(
        preflight(ws, &config, &spec, Launch::Fresh),
        Err(ScheduleRejection::NotOpen(_))
    ));
    assert!(matches!(
        preflight(ws, &config, &spec, Launch::Resume),
        Err(ScheduleRejection::NotOpen(_))
    ));
    spec.state = bisa_core::WorkItemState::Open;
    let disabled = EngineConfig {
        disabled_harnesses: vec!["mock".into()],
        ..design_off_config()
    };
    assert!(matches!(
        preflight(ws, &disabled, &spec, Launch::Fresh),
        Err(ScheduleRejection::HarnessDisabled(_))
    ));
    let gone = bisa_core::RunId::from_ulid(ulid::Ulid::from_parts(8, 8));
    spec.home = bisa_core::Home::Run { run: gone };
    assert!(matches!(
        preflight(ws, &config, &spec, Launch::Fresh),
        Err(ScheduleRejection::RunMissing(_))
    ));
    engine.shutdown().await;
}

/// Every kind of wait is armed with its definition — what a surface listing
/// armed waits shows — and a restart arms each again from the run snapshot.
#[tokio::test(flavor = "multi_thread")]
async fn every_kind_of_wait_shows_its_definition_before_and_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let watched = engine
        .create_workflow(new_workflow("watched", vec![notify("say", None, "hi")]))
        .unwrap();
    let wait = |id: &str, until: WaitFor| step(id, StepKind::Wait { until });
    let waits = [
        wait(
            "nap",
            WaitFor::Delay {
                secs: ValueRef::Fixed(600),
            },
        ),
        wait(
            "moment",
            WaitFor::Time {
                at: "2099-01-01T00:00:00Z".into(),
            },
        ),
        wait(
            "clock",
            WaitFor::Schedule {
                cron: ValueRef::Fixed("0 9 * * *".into()),
                tz: None,
            },
        ),
        wait(
            "word",
            WaitFor::Message {
                filter: bisa_core::MessageFilter {
                    r#in: None,
                    from: bisa_core::MessageFrom::You,
                    mentions: None,
                    contains: Some("go".into()),
                },
            },
        ),
        wait(
            "ended",
            WaitFor::Run {
                filter: bisa_core::RunFilter {
                    workflow: Some(watched.id),
                    outcome: None,
                },
            },
        ),
        wait(
            "topic",
            WaitFor::Platform {
                filter: bisa_core::PlatformFilter {
                    topic: "run.finished".into(),
                    fields: BTreeMap::new(),
                },
            },
        ),
        wait("hold", WaitFor::Release),
    ];
    let mut fork = step("fork", StepKind::Parallel);
    fork.then = waits
        .iter()
        .map(|w| bisa_core::Flow::to(w.id.clone()))
        .collect();
    let mut steps = vec![fork];
    steps.extend(waits.iter().cloned());
    let (_, run) = run_on(&engine, "every wait", new_workflow("waits", steps));
    let armed = |engine: &Engine| -> BTreeMap<String, WaitFor> {
        engine
            .armed_waits()
            .into_iter()
            .filter(|(r, _, _)| *r == run.id)
            .map(|(_, s, w)| (s.to_string(), w))
            .collect()
    };
    let check = |armed: &BTreeMap<String, WaitFor>| {
        assert_eq!(armed.len(), waits.len(), "{armed:?}");
        assert!(matches!(
            armed["nap"],
            WaitFor::Delay {
                secs: ValueRef::Fixed(600)
            }
        ));
        assert!(matches!(&armed["moment"], WaitFor::Time { at } if at.parse::<u64>().is_ok()));
        assert!(
            matches!(&armed["clock"], WaitFor::Schedule { cron: ValueRef::Fixed(c), .. } if c == "0 9 * * *")
        );
        assert!(
            matches!(&armed["word"], WaitFor::Message { filter } if filter.contains.as_deref() == Some("go"))
        );
        assert!(
            matches!(&armed["ended"], WaitFor::Run { filter } if filter.workflow == Some(watched.id))
        );
        assert!(
            matches!(&armed["topic"], WaitFor::Platform { filter } if filter.topic == "run.finished")
        );
        assert!(matches!(armed["hold"], WaitFor::Release));
    };
    until("every wait to be armed", || {
        let now = armed(&engine);
        (now.len() == waits.len()).then_some(now)
    })
    .await;
    check(&armed(&engine));
    engine.shutdown().await;

    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    check(&armed(&engine));
    engine.shutdown().await;
}

/// A `spawn` holding for a live child is paired with it again at a restart
/// — a goal's child and a run of the workspace's — and one whose child was
/// closed while the node was down completes at boot with no outcome.
#[tokio::test(flavor = "multi_thread")]
async fn a_spawn_holding_for_a_child_is_paired_again_at_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let hold = || {
        step(
            "hold",
            StepKind::Wait {
                until: WaitFor::Release,
            },
        )
    };
    let child_wf = engine
        .create_workflow(new_workflow("child", vec![hold()]))
        .unwrap();
    let spawning = |name: &str| {
        new_workflow(
            name,
            vec![step(
                "child",
                StepKind::Spawn {
                    statement_template: "the child's part".into(),
                    workflow: Some(child_wf.id),
                    assignees: vec![],
                    inputs: BTreeMap::new(),
                    wait: true,
                },
            )],
        )
    };
    let (parent_a, _) = run_on(&engine, "parent a", spawning("spawn-a"));
    let (parent_c, _) = run_on(&engine, "parent c", spawning("spawn-c"));
    let (_, ws_run) = workspace_run(&engine, spawning("spawn-b"));
    let child_of = |engine: &Engine, origin: &dyn Fn(&bisa_core::GoalOrigin) -> bool| {
        engine
            .workspace()
            .list_goals(None)
            .unwrap()
            .into_iter()
            .find(|g| origin(&g.origin))
    };
    let spawned_by = |parent: bisa_core::GoalId| move |o: &bisa_core::GoalOrigin| matches!(o, bisa_core::GoalOrigin::Spawned { parent: p } if *p == parent);
    let run_child = |run: bisa_core::RunId| move |o: &bisa_core::GoalOrigin| matches!(o, bisa_core::GoalOrigin::Run { run: r, .. } if *r == run);
    // Every child holds on its release, every parent waits on its spawn.
    let child_a = until("child a to hold", || {
        let child = child_of(&engine, &spawned_by(parent_a.id))?;
        let run = engine
            .workspace()
            .get_current_run(child.id)
            .ok()
            .flatten()?;
        (run.steps.get(&sid("hold"))?.state == StepState::Waiting).then_some((child, run))
    })
    .await;
    let child_c = until("child c to hold", || {
        let child = child_of(&engine, &spawned_by(parent_c.id))?;
        let run = engine
            .workspace()
            .get_current_run(child.id)
            .ok()
            .flatten()?;
        (run.steps.get(&sid("hold"))?.state == StepState::Waiting).then_some((child, run))
    })
    .await;
    let child_b = until("child b to hold", || {
        let child = child_of(&engine, &run_child(ws_run.id))?;
        let run = engine
            .workspace()
            .get_current_run(child.id)
            .ok()
            .flatten()?;
        (run.steps.get(&sid("hold"))?.state == StepState::Waiting).then_some((child, run))
    })
    .await;
    step_in_state(&engine, parent_a.id, "child", "waiting").await;
    engine.shutdown().await;

    // Child c is closed while the node is down.
    workspace(&dir)
        .set_goal_closed(child_c.0.id, ClosureReason::Abandoned { rationale: None })
        .unwrap();

    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let parent_c_run = finished_run(&engine, parent_c.id).await;
    assert_eq!(
        parent_c_run.outcome,
        Some(RunOutcome::Done),
        "{parent_c_run:?}"
    );
    assert_eq!(
        parent_c_run.steps[&sid("child")].output,
        Some(json!({ "child": child_c.0.id.to_string(), "outcome": Value::Null }))
    );
    engine
        .release_step(child_a.1.id, &sid("hold"), None)
        .unwrap();
    let parent_a_run = finished_run(&engine, parent_a.id).await;
    assert_eq!(
        parent_a_run.outcome,
        Some(RunOutcome::Done),
        "{parent_a_run:?}"
    );
    assert_eq!(
        parent_a_run.steps[&sid("child")].output,
        Some(json!({ "child": child_a.0.id.to_string(), "outcome": "done" }))
    );
    engine
        .release_step(child_b.1.id, &sid("hold"), None)
        .unwrap();
    let ws_done = run_finished(&engine, ws_run.id).await;
    assert_eq!(ws_done.outcome, Some(RunOutcome::Done), "{ws_done:?}");
    engine.shutdown().await;
}

/// A boundary event whose act is a notify into a conversation that does not
/// exist cannot act: the run's journal says so and the step it stands
/// beside goes on.
#[tokio::test(flavor = "multi_thread")]
async fn a_boundary_notify_that_cannot_act_is_said_on_the_journal() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let mut hold = step(
        "hold",
        StepKind::Wait {
            until: WaitFor::Release,
        },
    );
    hold.boundaries = vec![bisa_core::Boundary {
        name: bisa_core::Branch::new("ping").unwrap(),
        on: bisa_core::BoundaryOn::Signal {
            filter: bisa_core::SignalFilter {
                name: "ping".into(),
                fields: BTreeMap::new(),
            },
        },
        act: bisa_core::BoundaryAct::Notify {
            scope: Some("nowhere".into()),
            template: "still here".into(),
            mentions: vec![],
            author: None,
        },
    }];
    let (goal, run) = run_on(
        &engine,
        "reminded nowhere",
        new_workflow("reminded", vec![hold]),
    );
    step_in_state(&engine, goal.id, "hold", "waiting").await;
    engine
        .emit_signal("ping", json!({}), bisa_core::SignalScope::Workspace)
        .unwrap();
    until("the journal to say the act failed", || {
        notes(&engine, goal.id)
            .into_iter()
            .find(|n| n.contains("boundary event `ping` could not act"))
    })
    .await;
    engine.release_step(run.id, &sid("hold"), None).unwrap();
    assert_eq!(
        finished_run(&engine, goal.id).await.outcome,
        Some(RunOutcome::Done)
    );
    engine.shutdown().await;
}

/// Pointing a listening goal at a workflow with no start on an event — or
/// at none — turns its listening off: it has nothing left to hear.
#[tokio::test(flavor = "multi_thread")]
async fn choosing_a_workflow_that_hears_nothing_turns_a_goal_s_listening_off() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let hearing = |name: &str| {
        let mut on_hook = step(
            "ticket",
            StepKind::Start {
                on: bisa_core::StartOn::Hook { public: false },
                inputs: BTreeMap::new(),
                guard: bisa_core::Guard::default(),
            },
        );
        on_hook.then = vec![bisa_core::Flow::to(sid("say"))];
        new_workflow(name, vec![on_hook, notify("say", None, "heard")])
    };
    let plain = engine
        .create_workflow(new_workflow("plain", vec![notify("say", None, "plain")]))
        .unwrap();
    let (first, _) = goal_on(&engine, "listens, then does not", hearing("hears-1"));
    let (second, _) = goal_on(&engine, "listens, then has no workflow", hearing("hears-2"));
    for goal in [first.id, second.id] {
        engine
            .set_listening(
                bisa_core::ListenerHost::Goal { goal },
                BTreeMap::new(),
                None,
            )
            .unwrap();
        assert!(engine
            .workspace()
            .get_goal(goal)
            .unwrap()
            .listening
            .is_some());
    }
    let first = engine.set_workflow(first.id, Some(plain.id)).unwrap();
    assert!(first.listening.is_none(), "{first:?}");
    assert_eq!(first.workflow, Some(plain.id));
    let second = engine.set_workflow(second.id, None).unwrap();
    assert!(second.listening.is_none(), "{second:?}");
    assert_eq!(second.workflow, None);
    engine.shutdown().await;
}

/// A `judge` whose state reads a field the world never sent cannot be put
/// to the Decision-Making Agent: the effect fails through the funnel, and
/// the step it was for fails with the reason.
#[tokio::test(flavor = "multi_thread")]
async fn a_judge_whose_state_cannot_render_fails_through_the_funnel() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let option = |branch: &str, meaning: &str| bisa_core::JudgeOption {
        branch: bisa_core::Branch::new(branch).unwrap(),
        meaning: meaning.into(),
    };
    let mut judge = step(
        "judge",
        StepKind::Judge {
            state: "{steps.hold.output.foo}".into(),
            instructions: "Is it good?".into(),
            options: vec![option("good", "all is well"), option("bad", "look")],
            otherwise: bisa_core::Branch::new("unsure").unwrap(),
            min_confidence: None,
        },
    );
    judge.then = vec![
        bisa_core::Flow::branch(sid("end"), bisa_core::Branch::new("good").unwrap()),
        bisa_core::Flow::branch(sid("end"), bisa_core::Branch::new("bad").unwrap()),
        bisa_core::Flow::branch(sid("end"), bisa_core::Branch::new("unsure").unwrap()),
    ];
    let mut hold = step(
        "hold",
        StepKind::Wait {
            until: WaitFor::Release,
        },
    );
    hold.then = vec![bisa_core::Flow::to(sid("judge"))];
    let (goal, run) = run_on(
        &engine,
        "unreadable judgement",
        new_workflow("judged", vec![hold, judge, notify("end", None, "over")]),
    );
    let run = released(&engine, goal.id, run.id).await;
    assert_eq!(run.outcome, Some(RunOutcome::Failed), "{run:?}");
    assert!(
        failed_step(&run, "judge").contains("foo"),
        "{}",
        failed_step(&run, "judge")
    );
    engine.shutdown().await;
}

/// A step whose every harness is disabled by configuration never launches:
/// the item is refused at preflight and the step fails saying so.
#[tokio::test(flavor = "multi_thread")]
async fn a_step_whose_every_harness_is_disabled_is_refused_before_a_launch() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::start(
        workspace(&dir),
        catalog_with(vec![MockAdapter::default()]),
        EngineConfig {
            disabled_harnesses: vec!["mock".into()],
            ..design_off_config()
        },
    )
    .unwrap();
    let run = ran_with(
        &engine,
        "disabled",
        new_workflow("disabled", vec![agent_step("work", "mock")]),
        BTreeMap::new(),
    )
    .await;
    assert_eq!(run.outcome, Some(RunOutcome::Failed), "{run:?}");
    assert!(
        failed_step(&run, "work").contains("disabled"),
        "{}",
        failed_step(&run, "work")
    );
    engine.shutdown().await;
}

/// A failing command check keeps what the command said on stderr as
/// evidence, beside its status.
#[tokio::test(flavor = "multi_thread")]
async fn a_failing_check_keeps_what_the_command_said() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let run = ran_with(
        &engine,
        "loud failure",
        new_workflow(
            "loud",
            vec![step(
                "verify",
                StepKind::Check {
                    check: CheckKind::Command {
                        command: "ls /nonexistent-folder-of-this-check".into(),
                    },
                },
            )],
        ),
        BTreeMap::new(),
    )
    .await;
    assert_eq!(run.outcome, Some(RunOutcome::Failed), "{run:?}");
    let why = failed_step(&run, "verify");
    assert!(
        why.contains("No such file or directory"),
        "what the command said on stderr is the evidence: {why}"
    );
    assert!(why.contains("exit status:"), "{why}");
    engine.shutdown().await;
}

/// A harness that never ends its turn: work that is live until the runtime
/// goes.
fn endless() -> MockAdapter {
    MockAdapter {
        script: Some(vec![]),
        ..Default::default()
    }
}

/// An agent step running at the crash, and the item the restart finds.
async fn interrupted(
    dir: &tempfile::TempDir,
    statement: &str,
) -> (bisa_core::GoalId, bisa_core::WorkItemSpec) {
    let engine = engine_with(dir, vec![endless()]);
    let (goal, _) = run_on(
        &engine,
        statement,
        new_workflow("A", vec![agent_step("work", "mock")]),
    );
    step_in_state(&engine, goal.id, "work", "running").await;
    let item = until("the item to be launched", || {
        items_of(&engine, goal.id).into_iter().next()
    })
    .await;
    engine.shutdown().await;
    (goal.id, item)
}

/// What a restart does with an interrupted step whose item is not as the
/// crash should have left it: gone, it fails; accepted with its result in
/// the journal, the step takes the result; accepted with none, it fails;
/// cancelled, it fails naming the state.
#[tokio::test(flavor = "multi_thread")]
async fn a_restart_reads_the_interrupted_items_state_before_resuming_it() {
    use bisa_core::event::JournalPayload;
    use bisa_core::WorkItemTransition as T;

    // Gone.
    let dir = tempfile::tempdir().unwrap();
    let (goal, item) = interrupted(&dir, "item gone").await;
    let store = workspace(&dir);
    let home = Home::Goal { goal };
    store
        .transition_work_item(&home, item.id, &T::Cancel)
        .unwrap();
    store.delete_work_item(&home, item.id).unwrap();
    let engine = engine_with(&dir, vec![endless()]);
    let run = finished_run(&engine, goal).await;
    assert!(
        failed_step(&run, "work").contains("the work item to resume is gone"),
        "{}",
        failed_step(&run, "work")
    );
    engine.shutdown().await;

    // Accepted, with its result in the journal.
    let dir = tempfile::tempdir().unwrap();
    let (goal, item) = interrupted(&dir, "result accepted").await;
    let store = workspace(&dir);
    let home = Home::Goal { goal };
    store
        .transition_work_item(&home, item.id, &T::Submit)
        .unwrap();
    store
        .transition_work_item(&home, item.id, &T::Accept)
        .unwrap();
    store
        .append_journal(
            &home,
            JournalPayload::Result {
                work_item: item.id,
                output: json!({ "ok": true }),
                artifacts: vec![],
            },
            store.owner_keys(),
            None,
        )
        .unwrap();
    let engine = engine_with(&dir, vec![endless()]);
    let run = finished_run(&engine, goal).await;
    assert_eq!(run.outcome, Some(RunOutcome::Done), "{run:?}");
    assert_eq!(run.steps[&sid("work")].output, Some(json!({ "ok": true })));
    engine.shutdown().await;

    // Accepted, with no result anywhere.
    let dir = tempfile::tempdir().unwrap();
    let (goal, item) = interrupted(&dir, "accepted without a result").await;
    let store = workspace(&dir);
    let home = Home::Goal { goal };
    store
        .transition_work_item(&home, item.id, &T::Submit)
        .unwrap();
    store
        .transition_work_item(&home, item.id, &T::Accept)
        .unwrap();
    let engine = engine_with(&dir, vec![endless()]);
    let run = finished_run(&engine, goal).await;
    assert!(
        failed_step(&run, "work").contains("accepted but its result is not in the journal"),
        "{}",
        failed_step(&run, "work")
    );
    engine.shutdown().await;

    // Cancelled.
    let dir = tempfile::tempdir().unwrap();
    let (goal, item) = interrupted(&dir, "cancelled meanwhile").await;
    let store = workspace(&dir);
    store
        .transition_work_item(&Home::Goal { goal }, item.id, &T::Cancel)
        .unwrap();
    let engine = engine_with(&dir, vec![endless()]);
    let run = finished_run(&engine, goal).await;
    assert!(
        failed_step(&run, "work").contains("the work item to resume is cancelled"),
        "{}",
        failed_step(&run, "work")
    );
    engine.shutdown().await;
}

/// A spawn that does not wait leaves nothing to pair at a restart: a child
/// whose parent run finished, and one whose parent went on past the spawn,
/// are both passed over.
#[tokio::test(flavor = "multi_thread")]
async fn a_child_nobody_waits_for_is_passed_over_at_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let child_wf = engine
        .create_workflow(new_workflow(
            "child",
            vec![step(
                "hold",
                StepKind::Wait {
                    until: WaitFor::Release,
                },
            )],
        ))
        .unwrap();
    let spawn = || {
        step(
            "child",
            StepKind::Spawn {
                statement_template: "the child's part".into(),
                workflow: Some(child_wf.id),
                assignees: vec![],
                inputs: BTreeMap::new(),
                wait: false,
            },
        )
    };
    let (finished, _) = run_on(
        &engine,
        "parent that ends",
        new_workflow("ends", vec![spawn()]),
    );
    let (holding, _) = run_on(
        &engine,
        "parent that goes on",
        new_workflow(
            "goes-on",
            chain(vec![
                spawn(),
                step(
                    "hold",
                    StepKind::Wait {
                        until: WaitFor::Release,
                    },
                ),
            ]),
        ),
    );
    assert_eq!(
        finished_run(&engine, finished.id).await.outcome,
        Some(RunOutcome::Done)
    );
    step_in_state(&engine, holding.id, "hold", "waiting").await;
    let children = until("both children to exist", || {
        let children: Vec<_> = engine
            .workspace()
            .list_goals(None)
            .unwrap()
            .into_iter()
            .filter(|g| matches!(g.origin, bisa_core::GoalOrigin::Spawned { .. }))
            .collect();
        (children.len() == 2).then_some(children)
    })
    .await;
    engine.shutdown().await;

    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    for child in &children {
        let run = engine
            .workspace()
            .get_current_run(child.id)
            .unwrap()
            .unwrap();
        assert_eq!(
            run.status(),
            RunStatus::Waiting,
            "the child holds on its own"
        );
    }
    let parent = engine
        .workspace()
        .get_current_run(holding.id)
        .unwrap()
        .unwrap();
    assert_eq!(parent.status(), RunStatus::Waiting);
    assert_eq!(parent.steps[&sid("child")].state, StepState::done());
    engine.shutdown().await;
}

/// The preflight's other refusals: an item of a run that is over, a home
/// whose budget is spent, and a spawn the parent allows that is then judged
/// as any item is.
#[tokio::test(flavor = "multi_thread")]
async fn the_preflight_refuses_a_finished_run_and_a_spent_budget() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    let config = design_off_config();
    let (_, run) = workspace_run(
        &engine,
        new_workflow("quick", vec![notify("say", None, "hi")]),
    );
    let over = run_finished(&engine, run.id).await;
    assert_eq!(over.outcome, Some(RunOutcome::Done));
    let spec = |home: Home, run: Option<bisa_core::RunId>| bisa_core::WorkItemSpec {
        id: bisa_core::WorkItemId::from_ulid(ulid::Ulid::from_parts(7, 7)),
        home,
        run,
        step: Some(sid("say")),
        instructions: "work".into(),
        state: bisa_core::WorkItemState::Open,
        project: None,
        harness_candidates: vec!["mock".into()],
        model: None,
        effort: None,
        output_schema: None,
        budget: Default::default(),
        assignees: vec![],
        tier_ceiling: bisa_core::ToolTier::Write,
        agent: None,
        spawn_allowlist: vec![],
        depth_budget: 0,
        result_attempts: 0,
        interruptions: 0,
    };
    assert!(matches!(
        preflight(
            ws,
            &config,
            &spec(Home::Run { run: run.id }, Some(run.id)),
            Launch::Fresh
        ),
        Err(ScheduleRejection::RunFinished(_))
    ));

    engine.shutdown().await;

    // The step's own item, running on a harness that never ends its turn,
    // resumed once the home's budget is spent.
    let engine = engine_with(&dir, vec![endless()]);
    let ws = engine.workspace();
    let wf = engine
        .create_workflow(new_workflow("spends", vec![agent_step("work", "mock")]))
        .unwrap();
    let goal = engine
        .submit_goal(bisa_engine::SubmitRequest {
            workflow: Some(wf.id),
            start: true,
            budget: Some(bisa_core::Budget {
                max_usd_cents: Some(1),
                ..Default::default()
            }),
            ..bisa_engine::SubmitRequest::captured("a cent")
        })
        .unwrap();
    step_in_state(&engine, goal.id, "work", "running").await;
    let item = until("the item to be launched", || {
        items_of(&engine, goal.id).into_iter().next()
    })
    .await;
    let home = Home::Goal { goal: goal.id };
    ws.add_spend(&home, 0, 2, 0).unwrap();
    let judged = preflight(ws, &config, &item, Launch::Resume);
    assert!(
        matches!(judged, Err(ScheduleRejection::BudgetExhausted(_))),
        "{judged:?}"
    );

    let mut parent = item.clone();
    parent.depth_budget = 1;
    parent.spawn_allowlist = vec!["*".into()];
    let mut child = item.clone();
    child.state = bisa_core::WorkItemState::Open;
    let allowed = bisa_engine::scheduler::preflight_spawn(ws, &config, &parent, "helper", &child);
    assert!(
        matches!(allowed, Err(ScheduleRejection::BudgetExhausted(_))),
        "the parent allows it; the child is then judged as any item: {allowed:?}"
    );
    engine.shutdown().await;
}

/// Stopping one run aborts its own check task and no other's: a second
/// run's command goes on to its end.
#[tokio::test(flavor = "multi_thread")]
async fn stopping_a_run_aborts_its_own_check_and_keeps_another_runs() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let slow = |name: &str| {
        new_workflow(
            name,
            vec![step(
                "verify",
                StepKind::Check {
                    check: CheckKind::Command {
                        command: "sleep 2".into(),
                    },
                },
            )],
        )
    };
    let (stopped, _) = run_on(&engine, "stopped early", slow("slow-a"));
    let (kept, _) = run_on(&engine, "left to finish", slow("slow-b"));
    step_in_state(&engine, stopped.id, "verify", "running").await;
    step_in_state(&engine, kept.id, "verify", "running").await;
    engine.stop_goal(stopped.id, None).await.unwrap();
    let ended = finished_run(&engine, stopped.id).await;
    assert_eq!(ended.status(), RunStatus::Cancelled, "{ended:?}");
    let done = finished_run(&engine, kept.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done), "{done:?}");
    engine.shutdown().await;
}

/// A command check whose line reads a field the world never sent cannot
/// run: the step fails with the reason, and nothing is spawned.
#[tokio::test(flavor = "multi_thread")]
async fn a_check_whose_command_cannot_render_fails_without_running() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let mut hold = step(
        "hold",
        StepKind::Wait {
            until: WaitFor::Release,
        },
    );
    hold.then = vec![bisa_core::Flow::to(sid("verify"))];
    let (goal, run) = run_on(
        &engine,
        "unreadable check",
        new_workflow(
            "checked",
            vec![
                hold,
                step(
                    "verify",
                    StepKind::Check {
                        check: CheckKind::Command {
                            command: "test -n {steps.hold.output.foo}".into(),
                        },
                    },
                ),
            ],
        ),
    );
    let run = released(&engine, goal.id, run.id).await;
    assert_eq!(run.outcome, Some(RunOutcome::Failed), "{run:?}");
    assert!(
        failed_step(&run, "verify").contains("foo"),
        "{}",
        failed_step(&run, "verify")
    );
    engine.shutdown().await;
}
