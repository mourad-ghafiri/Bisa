//! What runs are holding for: armed `wait` steps, and the boundary events
//! of every live step.
//!
//! A `wait` catches one thing — a delay, a moment, a schedule, a named
//! signal, a message, a project's change, a run's end, a platform topic, or a
//! person's release — and its run does nothing until it moves. A **boundary
//! event** listens on a live step (an agent's, a person's, an approval, a
//! wait, a spawn that waits) for a timeout, a reminder's cadence, a message
//! or a signal: a divert stops the step and takes its flows, an act posts or
//! raises a signal beside it. This module is the engine's memory of both:
//! the ticker asks it what is due, the ear ([`crate::listen::ear`]) asks it
//! what an event wakes, a person's release finds it here.
//!
//! The **truth** is the run snapshot — a `Waiting` step and its `WaitFor`, a
//! live step's boundaries and the `fired` counts on its record. This is a
//! projection of it: every wait is armed by its effect and re-armed at boot;
//! the boundaries are synced from the snapshot after every write of the run
//! ([`sync_boundaries`]) — no arm and disarm effects, and the `entered` a
//! fire names makes a boundary of an earlier visit harmless. A reminder
//! re-arms from its `fired` count, never more than its `max`, across
//! restarts.
//!
//! A `spawn` step that waits for its child is armed here too, keyed on the
//! child goal: when the child's run finishes (or the child is closed), the
//! parent step completes with the child's outcome.

use crate::events::{EngineEvent, EnginePayload};
use crate::{ops, Inner};
use bisa_core::{
    json_path, BoundaryOn, Branch, GoalId, GoalOrigin, Heard, Hearer, MessageFilter,
    PlatformFilter, ProjectChange, ProjectFilter, ProjectId, RunEvent, RunFilter, RunId,
    RunOutcome, SignalFilter, SignalSource, StepId, StepKind, StepState, ValueRef, WaitFor,
    WorkflowRun,
};
use dashmap::DashMap;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// What a `wait` step waits for, **resolved against its run**: every template
/// rendered, every input read. Matching an event or computing a due time
/// never sees a placeholder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArmedUntil {
    Delay { secs: u64 },
    Time { at: u64 },
    Schedule { cron: String, tz: Option<String> },
    Signal { filter: SignalFilter },
    Message { filter: MessageFilter },
    Project { filter: ProjectFilter },
    Run { filter: RunFilter },
    Platform { filter: PlatformFilter },
    Release,
}

impl ArmedUntil {
    /// The resolved wait as a definition with fixed values — what a surface
    /// listing armed waits shows.
    pub fn as_wait_for(&self) -> WaitFor {
        match self {
            ArmedUntil::Delay { secs } => WaitFor::Delay {
                secs: ValueRef::Fixed(*secs),
            },
            ArmedUntil::Time { at } => WaitFor::Time { at: at.to_string() },
            ArmedUntil::Schedule { cron, tz } => WaitFor::Schedule {
                cron: ValueRef::Fixed(cron.clone()),
                tz: tz.clone(),
            },
            ArmedUntil::Signal { filter } => WaitFor::Signal {
                filter: filter.clone(),
            },
            ArmedUntil::Message { filter } => WaitFor::Message {
                filter: filter.clone(),
            },
            ArmedUntil::Project { filter } => WaitFor::Project {
                filter: filter.clone(),
            },
            ArmedUntil::Run { filter } => WaitFor::Run {
                filter: filter.clone(),
            },
            ArmedUntil::Platform { filter } => WaitFor::Platform {
                filter: filter.clone(),
            },
            ArmedUntil::Release => WaitFor::Release,
        }
    }

    /// Whether what the ear heard is what this wait holds for.
    fn hears(&self, heard: &Heard) -> bool {
        match self {
            ArmedUntil::Signal { filter } => filter.hears(heard),
            ArmedUntil::Message { filter } => filter.hears(heard),
            ArmedUntil::Project { filter } => filter.hears(heard),
            ArmedUntil::Run { filter } => filter.hears(heard),
            ArmedUntil::Platform { filter } => filter.hears(heard),
            ArmedUntil::Delay { .. }
            | ArmedUntil::Time { .. }
            | ArmedUntil::Schedule { .. }
            | ArmedUntil::Release => false,
        }
    }

    fn source(&self) -> Option<SignalSource> {
        Some(match self {
            ArmedUntil::Signal { .. } => SignalSource::Signal,
            ArmedUntil::Message { .. } => SignalSource::Message,
            ArmedUntil::Project { .. } => SignalSource::Project,
            ArmedUntil::Run { .. } => SignalSource::Run,
            ArmedUntil::Platform { .. } => SignalSource::Platform,
            _ => return None,
        })
    }
}

/// Render a template against a run, as prose.
fn render(inner: &Inner, run: &WorkflowRun, tmpl: &str) -> Result<String, crate::EngineError> {
    crate::effects::render(inner, run, tmpl)
}

