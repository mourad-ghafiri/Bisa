//! The one interpreter of [`RunEffect`]s.
//!
//! [`WorkflowRun::apply`] returns *data* — what must happen because the run
//! moved — and this is the only place that data becomes behaviour: a work
//! item launched, a question opened, a check run, a wait armed, a message
//! posted, a child goal spawned, live work cancelled. One `match`, exhaustive,
//! so a new effect is a compile error here rather than a step that silently
//! does half its job.
//!
//! Every outcome of an effect is itself a [`RunEvent`] handed back through
//! [`ops::record_run_event`] — the funnel — so the run never learns anything
//! from this module except through `apply`.

use crate::events::{EngineEvent, EnginePayload};
use crate::{assign, executor, ops, waits, warn_on_err, EngineError, Inner};
use bisa_core::event::JournalPayload;
use bisa_core::workitem::{WorkItemSpec, WorkItemState};
use bisa_core::{
    render_template_in, AgentId, AskKind, Assignee, BoundaryAct, Branch, Budget, CancelCause,
    ChannelId, CheckKind, Gate, Goal, GoalId, GoalOrigin, GoalText, InputKind, MessageBody,
    PauseReason, ProjectId, RenderContext, RunEffect, RunEvent, RunId, RunOutcome, SignalScope,
    StepId, StepKind, StepProject, StepState, TemplateCtx, ValueRef, WorkItemId,
    WorkItemTransition, WorkflowRun, WorkstreamTransition,
};
use bisa_store::{approval_subject, PostOrigin, WorkstreamFilter};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Run every effect the run produced, in order.
///
/// An effect names a step; before acting, the step is re-read from the run
/// as it stands *now*, because an earlier effect in the same batch may have
/// moved it (a check that failed synchronously, an `end` step that cancelled
/// the rest). An effect whose step is no longer where the effect expects it
/// is skipped, not forced.
pub fn run_effects(inner: &Arc<Inner>, run: &WorkflowRun, effects: Vec<RunEffect>) {
    for effect in effects {
        tracing::debug!(run = %run.id, ?effect, "running a run effect");
        let result = match &effect {
            RunEffect::StartAgent { step } => start_agent(inner, run.id, step),
            RunEffect::ResumeAgent { step, work_item } => {
                resume_agent(inner, run.id, step, *work_item)
            }
            RunEffect::Ask { step } => ask(inner, run.id, step),
            RunEffect::OpenGate { step } => open_gate(inner, run.id, step),
            RunEffect::RunCheck { step } => run_check(inner, run.id, step),
            RunEffect::CallConnector { step } => call_connector(inner, run.id, step),
            RunEffect::Judge { step } => judge(inner, run.id, step),
            RunEffect::Arm { step, until } => {
                waits::arm(inner, run.scope.goal(), run.id, step, until);
                Ok(())
            }
            RunEffect::Post { step } => post(inner, run.id, step),
            RunEffect::Emit { step } => emit_step(inner, run.id, step),
            RunEffect::SpawnGoal { step } => spawn_goal(inner, run.id, step),
            RunEffect::BoundaryAct { step, boundary } => {
                boundary_act(inner, run.id, step, boundary);
                Ok(())
            }
            RunEffect::CancelWork { steps } => {
                cancel_work(inner, run, steps);
                Ok(())
            }
            RunEffect::Finished { outcome } => {
                settle(inner, run, End::Outcome(*outcome));
                Ok(())
            }
            RunEffect::Cancelled { cause } => {
                settle(inner, run, End::Cancelled(cause.clone()));
                Ok(())
            }
        };
        if let Err(e) = result {
            // The effect itself could not even be attempted (the run vanished,
            // the store refused). The step is failed with that reason so the
            // run says what happened rather than hanging.
            tracing::warn!(run = %run.id, ?effect, "run effect failed: {e}");
            if let Some(step) = effect_step(&effect) {
                fail_step(inner, run.id, step, e.to_string());
            }
        }
    }
}

fn effect_step(effect: &RunEffect) -> Option<&StepId> {
    match effect {
        RunEffect::StartAgent { step }
        | RunEffect::ResumeAgent { step, .. }
        | RunEffect::Ask { step }
        | RunEffect::OpenGate { step }
        | RunEffect::RunCheck { step }
        | RunEffect::CallConnector { step }
        | RunEffect::Judge { step }
        | RunEffect::Arm { step, .. }
        | RunEffect::Post { step }
        | RunEffect::Emit { step }
        | RunEffect::SpawnGoal { step } => Some(step),
        // An act beside a live step never fails the step it stands beside.
        RunEffect::BoundaryAct { .. }
        | RunEffect::CancelWork { .. }
        | RunEffect::Finished { .. }
        | RunEffect::Cancelled { .. } => None,
    }
}

/// Fail a step through the funnel; a refusal (the step already moved) is
/// logged at debug, because a late failure on a settled step is the ordinary
/// shape of a race, not a fault.
pub(crate) fn fail_step(inner: &Arc<Inner>, run: RunId, step: &StepId, error: String) {
    match ops::record_run_event(
        inner,
        run,
        RunEvent::StepFailed {
            step: step.clone(),
            error,
        },
    ) {
        Ok(_) => {}
        Err(e) if e.is_refusal() => {
            tracing::debug!(run = %run, step = %step, "late failure not recorded: {e}")
        }
        Err(e) => tracing::warn!(run = %run, step = %step, "cannot fail the step: {e}"),
    }
}

/// A step failed and must not be tried again: a connector write the platform
/// may have received. The run machine spends no retry on it.
fn stop_step(inner: &Arc<Inner>, run: RunId, step: &StepId, error: String) {
    match ops::record_run_event(
        inner,
        run,
        RunEvent::StepStopped {
            step: step.clone(),
            error,
        },
    ) {
        Ok(_) => {}
        Err(e) if e.is_refusal() => {
            tracing::debug!(run = %run, step = %step, "late stop not recorded: {e}")
        }
        Err(e) => tracing::warn!(run = %run, step = %step, "cannot stop the step: {e}"),
    }
}

fn done_step(inner: &Arc<Inner>, run: RunId, step: &StepId, output: Value) {
    match ops::record_run_event(
        inner,
        run,
        RunEvent::StepDone {
            step: step.clone(),
            output,
        },
    ) {
        Ok(_) => {}
        Err(e) if e.is_refusal() => {
            tracing::debug!(run = %run, step = %step, "late completion not recorded: {e}")
        }
        Err(e) => tracing::warn!(run = %run, step = %step, "cannot complete the step: {e}"),
    }
}

/// The run as it stands, and the step's definition — or nothing when the
/// step is not in the state the effect expects any more.
fn live_step(
    inner: &Inner,
    run_id: RunId,
    step: &StepId,
    expected: StepState,
) -> Result<Option<(WorkflowRun, bisa_core::Step)>, EngineError> {
    let run = inner.ws.get_run(run_id)?;
    let Some(record) = run.steps.get(step) else {
        return Ok(None);
    };
    if record.state != expected {
        tracing::debug!(
            run = %run_id, step = %step,
            "skipping an effect: the step is {}, not {}", record.state.as_str(), expected.as_str()
        );
        return Ok(None);
    }
    let Some(def) = run.workflow.step(step).cloned() else {
        return Ok(None);
    };
    Ok(Some((run, def)))
}

