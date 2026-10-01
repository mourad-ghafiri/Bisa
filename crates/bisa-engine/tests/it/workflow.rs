//! A run, end to end: the engine walks a workflow's steps against the mock
//! harness, and every kind of step does what its definition says.
//!
//! Every test here drives the engine through its facade and the intake
//! socket — the same doors the node, the CLI and a harness session use — and
//! reads the outcome back from the store: the run snapshot, the goal, the
//! journal, the work items.

use crate::common;

use bisa_core::event::{JournalPayload, RunFact, StepFact};
use bisa_core::{
    Answer, AskOption, Branch, CancelCause, CheckKind, ClosureReason, Condition, Flow, Gate,
    GoalOrigin, GoalStatus, OnFail, Rule, RunOutcome, RunStatus, SignalFilter, SignalScope,
    StepKind, StepState, ValueRef, WaitFor, WorkItemState,
};
use bisa_engine::{Engine, EnginePayload, SubmitRequest};
use bisa_harness::mock::MockAdapter;
use common::*;
use serde_json::json;
use std::collections::BTreeMap;

#[tokio::test(flavor = "multi_thread")]
async fn a_run_walks_sequential_agent_steps_to_done() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"summary": "ok"}))]);
    let mut rx = engine.events();

    let (goal, run) = run_on(
        &engine,
        "build a tiny tool",
        new_workflow(
            "two steps",
            chain(vec![
                agent_step("design", "mock"),
                agent_step("build", "mock"),
            ]),
        ),
    );
    assert_eq!(run.status(), RunStatus::Running);
    assert_eq!(
        engine
            .workspace()
            .get_goal(goal.id)
            .unwrap()
            .status(Some(&run)),
        GoalStatus::Running
    );

    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert_eq!(
        done.steps[&sid("design")].output,
        Some(json!({"summary": "ok"}))
    );
    assert!(matches!(
        done.steps[&sid("build")].state,
        StepState::Done { .. }
    ));
    // One work item per agent step, each bound to its step and accepted.
    let items = items_of(&engine, goal.id);
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|i| i.run == Some(run.id)));
    assert!(items
        .iter()
        .all(|i| matches!(i.state, WorkItemState::Accepted)));
    // The bus told the story in order.
    wait_for(&mut rx, "the run finishing", |e| {
        matches!(
            &e.payload,
            EnginePayload::RunFinished {
                outcome: RunOutcome::Done,
                ..
            }
        )
    })
    .await;
    assert_eq!(
        engine
            .workspace()
            .get_goal(goal.id)
            .unwrap()
            .status(Some(&done)),
        GoalStatus::Done
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn parallel_steps_join_all_before_the_next() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);

    let mut start = agent_step("start", "mock");
    start.then = vec![Flow::to(sid("left")), Flow::to(sid("right"))];
    let mut left = agent_step("left", "mock");
    left.then = vec![Flow::to(sid("join"))];
    let mut right = agent_step("right", "mock");
    right.then = vec![Flow::to(sid("join"))];
    let join = agent_step("join", "mock");
    let (goal, _) = run_on(
        &engine,
        "fan out",
        new_workflow("diamond", vec![start, left, right, join]),
    );

    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    // The join started only after both arms finished.
    let arm_end = done.steps[&sid("left")]
        .finished_at
        .max(done.steps[&sid("right")].finished_at)
        .unwrap();
    assert!(done.steps[&sid("join")].started_at.unwrap() >= arm_end);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_decide_step_takes_one_branch_and_skips_the_other() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"size": "big"}))]);

    let mut triage = agent_step_promising("triage", "mock", &["size"]);
    triage.then = vec![Flow::to(sid("route"))];
    let mut route = step(
        "route",
        StepKind::Decide {
            rules: vec![Rule {
                when: Condition::OutputEquals {
                    step: sid("triage"),
                    path: "size".into(),
                    value: json!("big"),
                },
                branch: Branch::new("big").unwrap(),
            }],
            otherwise: Branch::new("small").unwrap(),
            pick: Default::default(),
        },
    );
    route.then = vec![
        Flow::branch(sid("heavy"), Branch::new("big").unwrap()),
        Flow::branch(sid("light"), Branch::new("small").unwrap()),
    ];
    let (goal, _) = run_on(
        &engine,
        "route it",
        new_workflow(
            "routed",
            vec![
                triage,
                route,
                agent_step("heavy", "mock"),
                agent_step("light", "mock"),
            ],
        ),
    );

    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert_eq!(
        done.steps[&sid("route")].state,
        StepState::chose(Branch::new("big").unwrap())
    );
    assert!(matches!(
        done.steps[&sid("heavy")].state,
        StepState::Done { .. }
    ));
    assert_eq!(done.steps[&sid("light")].state, StepState::Skipped);
    // Only one work item beyond triage: the untaken arm never ran.
    assert_eq!(items_of(&engine, goal.id).len(), 2);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_check_step_runs_in_the_goals_scratch_dir_when_it_has_no_project_and_fails_the_run_on_fail(
) {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);

    // A passing check on a goal with no project leaves evidence in the goal's scratch folder.
    let (goal, _) = run_on(
        &engine,
        "check me",
        new_workflow(
            "checked",
            chain(vec![step(
                "probe",
                StepKind::Check {
                    check: CheckKind::Command {
                        command: "test -d . && echo here > checked.txt".into(),
                    },
                },
            )]),
        ),
    );
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done), "{done:?}");
    let work = engine.workspace().paths().goal(goal.id).scratch();
    assert!(
        work.join("checked.txt").exists(),
        "the command ran in {}",
        work.display()
    );
    let evidence = &done.steps[&sid("probe")].output.as_ref().unwrap()["evidence"];
    assert!(evidence.to_string().contains("exit status"), "{evidence}");

    // A failing check with the default `on_fail: fail` fails the run.
    let (goal2, _) = run_on(
        &engine,
        "fail me",
        new_workflow(
            "failing",
            vec![step(
                "probe",
                StepKind::Check {
                    check: CheckKind::Command {
                        command: "echo nope >&2; exit 3".into(),
                    },
                },
            )],
        ),
    );
    let failed = finished_run(&engine, goal2.id).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed));
    assert_eq!(failed.steps[&sid("probe")].state, StepState::Failed);
    assert!(failed.steps[&sid("probe")]
        .error
        .as_deref()
        .unwrap()
        .contains("nope"));
    assert_eq!(
        engine
            .workspace()
            .get_goal(goal2.id)
            .unwrap()
            .status(Some(&failed)),
        GoalStatus::Failed
    );
    engine.shutdown().await;
}

/// A `check` command renders every substituted value as one shell word. An
/// input that reads like a second command is printed, not run: the marker
/// file it names is never created.
#[tokio::test(flavor = "multi_thread")]
async fn check_command_values_are_shell_quoted() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let marker = dir.path().join("marker-that-must-not-exist");
    let payload = format!("a; touch {}", marker.display());
    let mut draft = new_workflow(
        "quoted",
        vec![step(
            "probe",
            StepKind::Check {
                check: CheckKind::Command {
                    command: "echo {inputs.x}".into(),
                },
            },
        )],
    );
    draft.inputs = vec![bisa_core::InputDef {
        name: bisa_core::InputName::new("x").unwrap(),
        label: "X".into(),
        kind: bisa_core::InputKind::Text,
        default: None,
        required: true,
    }];
    let (goal, _) = goal_on(&engine, "quote me", draft);
    engine
        .start_run(
            goal.id,
            BTreeMap::from([("x".to_string(), json!(payload.clone()))]),
        )
        .unwrap();
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done), "{done:?}");
    let evidence = done.steps[&sid("probe")].output.as_ref().unwrap()["evidence"].to_string();
    assert!(
        evidence.contains("echo 'a; touch"),
        "the value was quoted: {evidence}"
    );
    assert!(
        !marker.exists(),
        "the value was one word to the shell, not a second command"
    );
    engine.shutdown().await;
}