/// An input's value, read from the run.
fn input_value<'a>(run: &'a WorkflowRun, input: &str) -> Result<&'a Value, crate::EngineError> {
    run.inputs.get(input).ok_or_else(|| {
        // LCOV_EXCL_START: bound values are judged at the door: Workspace::create_run binds (bind_inputs) and validates (validate_bound) every input before a run exists
        crate::EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-input-has-no-value-run",
            input = input.to_string()
        ))
    })
    // LCOV_EXCL_STOP
}

/// A whole number of seconds, fixed or read from an input.
fn seconds(run: &WorkflowRun, secs: &ValueRef<u64>) -> Result<u64, crate::EngineError> {
    match secs {
        ValueRef::Fixed(n) => Ok(*n),
        ValueRef::Input { input } => {
            let value = input_value(run, input.as_str())?;
            value.as_u64().ok_or_else(|| {
                // LCOV_EXCL_START: bound values are judged at the door: Workspace::create_run binds (bind_inputs) and validates (validate_bound) every input before a run exists
                crate::EngineError::Invalid(bisa_core::text!(
                    "error-engine-invalid-input-should-hold-number-seconds-got",
                    input = input.to_string(),
                    value = value.to_string()
                ))
            })
            // LCOV_EXCL_STOP
        }
    }
}

/// An input read as an assignee, for a message filter.
// LCOV_EXCL_START: bound values are judged at the door: Workspace::create_run binds (bind_inputs) and validates (validate_bound) every input before a run exists
fn assignee_input(
    run: &WorkflowRun,
    input: &bisa_core::InputName,
) -> Result<bisa_core::Assignee, crate::EngineError> {
    let value = input_value(run, input.as_str())?;
    value.as_str().and_then(|s| s.parse().ok()).ok_or_else(|| {
        crate::EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-input-should-hold-assignee-got",
            input = input.to_string(),
            value = value.to_string()
        ))
    })
}

/// An input read as a project id, for a project filter.
fn project_input(
    run: &WorkflowRun,
    input: &bisa_core::InputName,
) -> Result<ProjectId, crate::EngineError> {
    let value = input_value(run, input.as_str())?;
    value.as_str().and_then(|s| s.parse().ok()).ok_or_else(|| {
        crate::EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-input-should-hold-project-id-got",
            input = input.to_string(),
            v = value.to_string()
        ))
    })
}
// LCOV_EXCL_STOP

/// A moment, as a template renders it: Unix seconds, or an RFC 3339 time.
fn moment(text: &str) -> Result<u64, crate::EngineError> {
    let text = text.trim();
    if let Ok(secs) = text.parse::<u64>() {
        return Ok(secs);
    }
    chrono::DateTime::parse_from_rfc3339(text)
        .map(|t| t.timestamp().max(0) as u64)
        .map_err(|_| {
            crate::EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-wait-moment",
                text = text.to_string()
            ))
        })
}

/// Resolve a `wait` definition against its run: templates rendered through
/// the same renderer every other step uses, inputs read by name and checked
/// for the kind the wait needs. A value that cannot be resolved is an error
/// naming what was missing; the caller fails the step with it.
pub(crate) fn resolve(
    inner: &Inner,
    run: &WorkflowRun,
    until: &WaitFor,
) -> Result<ArmedUntil, crate::EngineError> {
    let text = |tmpl: &str| render(inner, run, tmpl);
    Ok(match until {
        WaitFor::Delay { secs } => ArmedUntil::Delay {
            secs: seconds(run, secs)?,
        },
        WaitFor::Time { at } => ArmedUntil::Time {
            at: moment(&text(at)?)?,
        },
        WaitFor::Schedule { cron, tz } => ArmedUntil::Schedule {
            cron: match cron {
                ValueRef::Fixed(s) => s.clone(),
                ValueRef::Input { input } => {
                    let value = input_value(run, input.as_str())?;
                    value
                        .as_str()
                        .ok_or_else(|| {
                            // LCOV_EXCL_START: bound values are judged at the door: Workspace::create_run binds (bind_inputs) and validates (validate_bound) every input before a run exists
                            crate::EngineError::Invalid(bisa_core::text!(
                                "error-engine-invalid-input-should-hold-cron-expression-got",
                                input = input.to_string(),
                                value = value.to_string()
                            ))
                            // LCOV_EXCL_STOP
                        })?
                        .to_string()
                }
            },
            tz: tz.clone(),
        },
        WaitFor::Signal { filter } => ArmedUntil::Signal {
            filter: filter.resolve(&text)?,
        },
        WaitFor::Message { filter } => ArmedUntil::Message {
            filter: filter.resolve(&text, &|name| assignee_input(run, name))?,
        },
        WaitFor::Project { filter } => ArmedUntil::Project {
            filter: filter.resolve(&text, &|name| project_input(run, name))?,
        },
        WaitFor::Run { filter } => ArmedUntil::Run {
            filter: filter.clone(),
        },
        WaitFor::Platform { filter } => ArmedUntil::Platform {
            filter: filter.resolve(&text)?,
        },
        WaitFor::Release => ArmedUntil::Release,
    })
}