// ---------------------------------------------------------------------------
// Templates and value references, against the run
// ---------------------------------------------------------------------------

/// Render a step's template against the run. An absent value is an error
/// rather than a placeholder left in text a person will read.
/// Render a step's template against its run, as prose.
pub(crate) fn render(inner: &Inner, run: &WorkflowRun, tmpl: &str) -> Result<String, EngineError> {
    render_in(inner, run, tmpl, RenderContext::Text)
}

/// Render a step's template against its run for one destination. A `check`
/// command renders in [`RenderContext::ShellCommand`], where every substituted
/// value is shell-quoted: a run input or a webhook payload that reads
/// `a; touch marker` is one word to `sh -c`, never a second command.
pub(crate) fn render_in(
    inner: &Inner,
    run: &WorkflowRun,
    tmpl: &str,
    context: RenderContext,
) -> Result<String, EngineError> {
    let goal = goal_of(inner, run)?;
    Ok(render_template_in(
        tmpl,
        &template_ctx(run, goal.as_ref()),
        context,
    )?)
}

/// The goal a run is for, read — `None` for a run of the workspace.
pub(crate) fn goal_of(inner: &Inner, run: &WorkflowRun) -> Result<Option<Goal>, EngineError> {
    match run.scope.goal() {
        Some(goal) => Ok(Some(inner.ws.get_goal(goal)?)),
        None => Ok(None),
    }
}

/// What a run's templates render against: its inputs, its steps' outputs —
/// and its goal's words when it is a goal's run. Never the event that began
/// it: a start's mapping reads the event, and the run reads typed inputs, so
/// a run by hand, a test run and an event's run look the same. A run of the
/// workspace has no goal to read, and a definition that reads one never
/// started there (`ProblemKind::NeedsGoal`).
pub(crate) fn template_ctx<'a>(run: &'a WorkflowRun, goal: Option<&'a Goal>) -> TemplateCtx<'a> {
    TemplateCtx {
        inputs: &run.inputs,
        steps: &run.steps,
        event: None,
        goal: goal.map(|g| GoalText {
            statement: &g.statement,
            title: g.title.as_deref(),
        }),
        params: bisa_core::no_values(),
        account: bisa_core::no_values(),
    }
}

/// Resolve `ValueRef<Assignee>`s: a fixed assignee as written, an input by
/// its value in wire form. An input that holds nothing resolves to nobody.
fn resolve_assignees(
    run: &WorkflowRun,
    refs: &[&ValueRef<Assignee>],
) -> Result<Vec<Assignee>, EngineError> {
    let mut out = Vec::new();
    for r in refs {
        match r {
            ValueRef::Fixed(a) => out.push(a.clone()),
            ValueRef::Input { input } => {
                let Some(value) = run.inputs.get(input.as_str()) else {
                    continue;
                };
                let Some(s) = value.as_str() else {
                    return Err(EngineError::Invalid(bisa_core::text!(
                        "error-engine-invalid-input-should-hold-assignee-got",
                        input = input.to_string(),
                        value = value.to_string()
                    )));
                };
                out.push(s.parse::<Assignee>().map_err(|e| {
                    EngineError::Invalid(bisa_core::text!(
                        "error-engine-invalid-input-not-assignee",
                        input = input.to_string(),
                        e = e.to_string()
                    ))
                })?);
            }
        }
    }
    Ok(out)
}

fn resolve_project(
    run: &WorkflowRun,
    r: Option<&ValueRef<ProjectId>>,
) -> Result<Option<ProjectId>, EngineError> {
    match r {
        None => Ok(None),
        Some(ValueRef::Fixed(p)) => Ok(Some(*p)),
        Some(ValueRef::Input { input }) => match run.inputs.get(input.as_str()) {
            None => Ok(None),
            Some(v) => {
                let Some(s) = v.as_str() else {
                    return Err(EngineError::Invalid(bisa_core::text!(
                        "error-engine-invalid-input-should-hold-project-id-got",
                        input = input.to_string(),
                        v = v.to_string()
                    )));
                };
                Ok(Some(s.parse::<ProjectId>().map_err(|_| {
                    EngineError::Invalid(bisa_core::text!(
                        "error-engine-invalid-input-not-project-id",
                        input = input.to_string(),
                        s = format!("{s:?}")
                    ))
                })?))
            }
        },
    }
}

// ---------------------------------------------------------------------------
// The effects
// ---------------------------------------------------------------------------

/// `StartAgent`: bind a work item to the step and hand it to the executor.
///
/// The item is recorded on the run (`StepStarted { work_item }`) **before**
/// it is launched, so the preflight — which checks that the step names this
/// item — can pass, and so a launch that is refused settles through the same
/// door a session that failed would.
fn start_agent(inner: &Arc<Inner>, run_id: RunId, step: &StepId) -> Result<(), EngineError> {
    let Some((run, def)) = live_step(inner, run_id, step, StepState::Running)? else {
        return Ok(());
    };
    let StepKind::Agent {
        instructions,
        assignee,
        project,
        harness,
        model,
        effort,
        output_schema,
        tier_ceiling,
    } = &def.kind
    else {
        return Ok(());
    };
    let prepared: Result<WorkItemSpec, EngineError> = (|| {
        let instructions = render(inner, &run, instructions)?;
        let assignees = resolve_assignees(&run, &assignee.iter().collect::<Vec<_>>())?;
        ops::check_assignees(inner, &assignees)?;
        // An agent step runs in a project when it has one — on a goal, named
        // and attached, or the goal's only one; in the workspace, the one it
        // names, which nothing attaches — and in its home's scratch folder
        // when it has none. Ambiguity is the typed refusal `?` carries.
        let named = resolve_project(&run, project.as_ref())?;
        let project = match run.scope.goal() {
            Some(goal) => match crate::projects::resolve_for_step(inner, goal, named, step)? {
                StepProject::Named(p) => {
                    ops::check_project_attached(inner, goal, p)?;
                    Some(p)
                }
                StepProject::Single(p) => Some(p),
                StepProject::Scratch | StepProject::Ambiguous { .. } => None,
            },
            None => named,
        };
        // What the step named, and nothing when it named nothing: where an
        // item runs is decided once who takes it is known
        // (`executor::harnesses_for`).
        let harness_candidates = harness.clone();
        Ok(WorkItemSpec {
            id: WorkItemId::from_ulid(ulid::Ulid::from_datetime(SystemTime::now())),
            home: run.home(),
            run: Some(run.id),
            step: Some(step.clone()),
            instructions,
            state: WorkItemState::Open,
            project,
            harness_candidates,
            model: model.clone(),
            effort: *effort,
            output_schema: output_schema.clone(),
            budget: Budget::default(),
            assignees,
            tier_ceiling: *tier_ceiling,
            agent: None,
            spawn_allowlist: vec![],
            depth_budget: 0,
            result_attempts: 0,
            interruptions: 0,
        })
    })();
    let spec = match prepared {
        Ok(spec) => spec,
        Err(e) => {
            fail_step(inner, run_id, step, e.to_string());
            return Ok(());
        }
    };
    inner.ws.put_work_item(&spec)?;
    // The run learns of the item before anything launches. A run that
    // refuses — the step moved under us — leaves no open item behind: the
    // one just written is cancelled, so nothing waits for a launch that
    // never comes and nobody has to sweep it at the next boot.
    if let Err(e) = ops::record_run_event(
        inner,
        run_id,
        RunEvent::StepStarted {
            step: step.clone(),
            work_item: Some(spec.id),
        },
    ) {
        executor::cancel_item(
            inner,
            &spec.home,
            spec.id,
            "the step it was made for did not take it",
        );
        return Err(e);
    }
    if let Err(rejection) =
        executor::launch_step_item(inner, spec.clone(), crate::scheduler::Launch::Fresh)
    {
        item_settled(inner, &spec, Err(rejection.to_string()));
    }
    Ok(())
}