/// An amendment re-checks placement and inputs before anything is written.
#[tokio::test(flavor = "multi_thread")]
async fn amend_rechecks_placement_and_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let (goal, run) = run_on(
        &engine,
        "amend me carefully",
        new_workflow(
            "amendable",
            chain(vec![
                step(
                    "hold",
                    StepKind::Wait {
                        until: WaitFor::Release,
                    },
                ),
                agent_step("old", "mock"),
            ]),
        ),
    );
    // Two projects attached: an agent step naming none has nowhere to run.
    let ws = engine.workspace();
    for slug in ["alpha", "beta"] {
        let p = ws
            .create_project(bisa_store::NewProject::managed(slug).unwrap())
            .unwrap();
        ws.attach(goal.id, p.id).unwrap();
    }
    let draft = new_workflow(
        "amendable",
        chain(vec![
            step(
                "hold",
                StepKind::Wait {
                    until: WaitFor::Release,
                },
            ),
            agent_step("new", "mock"),
        ]),
    );
    let err = engine.amend_run(goal.id, draft).unwrap_err();
    assert!(
        matches!(err, bisa_engine::EngineError::ProjectAmbiguous { .. }),
        "{err:?}"
    );
    assert_eq!(
        current_run(&engine, goal.id).workflow.revision,
        run.workflow.revision,
        "nothing was written"
    );

    // A new required input with no default: the run cannot bind it.
    let mut needs_input = new_workflow(
        "amendable",
        chain(vec![
            step(
                "hold",
                StepKind::Wait {
                    until: WaitFor::Release,
                },
            ),
            step(
                "ask",
                StepKind::Approval {
                    prompt: "Ship {inputs.who}?".into(),
                },
            ),
        ]),
    );
    needs_input.inputs = vec![bisa_core::InputDef {
        name: bisa_core::InputName::new("who").unwrap(),
        label: "Who".into(),
        kind: bisa_core::InputKind::Text,
        default: None,
        required: true,
    }];
    let err = engine.amend_run(goal.id, needs_input.clone()).unwrap_err();
    assert!(err.is_refusal(), "{err}");
    assert!(err.to_string().contains("who"), "{err}");
    assert_eq!(
        current_run(&engine, goal.id).workflow.revision,
        run.workflow.revision,
        "nothing was written"
    );

    // With a default, it lands and the run holds the value.
    needs_input.inputs[0].default = Some(json!("everyone"));
    needs_input.inputs[0].required = false;
    let amended = engine.amend_run(goal.id, needs_input).unwrap();
    assert_eq!(amended.inputs.get("who"), Some(&json!("everyone")));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn on_fail_then_routes_a_failed_step() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"fixed": true}))]);

    let mut probe = step(
        "probe",
        StepKind::Check {
            check: CheckKind::Command {
                command: "exit 1".into(),
            },
        },
    );
    probe.on_fail = OnFail::Then {
        step: sid("remedy"),
    };
    probe.then = vec![Flow::to(sid("celebrate"))];
    let remedy = agent_step("remedy", "mock");
    let celebrate = agent_step("celebrate", "mock");
    let (goal, _) = run_on(
        &engine,
        "remediate",
        new_workflow("remediated", vec![probe, remedy, celebrate]),
    );

    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done), "{done:?}");
    assert_eq!(done.steps[&sid("probe")].state, StepState::Failed);
    assert!(matches!(
        done.steps[&sid("remedy")].state,
        StepState::Done { .. }
    ));
    // `then` takes only the implicit edge: the ordinary successor is skipped.
    assert_eq!(done.steps[&sid("celebrate")].state, StepState::Skipped);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn retries_relaunch_a_failed_agent_step() {
    let dir = tempfile::tempdir().unwrap();
    // The default mock ends its turn without yielding a result, which is a
    // failed attempt every time.
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let mut build = agent_step("build", "mock");
    build.retries = 2;
    let (goal, _) = run_on(&engine, "keep trying", new_workflow("retried", vec![build]));

    let failed = finished_run(&engine, goal.id).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed));
    let record = &failed.steps[&sid("build")];
    assert_eq!(record.attempts, 3, "one attempt plus two retries");
    assert_eq!(
        items_of(&engine, goal.id).len(),
        3,
        "a fresh item per attempt"
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn max_visits_bounds_a_loop() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"again": true}))]);

    let mut work = agent_step_promising("work", "mock", &["again"]);
    work.then = vec![Flow::to(sid("again"))];
    work.max_visits = 2;
    let mut again = step(
        "again",
        StepKind::Decide {
            rules: vec![Rule {
                when: Condition::OutputEquals {
                    step: sid("work"),
                    path: "again".into(),
                    value: json!(true),
                },
                branch: Branch::new("loop").unwrap(),
            }],
            otherwise: Branch::new("stop").unwrap(),
            pick: Default::default(),
        },
    );
    again.then = vec![
        Flow::branch(sid("work"), Branch::new("loop").unwrap()),
        Flow::branch(sid("finish"), Branch::new("stop").unwrap()),
    ];
    again.max_visits = 5;
    let finish = step(
        "finish",
        StepKind::End {
            finish: bisa_core::Finish::Done,
        },
    );
    let (goal, _) = run_on(
        &engine,
        "loop",
        new_workflow("looped", vec![work, again, finish]),
    );

    let failed = finished_run(&engine, goal.id).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed));
    assert_eq!(failed.steps[&sid("work")].visits, 2);
    assert_eq!(failed.steps[&sid("work")].state, StepState::Failed);
    engine.shutdown().await;
}

/// The person's last *not sure* fails the human step with their words, the
/// run with it — and the gate is spent exactly once, on a decision the run
/// took.
#[tokio::test(flavor = "multi_thread")]
async fn a_human_steps_last_not_sure_fails_the_step_and_the_run_says_why() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = design_off_config();
    config.max_clarify_rounds = 1;
    let engine = Engine::start(
        workspace(&dir),
        catalog_with(vec![MockAdapter::default()]),
        config,
    )
    .unwrap();
    let ask = step(
        "which",
        StepKind::Human {
            prompt: "Which colour?".into(),
            options: vec![AskOption::new("red", "Red"), AskOption::new("blue", "Blue")],
            multi: false,
            assignee: None,
        },
    );
    let (goal, _) = run_on(
        &engine,
        "pick a colour",
        new_workflow("asked", chain(vec![ask, agent_step("paint", "mock")])),
    );
    step_in_state(&engine, goal.id, "which", "waiting").await;
    // The first *not sure* re-asks; the second is the last.
    engine
        .answer_step(
            current_run(&engine, goal.id).id,
            &sid("which"),
            &Answer::unsure(),
        )
        .unwrap();
    step_in_state(&engine, goal.id, "which", "waiting").await;
    engine
        .answer_step(
            current_run(&engine, goal.id).id,
            &sid("which"),
            &Answer::unsure().with_text("neither reads right"),
        )
        .unwrap();
    let failed = finished_run(&engine, goal.id).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed));
    let record = &failed.steps[&sid("which")];
    assert_eq!(record.state, StepState::Failed);
    assert_eq!(
        record.error.as_deref(),
        Some("the person is not sure: neither reads right")
    );
    assert_eq!(
        failed.steps[&sid("paint")].state,
        StepState::Cancelled,
        "nothing after it ran: the run's end cancels it"
    );
    engine.shutdown().await;
}