#[derive(Clone, Debug)]
struct Armed {
    /// The goal the run is for — `None` for a run of the workspace, which a
    /// goal's event never reaches.
    goal: Option<GoalId>,
    until: ArmedUntil,
    /// Unix seconds a delay, a moment or a schedule comes due.
    due: Option<u64>,
}

/// One boundary event armed on a live step, for the visit that began at
/// `entered`.
#[derive(Clone, Debug)]
struct ArmedBoundary {
    goal: Option<GoalId>,
    workflow: bisa_core::WorkflowId,
    entered: u64,
    /// The event, its seconds read and its filter rendered.
    on: BoundaryOn,
    diverts: bool,
    /// Where it stands among its step's boundaries: the order they fire in
    /// when one tick, or one event, wakes several.
    order: usize,
    /// When a timer next comes due; `None` for a message or a signal.
    due: Option<u64>,
}

impl ArmedBoundary {
    /// Diverts before acts — a divert that ends the step makes the act beside
    /// it stale, never the other way round — and each in declaration order.
    fn precedence(&self) -> (bool, usize) {
        (!self.diverts, self.order)
    }
}

/// What is armed right now, owned by [`crate::Inner`].
#[derive(Default)]
pub struct WaitState {
    armed: DashMap<(RunId, StepId), Armed>,
    /// child goal → (the parent run, the `spawn` step waiting).
    children: DashMap<GoalId, (RunId, StepId)>,
    boundaries: DashMap<(RunId, StepId, Branch), ArmedBoundary>,
}

impl WaitState {
    /// Whether a parent's `spawn` step still waits on `child`.
    pub fn awaits(&self, child: GoalId) -> bool {
        self.children.contains_key(&child)
    }

    /// The armed steps with what each resolved to, for a surface that wants
    /// to show them.
    pub fn armed(&self) -> Vec<(RunId, StepId, WaitFor)> {
        self.armed
            .iter()
            .map(|e| (e.key().0, e.key().1.clone(), e.value().until.as_wait_for()))
            .collect()
    }

    /// The boundary events armed on one step of one run, by name.
    pub fn boundaries_of(&self, run: RunId, step: &StepId) -> Vec<Branch> {
        let mut names: Vec<Branch> = self
            .boundaries
            .iter()
            .filter(|e| e.key().0 == run && &e.key().1 == step)
            .map(|e| e.key().2.clone())
            .collect();
        names.sort();
        names
    }
}

/// Who a run is, as an event's scope admits it.
fn hearer(goal: Option<GoalId>) -> Hearer {
    match goal {
        Some(goal) => Hearer::GoalRun(goal),
        None => Hearer::WorkspaceRun,
    }
}

/// When a wait comes due, from the moment its step was entered.
fn due_at(until: &ArmedUntil, entered_at: u64) -> Option<u64> {
    match until {
        ArmedUntil::Delay { secs } => Some(entered_at.saturating_add(*secs)),
        ArmedUntil::Time { at } => Some(*at),
        ArmedUntil::Schedule { cron, tz } => {
            crate::listen::schedule::cron_next(cron, tz.as_deref(), entered_at).ok()
        }
        _ => None,
    }
}

/// Arm a `wait` step: resolve its definition against the run, then remember
/// it. Called by the effect interpreter when the step is entered, and by
/// [`rearm_run`] on start for every step still `Waiting`. A wait whose
/// values cannot be resolved fails the step with the reason rather than
/// holding the run on nothing.
///
/// **A step has one clock**: its wait counts from the moment the step was
/// entered — as its boundary timers do ([`sync_boundaries`]) and as a re-arm
/// after a restart does — never from the moment the engine got round to
/// arming it. A delay and a timeout of the same length are due together.
pub fn arm(inner: &Arc<Inner>, goal: Option<GoalId>, run: RunId, step: &StepId, until: &WaitFor) {
    match inner.ws.get_run(run) {
        Ok(run) => {
            let entered = entered_at(&run, step).unwrap_or_else(now_secs);
            arm_resolving(inner, goal, &run, step, until, entered);
        }
        Err(e) => {
            crate::effects::fail_step(inner, run, step, format!("the wait cannot be armed: {e}"))
        }
    }
}

/// When a step of `run` was entered, once it was.
fn entered_at(run: &WorkflowRun, step: &StepId) -> Option<u64> {
    run.steps.get(step).and_then(|record| record.started_at)
}