/// Relaunch the work item an `agent` step already has — a restart's answer
/// for a session that died with the last process. The work is in the item's
/// checkout and stays there: `open_workstream_for` reuses the item's
/// workstream, `mark_in_progress` walks the item back from `Blocked`, and the
/// launch goes through the same door a fresh item's does — the preflight, the
/// reservation, the settlement. An item whose result had already landed when
/// the crash came (accepted, its step never told) is settled from the
/// journal's own `Result`, not run again.
fn resume_agent(
    inner: &Arc<Inner>,
    run_id: RunId,
    step: &StepId,
    work_item: WorkItemId,
) -> Result<(), EngineError> {
    let Some((run, _)) = live_step(inner, run_id, step, StepState::Running)? else {
        return Ok(());
    };
    let mut spec = match inner.ws.get_work_item(&run.home(), work_item) {
        Ok(spec) => spec,
        Err(e) => {
            fail_step(
                inner,
                run_id,
                step,
                format!("the work item to resume is gone: {e}"),
            );
            return Ok(());
        }
    };
    match &spec.state {
        WorkItemState::Accepted => {
            if let Some(output) = accepted_result(inner, &spec) {
                tracing::info!(work_item = %spec.id, "a restart found the item's result already accepted; the step takes it");
                done_step(inner, run_id, step, output);
            } else {
                fail_step(
                    inner,
                    run_id,
                    step,
                    "the work item was accepted but its result is not in the journal".into(),
                );
            }
            return Ok(());
        }
        WorkItemState::Cancelled
        | WorkItemState::Rejected { .. }
        | WorkItemState::Review { .. } => {
            fail_step(
                inner,
                run_id,
                step,
                format!("the work item to resume is {}", spec.state.as_str()),
            );
            return Ok(());
        }
        WorkItemState::Open
        | WorkItemState::Claimed { .. }
        | WorkItemState::InProgress { .. }
        | WorkItemState::Blocked { .. } => {}
    }
    spec.interruptions = spec.interruptions.saturating_add(1);
    inner.ws.put_work_item(&spec)?;
    if let Err(rejection) =
        executor::launch_step_item(inner, spec.clone(), crate::scheduler::Launch::Resume)
    {
        item_settled(inner, &spec, Err(rejection.to_string()));
    }
    Ok(())
}

/// The output the journal holds for an accepted item — its last `Result`.
fn accepted_result(inner: &Arc<Inner>, spec: &WorkItemSpec) -> Option<Value> {
    inner
        .ws
        .journal(&spec.home)
        .ok()?
        .into_iter()
        .rev()
        .find_map(|e| match e.payload {
            JournalPayload::Result {
                work_item, output, ..
            } if work_item == spec.id => Some(output),
            _ => None,
        })
}

/// A work item bound to a step settled: its result landed, or it failed. The
/// step follows — but only the item the step is *currently* running: a retry
/// creates a fresh item, and a late settlement of the old one moves nothing.
pub fn item_settled(inner: &Arc<Inner>, spec: &WorkItemSpec, result: Result<Value, String>) {
    let (Some(run_id), Some(step)) = (spec.run, spec.step.as_ref()) else {
        tracing::debug!(work_item = %spec.id, "settled an item bound to no step");
        return;
    };
    let Ok(run) = inner.ws.get_run(run_id) else {
        return;
    };
    let current = run.steps.get(step).and_then(|r| r.work_item);
    if current != Some(spec.id) {
        tracing::debug!(
            work_item = %spec.id, step = %step,
            "the step has moved on to another item; ignoring this settlement"
        );
        return;
    }
    match result {
        Ok(output) => {
            // The item's own record: submitted → accepted, so the board reads
            // the same as the run.
            if matches!(spec.state, WorkItemState::Review { .. }) {
                warn_on_err(
                    inner
                        .ws
                        .transition_work_item(&spec.home, spec.id, &WorkItemTransition::Accept),
                    "accepting a settled work item",
                );
            }
            inner.emit(EngineEvent::of_run(
                &run,
                Some(spec.id),
                EnginePayload::ResultAccepted,
            ));
            done_step(inner, run_id, step, output);
        }
        Err(error) => fail_step(inner, run_id, step, error),
    }
}

/// `Ask`: a `human` step becomes a question in the inbox.
fn ask(inner: &Arc<Inner>, run_id: RunId, step: &StepId) -> Result<(), EngineError> {
    let Some((run, def)) = live_step(inner, run_id, step, StepState::Waiting)? else {
        return Ok(());
    };
    let StepKind::Human {
        prompt,
        options,
        multi,
        ..
    } = &def.kind
    else {
        return Ok(());
    };
    let text = match render(inner, &run, prompt) {
        Ok(t) => t,
        Err(e) => {
            fail_step(inner, run_id, step, e.to_string());
            return Ok(());
        }
    };
    open_step_gate(
        inner,
        &run,
        step,
        Gate::Escalation,
        format!("step:{run_id}/{step}"),
        text,
        AskKind::Answer {
            options: options.clone(),
            multi: *multi,
        },
    );
    Ok(())
}

/// `OpenGate`: an `approval` step needs a signed decision.
fn open_gate(inner: &Arc<Inner>, run_id: RunId, step: &StepId) -> Result<(), EngineError> {
    let Some((run, def)) = live_step(inner, run_id, step, StepState::Waiting)? else {
        return Ok(());
    };
    let StepKind::Approval { prompt } = &def.kind else {
        return Ok(());
    };
    let text = match render(inner, &run, prompt) {
        Ok(t) => t,
        Err(e) => {
            fail_step(inner, run_id, step, e.to_string());
            return Ok(());
        }
    };
    open_step_gate(
        inner,
        &run,
        step,
        Gate::Approval,
        approval_subject(run_id, step),
        text,
        AskKind::Decision,
    );
    Ok(())
}