/// A placeholder that has nothing behind it at run time — here a signal's
/// payload without the field a later step reads — fails the step with the
/// reason: which step, and what it yielded instead.
#[tokio::test(flavor = "multi_thread")]
async fn a_placeholder_that_cannot_render_fails_the_step_with_the_reason() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let mut hook = step(
        "hook",
        StepKind::Wait {
            until: WaitFor::Signal {
                filter: SignalFilter {
                    name: "deploy.finished".into(),
                    fields: BTreeMap::new(),
                },
            },
        },
    );
    hook.then = vec![Flow::to(sid("report"))];
    let mut report = agent_step("report", "mock");
    if let StepKind::Agent { instructions, .. } = &mut report.kind {
        *instructions = "Report on issue {steps.hook.output.issue}".into();
    }
    let (goal, _) = run_on(
        &engine,
        "report the deploy",
        new_workflow("hooked", vec![hook, report]),
    );
    step_in_state(&engine, goal.id, "hook", "waiting").await;
    engine
        .emit_signal(
            "deploy.finished",
            json!({"status": "green", "sha": "abc"}),
            SignalScope::Goal { goal: goal.id },
        )
        .unwrap();
    let failed = finished_run(&engine, goal.id).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed), "{failed:?}");
    assert_eq!(
        failed.steps[&sid("report")].error.as_deref(),
        Some("{steps.hook.output.issue} has no value in this run: step `hook` yielded `status`, `sha` and no `issue`")
    );
    assert!(
        items_of(&engine, goal.id).is_empty(),
        "no session was launched on an instruction that could not render"
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_human_step_is_a_question_in_the_inbox_and_its_answer_advances_the_run() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let mut rx = engine.events();

    let ask = step(
        "which",
        StepKind::Human {
            prompt: "Which store for {goal.statement}?".into(),
            options: vec![
                AskOption::new("sqlite", "SQLite").recommend(),
                AskOption::new("pg", "Postgres"),
            ],
            multi: false,
            assignee: None,
        },
    );
    let (goal, run) = run_on(
        &engine,
        "pick the stack",
        new_workflow("asked", chain(vec![ask, agent_step("build", "mock")])),
    );
    assert_eq!(run.status(), RunStatus::Waiting);
    assert_eq!(
        engine
            .workspace()
            .get_goal(goal.id)
            .unwrap()
            .status(Some(&run)),
        GoalStatus::Waiting
    );
    let asked = wait_for(&mut rx, "the question", |e| {
        matches!(&e.payload, EnginePayload::QuestionAsked { .. })
    })
    .await;
    let EnginePayload::QuestionAsked {
        gate_id,
        text,
        expects,
    } = asked.payload
    else {
        unreachable!()
    };
    assert_eq!(text, "Which store for pick the stack?");
    assert_eq!(expects.options().len(), 2);
    let entry = engine.gate(&gate_id).unwrap();
    assert_eq!(entry.run, Some(run.id));
    assert_eq!(entry.step, Some(sid("which")));
    assert_eq!(entry.gate, Gate::Escalation);

    // A wrong answer is refused by the question's own shape.
    assert!(engine
        .decide(
            &gate_id,
            true,
            None,
            Some(&Answer::selecting(["mysql"])),
            None
        )
        .is_err());

    engine
        .decide(
            &gate_id,
            true,
            None,
            Some(&Answer::selecting(["sqlite"])),
            None,
        )
        .unwrap();
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert_eq!(
        done.steps[&sid("which")].answer.as_ref().unwrap().selected,
        vec!["sqlite"]
    );
    // The answer is a signed decision on the goal's journal.
    let journal = engine
        .workspace()
        .journal(&bisa_core::Home::from(goal.id))
        .unwrap();
    assert!(journal.iter().any(|e| matches!(
        &e.payload,
        JournalPayload::Decision { gate: Gate::Escalation, subject, .. }
            if subject == &format!("step:{}/which", run.id)
    )));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_approval_step_needs_a_signed_decision_and_a_decline_fails_the_step() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let mut rx = engine.events();

    let approve = step(
        "ship",
        StepKind::Approval {
            prompt: "Ship it?".into(),
        },
    );
    let (goal, run) = run_on(
        &engine,
        "ship",
        new_workflow("gated", chain(vec![approve, agent_step("publish", "mock")])),
    );
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
        unreachable!()
    };
    assert_eq!(
        engine.gate(&gate_id).unwrap().subject,
        format!("approval:{}/ship", run.id)
    );

    engine
        .decide(&gate_id, false, Some("not yet"), None, None)
        .unwrap();
    let failed = finished_run(&engine, goal.id).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed));
    assert_eq!(failed.steps[&sid("ship")].state, StepState::Failed);
    assert!(
        failed.steps[&sid("ship")].gate.is_some(),
        "the decision is recorded on the step"
    );
    assert_eq!(
        failed.steps[&sid("publish")].state,
        StepState::Cancelled,
        "the run's end cancels what never started"
    );
    assert!(
        items_of(&engine, goal.id).is_empty(),
        "nothing ran past the gate"
    );

    // The decision is the store's, verified against the journal.
    assert!(engine
        .workspace()
        .journal(&bisa_core::Home::from(goal.id))
        .unwrap()
        .iter()
        .any(|e| matches!(
            &e.payload,
            JournalPayload::Decision {
                gate: Gate::Approval,
                approve: false,
                ..
            }
        )));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_notify_step_with_mentions_wakes_the_agent_and_without_them_wakes_nobody() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    let assayer = ws
        .add_agent(bisa_store::NewAgent {
            name: "Assayer".into(),
            harness: "mock".into(),
            ..Default::default()
        })
        .unwrap();

    let notify = step(
        "tell",
        StepKind::Notify {
            scope: None,
            template: "heads up about {goal.statement}".into(),
            mentions: vec![ValueRef::Fixed(bisa_core::Assignee::Agent(
                assayer.id.to_string(),
            ))],
            author: None,
        },
    );
    let (goal, _) = run_on(&engine, "the launch", new_workflow("told", vec![notify]));
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    // The notification, and the mentioned agent's reply to it: a mention
    // wakes the agent in the thread.
    let mut messages = ws.messages(&goal.id.to_string(), None, 10).unwrap();
    messages.sort_by_key(|m| m.created_at);
    assert_eq!(messages.len(), 2, "{messages:?}");
    assert_eq!(messages[0].content, "heads up about the launch");
    assert_eq!(
        messages[0].author,
        ws.get_agent(&bisa_core::AgentId::workflow())
            .unwrap()
            .pubkey
            .as_hex(),
        "an unnamed notify is the Workflow Agent's line"
    );
    assert_eq!(
        ws.mentions_of(assayer.pubkey.as_hex(), 10).unwrap().len(),
        1
    );

    let quiet = step(
        "tell",
        StepKind::Notify {
            scope: None,
            template: "nobody in particular".into(),
            mentions: vec![],
            author: None,
        },
    );
    let (goal2, _) = run_on(&engine, "quietly", new_workflow("quiet", vec![quiet]));
    finished_run(&engine, goal2.id).await;
    assert_eq!(
        ws.mentions_of(assayer.pubkey.as_hex(), 10).unwrap().len(),
        1,
        "an unaddressed notification names nobody"
    );
    engine.shutdown().await;
}

/// A workflow's message is an agent's, never the person's: a `notify` that
/// names nobody speaks as the Workflow Agent, the run's designer.
#[tokio::test(flavor = "multi_thread")]
async fn a_notify_step_speaks_as_the_workflow_agent_unless_told_whom() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    let notify = step(
        "tell",
        StepKind::Notify {
            scope: None,
            template: "the run began".into(),
            mentions: vec![],
            author: None,
        },
    );
    let (goal, _) = run_on(&engine, "announced", new_workflow("told", vec![notify]));
    finished_run(&engine, goal.id).await;
    let messages = ws.messages(&goal.id.to_string(), None, 10).unwrap();
    assert_eq!(messages.len(), 1);
    let workflow_agent = ws.get_agent(&bisa_core::AgentId::workflow()).unwrap();
    assert_eq!(
        messages[0].author,
        workflow_agent.pubkey.as_hex(),
        "the workflow speaks as its designer"
    );
    assert_ne!(
        messages[0].author,
        ws.owner_principal().as_hex(),
        "never as the person"
    );
    engine.shutdown().await;
}

/// A `notify` with an `author` speaks as that agent — named outright, or
/// through an assignee input the run was started with.
#[tokio::test(flavor = "multi_thread")]
async fn a_notify_step_speaks_as_the_agent_it_names() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    let herald = ws
        .add_agent(bisa_store::NewAgent {
            name: "Herald".into(),
            harness: "mock".into(),
            ..Default::default()
        })
        .unwrap();
    let voiced = |author: ValueRef<bisa_core::Assignee>| {
        step(
            "tell",
            StepKind::Notify {
                scope: None,
                template: "hear ye".into(),
                mentions: vec![],
                author: Some(author),
            },
        )
    };

    let (goal, _) = run_on(
        &engine,
        "named outright",
        new_workflow(
            "heralded",
            vec![voiced(ValueRef::Fixed(bisa_core::Assignee::Agent(
                herald.id.to_string(),
            )))],
        ),
    );
    finished_run(&engine, goal.id).await;
    let messages = ws.messages(&goal.id.to_string(), None, 10).unwrap();
    assert_eq!(messages[0].author, herald.pubkey.as_hex());

    let mut wf = new_workflow(
        "voiced",
        vec![voiced(ValueRef::Input {
            input: bisa_core::InputName::new("voice").unwrap(),
        })],
    );
    wf.inputs = vec![bisa_core::InputDef {
        name: bisa_core::InputName::new("voice").unwrap(),
        label: "Voice".into(),
        kind: bisa_core::InputKind::Assignee,
        default: None,
        required: true,
    }];
    let (goal2, _) = goal_on(&engine, "named at start", wf);
    engine
        .start_run(
            goal2.id,
            BTreeMap::from([("voice".to_string(), json!(format!("agent:{}", herald.id)))]),
        )
        .unwrap();
    finished_run(&engine, goal2.id).await;
    let messages = ws.messages(&goal2.id.to_string(), None, 10).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].author, herald.pubkey.as_hex());
    engine.shutdown().await;
}