fn arm_resolving(
    inner: &Arc<Inner>,
    goal: Option<GoalId>,
    run: &WorkflowRun,
    step: &StepId,
    until: &WaitFor,
    at: u64,
) {
    match resolve(inner, run, until) {
        Ok(resolved) => arm_at(inner, goal, run.id, step, resolved, at),
        Err(e) => crate::effects::fail_step(
            inner,
            run.id,
            step,
            format!("the wait cannot be armed: {e}"),
        ),
    }
}

fn arm_at(
    inner: &Arc<Inner>,
    goal: Option<GoalId>,
    run: RunId,
    step: &StepId,
    until: ArmedUntil,
    at: u64,
) {
    let due = due_at(&until, at);
    if let ArmedUntil::Schedule { cron, .. } = &until {
        if due.is_none() {
            // A schedule that cannot yield a next time never comes due; say
            // so on the run rather than holding it forever.
            crate::effects::fail_step(
                inner,
                run,
                step,
                format!("schedule {cron:?} has no future occurrence"),
            );
            return;
        }
    }
    inner
        .waits
        .armed
        .insert((run, step.clone()), Armed { goal, until, due });
    if due.is_some() {
        crate::warn_on_err(
            inner.ws.set_wait_due(run, step, due),
            "recording when a wait comes due",
        );
    }
}

/// A `spawn` step of `run` waits for `child`'s run to finish.
pub fn arm_child(inner: &Arc<Inner>, run: RunId, step: &StepId, child: GoalId) {
    inner.waits.children.insert(child, (run, step.clone()));
}

/// Forget one step's wait and its boundaries (it was cancelled, diverted,
/// amended away, or moved).
pub fn disarm(inner: &Arc<Inner>, run: RunId, step: &StepId) {
    inner.waits.armed.remove(&(run, step.clone()));
    inner
        .waits
        .children
        .retain(|_, (r, s)| !(*r == run && s == step));
    inner
        .waits
        .boundaries
        .retain(|(r, s, _), _| !(*r == run && s == step));
}

/// Forget everything a run had armed.
pub fn disarm_run(inner: &Arc<Inner>, run: RunId) {
    inner.waits.armed.retain(|(r, _), _| *r != run);
    inner.waits.children.retain(|_, (r, _)| *r != run);
    inner.waits.boundaries.retain(|(r, _, _), _| *r != run);
}

/// The ticker's question: which delays, moments, schedules and boundary
/// timers are due at `now`? A wait becomes `Elapsed` on its run; a timer
/// fires its boundary for the visit it was armed for.
pub fn tick(inner: &Arc<Inner>, now: u64) {
    let due: Vec<(RunId, StepId)> = inner
        .waits
        .armed
        .iter()
        .filter(|e| e.value().due.is_some_and(|d| d <= now))
        .map(|e| e.key().clone())
        .collect();
    for (run, step) in due {
        inner.waits.armed.remove(&(run, step.clone()));
        crate::warn_on_err(
            inner.ws.set_wait_due(run, &step, None),
            "clearing a wait's due time",
        );
        record(inner, run, RunEvent::Elapsed { step });
    }
    let mut timers: Vec<((RunId, StepId, Branch), ArmedBoundary)> = inner
        .waits
        .boundaries
        .iter()
        .filter(|e| e.value().due.is_some_and(|d| d <= now))
        .map(|e| (e.key().clone(), e.value().clone()))
        .collect();
    timers.sort_by_key(|(_, b)| b.precedence());
    for ((run, step, name), boundary) in timers {
        inner
            .waits
            .boundaries
            .remove(&(run, step.clone(), name.clone()));
        fire_boundary(
            inner,
            run,
            &step,
            &name,
            &boundary,
            json!({ "at": now }),
            bisa_core::Chain::default(),
        );
    }
}

/// Whether any wait or boundary holds for what was heard — the ear's cheap
/// question before it reads a run's chain.
pub(crate) fn hears(inner: &Arc<Inner>, heard: &Heard) -> bool {
    inner
        .waits
        .armed
        .iter()
        .any(|e| e.value().until.hears(heard) && heard.scope.heard_by(hearer(e.value().goal)))
        || inner.waits.boundaries.iter().any(|e| {
            boundary_hears(&e.value().on, heard) && heard.scope.heard_by(hearer(e.value().goal))
        })
}

/// Whether any wait or boundary holds for events of `source`.
pub(crate) fn wants(inner: &Arc<Inner>, source: SignalSource) -> bool {
    inner
        .waits
        .armed
        .iter()
        .any(|e| e.value().until.source() == Some(source))
        || inner.waits.boundaries.iter().any(|e| match &e.value().on {
            BoundaryOn::Message { .. } => source == SignalSource::Message,
            BoundaryOn::Signal { .. } => source == SignalSource::Signal,
            BoundaryOn::After { .. } | BoundaryOn::Every { .. } => false,
        })
}