/// Open the gate for a step, journal the question and announce it. The
/// question is the Workflow Agent's fact — a gate the workflow raises is the
/// workflow's, not the person's — signed the way a guidance fact is.
pub(crate) fn open_step_gate(
    inner: &Arc<Inner>,
    run: &WorkflowRun,
    step: &StepId,
    gate: Gate,
    subject: String,
    question: String,
    expects: AskKind,
) -> String {
    let (gate_id, _rx) = inner.gates.open_for_step(
        run.home(),
        run.id,
        step.clone(),
        gate,
        subject,
        question.clone(),
        expects.clone(),
    );
    let (signer, attestation) = ops::signer_for(&inner.ws, Some(AgentId::WORKFLOW));
    warn_on_err(
        inner.ws.append_journal(
            &run.home(),
            JournalPayload::Question {
                work_item: None,
                gate: gate_id.clone(),
                text: question.clone(),
                expects: expects.clone(),
            },
            &signer,
            attestation,
        ),
        "journaling a step's question",
    );
    if expects.is_answer() {
        inner.emit(EngineEvent::of_run(
            run,
            None,
            EnginePayload::QuestionAsked {
                gate_id: gate_id.clone(),
                text: question.clone(),
                expects,
            },
        ));
    }
    inner.emit(EngineEvent::of_run(
        run,
        None,
        EnginePayload::GateOpened {
            gate_id: gate_id.clone(),
            gate,
            question,
        },
    ));
    gate_id
}

/// What settles a step whose effect unwound: it failed, in the words of why.
fn failed_by(
    inner: &Arc<Inner>,
    run: RunId,
    step: &StepId,
) -> impl FnOnce(String) + Send + 'static {
    let (inner, step) = (Arc::clone(inner), step.clone());
    move |reason| fail_step(&inner, run, &step, reason)
}

/// `RunCheck`: judge, off the caller's thread, under the check timeout.
fn run_check(inner: &Arc<Inner>, run_id: RunId, step: &StepId) -> Result<(), EngineError> {
    let Some((run, def)) = live_step(inner, run_id, step, StepState::Running)? else {
        return Ok(());
    };
    let StepKind::Check { check } = &def.kind else {
        return Ok(());
    };
    let unwound = failed_by(inner, run_id, step);
    let inner_ref = Arc::clone(inner);
    let inner = Arc::clone(inner);
    let step_key = step.clone();
    let step = step.clone();
    let check = check.clone();
    let work = async move {
        let timeout = Duration::from_secs(inner.config.check_timeout_secs.max(1));
        let (passed, evidence) = match &check {
            CheckKind::Command { command } => {
                match render_in(&inner, &run, command, RenderContext::ShellCommand) {
                    Ok(command) => {
                        let cwd = crate::projects::check_cwd(&inner, &run);
                        // The guard judges the command first; what runs has its
                        // placeholders restored, what is journaled never does.
                        let judge = crate::security::Judge::platform(Some(run.home()), Some(&cwd));
                        match crate::security::decide_command(&inner, &command, judge).await {
                            crate::security::CommandOutcome::Refused(reason) => {
                                (false, vec![format!("$ {command} refused: {reason}")])
                            }
                            crate::security::CommandOutcome::Run {
                                command: real,
                                shown,
                            } => {
                                match tokio::time::timeout(
                                    timeout,
                                    run_command_check(&real, &shown, &cwd),
                                )
                                .await
                                {
                                    Ok(result) => result,
                                    Err(_) => (
                                        false,
                                        vec![format!(
                                            "$ {shown} timed out after {}s",
                                            timeout.as_secs()
                                        )],
                                    ),
                                }
                            }
                        }
                    }
                    Err(e) => (false, vec![e.to_string()]),
                }
            }
            CheckKind::Schema { of: None, .. } => (
                false,
                vec![format!("step `{}` checks the output of no step", step)],
            ),
            CheckKind::Schema {
                schema,
                of: Some(of),
            } => match run.steps.get(of).and_then(|r| r.output.as_ref()) {
                Some(output) => {
                    let errors = schema_errors(schema, output);
                    if errors.is_empty() {
                        (true, vec![format!("schema ok for step `{of}`")])
                    } else {
                        (false, errors)
                    }
                }
                None => (
                    false,
                    vec![format!("step `{of}` produced no output to check")],
                ),
            },
        };
        inner.step_tasks.remove(&(run_id, step.clone()));
        if passed {
            done_step(&inner, run_id, &step, json!({ "evidence": evidence }));
        } else {
            fail_step(&inner, run_id, &step, evidence.join("\n"));
        }
    };
    // Registered by run and step, as a connector call is: a run that is
    // ended takes the check with it, shell command and all.
    let key = (run_id, step_key);
    let task = crate::spawn_settling("check", work, unwound);
    inner_ref.step_tasks.insert(key, task.abort_handle());
    Ok(())
}

/// `CallConnector`: call the outside platform, off the caller's thread,
/// under the operation's deadline plus the engine's own overhead budget
/// (`connectors::OVERHEAD`). The answer the operation selects is the step's
/// output; a refusal or a failure fails the step with a reason the redactor
/// has read — and a **write that may already have reached the platform**
/// (a timeout, a lost answer, a 5xx after sending) with no idempotency key
/// is *stopped*: it fails whatever `retries` allowed, because a resend the
/// platform cannot tell from the first would do the thing twice. The task is
/// remembered by run and step so a stopped run aborts it.
fn call_connector(inner: &Arc<Inner>, run_id: RunId, step: &StepId) -> Result<(), EngineError> {
    let Some((run, def)) = live_step(inner, run_id, step, StepState::Running)? else {
        return Ok(());
    };
    if !matches!(def.kind, StepKind::Connector { .. }) {
        return Ok(());
    }
    let inner = Arc::clone(inner);
    let step = step.clone();
    let shape = crate::connectors::write_shape(&inner, &def);
    let deadline = crate::connectors::step_deadline(&inner, &def);
    let key = (run_id, step.clone());
    // Unwound in the middle of a call, the step is settled as any other
    // failure of it would be: a write with no key may have reached the
    // platform, so it stops for a person and is never sent again.
    let unwound = {
        let (inner, step) = (Arc::clone(&inner), step.clone());
        move |reason: String| {
            inner.connector_calls.remove(&(run_id, step.clone()));
            if shape.writes && !shape.keyed {
                stop_step(
                    &inner,
                    run_id,
                    &step,
                    format!("{reason}; {}", bisa_core::run::AMBIGUOUS_WRITE),
                );
            } else {
                fail_step(&inner, run_id, &step, reason);
            }
        }
    };
    let work = {
        let inner = Arc::clone(&inner);
        let step = step.clone();
        async move {
            let result =
                match tokio::time::timeout(deadline, crate::connectors::call(&inner, &run, &def))
                    .await
                {
                    Ok(result) => result,
                    Err(_) => Err(EngineError::Connector(
                        crate::connectors::ConnectorError::Timeout(deadline),
                    )),
                };
            inner.connector_calls.remove(&(run_id, step.clone()));
            match result {
                Ok(output) => done_step(&inner, run_id, &step, output),
                Err(e) => {
                    let reason = crate::connectors::redact_reason(&inner, &e);
                    if shape.stops_on(&e) {
                        // The way out is named with the cause: what the
                        // platform may hold, and the idempotency header that
                        // would let the call be sent again (the same words a
                        // restart uses, `recovery.rs`).
                        stop_step(
                            &inner,
                            run_id,
                            &step,
                            format!("{reason}; {}", bisa_core::run::AMBIGUOUS_WRITE),
                        );
                    } else {
                        fail_step(&inner, run_id, &step, reason);
                    }
                }
            }
        }
    };
    let task = crate::spawn_settling("connector call", work, unwound);
    inner.connector_calls.insert(key, task.abort_handle());
    Ok(())
}