/// Only an agent speaks for a workflow: a team named outright is refused
/// when the workflow is recorded, and a person bound through an input is
/// refused at start — before anything is posted.
#[tokio::test(flavor = "multi_thread")]
async fn a_notify_author_that_is_not_an_agent_is_refused_at_validation_and_at_start() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    let voiced = |author: ValueRef<bisa_core::Assignee>| {
        step(
            "tell",
            StepKind::Notify {
                scope: None,
                template: "hear ye".into(),
                mentions: vec![],
                author: Some(author),
            },
        )
    };
    let err = engine
        .create_workflow(new_workflow(
            "teamed",
            vec![voiced(ValueRef::Fixed(bisa_core::Assignee::Team(
                "engineering".into(),
            )))],
        ))
        .unwrap_err();
    let bisa_engine::EngineError::Store(bisa_store::StoreError::WorkflowInvalid(problems)) = err
    else {
        panic!("expected the problems, got {err}");
    };
    assert!(
        problems
            .iter()
            .any(|p| p.kind == bisa_core::ProblemKind::NotifyAuthorNotAnAgent),
        "{problems:?}"
    );

    let mut wf = new_workflow(
        "voiced",
        vec![voiced(ValueRef::Input {
            input: bisa_core::InputName::new("voice").unwrap(),
        })],
    );
    wf.inputs = vec![bisa_core::InputDef {
        name: bisa_core::InputName::new("voice").unwrap(),
        label: "Voice".into(),
        kind: bisa_core::InputKind::Assignee,
        default: None,
        required: true,
    }];
    let (goal, _) = goal_on(&engine, "voiced by a person", wf);
    let err = engine
        .start_run(
            goal.id,
            BTreeMap::from([(
                "voice".to_string(),
                json!(format!("human:{}", ws.owner_principal().as_hex())),
            )]),
        )
        .unwrap_err();
    assert!(err.is_refusal(), "{err}");
    assert!(err.to_string().contains("not an agent"), "{err}");
    assert!(ws.get_goal(goal.id).unwrap().run.is_none());
    assert!(ws
        .messages(&goal.id.to_string(), None, 10)
        .unwrap()
        .is_empty());
    engine.shutdown().await;
}