/// The projects armed waits hold for, and what change of each — what the
/// ticker looks at for them.
pub(crate) fn watched_projects(inner: &Arc<Inner>) -> Vec<(ProjectId, ProjectChange)> {
    inner
        .waits
        .armed
        .iter()
        .filter_map(|e| match &e.value().until {
            ArmedUntil::Project { filter } => filter.project_id().map(|p| (p, filter.change)),
            _ => None,
        })
        .collect()
}

fn boundary_hears(on: &BoundaryOn, heard: &Heard) -> bool {
    match on {
        BoundaryOn::Message { filter } => filter.hears(heard),
        BoundaryOn::Signal { filter } => filter.hears(heard),
        BoundaryOn::After { .. } | BoundaryOn::Every { .. } => false,
    }
}

/// What the ear heard, offered to every run holding for it: a wait that
/// catches it completes with the event as its output — and consumes itself,
/// so an event can never loop through one — then every boundary that hears
/// it fires, diverts before acts, in declaration order. The chain behind the
/// event widens the run's.
pub fn on_heard(inner: &Arc<Inner>, heard: &Heard) {
    let hit: Vec<(RunId, StepId)> = inner
        .waits
        .armed
        .iter()
        .filter(|e| e.value().until.hears(heard) && heard.scope.heard_by(hearer(e.value().goal)))
        .map(|e| e.key().clone())
        .collect();
    for (run, step) in hit {
        inner.waits.armed.remove(&(run, step.clone()));
        record(
            inner,
            run,
            RunEvent::Heard {
                step,
                payload: heard.payload.clone(),
                chain: heard.chain.clone(),
            },
        );
    }
    let mut fired: Vec<((RunId, StepId, Branch), ArmedBoundary)> = inner
        .waits
        .boundaries
        .iter()
        .filter(|e| {
            boundary_hears(&e.value().on, heard) && heard.scope.heard_by(hearer(e.value().goal))
        })
        .map(|e| (e.key().clone(), e.value().clone()))
        .collect();
    fired.sort_by_key(|(_, b)| b.precedence());
    for ((run, step, name), boundary) in fired {
        fire_boundary(
            inner,
            run,
            &step,
            &name,
            &boundary,
            heard.payload.clone(),
            heard.chain.clone(),
        );
    }
}

/// Fire one boundary event through the funnel, for the visit it was armed
/// for, and say so on the bus when the run took it.
fn fire_boundary(
    inner: &Arc<Inner>,
    run: RunId,
    step: &StepId,
    name: &Branch,
    boundary: &ArmedBoundary,
    payload: Value,
    chain: bisa_core::Chain,
) {
    let event = RunEvent::BoundaryFired {
        step: step.clone(),
        boundary: name.clone(),
        entered: boundary.entered,
        payload,
        chain,
    };
    match ops::record_run_event(inner, run, event) {
        Ok(after) => {
            inner.emit(EngineEvent::of_run(
                &after,
                None,
                EnginePayload::BoundaryFired {
                    run,
                    workflow: boundary.workflow,
                    step: step.clone(),
                    boundary: name.clone(),
                    diverts: boundary.diverts,
                },
            ));
        }
        Err(e) if e.is_refusal() => {
            tracing::debug!(run = %run, step = %step, boundary = %name, "a boundary event no longer applies: {e}")
        }
        // LCOV_EXCL_START: a boundary's firing is refused by the run machine in words or written; any other error is the run unwritable (disk-only)
        Err(e) => {
            tracing::warn!(run = %run, step = %step, boundary = %name, "a boundary event could not fire: {e}")
            // LCOV_EXCL_STOP
        }
    }
}