/// The one question a `judge` step asks.
const JUDGE_QUESTION: &str = "branch";

/// Put a `judge` step's question to the Decision-Making Agent and finish the
/// step with what it chose. The step's output is `{choice, confidence, judged}`;
/// the run machine reads the branch off `choice` ([`bisa_core::run::judged_branch`]),
/// so an unsure answer and no answer both finish the step on its `otherwise`
/// — a judge that cannot judge is not a failed step. Only a state or an
/// instruction that cannot be rendered fails it.
fn judge(inner: &Arc<Inner>, run_id: RunId, step: &StepId) -> Result<(), EngineError> {
    let Some((run, def)) = live_step(inner, run_id, step, StepState::Running)? else {
        return Ok(());
    };
    let StepKind::Judge {
        state,
        instructions,
        options,
        min_confidence,
        ..
    } = &def.kind
    else {
        return Ok(());
    };
    let state = render_in(inner, &run, state, RenderContext::Text)?;
    let instructions = render_in(inner, &run, instructions, RenderContext::Text)?;
    let request = bisa_core::DecisionRequest::one(
        state,
        JUDGE_QUESTION,
        bisa_core::DecisionQuestion::choice(
            instructions,
            options
                .iter()
                .map(|o| (o.branch.as_str().to_string(), o.meaning.clone())),
        ),
    );
    let standing = crate::decider::Standing {
        home: Some(run.home()),
        run: Some(run_id),
        step: Some(step.clone()),
        min_confidence: *min_confidence,
        ..Default::default()
    };
    let unwound = failed_by(inner, run_id, step);
    let inner_ref = Arc::clone(inner);
    let inner = Arc::clone(inner);
    let step_key = step.clone();
    let step = step.clone();
    let work = async move {
        let judged = crate::decider::judge(
            &inner,
            bisa_core::DecisionPoint::WorkflowJudge,
            &standing,
            request,
        )
        .await;
        let (choice, confidence) = match &judged {
            crate::decider::Judged::Answered(response) => {
                let answer = response.answer(JUDGE_QUESTION);
                (
                    answer.and_then(|a| a.chosen()).map(str::to_string),
                    answer.map(|a| a.certainty()),
                )
            }
            crate::decider::Judged::Unsure(response) => {
                (None, response.answer(JUDGE_QUESTION).map(|a| a.certainty()))
            }
            crate::decider::Judged::Failed(_) | crate::decider::Judged::Off => (None, None),
        };
        // `choice` is the key the run machine reads (`run::JUDGE_CHOICE`).
        let output = json!({
            "choice": choice,
            "confidence": confidence,
            "judged": choice.is_some(),
        });
        inner.step_tasks.remove(&(run_id, step.clone()));
        done_step(&inner, run_id, &step, output);
    };
    let key = (run_id, step_key);
    let task = crate::spawn_settling("judgement", work, unwound);
    inner_ref.step_tasks.insert(key, task.abort_handle());
    Ok(())
}

/// Abort every `check` and `judge` task of a run that is ended: the run's
/// settle already decided its steps, and a shell command running on would
/// only outlive the reason it ran. Answers how many were aborted.
pub(crate) fn abort_step_tasks(inner: &Inner, run: RunId) -> usize {
    let mut aborted = 0;
    inner.step_tasks.retain(|(r, _), handle| {
        if *r == run {
            handle.abort();
            aborted += 1;
            false
        } else {
            true
        }
    });
    aborted
}

/// Validate `instance` against a JSON Schema, returning every error as text.
pub fn schema_errors(schema: &Value, instance: &Value) -> Vec<String> {
    match jsonschema::validator_for(schema) {
        Ok(validator) => validator
            .iter_errors(instance)
            .map(|e| e.to_string())
            .collect(),
        Err(e) => vec![format!("invalid schema: {e}")],
    }
}

/// Run a shell check in `cwd`. Exit 0 passes; the evidence is the command as
/// `shown` (the redacted text — a placeholder restored into `command` must
/// never reach the journal), its status and — on failure — the tail of what
/// it said.
pub async fn run_command_check(
    command: &str,
    shown: &str,
    cwd: &std::path::Path,
) -> (bool, Vec<String>) {
    // In a group of its own: a check that times out, or whose run is
    // stopped, takes what it started with it — never `sh` alone.
    let mut cmd = tokio::process::Command::new("sh");
    cmd.arg("-c").arg(command).current_dir(cwd);
    match bisa_harness::proc::group_output(cmd).await {
        Ok(out) => {
            let passed = out.status.success();
            let mut ev = vec![format!("$ {shown} -> {}", out.status)];
            if !passed {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let tail: String = stderr.chars().take(500).collect();
                if !tail.trim().is_empty() {
                    ev.push(tail);
                }
            }
            (passed, ev)
        }
        Err(e) => (false, vec![format!("$ {shown} failed to spawn: {e}")]),
    }
}

/// `Post`: a `notify` step speaks into a conversation as an agent — its
/// `author`, or the Workflow Agent — never as the person. A step that names
/// no conversation speaks in its goal's thread, or — a run of the workspace
/// has no goal — in `general`. Mentions wake the addressed agents through the
/// conversation responder; none wakes nobody.
fn post(inner: &Arc<Inner>, run_id: RunId, step: &StepId) -> Result<(), EngineError> {
    let Some((run, def)) = live_step(inner, run_id, step, StepState::Running)? else {
        return Ok(());
    };
    let StepKind::Notify {
        scope,
        template,
        mentions,
        author,
    } = &def.kind
    else {
        return Ok(());
    };
    match post_as(
        inner,
        &run,
        scope.as_deref(),
        template,
        mentions,
        author.as_ref(),
    ) {
        Ok(message) => done_step(inner, run_id, step, json!({ "message": message })),
        Err(e) => fail_step(inner, run_id, step, e.to_string()),
    }
    Ok(())
}