/// A step's question and an approval's gate are the Workflow Agent's facts
/// on the journal — the workflow raises them, not the person.
#[tokio::test(flavor = "multi_thread")]
async fn a_steps_question_is_the_workflow_agents_fact() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let ws = engine.workspace();
    let mut rx = engine.events();
    let workflow_agent = ws.get_agent(&bisa_core::AgentId::workflow()).unwrap();
    let question_author = |goal: bisa_core::GoalId| {
        ws.journal(&bisa_core::Home::from(goal))
            .unwrap()
            .into_iter()
            .find_map(|e| match &e.payload {
                JournalPayload::Question { .. } => Some(e.author),
                _ => None,
            })
            .expect("a question on the journal")
    };

    let ask = step(
        "which",
        StepKind::Human {
            prompt: "Which store?".into(),
            options: vec![],
            multi: false,
            assignee: None,
        },
    );
    let (goal, _) = run_on(
        &engine,
        "asked",
        new_workflow("asked", chain(vec![ask, agent_step("build", "mock")])),
    );
    wait_for(&mut rx, "the question", |e| {
        matches!(&e.payload, EnginePayload::QuestionAsked { .. })
    })
    .await;
    assert_eq!(question_author(goal.id), workflow_agent.pubkey);

    let gate = step(
        "ok",
        StepKind::Approval {
            prompt: "Ship it?".into(),
        },
    );
    let (goal2, _) = run_on(
        &engine,
        "gated",
        new_workflow("gated", chain(vec![gate, agent_step("build", "mock")])),
    );
    wait_for(&mut rx, "the gate", |e| {
        matches!(&e.payload, EnginePayload::GateOpened { .. })
    })
    .await;
    assert_eq!(question_author(goal2.id), workflow_agent.pubkey);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_spawn_step_creates_a_refines_child_and_waits_for_its_run() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let child_wf = engine
        .create_workflow(new_workflow("child work", vec![agent_step("do", "mock")]))
        .unwrap();

    let spawn = step(
        "delegate",
        StepKind::Spawn {
            statement_template: "handle {goal.statement} in detail".into(),
            workflow: Some(child_wf.id),
            assignees: vec![],
            inputs: Default::default(),
            wait: true,
        },
    );
    let (goal, _) = run_on(&engine, "the big one", new_workflow("parent", vec![spawn]));
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done), "{done:?}");
    let output = done.steps[&sid("delegate")].output.clone().unwrap();
    let child: bisa_core::GoalId = output["child"].as_str().unwrap().parse().unwrap();
    assert_eq!(output["outcome"], json!("done"));
    let child_goal = engine.workspace().get_goal(child).unwrap();
    assert_eq!(child_goal.origin, GoalOrigin::Spawned { parent: goal.id });
    assert_eq!(child_goal.statement, "handle the big one in detail");
    assert_eq!(
        engine.workspace().edges_from(child).unwrap(),
        vec![(goal.id, bisa_core::GoalEdgeKind::Refines)]
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn closing_a_goal_cancels_the_run_its_items_and_withdraws_its_gates() {
    let dir = tempfile::tempdir().unwrap();
    // A session that never ends: the item stays in progress until cancelled.
    let engine = engine_with(
        &dir,
        vec![MockAdapter {
            script: Some(vec![bisa_harness::SessionEvent::Lifecycle(
                bisa_harness::LifecycleEvent::Started,
            )]),
            ..Default::default()
        }],
    );
    let mut rx = engine.events();
    let mut fan = step(
        "fan",
        StepKind::Human {
            prompt: "Go?".into(),
            options: vec![],
            multi: false,
            assignee: None,
        },
    );
    fan.then = vec![];
    let (goal, run) = run_on(
        &engine,
        "abandon me",
        new_workflow(
            "abandoned",
            vec![
                // One start, done at once, fans out to the agent step and the
                // question — a run with two roots is refused at creation.
                {
                    let mut go = step(
                        "go",
                        StepKind::Notify {
                            scope: None,
                            template: "go".into(),
                            mentions: vec![],
                            author: None,
                        },
                    );
                    go.then = vec![Flow::to(sid("start")), Flow::to(sid("fan"))];
                    go
                },
                {
                    let mut s = agent_step("start", "mock");
                    s.then = vec![Flow::to(sid("later"))];
                    s
                },
                agent_step("later", "mock"),
                fan,
            ],
        ),
    );
    // Two arms: the agent step runs and the question waits.
    step_in_state(&engine, goal.id, "start", "running").await;
    wait_for(&mut rx, "the question", |e| {
        matches!(&e.payload, EnginePayload::GateOpened { .. })
    })
    .await;
    assert_eq!(engine.inbox().len(), 1);
    // A run queued behind the live one goes with the goal too.
    let queued = engine.start_run(goal.id, BTreeMap::new()).unwrap();
    assert_eq!(queued.status(), RunStatus::Queued);

    let closed = engine
        .close_goal(goal.id, ClosureReason::Abandoned { rationale: None })
        .unwrap();
    assert!(closed.is_closed());
    let cause = CancelCause::Closed {
        reason: ClosureReason::Abandoned { rationale: None },
    };
    let run = engine.workspace().get_run(run.id).unwrap();
    assert_eq!(run.status(), RunStatus::Cancelled);
    assert_eq!(run.cancelled, Some(cause.clone()));
    assert_eq!(run.steps[&sid("start")].state, StepState::Cancelled);
    assert_eq!(
        run.steps[&sid("later")].state,
        StepState::Cancelled,
        "the run's end cancels what never started"
    );
    let queued = engine.workspace().get_run(queued.id).unwrap();
    assert_eq!(queued.status(), RunStatus::Cancelled);
    assert_eq!(queued.cancelled, Some(cause));
    assert_eq!(queued.started_at, None, "it never started");
    assert!(engine.inbox().is_empty(), "the question was withdrawn");
    until("the item to be cancelled", || {
        items_of(&engine, goal.id)
            .iter()
            .all(|i| matches!(i.state, WorkItemState::Cancelled))
            .then_some(())
    })
    .await;
    // Both cancels are announced before the close, and no run finished.
    let mut cancelled = Vec::new();
    loop {
        let e = wait_for(&mut rx, "a run cancelled or the goal closed", |e| {
            matches!(
                &e.payload,
                EnginePayload::RunCancelled { .. }
                    | EnginePayload::GoalClosed { .. }
                    | EnginePayload::RunFinished { .. }
            )
        })
        .await;
        match e.payload {
            EnginePayload::RunCancelled { run, cause, .. } => {
                assert!(matches!(cause, CancelCause::Closed { .. }));
                cancelled.push(run);
            }
            EnginePayload::RunFinished { .. } => panic!("a cancelled run does not finish"),
            _ => break,
        }
    }
    cancelled.sort();
    let mut expected = vec![run.id, queued.id];
    expected.sort();
    assert_eq!(
        cancelled, expected,
        "the queued run and the live run, each announced"
    );
    assert_eq!(
        engine
            .workspace()
            .get_goal(goal.id)
            .unwrap()
            .status(Some(&run)),
        GoalStatus::Closed
    );
    engine.shutdown().await;
}

/// A one-`wait { release }`-step workflow: a run of it is live until a
/// person releases it, which is what a queue needs behind it.
fn held_workflow(name: &str) -> bisa_store::NewWorkflow {
    new_workflow(
        name,
        vec![step(
            "hold",
            StepKind::Wait {
                until: WaitFor::Release,
            },
        )],
    )
}

/// A held workflow that declares `who` and reads it after the hold — an
/// input nothing reads is refused at creation (`UnusedInput`).
fn held_workflow_with_who(name: &str) -> bisa_store::NewWorkflow {
    let mut wf = new_workflow(
        name,
        chain(vec![
            step(
                "hold",
                StepKind::Wait {
                    until: WaitFor::Release,
                },
            ),
            step(
                "tell",
                StepKind::Notify {
                    scope: None,
                    template: "hello {inputs.who}".into(),
                    mentions: vec![],
                    author: None,
                },
            ),
        ]),
    );
    wf.inputs = vec![text_input_def("who")];
    wf
}

/// The goal's runs in journal order, as `<status>` words, plus every run
/// fact the journal holds.
fn run_facts(engine: &bisa_engine::Engine, goal: bisa_core::GoalId) -> Vec<String> {
    engine
        .workspace()
        .journal(&bisa_core::Home::from(goal))
        .unwrap()
        .into_iter()
        .filter_map(|je| match je.payload {
            JournalPayload::Run { run, event } => Some(format!("{run}:{}", event.as_str())),
            _ => None,
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn starting_a_second_run_while_one_is_live_queues_it() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let mut rx = engine.events();
    let (goal, first) = run_on(&engine, "hold on", held_workflow("held"));
    assert_eq!(first.status(), RunStatus::Waiting);

    let second = engine.start_run(goal.id, BTreeMap::new()).unwrap();
    assert_eq!(second.status(), RunStatus::Queued);
    assert_eq!(second.started_at, None);
    let g = engine.workspace().get_goal(goal.id).unwrap();
    assert_eq!(g.run, Some(first.id), "the current run is the live one");
    assert_eq!(g.runs, vec![first.id, second.id]);
    assert_eq!(g.status(Some(&first)), GoalStatus::Waiting);
    let queued = wait_for(&mut rx, "the run queued", |e| {
        matches!(&e.payload, EnginePayload::RunQueued { .. })
    })
    .await;
    assert!(matches!(
        queued.payload,
        EnginePayload::RunQueued { run, position: 1, .. } if run == second.id
    ));
    let third = engine.start_run(goal.id, BTreeMap::new()).unwrap();
    let queued = wait_for(&mut rx, "the third run queued", |e| {
        matches!(&e.payload, EnginePayload::RunQueued { .. })
    })
    .await;
    assert!(matches!(
        queued.payload,
        EnginePayload::RunQueued { run, position: 2, .. } if run == third.id
    ));
    assert_eq!(
        engine
            .runs(goal.id)
            .unwrap()
            .iter()
            .map(|r| r.status())
            .collect::<Vec<_>>(),
        vec![RunStatus::Waiting, RunStatus::Queued, RunStatus::Queued]
    );

    // The live run ends: the second starts on its own, the third waits.
    engine
        .release_step(current_run(&engine, goal.id).id, &sid("hold"), None)
        .unwrap();
    let started = wait_for(&mut rx, "the queued run to start", |e| {
        matches!(&e.payload, EnginePayload::RunStarted { .. })
    })
    .await;
    assert!(matches!(
        started.payload,
        EnginePayload::RunStarted { run, .. } if run == second.id
    ));
    let g = engine.workspace().get_goal(goal.id).unwrap();
    assert_eq!(g.run, Some(second.id));
    let second = engine.workspace().get_run(second.id).unwrap();
    assert_eq!(second.status(), RunStatus::Waiting);
    assert!(second.started_at.is_some());
    assert_eq!(
        engine
            .workspace()
            .queued_runs(goal.id)
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect::<Vec<_>>(),
        vec![third.id]
    );
    assert_eq!(
        run_facts(&engine, goal.id),
        vec![
            format!("{}:started", first.id),
            format!("{}:queued", second.id),
            format!("{}:queued", third.id),
            format!("{}:finished", first.id),
            format!("{}:started", second.id),
        ]
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_queued_run_starts_when_the_live_one_finishes() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let mut rx = engine.events();
    // A live run that waits on a person; a second run of the same held
    // workflow queues behind it, and is released in its turn.
    let (goal, first) = run_on(&engine, "one after another", held_workflow("held"));
    let second = engine.start_run(goal.id, BTreeMap::new()).unwrap();
    assert_eq!(second.status(), RunStatus::Queued);
    engine
        .release_step(current_run(&engine, goal.id).id, &sid("hold"), None)
        .unwrap();
    wait_for(
        &mut rx,
        "the first run to finish",
        |e| matches!(&e.payload, EnginePayload::RunFinished { run, .. } if *run == first.id),
    )
    .await;
    wait_for(
        &mut rx,
        "the second run to start",
        |e| matches!(&e.payload, EnginePayload::RunStarted { run, .. } if *run == second.id),
    )
    .await;
    engine
        .release_step(current_run(&engine, goal.id).id, &sid("hold"), None)
        .unwrap();
    let done = until("the second run to finish", || {
        let run = engine.workspace().get_run(second.id).ok()?;
        run.is_finished().then_some(run)
    })
    .await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert_eq!(
        engine.workspace().get_goal(goal.id).unwrap().run,
        Some(second.id)
    );
    assert_eq!(
        run_facts(&engine, goal.id),
        vec![
            format!("{}:started", first.id),
            format!("{}:queued", second.id),
            format!("{}:finished", first.id),
            format!("{}:started", second.id),
            format!("{}:finished", second.id),
        ]
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn stopping_a_goal_cancels_the_live_run_withdraws_the_queue_and_reads_draft() {
    let dir = tempfile::tempdir().unwrap();
    // A session that never ends, so the stop has something to end.
    let engine = engine_with(
        &dir,
        vec![MockAdapter {
            script: Some(vec![bisa_harness::SessionEvent::Lifecycle(
                bisa_harness::LifecycleEvent::Started,
            )]),
            ..Default::default()
        }],
    );
    let mut rx = engine.events();
    let (goal, live) = run_on(
        &engine,
        "stop me",
        new_workflow("endless", vec![agent_step("work", "mock")]),
    );
    step_in_state(&engine, goal.id, "work", "running").await;
    let q1 = engine.start_run(goal.id, BTreeMap::new()).unwrap();
    let q2 = engine.start_run(goal.id, BTreeMap::new()).unwrap();

    let stopped = engine
        .stop_goal(goal.id, Some("enough for today".into()))
        .await
        .unwrap();
    assert_eq!(stopped.run, Some(live.id));
    assert_eq!(stopped.withdrawn, vec![q1.id, q2.id]);

    let live = engine.workspace().get_run(live.id).unwrap();
    assert_eq!(live.status(), RunStatus::Cancelled);
    assert_eq!(
        live.cancelled,
        Some(CancelCause::Stopped {
            rationale: Some("enough for today".into())
        })
    );
    for q in [q1.id, q2.id] {
        let run = engine.workspace().get_run(q).unwrap();
        assert_eq!(run.status(), RunStatus::Cancelled);
        assert_eq!(run.cancelled, Some(CancelCause::Withdrawn));
        assert_eq!(run.started_at, None);
    }
    let g = engine.workspace().get_goal(goal.id).unwrap();
    assert!(!g.is_closed(), "a stop leaves the goal open");
    assert_eq!(g.run, Some(live.id));
    assert_eq!(
        g.status(Some(&live)),
        GoalStatus::Draft,
        "ready for a new run"
    );
    assert!(engine.workspace().queued_runs(goal.id).unwrap().is_empty());
    until("the item to be cancelled", || {
        items_of(&engine, goal.id)
            .iter()
            .all(|i| matches!(i.state, WorkItemState::Cancelled))
            .then_some(())
    })
    .await;
    // Every cancel is on the bus, with its cause; nothing finished.
    let mut seen = Vec::new();
    while seen.len() < 3 {
        let e = wait_for(&mut rx, "a run cancelled", |e| {
            matches!(
                &e.payload,
                EnginePayload::RunCancelled { .. } | EnginePayload::RunFinished { .. }
            )
        })
        .await;
        match e.payload {
            EnginePayload::RunCancelled { run, cause, .. } => seen.push((run, cause.as_str())),
            other => panic!("a stop finishes nothing: {other:?}"),
        }
    }
    assert_eq!(
        seen,
        vec![
            (q1.id, "withdrawn"),
            (q2.id, "withdrawn"),
            (live.id, "stopped")
        ],
        "the queue is withdrawn first, then the live run is stopped"
    );
    // The stop's facts, then a new run starts at once on the idle goal.
    let again = engine.start_run(goal.id, BTreeMap::new()).unwrap();
    assert_eq!(again.status(), RunStatus::Running);
    assert_eq!(
        run_facts(&engine, goal.id),
        vec![
            format!("{}:started", live.id),
            format!("{}:queued", q1.id),
            format!("{}:queued", q2.id),
            format!("{}:cancelled", q1.id),
            format!("{}:cancelled", q2.id),
            format!("{}:cancelled", live.id),
            format!("{}:started", again.id),
        ]
    );
    // A stop with nothing to stop is a quiet no-op on the same goal.
    engine.stop_goal(goal.id, None).await.unwrap();
    let nothing = engine.stop_goal(goal.id, None).await.unwrap();
    assert_eq!(nothing, bisa_engine::Stopped::default());
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn restarting_a_goal_yields_a_new_live_run_with_the_same_inputs_and_the_queue_keeps_its_place(
) {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let mut rx = engine.events();
    let wf = held_workflow_with_who("held with input");
    let (goal, _) = goal_on(&engine, "restart me", wf);
    let first = engine
        .start_run(goal.id, BTreeMap::from([("who".into(), json!("us"))]))
        .unwrap();
    assert_eq!(first.status(), RunStatus::Waiting);
    let queued = engine
        .start_run(goal.id, BTreeMap::from([("who".into(), json!("them"))]))
        .unwrap();
    assert_eq!(queued.status(), RunStatus::Queued);

    let restarted = engine.restart_goal(goal.id).await.unwrap();
    assert_ne!(restarted.id, first.id);
    assert_eq!(restarted.status(), RunStatus::Waiting, "started at once");
    assert_eq!(
        restarted.inputs["who"],
        json!("us"),
        "the last run's inputs"
    );
    assert_eq!(restarted.workflow.id, first.workflow.id);
    let first = engine.workspace().get_run(first.id).unwrap();
    assert_eq!(first.cancelled, Some(CancelCause::Restarted));
    let g = engine.workspace().get_goal(goal.id).unwrap();
    assert_eq!(g.run, Some(restarted.id));
    assert_eq!(g.runs, vec![first.id, queued.id, restarted.id]);
    assert_eq!(
        engine
            .workspace()
            .queued_runs(goal.id)
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect::<Vec<_>>(),
        vec![queued.id],
        "the queue keeps its place behind the restart"
    );
    let cancelled = wait_for(&mut rx, "the restart's cancel", |e| {
        matches!(&e.payload, EnginePayload::RunCancelled { .. })
    })
    .await;
    assert!(matches!(
        cancelled.payload,
        EnginePayload::RunCancelled { run, cause: CancelCause::Restarted, .. } if run == first.id
    ));
    let started = wait_for(&mut rx, "the restart's start", |e| {
        matches!(&e.payload, EnginePayload::RunStarted { .. })
    })
    .await;
    assert!(
        matches!(started.payload, EnginePayload::RunStarted { run, .. } if run == restarted.id),
        "the restart's replacement starts, not the queued run"
    );

    // A restart of a goal with nothing live runs the last run again.
    engine
        .release_step(current_run(&engine, goal.id).id, &sid("hold"), None)
        .unwrap();
    wait_for(
        &mut rx,
        "the queued run to start",
        |e| matches!(&e.payload, EnginePayload::RunStarted { run, .. } if *run == queued.id),
    )
    .await;
    engine
        .release_step(current_run(&engine, goal.id).id, &sid("hold"), None)
        .unwrap();
    until("the queued run to finish", || {
        engine
            .workspace()
            .get_run(queued.id)
            .ok()
            .filter(|r| r.is_finished())
    })
    .await;
    let again = engine.restart_goal(goal.id).await.unwrap();
    assert_eq!(
        again.inputs["who"],
        json!("them"),
        "the latest run's inputs"
    );
    assert_eq!(again.status(), RunStatus::Waiting);

    // Closed, or never run: refused.
    let never = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..SubmitRequest::captured("never ran")
        })
        .unwrap();
    let err = engine.restart_goal(never.id).await.unwrap_err();
    assert!(
        err.is_refusal() && err.to_string().contains("no run"),
        "{err}"
    );
    engine
        .close_goal(goal.id, ClosureReason::Abandoned { rationale: None })
        .unwrap();
    let err = engine.restart_goal(goal.id).await.unwrap_err();
    assert!(
        err.is_refusal() && err.to_string().contains("closed"),
        "{err}"
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn withdrawing_is_for_a_queued_run_only() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let mut rx = engine.events();
    let (goal, live) = run_on(&engine, "withdraw", held_workflow("held"));
    let q1 = engine.start_run(goal.id, BTreeMap::new()).unwrap();
    let q2 = engine.start_run(goal.id, BTreeMap::new()).unwrap();

    let withdrawn = engine.withdraw_run(goal.id, q1.id).unwrap();
    assert_eq!(withdrawn.status(), RunStatus::Cancelled);
    assert_eq!(withdrawn.cancelled, Some(CancelCause::Withdrawn));
    let cancelled = wait_for(&mut rx, "the withdrawal", |e| {
        matches!(&e.payload, EnginePayload::RunCancelled { .. })
    })
    .await;
    assert!(matches!(
        cancelled.payload,
        EnginePayload::RunCancelled { run, cause: CancelCause::Withdrawn, .. } if run == q1.id
    ));
    assert_eq!(
        engine
            .workspace()
            .queued_runs(goal.id)
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect::<Vec<_>>(),
        vec![q2.id]
    );
    assert_eq!(
        engine.workspace().get_goal(goal.id).unwrap().run,
        Some(live.id),
        "the live run is untouched"
    );

    // The live run: a conflict. A run of another goal: not found.
    let err = engine.withdraw_run(goal.id, live.id).unwrap_err();
    assert!(
        err.is_refusal() && err.to_string().contains("not queued"),
        "{err}"
    );
    let (other, _) = run_on(&engine, "another", held_workflow("held 2"));
    let err = engine.withdraw_run(other.id, q2.id).unwrap_err();
    assert!(err.to_string().contains("not found"), "{err}");
    assert_eq!(
        engine.workspace().get_run(q2.id).unwrap().status(),
        RunStatus::Queued,
        "still queued"
    );
    let err = engine.withdraw_run(goal.id, q1.id).unwrap_err();
    assert!(err.is_refusal(), "withdrawn twice: {err}");
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_invalid_workflow_is_refused_at_start_with_its_problems() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    // A dangling flow cannot even be recorded as a runnable workflow…
    let mut broken = agent_step("a", "mock");
    broken.then = vec![Flow::to(sid("nowhere"))];
    let err = engine
        .create_workflow(new_workflow("broken", vec![broken]))
        .unwrap_err();
    let bisa_engine::EngineError::Store(bisa_store::StoreError::WorkflowInvalid(problems)) = err
    else {
        panic!("expected the problems, got {err}");
    };
    assert!(problems
        .iter()
        .any(|p| p.kind == bisa_core::ProblemKind::UnknownStep));

    // …and a draft saved with problems is refused at start, with them.
    let (goal, wf) = goal_on(
        &engine,
        "fix me first",
        new_workflow("ok", vec![agent_step("a", "mock")]),
    );
    let mut draft = bisa_store::NewWorkflow::from(&wf);
    draft.steps[0].then = vec![Flow::to(sid("nowhere"))];
    let (saved, problems) = engine.save_workflow(wf.id, draft, wf.revision).unwrap();
    assert_eq!(problems.len(), 1);
    assert_eq!(saved.revision, wf.revision + 1);
    let err = engine.start_run(goal.id, BTreeMap::new()).unwrap_err();
    assert!(err.is_refusal());
    assert!(err.to_string().contains("nowhere"), "{err}");
    assert!(engine.workspace().get_goal(goal.id).unwrap().run.is_none());
    engine.shutdown().await;
}

/// A cron or a notify scope read from an input is wrong only once a value is
/// bound — and it is refused then, at start, with the validator's own
/// problems, rather than failing the step it reaches deep into the run.
#[tokio::test(flavor = "multi_thread")]
async fn a_bad_cron_or_scope_input_is_refused_at_start_with_its_problems() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let input = |name: &str| bisa_core::InputDef {
        name: bisa_core::InputName::new(name).unwrap(),
        label: name.to_uppercase(),
        kind: bisa_core::InputKind::Text,
        default: None,
        required: true,
    };
    let mut clock = step(
        "clock",
        StepKind::Wait {
            until: WaitFor::Schedule {
                cron: ValueRef::Input {
                    input: bisa_core::InputName::new("when").unwrap(),
                },
                tz: None,
            },
        },
    );
    clock.then = vec![Flow::to(sid("tell"))];
    let tell = step(
        "tell",
        StepKind::Notify {
            scope: Some("{inputs.channel}".into()),
            template: "it is time".into(),
            mentions: vec![],
            author: None,
        },
    );
    let mut draft = new_workflow("timed", vec![clock, tell]);
    draft.inputs = vec![input("when"), input("channel")];
    let (goal, _) = goal_on(&engine, "on time", draft);

    let bad = BTreeMap::from([
        ("when".to_string(), json!("not a cron")),
        ("channel".to_string(), json!("no such channel!")),
    ]);
    let err = engine.start_run(goal.id, bad).unwrap_err();
    assert!(err.is_refusal(), "{err}");
    let bisa_engine::EngineError::Store(bisa_store::StoreError::WorkflowInvalid(problems)) = &err
    else {
        panic!("expected the problems, got {err}");
    };
    let kinds: Vec<_> = problems.iter().map(|p| p.kind).collect();
    assert!(
        kinds.contains(&bisa_core::ProblemKind::BadCron),
        "{kinds:?}"
    );
    assert!(
        kinds.contains(&bisa_core::ProblemKind::NotifyScopeUnknown),
        "{kinds:?}"
    );
    assert!(engine.workspace().get_goal(goal.id).unwrap().run.is_none());

    let good = BTreeMap::from([
        ("when".to_string(), json!("0 9 * * 1")),
        ("channel".to_string(), json!("general")),
    ]);
    let run = engine.start_run(goal.id, good).unwrap();
    assert_eq!(run.status(), RunStatus::Waiting);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn amending_replaces_only_steps_not_started() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let hold = step(
        "hold",
        StepKind::Wait {
            until: WaitFor::Release,
        },
    );
    let (goal, run) = run_on(
        &engine,
        "amend me",
        new_workflow("amendable", chain(vec![hold, agent_step("old", "mock")])),
    );
    // Swap the not-yet-started step for two others.
    let mut draft = new_workflow(
        "amendable",
        chain(vec![
            step(
                "hold",
                StepKind::Wait {
                    until: WaitFor::Release,
                },
            ),
            agent_step("new", "mock"),
            agent_step("newer", "mock"),
        ]),
    );
    draft.description = "amended".into();
    let amended = engine.amend_run(goal.id, draft.clone()).unwrap();
    assert_eq!(amended.workflow.revision, run.workflow.revision + 1);
    assert!(amended.steps.contains_key(&sid("new")));
    assert!(!amended.steps.contains_key(&sid("old")));

    // Touching the started step is refused.
    let mut bad = draft.clone();
    bad.steps[0].kind = StepKind::Approval { prompt: "?".into() };
    let err = engine.amend_run(goal.id, bad).unwrap_err();
    assert!(err.is_refusal(), "{err}");
    assert!(err.to_string().contains("hold"), "{err}");

    engine
        .release_step(current_run(&engine, goal.id).id, &sid("hold"), None)
        .unwrap();
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert!(matches!(
        done.steps[&sid("newer")].state,
        StepState::Done { .. }
    ));
    let journal = engine
        .workspace()
        .journal(&bisa_core::Home::from(goal.id))
        .unwrap();
    assert!(journal.iter().any(|e| matches!(
        &e.payload,
        JournalPayload::Run {
            event: RunFact::Amended { .. },
            ..
        }
    )));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn every_step_change_is_a_journal_fact_and_a_bus_event() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let mut rx = engine.events();
    // A second receiver for the finish: the snapshot lands before its facts
    // are journaled, and the bus hears the finish after both.
    let mut finish = engine.events();
    let (goal, run) = run_on(
        &engine,
        "record me",
        new_workflow("recorded", vec![agent_step("only", "mock")]),
    );
    wait_for(&mut finish, "the run to finish", |e| {
        matches!(&e.payload, EnginePayload::RunFinished { .. })
    })
    .await;

    let journal = engine
        .workspace()
        .journal(&bisa_core::Home::from(goal.id))
        .unwrap();
    let step_facts: Vec<&StepFact> = journal
        .iter()
        .filter_map(|e| match &e.payload {
            JournalPayload::Step {
                run: r,
                step,
                event,
            } if *r == run.id && step == &sid("only") => Some(event),
            _ => None,
        })
        .collect();
    assert!(
        step_facts
            .iter()
            .any(|f| matches!(f, StepFact::Started { work_item: Some(_) })),
        "{step_facts:?}"
    );
    assert!(
        step_facts
            .iter()
            .any(|f| matches!(f, StepFact::Done { .. })),
        "{step_facts:?}"
    );
    assert!(journal.iter().any(|e| matches!(
        &e.payload,
        JournalPayload::Run {
            event: RunFact::Started { .. },
            ..
        }
    )));
    assert!(journal.iter().any(|e| matches!(
        &e.payload,
        JournalPayload::Run {
            event: RunFact::Finished {
                outcome: RunOutcome::Done
            },
            ..
        }
    )));

    let mut states = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        if let EnginePayload::StepChanged {
            step, state, kind, ..
        } = ev.payload
        {
            if step == sid("only") {
                assert_eq!(kind, "agent");
                states.push(state);
            }
        }
    }
    assert_eq!(states.first().map(String::as_str), Some("running"));
    assert_eq!(states.last().map(String::as_str), Some("done"));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_goal_may_have_several_runs_and_status_reads_the_current_one() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let (goal, first) = run_on(
        &engine,
        "twice",
        new_workflow("repeatable", vec![agent_step("go", "mock")]),
    );
    finished_run(&engine, goal.id).await;
    let second = engine.start_run(goal.id, BTreeMap::new()).unwrap();
    assert_ne!(first.id, second.id);
    let goal = engine.workspace().get_goal(goal.id).unwrap();
    assert_eq!(goal.runs, vec![first.id, second.id]);
    assert_eq!(goal.run, Some(second.id));
    assert_eq!(engine.workspace().list_runs(goal.id).unwrap().len(), 2);
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.id, second.id);
    assert_eq!(goal.status(Some(&done)), GoalStatus::Done);
    // A goal with no run is a draft; a manual capture with a workflow too.
    let draft = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..SubmitRequest::captured("later")
        })
        .unwrap();
    assert_eq!(draft.status(None), GoalStatus::Draft);
    engine.shutdown().await;
}

// ---------------------------------------------------------------------------
// Gates and loops, through the engine
// ---------------------------------------------------------------------------

fn text_input_def(name: &str) -> bisa_core::InputDef {
    bisa_core::InputDef {
        name: bisa_core::InputName::new(name).unwrap(),
        label: name.to_uppercase(),
        kind: bisa_core::InputKind::Text,
        default: None,
        required: true,
    }
}

fn bool_input_def(name: &str) -> bisa_core::InputDef {
    bisa_core::InputDef {
        kind: bisa_core::InputKind::Bool,
        ..text_input_def(name)
    }
}

fn labelled(step: &mut bisa_core::Step, flows: &[(&str, &str)]) {
    step.then = flows
        .iter()
        .map(|(to, branch)| Flow::branch(sid(to), Branch::new(*branch).unwrap()))
        .collect();
}

/// An `if` step takes `yes` when its condition holds and `no` otherwise —
/// judged at once, the other side skipped.
#[tokio::test(flavor = "multi_thread")]
async fn an_if_step_takes_yes_and_no() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let build = |go: bool| {
        let mut gate = step(
            "gate",
            StepKind::If {
                when: Condition::InputEquals {
                    input: bisa_core::InputName::new("go").unwrap(),
                    value: json!(true),
                },
            },
        );
        labelled(&mut gate, &[("ship", "yes"), ("hold", "no")]);
        let mut draft = new_workflow(
            "gated",
            vec![gate, agent_step("ship", "mock"), agent_step("hold", "mock")],
        );
        draft.inputs = vec![bool_input_def("go")];
        let (goal, _) = goal_on(&engine, if go { "go" } else { "stop" }, draft);
        engine
            .start_run(goal.id, BTreeMap::from([("go".to_string(), json!(go))]))
            .unwrap();
        goal
    };
    let yes = build(true);
    let done = finished_run(&engine, yes.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert_eq!(
        done.steps[&sid("gate")].state,
        StepState::chose(Branch::new("yes").unwrap())
    );
    assert!(matches!(
        done.steps[&sid("ship")].state,
        StepState::Done { .. }
    ));
    assert_eq!(done.steps[&sid("hold")].state, StepState::Skipped);
    let no = build(false);
    let done = finished_run(&engine, no.id).await;
    assert_eq!(done.steps[&sid("ship")].state, StepState::Skipped);
    assert!(matches!(
        done.steps[&sid("hold")].state,
        StepState::Done { .. }
    ));
    engine.shutdown().await;
}

/// A `switch` step renders its subject and takes the matching case, else
/// `otherwise`; its output is the value it matched on.
#[tokio::test(flavor = "multi_thread")]
async fn a_switch_step_matches_a_case() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let mut sw = step(
        "sev",
        StepKind::Switch {
            on: "{inputs.env}".into(),
            cases: vec![bisa_core::Case {
                value: "prod".into(),
                branch: Branch::new("page").unwrap(),
            }],
            otherwise: Branch::new("note").unwrap(),
        },
    );
    labelled(&mut sw, &[("page", "page"), ("note", "note")]);
    let mut draft = new_workflow(
        "switched",
        vec![sw, agent_step("page", "mock"), agent_step("note", "mock")],
    );
    draft.inputs = vec![text_input_def("env")];
    let (goal, _) = goal_on(&engine, "deploy", draft);
    engine
        .start_run(
            goal.id,
            BTreeMap::from([("env".to_string(), json!("prod"))]),
        )
        .unwrap();
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert_eq!(
        done.steps[&sid("sev")].output,
        Some(json!({"value": "prod"}))
    );
    assert!(matches!(
        done.steps[&sid("page")].state,
        StepState::Done { .. }
    ));
    assert_eq!(done.steps[&sid("note")].state, StepState::Skipped);
    engine.shutdown().await;
}