/// Keep a run's armed boundaries equal to its snapshot: every live step's
/// boundaries armed for its current visit — a timer due from the visit's
/// entry and its `fired` count, a reminder never past its `max` — and
/// nothing armed for a step that is no longer live or a visit that is over.
/// Called after every write of the run and at boot.
pub fn sync_boundaries(inner: &Arc<Inner>, run: &WorkflowRun) {
    if run.is_finished() {
        inner.waits.boundaries.retain(|(r, _, _), _| *r != run.id);
        return;
    }
    // What no longer applies goes first.
    inner.waits.boundaries.retain(|(r, s, _), armed| {
        if *r != run.id {
            return true;
        }
        run.steps.get(s).is_some_and(|rec| {
            matches!(rec.state, StepState::Running | StepState::Waiting)
                && rec.entered == armed.entered
        })
    });
    for def in &run.workflow.steps {
        if def.boundaries.is_empty() {
            continue;
        }
        let Some(record) = run.steps.get(&def.id) else {
            continue; // LCOV_EXCL_LINE: every armed boundary belongs to a step of its run
        };
        if !matches!(record.state, StepState::Running | StepState::Waiting) {
            continue;
        }
        let entered_at = record.started_at.unwrap_or_else(now_secs);
        for (order, boundary) in def.boundaries.iter().enumerate() {
            let key = (run.id, def.id.clone(), boundary.name.clone());
            let fired = record
                .fired
                .get(&boundary.name)
                .map(|f| f.count)
                .unwrap_or(0);
            if !boundary.on.may_fire_again(fired) {
                inner.waits.boundaries.remove(&key);
                continue;
            }
            let on = match resolve_boundary(inner, run, &boundary.on) {
                Ok(on) => on,
                // LCOV_EXCL_START: a boundary that resolved when it was armed resolves again at a restart from the same snapshot
                Err(e) => {
                    tracing::warn!(run = %run.id, step = %def.id, boundary = %boundary.name, "a boundary event could not be armed: {e}");
                    continue;
                    // LCOV_EXCL_STOP
                }
            };
            let due = match &on {
                BoundaryOn::After { secs } | BoundaryOn::Every { secs, .. } => {
                    let secs = match secs {
                        ValueRef::Fixed(n) => *n,
                        // LCOV_EXCL_START: resolve_boundary fixed the seconds the line above; the arm keeps the match total
                        ValueRef::Input { .. } => continue,
                        // LCOV_EXCL_STOP
                    };
                    on.next_due(secs, entered_at, fired)
                }
                BoundaryOn::Message { .. } | BoundaryOn::Signal { .. } => None,
            };
            inner.waits.boundaries.insert(
                key,
                ArmedBoundary {
                    goal: run.scope.goal(),
                    workflow: run.workflow.id,
                    entered: record.entered,
                    on,
                    diverts: boundary.diverts(),
                    order,
                    due,
                },
            );
        }
    }
}

/// A boundary's event with its seconds read and its filter rendered.
fn resolve_boundary(
    inner: &Inner,
    run: &WorkflowRun,
    on: &BoundaryOn,
) -> Result<BoundaryOn, crate::EngineError> {
    let text = |tmpl: &str| render(inner, run, tmpl);
    Ok(match on {
        BoundaryOn::After { secs } => BoundaryOn::After {
            secs: ValueRef::Fixed(seconds(run, secs)?),
        },
        BoundaryOn::Every { secs, max } => BoundaryOn::Every {
            secs: ValueRef::Fixed(seconds(run, secs)?),
            max: *max,
        },
        BoundaryOn::Message { filter } => BoundaryOn::Message {
            filter: filter.resolve(&text, &|name| assignee_input(run, name))?,
        },
        BoundaryOn::Signal { filter } => BoundaryOn::Signal {
            filter: filter.resolve(&text)?,
        },
    })
}

/// A person — or a call — releases a `release` wait of a run, a goal's or
/// the workspace's; a payload, when given, is the step's output.
pub fn release(
    inner: &Arc<Inner>,
    run: RunId,
    step: &StepId,
    payload: Option<Value>,
) -> Result<(), crate::EngineError> {
    // The run first: a release it refuses — the step is not waiting, the run
    // is over — must leave what is armed, armed.
    ops::record_run_event(
        inner,
        run,
        RunEvent::Released {
            step: step.clone(),
            payload,
        },
    )?;
    inner.waits.armed.remove(&(run, step.clone()));
    Ok(())
}

/// `child`'s run finished (or the child was closed): the parent's `spawn`
/// step completes with the child's outcome.
pub fn child_finished(inner: &Arc<Inner>, child: GoalId, outcome: Option<RunOutcome>) {
    let Some((_, (run, step))) = inner.waits.children.remove(&child) else {
        return;
    };
    record(
        inner,
        run,
        RunEvent::StepDone {
            step,
            output: crate::effects::child_output(child, outcome),
        },
    );
}

/// The wait ticker: a delay, a moment, a schedule and a boundary timer come
/// due on this clock whether or not listening is on — a run's own timers
/// never depend on a person's switch. One iteration that panics is an
/// `error` line, not the end of every timer in the process.
pub async fn run_ticker(inner: Arc<Inner>) {
    let secs = inner.config.wait_tick_secs.max(1);
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(secs));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    interval.tick().await;
    loop {
        interval.tick().await;
        inner.pause.wait_running().await;
        let inner = Arc::clone(&inner);
        crate::survive("wait tick", async move { tick(&inner, now_secs()) }).await;
    }
}

/// Re-arm every `Waiting` `wait` step and every live step's boundaries of
/// one unfinished run, from its snapshot — the truth, never the index. A
/// delay, a moment and a schedule all count from the moment the step was
/// entered: a restart buys none a fresh clock, and a schedule whose
/// occurrence fell in the downtime is due at once, once. A signal wait is
/// re-armed and then asked whether a named signal it holds for was raised
/// while nobody was listening; the first that matches completes it.
pub fn rearm_run(inner: &Arc<Inner>, run: &WorkflowRun) {
    if run.is_finished() {
        return; // LCOV_EXCL_LINE: the recovery walk re-arms live runs alone
    }
    let now = now_secs();
    for def in &run.workflow.steps {
        let StepKind::Wait { until } = &def.kind else {
            continue;
        };
        let Some(record) = run.steps.get(&def.id) else {
            continue; // LCOV_EXCL_LINE: every wait belongs to a step of its run
        };
        if record.state != StepState::Waiting {
            continue;
        }
        let at = record.started_at.unwrap_or(now);
        arm_resolving(inner, run.scope.goal(), run, &def.id, until, at);
        if matches!(until, WaitFor::Signal { .. }) {
            replay_signals(inner, run, &def.id, at);
        }
    }
    sync_boundaries(inner, run);
}