/// One post a run makes — a `notify` step's, or a boundary's act beside a
/// live step: rendered against the run, spoken as the author it names,
/// announced (nothing triages it, and no message start hears it).
fn post_as(
    inner: &Arc<Inner>,
    run: &WorkflowRun,
    scope: Option<&str>,
    template: &str,
    mentions: &[ValueRef<Assignee>],
    author: Option<&ValueRef<Assignee>>,
) -> Result<String, EngineError> {
    let text = render(inner, run, template)?;
    let scope_id = match (scope, run.scope.goal()) {
        (Some(s), _) => render(inner, run, s)?,
        (None, Some(goal)) => goal.to_string(),
        (None, None) => ChannelId::general().to_string(),
    };
    let speaker = notify_author(run, author)?;
    let assignees = resolve_assignees(run, &mentions.iter().collect::<Vec<_>>())?;
    let mut principals = Vec::new();
    for a in &assignees {
        principals.extend(assign::principals(inner, a));
    }
    principals.dedup();
    Ok(inner.ws.post_message(
        &scope_id,
        MessageBody::post(text),
        None,
        &principals,
        &[],
        Some(&speaker),
        // The platform announcing, not a person asking: nothing triages.
        PostOrigin::Announced,
    )?)
}

/// Where a run's signals belong: its goal, or the workspace.
fn run_signal_scope(run: &WorkflowRun) -> SignalScope {
    match run.scope.goal() {
        Some(goal) => SignalScope::Goal { goal },
        None => SignalScope::Workspace,
    }
}

/// A run's named signal — an `emit` step's or an act's: its name and its
/// payload rendered against the run, raised through the one door with the
/// run's chain.
fn raise(
    inner: &Arc<Inner>,
    run: &WorkflowRun,
    signal: &str,
    payload: &std::collections::BTreeMap<String, String>,
    dedupe: &str,
) -> Result<(String, String), EngineError> {
    let name = render(inner, run, signal)?;
    let mut body = serde_json::Map::new();
    for (key, tmpl) in payload {
        body.insert(key.clone(), Value::String(render(inner, run, tmpl)?));
    }
    let emitted = crate::listen::emit::emit(
        inner,
        &name,
        Value::Object(body),
        run_signal_scope(run),
        run.chain.clone(),
        Some(dedupe),
    )?;
    Ok((name, emitted.signal))
}

/// `Emit`: an `emit` step raises its named signal and is done — its output
/// `{ signal }`. The dedupe key `emit:<run>:<step>:<entered>` makes the step
/// run again after a restart one signal, not two; an emit never fails
/// because a listener refused it.
fn emit_step(inner: &Arc<Inner>, run_id: RunId, step: &StepId) -> Result<(), EngineError> {
    let Some((run, def)) = live_step(inner, run_id, step, StepState::Running)? else {
        return Ok(());
    };
    let StepKind::Emit { signal, payload } = &def.kind else {
        return Ok(());
    };
    let entered = run.steps.get(step).map(|r| r.entered).unwrap_or(0);
    let dedupe = format!("emit:{run_id}:{step}:{entered}");
    match raise(inner, &run, signal, payload, &dedupe) {
        Ok((name, id)) => done_step(inner, run_id, step, json!({ "signal": name, "id": id })),
        Err(e) => fail_step(inner, run_id, step, e.to_string()),
    }
    Ok(())
}

/// `BoundaryAct`: a boundary event that does not divert did what it does
/// beside its live step — a post, or a named signal. The step goes on
/// whatever came of it: a post that could not be made is said on the run's
/// home, never a failure of the step it stands beside.
fn boundary_act(inner: &Arc<Inner>, run_id: RunId, step: &StepId, name: &Branch) {
    let Ok(run) = inner.ws.get_run(run_id) else {
        return;
    };
    let Some(boundary) = run
        .workflow
        .step(step)
        .and_then(|d| d.boundary(name))
        .cloned()
    else {
        return;
    };
    let record = run.steps.get(step);
    let entered = record.map(|r| r.entered).unwrap_or(0);
    let count = record
        .and_then(|r| r.fired.get(name))
        .map(|f| f.count)
        .unwrap_or(0);
    let done = match &boundary.act {
        BoundaryAct::Divert => Ok(()),
        BoundaryAct::Notify {
            scope,
            template,
            mentions,
            author,
        } => post_as(
            inner,
            &run,
            scope.as_deref(),
            template,
            mentions,
            author.as_ref(),
        )
        .map(|_| ()),
        BoundaryAct::Emit { signal, payload } => raise(
            inner,
            &run,
            signal,
            payload,
            &format!("boundary:{run_id}:{step}:{entered}:{name}:{count}"),
        )
        .map(|_| ()),
    };
    if let Err(e) = done {
        ops::journal_note_as(
            inner,
            None,
            run.home(),
            format!("step `{step}`: boundary event `{name}` could not act: {e}"),
        );
    }
}

/// Whose message a `notify` is: its `author` when it names an agent, the
/// Workflow Agent — the run's designer — when it names nobody. A person or a
/// team is refused: only an agent speaks for a workflow. Validation refuses a
/// fixed non-agent before a run starts; this is the input-bound case.
fn notify_author(
    run: &WorkflowRun,
    author: Option<&ValueRef<Assignee>>,
) -> Result<AgentId, EngineError> {
    let resolved = resolve_assignees(run, &author.into_iter().collect::<Vec<_>>())?;
    match resolved.first() {
        None => Ok(AgentId::workflow()),
        Some(Assignee::Agent(id)) => AgentId::new(id).map_err(|e| {
            EngineError::Invalid(bisa_core::text!(
                "error-engine-effects-refused",
                detail = e.to_string()
            ))
        }),
        Some(other) => Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-message-s-author-not-agent-only-agent",
            other = other.to_string()
        ))),
    }
}