/// A `for_each` step runs its body once per item — the body's last step
/// flowing back — then takes `done`; every iteration is a fresh work item.
#[tokio::test(flavor = "multi_thread")]
async fn a_for_each_walks_three_items() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let mut each = step(
        "each",
        StepKind::ForEach {
            items: "{inputs.items}".into(),
            max_iterations: 10,
        },
    );
    labelled(&mut each, &[("body", "each"), ("after", "done")]);
    let mut body = agent_step("body", "mock");
    body.then = vec![Flow::to(sid("each"))];
    let mut draft = new_workflow("walk", vec![each, body, agent_step("after", "mock")]);
    draft.inputs = vec![text_input_def("items")];
    let (goal, _) = goal_on(&engine, "walk the list", draft);
    engine
        .start_run(
            goal.id,
            BTreeMap::from([("items".to_string(), json!("[\"a\", \"b\", \"c\"]"))]),
        )
        .unwrap();
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert_eq!(
        done.steps[&sid("each")].state,
        StepState::chose(Branch::new("done").unwrap())
    );
    assert_eq!(
        done.steps[&sid("each")].output,
        Some(json!({"index": 3, "count": 3}))
    );
    assert!(
        done.steps[&sid("each")].cursor.is_none(),
        "the cursor is cleared on done"
    );
    assert!(matches!(
        done.steps[&sid("after")].state,
        StepState::Done { .. }
    ));
    let items = items_of(&engine, goal.id);
    assert_eq!(
        items
            .iter()
            .filter(|i| i.step.as_ref() == Some(&sid("body")))
            .count(),
        3,
        "one work item per iteration"
    );
    engine.shutdown().await;
}