/// How many recent named signals a re-armed signal wait reads back.
const SIGNAL_REPLAY_WINDOW: usize = 500;

/// A signal wait re-armed after a restart: the named signals raised since the
/// step was entered — every emit is recorded once, listener or not — are the
/// ones it would have heard. The first that matches completes it.
fn replay_signals(inner: &Arc<Inner>, run: &WorkflowRun, step: &StepId, since: u64) {
    let key = (run.id, step.clone());
    let Some(armed) = inner.waits.armed.get(&key).map(|a| a.value().clone()) else {
        // LCOV_EXCL_START: replay_signals is called for the signal wait just armed, under the same key
        return;
        // LCOV_EXCL_STOP
    };
    let ArmedUntil::Signal { filter } = &armed.until else {
        // LCOV_EXCL_START: replay_signals is called for the signal wait just armed, under the same key
        return;
        // LCOV_EXCL_STOP
    };
    let Ok(signals) = inner
        .ws
        .named_signals_since(&filter.name, since, SIGNAL_REPLAY_WINDOW)
    else {
        // LCOV_EXCL_START: the signal ledger is read from the index, which fails only unreadable (disk-only)
        return;
        // LCOV_EXCL_STOP
    };
    let hit = signals.into_iter().find(|s| {
        s.scope.heard_by(hearer(run.scope.goal()))
            && filter
                .fields
                .iter()
                .all(|(path, v)| bisa_core::field_matches(json_path(&s.payload, path), v))
    });
    if let Some(signal) = hit {
        tracing::info!(run = %run.id, step = %step, signal = %signal.id, "a signal wait re-armed after a restart hears the signal it missed");
        inner.waits.armed.remove(&key);
        record(
            inner,
            run.id,
            RunEvent::Heard {
                step: step.clone(),
                payload: signal.payload,
                chain: signal.chain,
            },
        );
    }
}

/// Spawn steps waiting on a child, across every goal: the child names where
/// it came from in its origin — a goal's child its parent goal, whose
/// current run's `Waiting` spawn step names nothing, so the pairing is read
/// off the run; a run of the workspace's child the run and the step itself.
pub fn rearm_children(inner: &Arc<Inner>) {
    let Ok(goals) = inner.ws.list_goals(None) else {
        // LCOV_EXCL_START: the goals list is read from the index, which fails only unreadable (disk-only)
        return;
        // LCOV_EXCL_STOP
    };
    for child in goals {
        let (parent_run, named) = match &child.origin {
            GoalOrigin::Spawned { parent } => match inner.ws.get_current_run(*parent) {
                Ok(Some(run)) => (run, None),
                // LCOV_EXCL_START: a spawned child's parent goal keeps its run until the child is closed with it (close_goal)
                _ => continue,
                // LCOV_EXCL_STOP
            },
            GoalOrigin::Run { run, step } => match inner.ws.get_run(*run) {
                Ok(run) => (run, Some(step.clone())),
                // LCOV_EXCL_START: a run of the workspace outlives the children it spawned until they end (effects::child_finished)
                Err(_) => continue,
                // LCOV_EXCL_STOP
            },
            GoalOrigin::Captured => continue,
        };
        if parent_run.is_finished() {
            continue;
        }
        let waiting_spawn = |sid: &StepId| {
            parent_run.steps.get(sid).is_some_and(|rec| {
                rec.state == StepState::Waiting
                    && parent_run
                        .workflow
                        .step(sid)
                        .is_some_and(|s| matches!(s.kind, StepKind::Spawn { .. }))
            })
        };
        let step = match named {
            Some(step) => waiting_spawn(&step).then_some(step),
            None => parent_run
                .workflow
                .steps
                .iter()
                .map(|s| s.id.clone())
                .find(|sid| waiting_spawn(sid)),
        };
        let Some(step) = step else {
            continue;
        };
        let child_done = child.is_closed()
            || child
                .run
                .and_then(|r| inner.ws.get_run(r).ok())
                .is_some_and(|r| r.is_finished());
        if child_done {
            let outcome = child
                .run
                .and_then(|r| inner.ws.get_run(r).ok())
                .and_then(|r| r.outcome);
            record(
                inner,
                parent_run.id,
                RunEvent::StepDone {
                    step,
                    output: crate::effects::child_output(child.id, outcome),
                },
            );
        } else if !inner.waits.children.contains_key(&child.id) {
            inner.waits.children.insert(child.id, (parent_run.id, step));
        }
    }
}