/// `SpawnGoal`: a `spawn` step creates a goal and, when it names a workflow,
/// starts the child's run at its start by hand (`Begin::RunNow`). On a goal's run the child refines its parent,
/// moves as its parent does and inherits the parent's attached projects in
/// `ops::submit`, so a sub-goal never has to mint a project of its own. A run
/// of the workspace has no goal to refine: its child is a goal of its own,
/// born of the run (`GoalOrigin::Run`) in the workspace's default mode. With
/// `wait`, the step completes when the child's run finishes; without it, at
/// once.
fn spawn_goal(inner: &Arc<Inner>, run_id: RunId, step: &StepId) -> Result<(), EngineError> {
    let expected = {
        let run = inner.ws.get_run(run_id)?;
        match run.steps.get(step).map(|r| r.state.clone()) {
            Some(s @ (StepState::Running | StepState::Waiting)) => s,
            _ => return Ok(()),
        }
    };
    let Some((run, def)) = live_step(inner, run_id, step, expected)? else {
        return Ok(());
    };
    let StepKind::Spawn {
        statement_template,
        workflow,
        assignees,
        inputs,
        wait,
    } = &def.kind
    else {
        return Ok(());
    };
    let outcome: Result<GoalId, EngineError> = (|| {
        let statement = render(inner, &run, statement_template)?;
        let assignees = resolve_assignees(&run, &assignees.iter().collect::<Vec<_>>())?;
        let (mode, origin) = match run.scope.goal() {
            // A child moves as its parent does: an auto goal's sub-goal is
            // adopted and repaired alone too.
            Some(parent) => (
                inner.ws.get_goal(parent)?.mode,
                GoalOrigin::Spawned { parent },
            ),
            None => (
                ops::default_goal_mode(inner),
                GoalOrigin::Run {
                    run: run.id,
                    step: step.clone(),
                },
            ),
        };
        let child = ops::submit(
            inner,
            ops::SubmitRequest {
                statement,
                title: None,
                budget: None,
                mode,
                origin,
                workflow: *workflow,
                inputs: Default::default(),
                start: false,
                assignees,
                tags: Default::default(),
                documents: vec![],
            },
        )?;
        // A spawned child runs now, at its workflow's start by hand — never
        // listens: the step that spawned it is what it was for — with what
        // the step gives it, each word read by the kind the child's input
        // declares.
        if let Some(workflow) = workflow {
            let asked = inner.ws.get_workflow(*workflow)?.inputs;
            let mut given = std::collections::BTreeMap::new();
            for (name, template) in inputs {
                let said = render(inner, &run, template)?;
                let value = asked
                    .iter()
                    .find(|def| def.name.as_str() == name)
                    .map_or_else(|| Value::String(said.clone()), |def| def.kind.read(&said));
                given.insert(name.clone(), value);
            }
            // A goal born of a run of the workspace has no parent to take
            // its projects from: it works where the run that made it says,
            // as the run itself does. A goal's child has its parent's, and
            // what the step names beside them is attached by nobody.
            if run.scope.goal().is_none() {
                ops::attach_given(inner, child.id, &given)?;
            }
            ops::begin_goal(inner, child.id, given, ops::Begin::RunNow)?;
        }
        let owner = inner.ws.owner_keys().clone();
        warn_on_err(
            inner.ws.append_journal(
                &run.home(),
                JournalPayload::Note {
                    text: format!("step `{step}` spawned {}", child.id),
                },
                &owner,
                None,
            ),
            "journaling a spawned child",
        );
        Ok(child.id)
    })();
    match outcome {
        Err(e) => fail_step(inner, run_id, step, e.to_string()),
        Ok(child) => {
            if !*wait {
                done_step(inner, run_id, step, json!({ "child": child.to_string() }));
                return Ok(());
            }
            // The child may already be finished (a workflow of one `end`
            // step); otherwise the parent waits for it.
            let finished = inner
                .ws
                .get_goal(child)
                .ok()
                .and_then(|g| g.run.and_then(|r| inner.ws.get_run(r).ok()))
                .filter(|r| r.is_finished());
            match finished {
                Some(r) => done_step(inner, run_id, step, child_output(child, r.outcome)),
                None => waits::arm_child(inner, run_id, step, child),
            }
        }
    }
    Ok(())
}

/// What a `spawn` step's output looks like once its child finished.
pub(crate) fn child_output(child: GoalId, outcome: Option<RunOutcome>) -> Value {
    json!({
        "child": child.to_string(),
        "outcome": outcome.map(|o| o.as_str()),
    })
}

/// `CancelWork`: the steps were live and are no longer — cancelled, amended
/// away, or diverted by a boundary event. Their work items are cancelled,
/// their questions withdrawn, their waits and boundaries disarmed; a `spawn`
/// stops waiting and its child goes on.
fn cancel_work(inner: &Arc<Inner>, run: &WorkflowRun, steps: &[StepId]) {
    for step in steps {
        if let Some(item) = run.steps.get(step).and_then(|r| r.work_item) {
            executor::cancel_item(inner, &run.home(), item, "the step was cancelled");
        }
        for gate in inner.gates.pending_for_step(run.id, step) {
            inner.gates.withdraw(&gate.id);
        }
        waits::disarm(inner, run.id, step);
    }
}

/// How a run ended: the one settle path reads it.
pub(crate) enum End {
    Outcome(RunOutcome),
    Cancelled(CancelCause),
}

/// `Finished` and `Cancelled`: the run is over. What a started run held is
/// let go — its workstreams close as records, its waits are disarmed; for a
/// goal's run an amendment still held for it is dropped, for a run of the
/// workspace the answers people gave its sessions are forgotten. A run that
/// never started held nothing. The bus hears which end it was. A parent
/// waiting on this goal moves on for an outcome and for a close — never for a
/// stop or a restart, since the child may run again. A goal's failed run asks
/// for its repair; then the goal's next queued run starts, unless this run
/// was restarted: the restart starts its own replacement, ahead of the queue.
/// A run of the workspace is nobody's child and nobody's queue.
fn settle(inner: &Arc<Inner>, run: &WorkflowRun, end: End) {
    let goal = run.scope.goal();
    if run.started_at.is_some() {
        match goal {
            Some(goal) => {
                release_goal_workstreams(inner, goal);
                ops::drop_held_amendments(inner, goal);
            }
            None => {
                release_run_workstreams(inner, run);
                inner.security.forget_home(&run.home());
            }
        }
        waits::disarm_run(inner, run.id);
    }
    match &end {
        End::Outcome(outcome) => {
            inner.emit(EngineEvent::of_run(
                run,
                None,
                EnginePayload::RunFinished {
                    run: run.id,
                    workflow: run.workflow.id,
                    outcome: *outcome,
                },
            ));
            if let Some(goal) = goal {
                waits::child_finished(inner, goal, Some(*outcome));
                if *outcome == RunOutcome::Failed {
                    crate::guided::notify_run_failed(inner, run);
                }
            }
        }
        End::Cancelled(cause) => {
            inner.emit(EngineEvent::of_run(
                run,
                None,
                EnginePayload::RunCancelled {
                    run: run.id,
                    workflow: run.workflow.id,
                    cause: cause.clone(),
                },
            ));
            if let (Some(goal), CancelCause::Closed { .. }) = (goal, cause) {
                waits::child_finished(inner, goal, None);
            }
        }
    }
    // A run a listener began lets the listener's waiting signals go.
    if let Some(key) = run.event.as_ref().and_then(|e| e.listener.as_ref()) {
        crate::listen::dispatch::release_ready(inner, key);
    }
    match goal {
        Some(goal) => {
            // A goal that listens stops on a failed run, and its queued event
            // runs are withdrawn before its queue moves: the repair has a gap.
            if matches!(end, End::Outcome(RunOutcome::Failed)) {
                crate::listen::dispatch::pause_goal(
                    inner,
                    goal,
                    PauseReason::RunFailed { run: run.id },
                );
            }
            if !matches!(end, End::Cancelled(CancelCause::Restarted)) {
                ops::advance_queue(inner, goal);
            }
        }
        None => put_away_old_runs(inner, run),
    }
}