/// A `while` step loops while its condition holds — here, until the body
/// says it is done — then exits.
#[tokio::test(flavor = "multi_thread")]
async fn a_while_step_loops_then_exits() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"done": true}))]);
    let mut hold = step(
        "again",
        StepKind::While {
            when: Condition::Not {
                of: Box::new(Condition::OutputEquals {
                    step: sid("body"),
                    path: "done".into(),
                    value: json!(true),
                }),
            },
            max_iterations: 5,
        },
    );
    labelled(&mut hold, &[("body", "loop"), ("after", "done")]);
    let mut body = agent_step_promising("body", "mock", &["done"]);
    body.then = vec![Flow::to(sid("again"))];
    let (goal, _) = run_on(
        &engine,
        "loop once",
        new_workflow("looped", vec![hold, body, agent_step("after", "mock")]),
    );
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert_eq!(
        done.steps[&sid("again")].state,
        StepState::chose(Branch::new("done").unwrap())
    );
    assert_eq!(
        done.steps[&sid("again")].output,
        Some(json!({"index": 1})),
        "one iteration ran"
    );
    assert!(matches!(
        done.steps[&sid("body")].state,
        StepState::Done { .. }
    ));
    assert!(matches!(
        done.steps[&sid("after")].state,
        StepState::Done { .. }
    ));
    engine.shutdown().await;
}

/// A `join: one` target fails when two arms arrive.
#[tokio::test(flavor = "multi_thread")]
async fn a_one_join_fails_on_two_arrivals() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let mut head = agent_step("head", "mock");
    head.then = vec![Flow::to(sid("left")), Flow::to(sid("right"))];
    let mut left = agent_step("left", "mock");
    left.then = vec![Flow::to(sid("one"))];
    let mut right = agent_step("right", "mock");
    right.then = vec![Flow::to(sid("one"))];
    let mut one = agent_step("one", "mock");
    one.join = bisa_core::Join::One;
    let (goal, _) = run_on(
        &engine,
        "both arrive",
        new_workflow("xor", vec![head, left, right, one]),
    );
    let failed = finished_run(&engine, goal.id).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed));
    assert_eq!(failed.steps[&sid("one")].state, StepState::Failed);
    let why = failed.steps[&sid("one")].error.clone().unwrap_or_default();
    assert!(why.contains("exactly one flow may arrive; 2 did"), "{why}");
    engine.shutdown().await;
}