fn record(inner: &Arc<Inner>, run: RunId, event: RunEvent) {
    match ops::record_run_event(inner, run, event) {
        Ok(_) => {}
        Err(e) if e.is_refusal() => {
            // LCOV_EXCL_START: a refusal here is the step having moved on between two settlements of it, a race no test can stage
            tracing::debug!(run = %run, "a wait moved a step that had already moved: {e}")
            // LCOV_EXCL_STOP
        }
        Err(e) => tracing::warn!(run = %run, "a wait could not move its step: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::{Chain, SignalScope};
    use std::collections::BTreeMap;

    fn goal(n: u64) -> GoalId {
        GoalId::from_ulid(ulid::Ulid::from_parts(n, n as u128))
    }

    #[test]
    fn a_delay_is_due_from_when_it_was_armed_and_a_release_never_is() {
        assert_eq!(due_at(&ArmedUntil::Delay { secs: 30 }, 100), Some(130));
        assert_eq!(due_at(&ArmedUntil::Time { at: 500 }, 100), Some(500));
        assert_eq!(due_at(&ArmedUntil::Release, 100), None);
        assert_eq!(
            due_at(
                &ArmedUntil::Signal {
                    filter: SignalFilter {
                        name: "x".into(),
                        fields: BTreeMap::new()
                    }
                },
                100
            ),
            None
        );
        let cron = due_at(
            &ArmedUntil::Schedule {
                cron: "0 9 * * *".into(),
                tz: None,
            },
            1_704_067_200,
        );
        assert_eq!(cron, Some(1_704_067_200 + 9 * 3600));
        assert_eq!(
            due_at(
                &ArmedUntil::Schedule {
                    cron: "not a cron".into(),
                    tz: None
                },
                0
            ),
            None
        );
    }

    #[test]
    fn a_moment_is_unix_seconds_or_rfc_3339() {
        assert_eq!(moment("1704067200").unwrap(), 1_704_067_200);
        assert_eq!(moment(" 2024-01-01T00:00:00Z ").unwrap(), 1_704_067_200);
        assert_eq!(moment("2024-01-01T01:00:00+01:00").unwrap(), 1_704_067_200);
        assert!(moment("next tuesday").is_err());
    }

    #[test]
    fn a_resolved_wait_reads_back_as_a_fixed_definition() {
        assert_eq!(
            ArmedUntil::Delay { secs: 5 }.as_wait_for(),
            WaitFor::Delay {
                secs: ValueRef::Fixed(5)
            }
        );
        assert_eq!(
            ArmedUntil::Schedule {
                cron: "0 9 * * *".into(),
                tz: Some("UTC".into()),
            }
            .as_wait_for(),
            WaitFor::Schedule {
                cron: ValueRef::Fixed("0 9 * * *".into()),
                tz: Some("UTC".into()),
            }
        );
    }

    /// A goal's run hears its goal's events and nobody's in particular; a
    /// run of the workspace never hears a goal's.
    #[test]
    fn a_wait_hears_only_what_its_runs_scope_admits() {
        let wait = ArmedUntil::Signal {
            filter: SignalFilter {
                name: "deploy.finished".into(),
                fields: BTreeMap::from([("env".into(), "prod".into())]),
            },
        };
        let heard = |scope: SignalScope, env: &str| Heard {
            source: SignalSource::Signal,
            name: Some("deploy.finished".into()),
            scope,
            payload: json!({ "env": env }),
            chain: Chain::default(),
        };
        let a = goal(1);
        let of_a = heard(SignalScope::Goal { goal: a }, "prod");
        assert!(wait.hears(&of_a));
        assert!(of_a.scope.heard_by(hearer(Some(a))));
        assert!(!of_a.scope.heard_by(hearer(Some(goal(2)))));
        assert!(!of_a.scope.heard_by(hearer(None)), "not a workspace run");
        assert!(heard(SignalScope::Workspace, "prod")
            .scope
            .heard_by(hearer(None)));
        assert!(!wait.hears(&heard(SignalScope::Workspace, "staging")));
    }

    #[test]
    fn what_a_boundary_hears_and_what_it_never_does() {
        let signal = BoundaryOn::Signal {
            filter: SignalFilter {
                name: "stop.now".into(),
                fields: BTreeMap::new(),
            },
        };
        let heard = Heard {
            source: SignalSource::Signal,
            name: Some("stop.now".into()),
            scope: SignalScope::Workspace,
            payload: json!({}),
            chain: Chain::default(),
        };
        assert!(boundary_hears(&signal, &heard));
        let timer = BoundaryOn::After {
            secs: ValueRef::Fixed(60),
        };
        assert!(!boundary_hears(&timer, &heard), "the clock drives a timer");
    }
}