/// A workflow's run history in the workspace is bounded — `workflow.runs.keep`
/// finished runs, the newest — and the bound is applied the moment a run of
/// it ends, so a schedule that starts one daily never grows the workspace
/// without end. The run that just ended is finished and counted: with the
/// bound at one, it is the one kept. A folder that cannot be put away is
/// said in the log and costs nothing else; the next end tries again.
fn put_away_old_runs(inner: &Arc<Inner>, run: &WorkflowRun) {
    let kept = match inner.ws.workspace_runs_kept() {
        Ok(kept) => kept,
        Err(e) => {
            tracing::warn!(run = %run.id, "the run bound could not be read; nothing is put away: {e}");
            return;
        }
    };
    match inner
        .ws
        .forget_finished_workspace_runs_beyond(run.workflow.id, kept)
    {
        Ok(gone) if gone.is_empty() => {}
        Ok(gone) => tracing::info!(
            workflow = %run.workflow.id,
            kept,
            gone = gone.len(),
            "the oldest finished runs beyond the bound were put away"
        ),
        Err(e) => tracing::warn!(
            workflow = %run.workflow.id,
            "an old run could not be put away: {e}"
        ),
    }
}

/// Every open workstream made for the goal is closed as a record. The
/// checkouts stay on disk: an unpushed branch is the only copy of somebody's
/// work, and finishing a run is not permission to delete it. A **copy** — a
/// non-git project's workstream — is the one exception: its patch is the
/// copy of the work, captured when its item settled, and the tree stayed only
/// for the run's later steps to read; it goes with the run.
pub(crate) fn release_goal_workstreams(inner: &Arc<Inner>, goal: GoalId) {
    release_workstreams(
        inner,
        WorkstreamFilter::Goal(goal),
        crate::events::EventScope::of_goal(goal),
    );
}

/// Every open workstream a run of the workspace's items opened is closed as
/// a record — there is no goal holding them — on the same terms.
fn release_run_workstreams(inner: &Arc<Inner>, run: &WorkflowRun) {
    release_workstreams(
        inner,
        WorkstreamFilter::Run(run.id),
        crate::events::EventScope::of_run(run),
    );
}

fn release_workstreams(
    inner: &Arc<Inner>,
    filter: WorkstreamFilter,
    scope: crate::events::EventScope,
) {
    let workstreams = match inner.ws.list_workstreams(filter) {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!(?filter, "cannot list workstreams to release: {e}");
            return;
        }
    };
    for w in workstreams {
        if w.state.is_terminal() {
            continue;
        }
        if matches!(w.kind, bisa_core::WorkstreamKind::Copy) {
            // The tree goes with the record — the clean script reported,
            // never a refusal, since a run ending has nobody to refuse to —
            // in a task of its own: the scripts and the disk are awaited.
            // Nothing stands in the tree when it goes: a session still at
            // work in it — a worker being aborted by the very cancel that
            // ends the run — is stopped and waited for first.
            let inner = Arc::clone(inner);
            tokio::spawn(crate::survive(
                "closing a copy when its run ended",
                async move {
                    let told = crate::sessions::stop_workstream(&inner, w.id);
                    crate::sessions::await_stopped(
                        &inner,
                        crate::sessions::Scope::Workstream(w.id),
                        &std::collections::HashSet::new(),
                        told,
                        inner.config.stop_deadline(),
                    )
                    .await;
                    if let Err(e) = crate::projects::close_workstream_with(
                        &inner,
                        w.id,
                        true,
                        crate::scripts::ScriptPolicy::Report,
                    )
                    .await
                    {
                        tracing::warn!(workstream = %w.id, "closing a copy when its run ended: {e}");
                    }
                },
            ));
            continue;
        }
        match inner
            .ws
            .transition_workstream(w.id, &WorkstreamTransition::Close)
        {
            Ok(closed) => inner.emit(scope.event(
                w.work_item,
                EnginePayload::WorkstreamChanged {
                    workstream: w.id,
                    state: closed.state,
                },
            )),
            Err(e) => warn_on_err::<(), _>(Err(e), "closing a workstream when a run finished"),
        }
    }
}

/// The kind an input value must parse to when it arrives as text — a CLI
/// `--input k=v`, a form field.
pub fn coerce_input(kind: &InputKind, raw: &str) -> Result<Value, String> {
    match kind {
        // Whole text stays a whole number: a delay's seconds read from an
        // input are asked for as `u64`, and `30.0` is not one.
        InputKind::Number => {
            let text = raw.trim();
            if let Ok(n) = text.parse::<i64>() {
                return Ok(Value::Number(n.into()));
            }
            if let Ok(n) = text.parse::<u64>() {
                return Ok(Value::Number(n.into()));
            }
            text.parse::<f64>()
                .ok()
                .and_then(serde_json::Number::from_f64)
                .map(Value::Number)
                .ok_or_else(|| format!("{raw:?} is not a number"))
        }
        InputKind::Bool => match raw.trim() {
            "true" | "yes" | "on" | "1" => Ok(Value::Bool(true)),
            "false" | "no" | "off" | "0" => Ok(Value::Bool(false)),
            other => Err(format!("{other:?} is not a boolean")),
        },
        InputKind::Text
        | InputKind::Choice { .. }
        | InputKind::Assignee
        | InputKind::Project
        | InputKind::Account { .. } => Ok(Value::String(raw.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inputs_coerce_by_kind_and_refuse_nonsense() {
        let three = coerce_input(&InputKind::Number, "3").unwrap();
        assert_eq!(three, json!(3));
        assert_eq!(three.as_u64(), Some(3), "whole text is a whole number");
        assert_eq!(coerce_input(&InputKind::Number, " 30 ").unwrap(), json!(30));
        assert_eq!(coerce_input(&InputKind::Number, "-2").unwrap(), json!(-2));
        assert_eq!(coerce_input(&InputKind::Number, "2.5").unwrap(), json!(2.5));
        assert_eq!(
            coerce_input(&InputKind::Number, "18446744073709551615").unwrap(),
            json!(u64::MAX)
        );
        assert_eq!(coerce_input(&InputKind::Bool, "yes").unwrap(), json!(true));
        assert_eq!(coerce_input(&InputKind::Text, "x").unwrap(), json!("x"));
        assert!(coerce_input(&InputKind::Number, "three").is_err());
        assert!(coerce_input(&InputKind::Bool, "maybe").is_err());
    }

    #[test]
    fn schema_errors_name_every_violation() {
        let schema = json!({"type": "object", "required": ["a", "b"]});
        let errors = schema_errors(&schema, &json!({}));
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(schema_errors(&schema, &json!({"a": 1, "b": 2})).is_empty());
        assert_eq!(
            schema_errors(&json!({"type": "nonsense"}), &json!(1)).len(),
            1
        );
    }

    #[test]
    fn a_childs_output_carries_its_outcome() {
        let id = GoalId::from_ulid(ulid::Ulid::from_parts(1, 1));
        let v = child_output(id, Some(RunOutcome::Done));
        assert_eq!(v["outcome"], json!("done"));
        assert_eq!(v["child"], json!(id.to_string()));
        assert_eq!(child_output(id, None)["outcome"], Value::Null);
    }
}
