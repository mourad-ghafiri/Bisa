//! Engine-side goal and run operations: the single creation path, the run
//! funnel — a goal's runs and the workspace's — proposals and amendments,
//! closing, and gate decisions.
//!
//! Every surface — HTTP, CLI, guided, listening, MCP — produces byte-identical
//! durable effects through the functions here. A goal's work begins at one
//! door, [`begin_goal`]: it listens when its workflow begins on events, and
//! runs by hand otherwise. In particular
//! [`record_run_event`] is **the** funnel: the store applies the event and
//! journals the facts, the bus hears every step that changed, and the effects
//! become behaviour in [`crate::effects`]. Nothing else moves a run.

use crate::events::{EngineEvent, EnginePayload};
use crate::sessions::{self, Scope};
use crate::{effects, guided, waits, warn_on_err, EngineError, Inner};
use bisa_core::event::{JournalEvent, JournalPayload};
use bisa_core::goal::{Budget, Goal};
use bisa_core::{
    AgentId, Answer, ApprovalId, AskKind, Assignee, AttachmentRef, CancelCause, Gate, GoalId,
    GoalMode, GoalOrigin, GoalStatus, GuidancePhase, Home, ListenerHost, Problem, ProjectId,
    RunEffect, RunEntry, RunError, RunEvent, RunId, RunOutcome, RunScope, RunStatus, Signal,
    SignalScope, SignalSource, StepId, StepKind, StepRecord, StepState, Tags, WaitFor, WorkItemId,
    Workflow, WorkflowId, WorkflowOrigin, WorkflowRun, WorkstreamId,
};
use bisa_store::{
    approval_subject, NewGoal, NewWorkflow, StoreError, WorkflowScope, Workspace, WorkstreamFilter,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Outcome of a gate decision, shaped for rendering on any surface (and
/// round-trippable so the CLI can consume the daemon's JSON): the home the
/// gate was on, and where that home stands after it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecideOutcome {
    pub home: Home,
    pub gate: Gate,
    pub approve: bool,
    pub status: HomeStatus,
    /// The public hook secrets an adoption minted when it made the goal
    /// listen — shown here, once.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<crate::listen::turn::HookSecret>,
}

impl DecideOutcome {
    /// This outcome, showing the secrets the decision minted.
    pub(crate) fn showing(mut self, secrets: Vec<crate::listen::turn::HookSecret>) -> Self {
        self.secrets = secrets;
        self
    }
}

/// What deciding a gate did: the signed decision, and the public hook secrets
/// an adoption minted when it made its goal listen.
#[derive(Debug, Clone)]
pub struct Decided {
    pub approval: ApprovalId,
    pub secrets: Vec<crate::listen::turn::HookSecret>,
}

/// Where a home stands: a goal's status, or a run of the workspace's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "of")]
pub enum HomeStatus {
    Goal { status: GoalStatus },
    Run { status: RunStatus },
}

impl HomeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            HomeStatus::Goal { status } => status.as_str(),
            HomeStatus::Run { status } => status.as_str(),
        }
    }

    /// Whether work goes on after the decision: the goal is running, or the
    /// run of the workspace is.
    pub fn is_running(&self) -> bool {
        matches!(
            self,
            HomeStatus::Goal {
                status: GoalStatus::Running
            } | HomeStatus::Run {
                status: RunStatus::Running
            }
        )
    }
}

// ---------------------------------------------------------------------------
// Signing + agent resolution
// ---------------------------------------------------------------------------

/// Resolved signing identity for a session: keys + optional NIP-OA
/// attestation tag.
pub type Signer = (nostr::key::Keys, Option<nostr::event::Tag>);

/// Resolve the signing identity for a session: the Agent's own attested keys
/// when the work runs under an Agent definition, the owner otherwise. An
/// unresolvable agent falls back to owner signing (logged) rather than
/// blocking the write.
pub(crate) fn signer_for(ws: &Workspace, agent: Option<&str>) -> Signer {
    if let Some(agent) = agent {
        match AgentId::new(agent)
            .map_err(EngineError::from)
            .and_then(|id| Ok(ws.signer_for(&id)?))
        {
            Ok(s) => return s,
            Err(e) => {
                tracing::warn!("agent {agent} signer unavailable ({e}); signing as owner");
            }
        }
    }
    (ws.owner_keys().clone(), None)
}

/// Forget a goal: its journal, snapshots, runs, work and what it listened
/// with — and everything in memory that names it: its gates withdrawn, its
/// waits disarmed (a parent waiting on it moves on), its items' marks
/// stopped and dropped, its roster rows forgotten. Safe on its own, whoever
/// calls it; a retirement stops and closes first so the sessions end in
/// order, and finds nothing left to do here but the folder.
pub fn delete_goal(inner: &Arc<Inner>, id: GoalId) -> Result<(), EngineError> {
    let run = inner.ws.get_current_run(id)?;
    let listened = inner
        .ws
        .listening(&ListenerHost::Goal { goal: id })?
        .is_some();
    inner.ws.delete_goal(id)?;
    if listened {
        crate::listen::invalidate(inner);
        crate::listen::turn::announce(inner, ListenerHost::Goal { goal: id }, false);
    }
    for gate in inner.gates.pending_for_goal(id) {
        inner.gates.withdraw(&gate.id);
    }
    if let Some(run) = run {
        waits::disarm_run(inner, run.id);
    }
    waits::child_finished(inner, id, None);
    let home = Home::Goal { goal: id };
    let items: Vec<WorkItemId> = inner
        .inflight
        .iter()
        .filter(|e| e.value().home == home)
        .map(|e| *e.key())
        .collect();
    for item in items {
        crate::executor::stop_item(inner, item);
        inner.inflight.remove(&item);
    }
    inner.active_items.retain(|_, of| *of != home);
    let rows: Vec<crate::registry::LiveRunId> = inner
        .presence
        .snapshot()
        .into_iter()
        .filter(|row| row.goal == Some(id))
        .map(|row| row.id)
        .collect();
    for row in rows {
        inner.presence.forget(inner, row);
    }
    Ok(())
}

/// Append a plain note to a home's journal — a goal's, or a run of the
/// workspace's — signed by `agent` when the note is an agent's and by the
/// owner otherwise.
pub fn add_note(
    inner: &Arc<Inner>,
    home: impl Into<Home>,
    text: String,
    agent: Option<&str>,
) -> Result<(), EngineError> {
    let (signer, attestation) = signer_for(&inner.ws, agent);
    inner.ws.append_journal(
        &home.into(),
        JournalPayload::Note { text },
        &signer,
        attestation,
    )?;
    Ok(())
}

/// Set who carries a goal, after checking every assignee exists and is
/// enabled.
pub fn set_goal_assignees(
    inner: &Arc<Inner>,
    goal: GoalId,
    assignees: Vec<Assignee>,
) -> Result<Goal, EngineError> {
    check_assignees(inner, &assignees)?;
    Ok(inner.ws.set_goal_assignees(goal, assignees)?)
}

/// Rewrite a goal's own record — its title, statement, tags, budget. Its
/// workflow moves through [`set_workflow`], its runs through [`start_run`],
/// and it closes through [`close_goal`].
pub fn update_goal(inner: &Arc<Inner>, goal: Goal) -> Result<Goal, EngineError> {
    Ok(inner.ws.update_goal(goal)?)
}

/// A resolved guided/worker agent definition (subset the engine needs).
#[derive(Debug, Clone)]
pub struct AgentPromptInfo {
    pub system_prompt: String,
    pub harness: Vec<String>,
    /// The agent's model plan; the executor walks it against the health
    /// ledger.
    pub models: bisa_core::ModelPlan,
}

/// Resolve an Agent definition into the pieces the guided driver and the
/// executor need. A disabled agent resolves to nothing: it is not a member of
/// anything and runs nothing.
pub(crate) fn agent_info(ws: &Workspace, agent: &AgentId) -> Option<AgentPromptInfo> {
    let def = ws.get_agent(agent).ok()?;
    if !def.enabled {
        return None;
    }
    Some(AgentPromptInfo {
        system_prompt: def.system_prompt,
        harness: vec![def.harness],
        models: def.models,
    })
}

// ---------------------------------------------------------------------------
// Creation — the single path
// ---------------------------------------------------------------------------

/// Everything a capture may say. `POST /goals`, the CLI, A2A, a `spawn` step
/// and the General Agent's `capture_goal` all build one of these.
#[derive(Debug, Clone)]
pub struct SubmitRequest {
    pub statement: String,
    pub title: Option<String>,
    /// The goal's own budget when the caller decided one — a person's;
    /// `None` takes the workspace default (`budget.default.*`),
    /// and `Some(Budget::default())` is a goal with no ceiling whatever the
    /// default says.
    pub budget: Option<Budget>,
    /// How the goal moves. Without a workflow, an auto or guided goal wakes
    /// the Workflow Agent to design one; a manual goal waits for its person.
    pub mode: GoalMode,
    pub origin: GoalOrigin,
    /// The workflow the goal will run, when the caller already knows it.
    pub workflow: Option<WorkflowId>,
    /// Inputs for the run — or what the goal listens with, when its
    /// workflow begins on events — when `start` is set.
    pub inputs: BTreeMap<String, Value>,
    /// Begin the goal's work at once ([`begin_goal`]): a run, or listening.
    /// Meaningless without `workflow`.
    pub start: bool,
    pub assignees: Vec<Assignee>,
    pub tags: Tags,
    /// Files given as the goal's initial context — uploaded through the
    /// attachment store already, materialised under the goal's `documents/`
    /// before anything runs on it (`documents.rs`).
    pub documents: Vec<AttachmentRef>,
}

impl SubmitRequest {
    /// A person's capture: auto, no workflow yet.
    pub fn captured(statement: impl Into<String>) -> Self {
        Self {
            statement: statement.into(),
            title: None,
            budget: None,
            mode: GoalMode::Auto,
            origin: GoalOrigin::Captured,
            workflow: None,
            inputs: BTreeMap::new(),
            start: false,
            assignees: vec![],
            tags: Tags::default(),
            documents: vec![],
        }
    }
}

/// What a capture made: the goal, and the public hook secrets beginning its
/// work minted — a goal captured with a workflow of events listens at once —
/// to be shown this once.
#[derive(Debug, Clone)]
pub struct Submitted {
    pub goal: Goal,
    pub secrets: Vec<crate::listen::turn::HookSecret>,
}

/// Capture a new goal, for a caller with nobody to show a secret to.
/// [`submit_showing`] is the creation path; this is its goal.
pub fn submit(inner: &Arc<Inner>, req: SubmitRequest) -> Result<Goal, EngineError> {
    submit_showing(inner, req).map(|made| made.goal)
}

/// Capture a new goal. THE creation path.
///
/// A workflow named here is checked to exist before the goal is written; an
/// auto or guided goal without one wakes the Workflow Agent to design it, a
/// manual goal is left for its person's designer; `start` begins the goal's
/// work with `inputs` at once — a run, or listening when the workflow begins
/// on events ([`begin_goal`]), whose public hook secrets are answered with
/// the goal.
pub fn submit_showing(inner: &Arc<Inner>, req: SubmitRequest) -> Result<Submitted, EngineError> {
    // Everything the goal needs is refused *before* the first write — the
    // assignees, the workflow, the documents' bytes, the inputs against the
    // workflow's contract — so a refused request never leaves a goal behind
    // with no workflow, no documents or no run.
    check_assignees(inner, &req.assignees)?;
    if let Some(wf) = req.workflow {
        let workflow = inner.ws.get_workflow(wf)?;
        if req.start {
            if workflow.is_archived() {
                return Err(EngineError::Invalid(bisa_core::text!(
                    "error-engine-invalid-workflow-archived-unarchive-before-running",
                    a0 = (workflow.name).to_string()
                )));
            }
            // What a workflow of events listens with is checked when it
            // begins; what a run binds is checked here.
            if workflow.event_starts().is_empty() {
                workflow
                    .bind_inputs(req.inputs.clone())
                    .map_err(bisa_store::StoreError::from)?;
            }
        }
    }
    crate::documents::check(inner, &req.documents)?;
    let mut goal = inner.ws.create_goal(NewGoal {
        statement: req.statement,
        title: req.title,
        origin: req.origin,
        mode: req.mode,
        assignees: req.assignees,
        tags: req.tags,
    })?;
    // A goal made without a budget of its own takes the workspace's default
    // (`budget.default.*`) — the ceiling a standing goal its events start
    // spends against for a year. A budget the caller decided is kept as
    // given, an empty one included: no ceiling, whatever the default says.
    let budget = budget_or_default(inner, req.budget);
    if !budget.is_unlimited() {
        goal.budget = budget;
        goal = inner.ws.update_goal(goal)?;
    }
    // The workspace's shape changed: the feed hears it before the run does.
    inner.emit(EngineEvent::scoped(
        goal.id,
        None,
        EnginePayload::GoalCreated {
            goal: goal.id,
            origin: goal.origin.clone(),
        },
    ));
    // The context comes before the work: the designer and the first step
    // find the documents in place.
    crate::documents::add_documents(inner, goal.id, &req.documents)?;
    // A child works where its parent works: its attachments are inherited
    // before anything can run on it, so placement resolves there the way it
    // would on the parent, and a sub-goal never mints a project of its own.
    if let GoalOrigin::Spawned { parent } = goal.origin {
        crate::projects::inherit_attachments(inner, parent, goal.id)?;
    }
    let mut secrets = Vec::new();
    match req.workflow {
        Some(wf) => {
            goal = inner.ws.set_goal_workflow(goal.id, Some(wf))?;
            if req.start {
                let begin = |inputs| begin_goal(inner, goal.id, inputs, Begin::Auto);
                secrets = match goal.origin {
                    // Its person's words: where they say the work is done.
                    GoalOrigin::Captured => {
                        let given = req.inputs.clone();
                        begun_by_its_person(inner, goal.id, &given, || begin(req.inputs))?
                    }
                    GoalOrigin::Spawned { .. } | GoalOrigin::Run { .. } => begin(req.inputs)?,
                }
                .secrets();
                goal = inner.ws.get_goal(goal.id)?;
            }
        }
        None => {
            if req.mode.designs() {
                if inner.config.design_enabled {
                    guided::notify_captured(inner, goal.id);
                } else {
                    guided::note_off(inner, goal.id);
                }
            }
        }
    }
    Ok(Submitted { goal, secrets })
}

// ---------------------------------------------------------------------------
// The run funnel
// ---------------------------------------------------------------------------

/// How a goal's work begins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Begin {
    /// A person's start, an adoption, auto adoption, a capture: the goal
    /// listens when its workflow begins on events, and runs by hand
    /// otherwise.
    Auto,
    /// A `spawn`, *Run now*: a run at the manual entry, whatever else the
    /// workflow begins on.
    RunNow,
}

/// What beginning a goal's work did.
#[derive(Clone, Debug)]
pub enum Begun {
    Listening(crate::listen::turn::TurnedOn),
    Run(Box<WorkflowRun>),
}

impl Begun {
    /// The public hook secrets beginning minted — a goal that listens — to
    /// be shown once; none for a run.
    pub fn secrets(self) -> Vec<crate::listen::turn::HookSecret> {
        match self {
            Begun::Listening(turned) => turned.secrets,
            Begun::Run(_) => Vec::new(),
        }
    }
}

/// **The one door a goal's work begins through.** `Auto` arms the goal's
/// event starts when its workflow has any — with `inputs` as what it listens
/// with, or, given none, with what it listened with before: a goal started
/// again after a pause is asked for nothing it was already given — and runs
/// it by hand otherwise; `RunNow` runs at the manual entry. Refusals are the
/// listening door's and the run funnel's.
pub fn begin_goal(
    inner: &Arc<Inner>,
    goal_id: GoalId,
    inputs: BTreeMap<String, Value>,
    begin: Begin,
) -> Result<Begun, EngineError> {
    let goal = inner.ws.get_goal(goal_id)?;
    let listens = match (begin, goal.workflow) {
        (Begin::Auto, Some(wf)) => !inner.ws.get_workflow(wf)?.event_starts().is_empty(),
        _ => false,
    };
    if listens {
        // A start settles an adoption still pending and a design question
        // still open, as a run's start does.
        withdraw_design_gates(inner, goal_id);
        let given = (!inputs.is_empty()).then_some(inputs);
        let turned = crate::listen::turn::listen_again(inner, goal_id, given)?;
        return Ok(Begun::Listening(turned));
    }
    Ok(Begun::Run(Box::new(start_run(
        inner,
        goal_id,
        inputs,
        RunEntry::by_hand(),
        None,
    )?)))
}

/// The adoption and design questions a start settles.
fn withdraw_design_gates(inner: &Arc<Inner>, goal_id: GoalId) {
    for gate in inner.gates.pending_for_goal(goal_id) {
        if gate.subject.starts_with("adopt:") || gate.subject.starts_with("ask_human:") {
            inner.gates.withdraw(&gate.id);
        }
    }
}

/// Make a run of the goal's workflow at `entry`: started at once when the
/// goal has no live run, queued behind it otherwise. `dispatched` names the
/// signal it was made from, once. Refusals are the store's: a closed goal, a
/// workflow with problems, a missing input, an entry that is no start.
pub fn start_run(
    inner: &Arc<Inner>,
    goal_id: GoalId,
    inputs: BTreeMap<String, Value>,
    entry: RunEntry,
    dispatched: Option<String>,
) -> Result<WorkflowRun, EngineError> {
    let goal = inner.ws.get_goal(goal_id)?;
    let Some(workflow) = goal.workflow else {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-goal-has-no-workflow-pick-one-let",
            goal_id = goal_id.to_string()
        )));
    };
    // Before anything is written: an agent step that names no project on a
    // goal with several has nowhere unambiguous to run.
    crate::projects::refuse_ambiguous_steps(inner, goal_id, &inner.ws.get_workflow(workflow)?)?;
    let (run, effects) = inner.ws.create_run(
        RunScope::Goal { goal: goal_id },
        workflow,
        inputs,
        entry,
        dispatched,
    )?;
    // A start settles any adoption still pending — the person chose — and any
    // design question still open: the design is over.
    withdraw_design_gates(inner, goal_id);
    if run.is_queued() {
        let position = inner
            .ws
            .queued_runs(goal_id)?
            .iter()
            .position(|r| r.id == run.id)
            .map_or(1, |i| i + 1);
        inner.emit(EngineEvent::of_run(
            &run,
            None,
            EnginePayload::RunQueued {
                run: run.id,
                workflow,
                position,
            },
        ));
        return Ok(run);
    }
    announce_started(inner, &run, effects);
    Ok(inner.ws.get_run(run.id)?)
}

/// The ceiling a caller decided — a listening workflow's, a person's — kept
/// as given, an empty one included: no ceiling, whatever the default says.
/// None decided takes the workspace's default (`budget.default.*`).
fn budget_or_default(inner: &Arc<Inner>, decided: Option<Budget>) -> Budget {
    decided.unwrap_or_else(|| {
        inner.ws.default_budget().unwrap_or_else(|e| {
            tracing::warn!("the default budget could not be read; the work runs unlimited: {e}");
            Budget::default()
        })
    })
}

/// Start a run of a library workflow **in the workspace** at `entry`: no
/// goal behind it, its own folder, started at once beside any other run of
/// the workflow — a run of the workspace never queues. It spends against
/// `budget`, the ceiling its starter decided (a listening workflow's), else
/// the workspace's default. Refusals are the store's: an archived workflow,
/// a goal's design, a workflow with problems, a missing input, an entry that
/// is no start, and a definition that reads its goal (`{goal.statement}`,
/// `{goal.title}`), which a run of the workspace has none of.
pub fn start_workspace_run(
    inner: &Arc<Inner>,
    workflow: WorkflowId,
    inputs: BTreeMap<String, Value>,
    entry: RunEntry,
    budget: Option<Budget>,
    dispatched: Option<String>,
) -> Result<WorkflowRun, EngineError> {
    let scope = RunScope::Workspace {
        budget: budget_or_default(inner, budget),
    };
    let (run, effects) = inner
        .ws
        .create_run(scope, workflow, inputs, entry, dispatched)?;
    announce_started(inner, &run, effects);
    Ok(inner.ws.get_run(run.id)?)
}

/// What a test run is: a run begun at `start` as if its event had happened
/// with `payload` — the event the mapping reads, marked a test, the inputs
/// it maps laid over `inputs`. The one door a person tries an event start
/// through.
pub fn test_entry(
    wf: &Workflow,
    start: &StepId,
    payload: Value,
    scope: SignalScope,
    mut inputs: BTreeMap<String, Value>,
) -> Result<(RunEntry, BTreeMap<String, Value>), EngineError> {
    let Some(StepKind::Start {
        on,
        inputs: mapping,
        ..
    }) = wf.step(start).map(|s| &s.kind)
    else {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-store-invalid-run-entry-not-a-start",
            step = start.to_string(),
            workflow = wf.name.clone()
        )));
    };
    let signal = Signal {
        id: crate::listen::ear::new_signal_id(),
        listener: None,
        source: SignalSource::Test,
        name: match on {
            bisa_core::StartOn::Signal { filter } => Some(filter.name.clone()),
            _ => None,
        },
        at: now_secs(),
        payload: match payload {
            Value::Object(map) => Value::Object(map),
            Value::Null => serde_json::json!({}),
            other => serde_json::json!({ "value": other }),
        },
        scope,
        chain: bisa_core::Chain::default(),
        dedupe_key: None,
    };
    let event = serde_json::to_value(&signal).map_err(|e| {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-signal-not-json",
            e = e.to_string()
        ))
    })?;
    let mapped = bisa_core::map_event(mapping, &wf.inputs, &event).map_err(|e| {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-test-event-does-not-map",
            e = e.to_string()
        ))
    })?;
    inputs.extend(mapped);
    Ok((RunEntry::at(start.clone(), Some(signal)), inputs))
}

/// A run the store just started: the bus hears it, every step it entered,
/// and its effects run.
fn announce_started(inner: &Arc<Inner>, run: &WorkflowRun, effects: Vec<RunEffect>) {
    inner.emit(EngineEvent::of_run(
        run,
        None,
        EnginePayload::RunStarted {
            run: run.id,
            workflow: run.workflow.id,
        },
    ));
    emit_step_changes(inner, &BTreeMap::new(), run);
    waits::sync_boundaries(inner, run);
    effects::run_effects(inner, run, effects);
}

/// Start the goal's next queued run, when it has one and nothing is live.
/// Called wherever a run ends and at boot; idempotent, and safe to race: the
/// store refuses a start while a run is live, so of two advances one starts
/// the run and the other finds it started. A run withdrawn between the read
/// and the start is skipped for the next. Nothing on a closed goal.
pub(crate) fn advance_queue(inner: &Arc<Inner>, goal_id: GoalId) -> Option<WorkflowRun> {
    loop {
        let goal = match inner.ws.get_goal(goal_id) {
            Ok(goal) => goal,
            Err(e) => {
                tracing::debug!(goal = %goal_id, "the queue cannot advance: {e}");
                return None;
            }
        };
        if goal.is_closed() {
            return None;
        }
        let next = match inner.ws.next_queued_run(goal_id) {
            Ok(next) => next?,
            Err(e) => {
                tracing::warn!(goal = %goal_id, "cannot read the goal's queue: {e}");
                return None;
            }
        };
        match inner.ws.start_queued_run(next.id) {
            Ok((run, effects)) => {
                announce_started(inner, &run, effects);
                return inner.ws.get_run(run.id).ok();
            }
            // Withdrawn under the read: the next in line.
            Err(StoreError::RunNotQueued { .. }) => continue,
            // Another start won the race, or a person started a new run:
            // this queue advances when that run ends.
            Err(StoreError::RunNotFinished { .. }) => return None,
            Err(e) => {
                tracing::warn!(goal = %goal_id, run = %next.id, "cannot start the queued run: {e}");
                return None;
            }
        }
    }
}

/// What a stop did.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Stopped {
    /// The live run it cancelled, when there was one.
    pub run: Option<RunId>,
    /// The queued runs it withdrew, in queue order.
    pub withdrawn: Vec<RunId>,
}

/// The goal's workstreams, for a session stop that must reach the sessions
/// standing in its checkouts.
fn workstreams_of_goal(inner: &Inner, goal: GoalId) -> Result<HashSet<WorkstreamId>, EngineError> {
    Ok(inner
        .ws
        .list_workstreams(WorkstreamFilter::Goal(goal))?
        .into_iter()
        .map(|w| w.id)
        .collect())
}

/// Stop a goal: it stops listening, its sessions are ended, its queued runs
/// withdrawn, its live run cancelled — in that order, so nothing its events
/// start slips in and the live run's settle finds nothing to advance — and
/// the wait for the sessions to be gone is bounded. The goal stays open and
/// reads `draft`, ready for a new run. A goal with nothing live, nothing
/// queued and nothing heard is left as it is.
pub async fn stop_goal(
    inner: &Arc<Inner>,
    goal_id: GoalId,
    rationale: Option<String>,
) -> Result<Stopped, EngineError> {
    let goal = inner.ws.get_goal(goal_id)?;
    // A closed goal is a state the stop meets, not a malformed ask: a
    // conflict, so a screen greys the verb rather than blaming the request.
    if goal.is_closed() {
        return Err(EngineError::Conflict(bisa_core::text!(
            "error-engine-invalid-goal-closed",
            goal_id = goal_id.to_string()
        )));
    }
    crate::listen::turn::turn_off(inner, ListenerHost::Goal { goal: goal_id })?;
    // The runs first, the sessions after: an aborted session settles its
    // item at once, and a settle that met a live run would fail it — the
    // stop's cancel has to be the fact already written when it lands.
    let mut stopped = Stopped::default();
    for queued in inner.ws.queued_runs(goal_id)? {
        if withdraw_if_queued(inner, queued.id)? {
            stopped.withdrawn.push(queued.id);
        }
    }
    if let Some(live) = inner.ws.live_run(goal_id)? {
        if cancel_unless_over(inner, live.id, CancelCause::Stopped { rationale })? {
            // A call out to a platform stops with the run: nothing waits on
            // its deadline for a step nobody is reading any more.
            crate::connectors::abort_calls(inner, live.id);
            stopped.run = Some(live.id);
        }
    }
    let workstreams = workstreams_of_goal(inner, goal_id)?;
    let ended = sessions::stop_for(inner, Scope::Goal(goal_id), &workstreams);
    sessions::await_stopped(
        inner,
        Scope::Goal(goal_id),
        &workstreams,
        ended,
        sessions::STOP_DEADLINE,
    )
    .await;
    Ok(stopped)
}

/// Cancel a run for `cause`, unless it was over when the cancel reached it
/// ([`over_already`]). Answers whether this cancelled it.
fn cancel_unless_over(
    inner: &Arc<Inner>,
    run_id: RunId,
    cause: CancelCause,
) -> Result<bool, EngineError> {
    match record_run_event(inner, run_id, RunEvent::Cancel { cause }) {
        Ok(_) => Ok(true),
        Err(refused) if over_already(&refused) => Ok(false),
        Err(e) => Err(e),
    }
}

/// Restart a goal: a new run of the last run's workflow with its inputs, at
/// the start it began at and on the event that began it, started at once. A
/// live run is cancelled first (cause *restarted*, so its settle advances
/// nothing) and the queue keeps its place behind the new run. Refused on a
/// closed goal, on one that never ran, and when the start it began at is
/// gone from the workflow as it stands.
pub async fn restart_goal(inner: &Arc<Inner>, goal_id: GoalId) -> Result<WorkflowRun, EngineError> {
    let goal = inner.ws.get_goal(goal_id)?;
    // Both refusals are states of the goal, not faults of the request.
    if goal.is_closed() {
        return Err(EngineError::Conflict(bisa_core::text!(
            "error-engine-invalid-goal-closed",
            goal_id = goal_id.to_string()
        )));
    }
    let Some(latest) = goal.run else {
        return Err(EngineError::Conflict(bisa_core::text!(
            "error-engine-conflict-goal-has-no-run-restart-start-one",
            goal_id = goal_id.to_string()
        )));
    };
    let latest = inner.ws.get_run(latest)?;
    // Before anything is cancelled: a restart that has nowhere to begin must
    // leave the run it would have replaced as it is.
    let entry = restart_entry(inner, &latest)?;
    // The cancel before the sessions end, as `stop_goal` orders it; a run
    // that was over had no session left to end.
    if latest.is_live() && cancel_unless_over(inner, latest.id, CancelCause::Restarted)? {
        let workstreams = workstreams_of_goal(inner, goal_id)?;
        let ended = sessions::stop_for(inner, Scope::Goal(goal_id), &workstreams);
        sessions::await_stopped(
            inner,
            Scope::Goal(goal_id),
            &workstreams,
            ended,
            sessions::STOP_DEADLINE,
        )
        .await;
    }
    if goal.workflow != Some(latest.workflow.id) {
        // The goal moved on to another workflow since: a restart runs the
        // last run's, and the store refuses the swap while anything is queued.
        set_workflow(inner, goal_id, Some(latest.workflow.id))?;
    }
    start_run(inner, goal_id, latest.inputs.clone(), entry, None)
}

/// Where a restart begins: the start the run began at, on the event that
/// began it — never the signal it was made from, which made that run alone.
/// Refused when the start is gone from the workflow as it stands.
fn restart_entry(inner: &Arc<Inner>, run: &WorkflowRun) -> Result<RunEntry, EngineError> {
    if let Some(step) = &run.start {
        let current = inner.ws.get_workflow(run.workflow.id)?;
        if !current.is_entry(step) {
            return Err(EngineError::Conflict(bisa_core::text!(
                "error-engine-conflict-restart-entry-gone",
                step = step.to_string(),
                workflow = current.name.clone()
            )));
        }
    }
    Ok(RunEntry {
        step: run.start.clone(),
        event: run.event.clone(),
    })
}

/// Take a queued run out of the goal's queue. Refused for a run of another
/// goal (not found) and for one that is not queued (a conflict: it is live
/// or over — stop the goal, or nothing to do).
pub fn withdraw_run(
    inner: &Arc<Inner>,
    goal_id: GoalId,
    run_id: RunId,
) -> Result<WorkflowRun, EngineError> {
    let run = inner.ws.get_run(run_id)?;
    if run.scope.goal() != Some(goal_id) {
        return Err(EngineError::Store(StoreError::RunNotFound(
            run_id.to_string(),
        )));
    }
    // No look before the cancel: the run itself refuses a withdrawal once it
    // has started or ended, under its one writer, and that refusal is the
    // answer — a run that starts as it is withdrawn is left going.
    if !withdraw_if_queued(inner, run_id)? {
        let now = inner.ws.get_run(run_id)?;
        return Err(EngineError::Conflict(bisa_core::text!(
            "error-engine-conflict-run-not-queued-only-queued-run-withdrawn",
            run_id = run_id.to_string(),
            a0 = (now.status().as_str()).to_string()
        )));
    }
    Ok(inner.ws.get_run(run_id)?)
}

/// Take a run out of its queue, if it is still in one. Answers whether this
/// withdrew it: a run that started, or ended, before the withdrawal reached
/// it says so itself and is left as it is — never a fault of the caller,
/// who then reads the run as it stands.
pub(crate) fn withdraw_if_queued(inner: &Arc<Inner>, run_id: RunId) -> Result<bool, EngineError> {
    let withdrawn = record_run_event(
        inner,
        run_id,
        RunEvent::Cancel {
            cause: CancelCause::Withdrawn,
        },
    );
    match withdrawn {
        Ok(_) => Ok(true),
        Err(refused) if over_already(&refused) || started_already(&refused) => Ok(false),
        Err(e) => Err(e),
    }
}

/// A run of the workspace by its id — refused, as a conflict naming its
/// goal, for a goal's run: that one is stopped and restarted from its goal,
/// whose queue it belongs to.
fn workspace_run(inner: &Arc<Inner>, run_id: RunId) -> Result<WorkflowRun, EngineError> {
    let run = inner.ws.get_run(run_id)?;
    if let Some(goal) = run.scope.goal() {
        return Err(EngineError::Conflict(bisa_core::text!(
            "error-engine-conflict-run-is-goal-s-act-from-goal",
            run = run_id.to_string(),
            goal = goal.to_string()
        )));
    }
    Ok(run)
}

/// The workstreams a run's items opened, for a session stop that must reach
/// the sessions standing in its checkouts.
fn workstreams_of_run(inner: &Inner, run: RunId) -> Result<HashSet<WorkstreamId>, EngineError> {
    Ok(inner
        .ws
        .list_workstreams(WorkstreamFilter::Run(run))?
        .into_iter()
        .map(|w| w.id)
        .collect())
}

/// Cancel a live run of the workspace for `cause`, then end its sessions:
/// the run first — an aborted session settles its item at once, and a
/// settle that met a live run would fail it — then the sessions, awaited
/// within the bound. Answers the run after and how its sessions settled.
pub(crate) async fn end_workspace_run(
    inner: &Arc<Inner>,
    run_id: RunId,
    cause: CancelCause,
) -> Result<(WorkflowRun, sessions::Settled), EngineError> {
    let run = record_run_event(inner, run_id, RunEvent::Cancel { cause })?;
    // A call out to a platform stops with the run: nothing waits on its
    // deadline for a step nobody is reading any more.
    crate::connectors::abort_calls(inner, run_id);
    let workstreams = workstreams_of_run(inner, run_id)?;
    let ended = sessions::stop_for(inner, Scope::Run(run_id), &workstreams);
    let settled = sessions::await_stopped(
        inner,
        Scope::Run(run_id),
        &workstreams,
        ended,
        sessions::STOP_DEADLINE,
    )
    .await;
    Ok((run, settled))
}

/// Whether a withdrawal was refused because the run had started: it is no
/// longer in a queue, and is somebody's to stop, not to withdraw.
fn started_already(refused: &EngineError) -> bool {
    matches!(
        refused,
        EngineError::Store(StoreError::Run(RunError::AlreadyStarted))
            | EngineError::Run(RunError::AlreadyStarted)
    )
}

/// Whether a cancel was refused because the run was over: it ended by
/// itself, or somebody else ended it, before the cancel reached it. What the
/// caller wanted is so — nothing of the run is going — and it is no fault.
fn over_already(refused: &EngineError) -> bool {
    matches!(
        refused,
        EngineError::Store(StoreError::Run(RunError::Finished))
            | EngineError::Run(RunError::Finished)
    )
}

/// [`end_workspace_run`] for a caller that wants the run over, whoever ends
/// it: a run that was over when the cancel reached it is left as it is.
/// There is no look before the cancel — the run's own refusal is the one
/// answer, so a run that ends between a look and a cancel cannot fail a stop
/// of every run of its workflow half-way through.
async fn end_unless_over(
    inner: &Arc<Inner>,
    run_id: RunId,
    cause: CancelCause,
) -> Result<WorkflowRun, EngineError> {
    match end_workspace_run(inner, run_id, cause).await {
        Ok((run, _)) => Ok(run),
        Err(refused) if over_already(&refused) => Ok(inner.ws.get_run(run_id)?),
        Err(e) => Err(e),
    }
}

/// Stop one run of the workspace: cancelled (cause *stopped*), its sessions
/// ended. A run already over is answered as it is — there is nothing left
/// to stop. A goal's run is refused: stop its goal.
pub async fn stop_run(
    inner: &Arc<Inner>,
    run_id: RunId,
    rationale: Option<String>,
) -> Result<WorkflowRun, EngineError> {
    workspace_run(inner, run_id)?;
    end_unless_over(inner, run_id, CancelCause::Stopped { rationale }).await
}

/// Restart one run of the workspace: a live one is cancelled first (cause
/// *restarted*) and its sessions ended; then a new run of its workflow — the
/// library's revision as it stands now — starts with the same inputs, at the
/// same start, on the same event and under the same ceiling. Refused when the
/// start is gone from the current revision; a goal's run is refused too:
/// restart its goal.
pub async fn restart_run(inner: &Arc<Inner>, run_id: RunId) -> Result<WorkflowRun, EngineError> {
    let run = workspace_run(inner, run_id)?;
    let entry = restart_entry(inner, &run)?;
    end_unless_over(inner, run_id, CancelCause::Restarted).await?;
    start_workspace_run(
        inner,
        run.workflow.id,
        run.inputs.clone(),
        entry,
        run.scope.budget().cloned(),
        None,
    )
}

/// Stop every run of the workspace of the workflow that is going. A goal's
/// run of it is the goal's, and is left alone. Answers the runs stopped.
pub async fn stop_workflow(
    inner: &Arc<Inner>,
    workflow: WorkflowId,
) -> Result<Vec<RunId>, EngineError> {
    inner.ws.get_workflow(workflow)?;
    let mut stopped = Vec::new();
    for run in inner.ws.live_workspace_runs(Some(workflow))? {
        // One that ended by itself since the list was read was not stopped
        // by this, and is not said to have been.
        let after = stop_run(inner, run.id, None).await?;
        if after.cancelled.is_some() {
            stopped.push(run.id);
        }
    }
    Ok(stopped)
}

/// Restart every run of the workspace of the workflow that is going.
/// Answers the new runs.
pub async fn restart_workflow(
    inner: &Arc<Inner>,
    workflow: WorkflowId,
) -> Result<Vec<RunId>, EngineError> {
    inner.ws.get_workflow(workflow)?;
    let mut runs = Vec::new();
    for run in inner.ws.live_workspace_runs(Some(workflow))? {
        runs.push(restart_run(inner, run.id).await?.id);
    }
    Ok(runs)
}

/// **The funnel.** Apply one event to a run, announce every step it moved,
/// and run the effects. Every path that changes a run comes through here.
pub fn record_run_event(
    inner: &Arc<Inner>,
    run_id: RunId,
    event: RunEvent,
) -> Result<WorkflowRun, EngineError> {
    record_run_event_with_effects(inner, run_id, event).map(|(run, _)| run)
}

/// The funnel, answering the effects it ran as well as the run after — for
/// a caller that says what became of a step, like the restart walk.
pub fn record_run_event_with_effects(
    inner: &Arc<Inner>,
    run_id: RunId,
    event: RunEvent,
) -> Result<(WorkflowRun, Vec<RunEffect>), EngineError> {
    let before = inner.ws.get_run(run_id)?;
    let (run, effects) = inner.ws.record_run_event(run_id, event)?;
    emit_step_changes(inner, &before.steps, &run);
    // The boundaries follow the snapshot: what a step's new visit arms, what
    // a step that stopped no longer does.
    waits::sync_boundaries(inner, &run);
    effects::run_effects(inner, &run, effects.clone());
    Ok((inner.ws.get_run(run_id)?, effects))
}

/// Journal a question a gate opened outside a run step — a publish, a
/// guard's escalation, a permission — on the journal of the home it is
/// asked in, so a restart that finds its asker dead can withdraw it by name
/// rather than let it vanish.
pub fn journal_question(
    inner: &Inner,
    home: Home,
    work_item: Option<bisa_core::WorkItemId>,
    subject: &str,
    text: &str,
    expects: AskKind,
) {
    let (signer, attestation) = signer_for(&inner.ws, None);
    crate::warn_on_err(
        inner.ws.append_journal(
            &home,
            JournalPayload::Question {
                work_item,
                gate: subject.to_string(),
                text: text.to_string(),
                expects,
            },
            &signer,
            attestation,
        ),
        "journaling a gate's question",
    );
}

/// One `StepChanged` per record whose state (or visit) differs.
fn emit_step_changes(inner: &Arc<Inner>, before: &BTreeMap<StepId, StepRecord>, run: &WorkflowRun) {
    for (step, record) in &run.steps {
        // A state, a seq, or the work item the step now runs on — a
        // `StepStarted` moves neither of the first two and is still news.
        let changed = match before.get(step) {
            Some(prev) => {
                prev.state != record.state
                    || prev.seq != record.seq
                    || prev.work_item != record.work_item
            }
            None => record.state != StepState::Pending,
        };
        if !changed {
            continue;
        }
        let kind = run
            .workflow
            .step(step)
            .map(|s| s.kind.as_str())
            .unwrap_or("unknown");
        inner.emit(EngineEvent::of_run(
            run,
            record.work_item,
            EnginePayload::StepChanged {
                run: run.id,
                workflow: run.workflow.id,
                step: step.clone(),
                state: record.state.as_str().to_string(),
                kind: kind.to_string(),
            },
        ));
    }
}

/// The goal's current run, when it has one.
pub fn current_run(inner: &Arc<Inner>, goal: GoalId) -> Result<Option<WorkflowRun>, EngineError> {
    Ok(inner.ws.get_current_run(goal)?)
}

// ---------------------------------------------------------------------------
// Workflows: the library, proposals, amendments
// ---------------------------------------------------------------------------

/// Create a local workflow. Refused with every problem when it cannot start.
pub fn create_workflow(inner: &Arc<Inner>, new: NewWorkflow) -> Result<Workflow, EngineError> {
    let wf = inner
        .ws
        .create_workflow(new, bisa_core::WorkflowOrigin::Workspace)?;
    announce_workflow(inner, &wf, false);
    Ok(wf)
}

/// Create a local workflow as a draft — kept with its problems, which come
/// back beside it. The designer's first save of a new canvas.
pub fn create_workflow_draft(
    inner: &Arc<Inner>,
    new: NewWorkflow,
) -> Result<(Workflow, Vec<Problem>), EngineError> {
    let (wf, problems) = inner
        .ws
        .create_workflow_draft(new, bisa_core::WorkflowOrigin::Workspace)?;
    announce_workflow(inner, &wf, false);
    Ok((wf, problems))
}

/// Save an edited workflow as the next revision of `id` **without** refusing
/// its problems — the designer's autosave. The problems come back for
/// display. `expected_revision` is the revision the editor holds; a stored
/// copy that moved past it is a revision conflict and nothing is written.
pub fn save_workflow(
    inner: &Arc<Inner>,
    id: WorkflowId,
    body: NewWorkflow,
    expected_revision: u64,
) -> Result<(Workflow, Vec<Problem>), EngineError> {
    let (wf, problems) = inner.ws.save_workflow_draft(id, body, expected_revision)?;
    announce_workflow(inner, &wf, false);
    Ok((wf, problems))
}

/// The Workflow Agent's write to a library workflow, asked for in the
/// conversation about it: the next revision of `id`, **refused with its
/// problems** — an agent validates first and a half-connected graph is not
/// its to leave — and refused as a revision conflict when the stored copy
/// moved past `expected_revision`, so a person's save in between is never
/// written over unseen. Announced as the agent's hand (`designed`): the
/// designer open beside the conversation adopts the revision or offers the
/// conflict banner, and the workflow's Inbox row learns of it.
pub fn revise_workflow(
    inner: &Arc<Inner>,
    id: WorkflowId,
    body: NewWorkflow,
    expected_revision: u64,
) -> Result<Workflow, EngineError> {
    let wf = inner.ws.update_workflow(id, body, expected_revision)?;
    announce_workflow(inner, &wf, true);
    Ok(wf)
}

/// Forget a workflow. Refused while a goal, another workflow's `spawn` step
/// or `run` start uses it; what it listened with goes with it. The bus
/// hears a deletion, not a change: the designer open on it leaves.
pub fn delete_workflow(inner: &Arc<Inner>, id: WorkflowId) -> Result<(), EngineError> {
    inner.ws.get_workflow(id)?;
    let listened = inner
        .ws
        .listening(&ListenerHost::Workspace { workflow: id })?
        .is_some();
    inner.ws.delete_workflow(id)?;
    if listened {
        crate::listen::invalidate(inner);
        crate::listen::turn::announce(inner, ListenerHost::Workspace { workflow: id }, false);
    }
    inner.emit(EngineEvent::global(EnginePayload::WorkflowDeleted {
        workflow: id,
    }));
    Ok(())
}

/// Put a workflow away, or take it back out: out of the library and the
/// pickers, refused for a goal or a run while it is. The runs that hold a
/// copy are untouched; while one of its runs of the workspace goes, putting
/// it away is refused — retiring it ends them first (`retire`). A workflow
/// put away stops listening, and taking it back out does not turn it on
/// again: a person does. Announced as the mark it is.
pub fn archive_workflow(
    inner: &Arc<Inner>,
    id: WorkflowId,
    archived: bool,
) -> Result<Workflow, EngineError> {
    let wf = inner.ws.set_workflow_archived(id, archived)?;
    if archived {
        crate::listen::turn::turn_off(inner, ListenerHost::Workspace { workflow: id })?;
    }
    inner.emit(EngineEvent::global(EnginePayload::WorkflowArchived {
        workflow: id,
        archived,
    }));
    Ok(wf)
}

/// Every problem a definition has, against the workspace as it stands.
pub fn validate_workflow(inner: &Arc<Inner>, wf: &Workflow) -> Result<Vec<Problem>, EngineError> {
    Ok(inner.ws.validate_workflow(wf)?)
}

/// Copy a goal's design into the library. Refused for a library workflow.
pub fn promote_workflow(inner: &Arc<Inner>, id: WorkflowId) -> Result<Workflow, EngineError> {
    let copy = inner.ws.promote_workflow(id)?;
    announce_workflow(inner, &copy, false);
    Ok(copy)
}

/// The goal, when nothing is running or waiting to run on it: refused while
/// it is closed, while a run is live (*amend it*) or while runs are queued
/// (a queue outranks a repair: the next run starts as the failed one ends).
/// Checked **before** anything is written by a design or a proposal, so a
/// refusal leaves no half-recorded definition behind.
fn refuse_unless_idle(inner: &Arc<Inner>, goal_id: GoalId) -> Result<Goal, EngineError> {
    let goal = inner.ws.get_goal(goal_id)?;
    if goal.is_closed() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-goal-closed",
            goal_id = goal_id.to_string()
        )));
    }
    if inner.ws.goal_is_busy(&goal)? {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-goal-has-live-queued-run-amend-live",
            goal_id = goal_id.to_string()
        )));
    }
    Ok(goal)
}

/// The goal's own design, when the goal points at one.
fn own_design(inner: &Arc<Inner>, goal: &Goal) -> Option<Workflow> {
    let current = inner.ws.get_workflow(goal.workflow?).ok()?;
    (current.origin == WorkflowOrigin::Goal { goal: goal.id }).then_some(current)
}

/// A person's design drawn on the goal's own tab: recorded as the goal's
/// (`WorkflowOrigin::Goal`) **as a draft** — its problems come back rather
/// than refusing it, because a design is drawn over many saves and a start
/// validates again — pointed at, and left for the person to start: their own
/// design needs no adoption gate. Refused while a run is unfinished. When the
/// goal already has a design, `expected_revision` must name the revision the
/// person edited; a design that moved under them is a revision conflict.
pub fn design_workflow(
    inner: &Arc<Inner>,
    goal_id: GoalId,
    draft: NewWorkflow,
    expected_revision: Option<u64>,
) -> Result<(Workflow, Vec<Problem>), EngineError> {
    let goal = refuse_unless_idle(inner, goal_id)?;
    let (wf, problems) = match own_design(inner, &goal) {
        Some(current) => {
            let expected = expected_revision.ok_or_else(|| {
                EngineError::Invalid(bisa_core::text!(
                    "error-engine-invalid-goal-already-has-design-revision-send-revision",
                    goal_id = goal_id.to_string(),
                    a0 = (current.revision).to_string()
                ))
            })?;
            inner.ws.save_workflow_draft(current.id, draft, expected)?
        }
        None => inner
            .ws
            .create_workflow_draft(draft, WorkflowOrigin::Goal { goal: goal_id })?,
    };
    announce_workflow(inner, &wf, false);
    inner.ws.set_goal_workflow(goal_id, Some(wf.id))?;
    // Editing a proposed design makes it the person's own: the
    // agent's adoption gate named the revision they just changed, so it is
    // stale. Withdraw it, as a by-hand `set_workflow` does — the goal then
    // shows the edited steps with an explicit start, and nothing runs on its
    // own.
    for gate in inner.gates.pending_for_goal(goal_id) {
        if gate.subject.starts_with("adopt:") {
            inner.gates.withdraw(&gate.id);
        }
    }
    Ok((wf, problems))
}

/// Say a workflow changed, workspace-wide. `designed` is the Workflow
/// Agent's hand — see [`EnginePayload::WorkflowChanged`].
fn announce_workflow(inner: &Arc<Inner>, wf: &Workflow, designed: bool) {
    inner.emit(EngineEvent::global(EnginePayload::WorkflowChanged {
        workflow: wf.id,
        revision: wf.revision,
        designed,
    }));
}

/// Point a goal at the workflow its next run will use. Refused while a run
/// is unfinished.
pub fn set_workflow(
    inner: &Arc<Inner>,
    goal_id: GoalId,
    workflow: Option<WorkflowId>,
) -> Result<Goal, EngineError> {
    let mut goal = inner.ws.set_goal_workflow(goal_id, workflow)?;
    // Listening follows the goal's workflow: a workflow with no start on an
    // event leaves the goal nothing to hear.
    if goal.listening.is_some() {
        let hears = match workflow {
            Some(id) => !inner.ws.get_workflow(id)?.event_starts().is_empty(),
            None => false,
        };
        if !hears {
            crate::listen::turn::turn_off(inner, ListenerHost::Goal { goal: goal_id })?;
            goal = inner.ws.get_goal(goal_id)?;
        }
    }
    // A choice made by hand settles any adoption still pending.
    for gate in inner.gates.pending_for_goal(goal_id) {
        if gate.subject.starts_with("adopt:") {
            inner.gates.withdraw(&gate.id);
        }
    }
    if let Some(id) = workflow {
        let wf = inner.ws.get_workflow(id)?;
        inner.emit(EngineEvent::scoped(
            goal_id,
            None,
            EnginePayload::WorkflowChanged {
                workflow: id,
                revision: wf.revision,
                designed: false,
            },
        ));
    }
    Ok(goal)
}

/// The Workflow Agent's proposal: the definition is recorded as **this
/// goal's design** (`WorkflowOrigin::Goal`, validated, nothing installed)
/// and the goal points at it. What happens next is the goal's mode:
///
/// - **guided** — an **Adopt** gate opens for the person; the proposer never
///   adopts, and the library never sees a proposal.
/// - **auto** — the platform adopts it: a note says so and the run starts on
///   the inputs' defaults. When it cannot start unattended — a required
///   input without a default, or a goal past its repair budget — the gate
///   opens as in guided, its question saying why a person is needed. No
///   `Decision` is forged: a signed decision is a person's.
/// - **manual** — the design is the person's draft on the Workflow tab, no
///   gate; they edit it and start it.
///
/// Refused while a run is unfinished. A goal already pointing at its own
/// design gets a new revision of it; otherwise a fresh design is made and
/// the goal repointed. Returns the gate when one opened.
pub fn propose_workflow(
    inner: &Arc<Inner>,
    goal_id: GoalId,
    draft: NewWorkflow,
    by: Option<&str>,
) -> Result<(Workflow, Option<String>), EngineError> {
    let goal = refuse_unless_idle(inner, goal_id)?;
    // Which wake this proposal answers — a design, or the repair of a failed
    // run — is what the agent's words in the thread say.
    let phase = inner
        .guided
        .phase_of(goal_id)
        .unwrap_or(GuidancePhase::Design);
    // The proposal is its proposer's in the record: the agent that made it,
    // the owner when a person or no one is named.
    let author = by
        .and_then(|a| AgentId::new(a).ok())
        .and_then(|id| inner.ws.get_agent(&id).ok())
        .map(|agent| agent.pubkey)
        .unwrap_or_else(|| inner.ws.owner_principal());
    let wf = match own_design(inner, &goal) {
        Some(current) => inner
            .ws
            .update_workflow(current.id, draft, current.revision)?,
        None => {
            inner
                .ws
                .create_workflow_as(draft, WorkflowOrigin::Goal { goal: goal_id }, author)?
        }
    };
    announce_workflow(inner, &wf, true);
    inner.ws.set_goal_workflow(goal_id, Some(wf.id))?;
    // A new proposal settles what came before it: an earlier adoption, and a
    // question the designer asked on the way — a question that outlives the
    // proposal would sit in *Your move* doing nothing when answered.
    for gate in inner.gates.pending_for_goal(goal_id) {
        if gate.subject.starts_with("adopt:") || gate.subject.starts_with("ask_human:") {
            inner.gates.withdraw(&gate.id);
        }
    }
    let gate = match goal.mode {
        GoalMode::Guided => Some(open_adopt_gate(inner, &goal, &wf, phase, None, by)?),
        GoalMode::Auto => match adopt_alone(inner, &goal, &wf) {
            // The Decision-Making Agent, when it is on here, reads the workflow
            // against the goal first — off this path, since a proposal never
            // waits on a model. The adoption, or the gate, follows its answer.
            Ok(()) if adoption_is_judged(inner, &wf) => {
                judge_adoption(
                    inner,
                    goal.clone(),
                    wf.clone(),
                    phase,
                    by.map(str::to_string),
                );
                None
            }
            Ok(()) => {
                adopt_now(inner, goal_id, &wf, phase, by)?;
                None
            }
            Err(why) => Some(open_adopt_gate(inner, &goal, &wf, phase, Some(why), by)?),
        },
        GoalMode::Manual => {
            guided::note_proposed(inner, goal_id, &wf, phase, &guided::ProposalFate::Drafted);
            add_note(
                inner,
                goal_id,
                format!(
                    "the Workflow Agent drafted \"{}\" (revision {}) — edit it on the Workflow tab and start it",
                    wf.name, wf.revision
                ),
                by,
            )?;
            None
        }
    };
    inner.emit(EngineEvent::scoped(
        goal_id,
        None,
        EnginePayload::WorkflowProposed {
            workflow: wf.id,
            revision: wf.revision,
            gate_id: gate.clone(),
        },
    ));
    Ok((wf, gate))
}

/// An auto goal adopts `wf` and begins its work, with nobody asked: it runs
/// a design that begins by hand, and listens for the events of one that
/// begins on them.
fn adopt_now(
    inner: &Arc<Inner>,
    goal_id: GoalId,
    wf: &Workflow,
    phase: GuidancePhase,
    by: Option<&str>,
) -> Result<(), EngineError> {
    let fate = if wf.event_starts().is_empty() {
        guided::ProposalFate::Adopted
    } else {
        guided::ProposalFate::Listening
    };
    guided::note_proposed(inner, goal_id, wf, phase, &fate);
    add_note(
        inner,
        goal_id,
        format!(
            "adopted \"{}\" (revision {}) — auto mode, no decision asked",
            wf.name, wf.revision
        ),
        by,
    )?;
    begin_goal(inner, goal_id, BTreeMap::new(), Begin::Auto)?;
    Ok(())
}

/// The one question an adoption asks the Decision-Making Agent, and its rubric.
const ADOPT_QUESTION: &str = "fit";
const ADOPT_LEVELS: [&str; 3] = [
    "it does not reach the goal, or does something the goal did not ask for",
    "it reaches part of the goal, or reaches it with steps that look wrong",
    "it reaches the goal, and every step serves it",
];
/// The score from which an auto goal adopts alone: nearer the top level than
/// the middle one.
const ADOPT_FROM: f64 = 2.5;

fn adoption_standing(wf: &Workflow, goal: GoalId) -> crate::decider::Standing {
    crate::decider::Standing {
        home: Some(Home::Goal { goal }),
        switched_on: wf.decision_making,
        ..Default::default()
    }
}

/// Whether the Decision-Making Agent reads an auto goal's workflow before it
/// is adopted alone: the workspace's switch, or the workflow's own.
fn adoption_is_judged(inner: &Arc<Inner>, wf: &Workflow) -> bool {
    let standing = crate::decider::Standing {
        switched_on: wf.decision_making,
        ..Default::default()
    };
    crate::decider::is_on(inner, bisa_core::DecisionPoint::GoalAdopt, &standing)
}

/// Ask the Decision-Making Agent whether `wf` reaches `goal`, then do what an
/// auto goal does with the answer: adopt alone when it is sure it does, and
/// put the adoption to the person — with the reason — when it is not. No
/// answer is not a reason to stop an auto goal: the rule stands, and it adopts
/// alone.
fn judge_adoption(
    inner: &Arc<Inner>,
    goal: Goal,
    wf: Workflow,
    phase: GuidancePhase,
    by: Option<String>,
) {
    // What the task owes the goal if it unwinds: the proposal is put to the
    // person with what happened. Nothing is adopted alone after a fault, and
    // an auto goal never hangs proposed with nobody left to adopt it.
    let unwound = {
        let (inner, goal, wf, by) = (Arc::clone(inner), goal.clone(), wf.clone(), by.clone());
        move |reason: String| {
            if !still_proposed(&inner, &goal, &wf) {
                return;
            }
            warn_on_err(
                open_adopt_gate(&inner, &goal, &wf, phase, Some(reason), by.as_deref()).map(|_| ()),
                "putting an adoption to a person after its judgement broke",
            );
        }
    };
    let inner = Arc::clone(inner);
    let work = async move {
        let steps: Vec<String> = wf
            .steps
            .iter()
            .map(|s| format!("{} ({}): {}", s.name, s.kind.as_str(), s.summary()))
            .collect();
        let request = bisa_core::DecisionRequest::one(
            serde_json::json!({ "goal": goal.statement, "workflow": wf.name, "steps": steps }),
            ADOPT_QUESTION,
            bisa_core::DecisionQuestion::score(
                "Does running this workflow reach the goal?",
                ADOPT_LEVELS,
            ),
        );
        let judged = crate::decider::judge(
            &inner,
            bisa_core::DecisionPoint::GoalAdopt,
            &adoption_standing(&wf, goal.id),
            request,
        )
        .await;
        if !still_proposed(&inner, &goal, &wf) {
            return;
        }
        let doubt = judged
            .answered()
            .and_then(|r| r.answer(ADOPT_QUESTION)?.score())
            .filter(|score| *score < ADOPT_FROM)
            .map(|score| {
                format!(
                    "the Decision-Making Agent rates how far it reaches the goal {score:.1} of 3"
                )
            });
        let by = by.as_deref();
        let done = match doubt {
            Some(why) => open_adopt_gate(&inner, &goal, &wf, phase, Some(why), by).map(|_| ()),
            None => adopt_now(&inner, goal.id, &wf, phase, by),
        };
        warn_on_err(done, "settling a judged adoption");
    };
    crate::spawn_settling("judgement of the adoption", work, unwound);
}

/// Whether the proposal is still the goal's to adopt. The goal may have
/// moved while a model read: a newer proposal, a run somebody started — then
/// this one is nobody's to adopt any more.
fn still_proposed(inner: &Arc<Inner>, goal: &Goal, wf: &Workflow) -> bool {
    refuse_unless_idle(inner, goal.id).is_ok()
        && inner
            .ws
            .get_goal(goal.id)
            .is_ok_and(|g| g.workflow == Some(wf.id))
}

/// Open the Adopt gate for a proposal the person decides: the question,
/// journaled under its subject so a restarted daemon still finds it, the
/// note, and the `GateOpened` frame. `why` — an auto goal's reason for
/// asking after all — rides on the question.
fn open_adopt_gate(
    inner: &Arc<Inner>,
    goal: &Goal,
    wf: &Workflow,
    phase: GuidancePhase,
    why: Option<String>,
    by: Option<&str>,
) -> Result<String, EngineError> {
    let goal_id = goal.id;
    guided::note_proposed(
        inner,
        goal_id,
        wf,
        phase,
        &guided::ProposalFate::Gated { why: why.clone() },
    );
    let subject = adopt_subject(wf);
    // Short on purpose: the card that carries this question renders the
    // proposal itself — its description, every step with a summary, the
    // inputs — from the node's `ProposalView`.
    let mut question = format!(
        "Adopt \"{}\" for \"{}\"?",
        wf.name,
        goal.title.as_deref().unwrap_or(&goal.statement)
    );
    if let Some(why) = why {
        question.push_str(&format!(" It needs you: {why}."));
    }
    let (gate_id, _rx) = inner.gates.open(
        Home::Goal { goal: goal_id },
        None,
        Gate::Approval,
        subject.clone(),
        question.clone(),
        AskKind::Decision,
    );
    // The question is journaled under its *subject*, so a daemon that
    // restarted can still find the pending adoption and decide it durably.
    let owner = inner.ws.owner_keys().clone();
    warn_on_err(
        inner.ws.append_journal(
            &Home::Goal { goal: goal_id },
            JournalPayload::Question {
                work_item: None,
                gate: subject,
                text: question.clone(),
                expects: AskKind::Decision,
            },
            &owner,
            None,
        ),
        "journaling an adoption question",
    );
    add_note(
        inner,
        goal_id,
        format!(
            "proposed workflow \"{}\" (revision {})",
            wf.name, wf.revision
        ),
        by,
    )?;
    inner.emit(EngineEvent::scoped(
        goal_id,
        None,
        EnginePayload::GateOpened {
            gate_id: gate_id.clone(),
            gate: Gate::Approval,
            question,
        },
    ));
    Ok(gate_id)
}

/// Whether an auto goal may adopt its proposal without a person: its inputs
/// bind on their defaults, it has not failed past `goals.auto.repair_limit`
/// in a row, and every event it would listen for is one nobody needs to see
/// armed — a schedule, a signal, a run's end, a platform topic, a message. A
/// hook, a check, a poll or a project start is a person's to arm. The reason
/// it may not is what the Adopt gate then says.
///
/// What is judged is `proposal` — the workflow being adopted — never the
/// goal's record of one: a first proposal is attached after the goal was
/// read, and a rule that looked there would find nothing to refuse.
fn adopt_alone(inner: &Arc<Inner>, goal: &Goal, proposal: &Workflow) -> Result<(), String> {
    repairs_spent(failed_runs(inner, goal), auto_repair_limit(inner))?;
    let watched: Vec<String> = proposal
        .event_starts()
        .iter()
        .filter(|(_, on)| !on.arms_unattended())
        .map(|(step, on)| format!("`{}` starts on a {}", step.id, on.as_str()))
        .collect();
    if !watched.is_empty() {
        return Err(format!(
            "a person arms what it listens for — {}",
            watched.join(", ")
        ));
    }
    check_adoption_inputs(inner, goal.id, None)
        .map_err(|e| format!("it cannot start unattended — {e}"))?;
    Ok(())
}

/// Whether an auto goal that failed `failed` times is past what it repairs on
/// its own. The first run is no repair: a goal fails once and is repaired
/// once, so `limit` repairs are spent when it has failed `limit + 1` times —
/// at `failed == limit` it still goes alone, one past it a person decides. A
/// limit of nothing repairs nothing: the first failure waits for a person.
fn repairs_spent(failed: u64, limit: u64) -> Result<(), String> {
    if failed <= limit {
        return Ok(());
    }
    Err(format!(
        "it has failed {failed} time{}, past the {limit} repair{} an auto goal takes on its own",
        if failed == 1 { "" } else { "s" },
        if limit == 1 { "" } else { "s" }
    ))
}

/// `goals.auto.repair_limit`, resolved; the registry's default when unreadable.
fn auto_repair_limit(inner: &Arc<Inner>) -> u64 {
    inner
        .ws
        .setting(AUTO_REPAIR_LIMIT_KEY, None)
        .ok()
        .and_then(|r| r.value.as_u64())
        .unwrap_or(DEFAULT_AUTO_REPAIR_LIMIT)
}

pub const AUTO_REPAIR_LIMIT_KEY: &str = "goals.auto.repair_limit";
const DEFAULT_AUTO_REPAIR_LIMIT: u64 = 3;
pub const DEFAULT_MODE_KEY: &str = "goals.default_mode";

/// `goals.default_mode`, resolved — the mode a capture takes when it does
/// not say. The registry's default, `auto`, when unreadable.
pub fn default_goal_mode(inner: &Arc<Inner>) -> GoalMode {
    inner
        .ws
        .setting(DEFAULT_MODE_KEY, None)
        .ok()
        .and_then(|r| r.value.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or_default()
}

/// How many of the goal's runs failed **in a row**, newest back to the last
/// one that was done — the count an auto goal's repair budget is spent
/// against. A standing goal that runs for a year is judged by its latest
/// trouble, never by every failure it ever had.
fn failed_runs(inner: &Arc<Inner>, goal: &Goal) -> u64 {
    goal.runs
        .iter()
        .rev()
        .filter_map(|r| inner.ws.get_run(*r).ok())
        .filter(|r| r.is_finished())
        .take_while(|r| r.outcome != Some(RunOutcome::Done))
        .filter(|r| r.outcome == Some(RunOutcome::Failed))
        .count() as u64
}

fn adopt_subject(wf: &Workflow) -> String {
    format!("adopt:{}@{}", wf.id, wf.revision)
}

/// The amended definition a run would move to: the run's own workflow id and
/// the next revision, with the draft's content.
fn amendment_of(run: &WorkflowRun, draft: NewWorkflow) -> Workflow {
    Workflow {
        name: draft.name,
        description: draft.description,
        inputs: draft.inputs,
        steps: draft.steps,
        tags: draft.tags,
        decision_making: draft.decision_making,
        revision: run.workflow.revision + 1,
        ..run.workflow.clone()
    }
}

/// The checks an amendment passes before anything is written, beyond what
/// the run itself refuses: every agent step still has one place to run,
/// and the run's inputs still bind to the amended definition.
fn check_amendment_placement(
    inner: &Arc<Inner>,
    run: &WorkflowRun,
    candidate: &Workflow,
) -> Result<(), EngineError> {
    if let Some(goal) = run.scope.goal() {
        crate::projects::refuse_ambiguous_steps(inner, goal, candidate)?;
    }
    let bound = candidate
        .bind_inputs(run.inputs.clone())
        .map_err(|e| EngineError::Run(bisa_core::RunError::AmendNeedsInput(e)))?;
    let accounts = inner.ws.list_all_connector_accounts()?;
    let problems = candidate.validate_bound(
        &run.scope,
        &bound,
        &accounts,
        &bisa_store::StoreSyntaxChecks,
    );
    if !problems.is_empty() {
        return Err(bisa_store::StoreError::WorkflowInvalid(problems).into());
    }
    Ok(())
}

/// The goal's unfinished run, for an amendment. Refused when there is none
/// or it is finished.
fn amendable_run(inner: &Arc<Inner>, goal_id: GoalId) -> Result<WorkflowRun, EngineError> {
    let run = inner.ws.get_current_run(goal_id)?.ok_or_else(|| {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-goal-has-no-run-amend",
            goal_id = goal_id.to_string()
        ))
    })?;
    if run.is_finished() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-run-finished-propose-new-workflow-start-another",
            a0 = (run.id).to_string()
        )));
    }
    Ok(run)
}

/// Amend the goal's running workflow: steps that have not started may
/// change, the rest is frozen. Before anything is written: the definition is
/// validated, every agent step still has a project to run in, and the run's
/// inputs still bind; the run itself refuses a touched started step or a
/// changed workflow id.
pub fn amend_run(
    inner: &Arc<Inner>,
    goal_id: GoalId,
    draft: NewWorkflow,
    by: Option<&str>,
) -> Result<WorkflowRun, EngineError> {
    let run = amendable_run(inner, goal_id)?;
    let workflow = amendment_of(&run, draft);
    let problems = inner.ws.validate_workflow(&workflow)?;
    if !problems.is_empty() {
        return Err(bisa_store::StoreError::WorkflowInvalid(problems).into());
    }
    check_amendment_placement(inner, &run, &workflow)?;
    let revision = workflow.revision;
    let run = record_run_event(inner, run.id, RunEvent::Amended { workflow })?;
    add_note(
        inner,
        goal_id,
        format!("workflow amended (revision {revision})"),
        by,
    )?;
    inner.emit(EngineEvent::scoped(
        goal_id,
        None,
        // The Workflow Agent's amendment applied alone names it in `by`; a
        // person's own, or one they approved, names nobody.
        EnginePayload::WorkflowChanged {
            workflow: run.workflow.id,
            revision,
            designed: by.is_some(),
        },
    ));
    Ok(run)
}

/// How a held amendment is named: the goal's design list shows it for what
/// it is, and [`drop_held_amendments`] finds it by this.
pub const AMENDMENT_SUFFIX: &str = " · amendment";

/// The Workflow Agent's amendment: checked now (validation, placement, the
/// run's inputs, and that it touches no started step). On an **auto** goal
/// it is applied at once ([`amend_run`]) and a note says so. Otherwise it is
/// held durably as **the goal's own** workflow (`WorkflowOrigin::Goal`) and
/// gated — the person approves and [`amend_run`] applies it. Held under the
/// goal so a restart still finds it, and so it goes when the goal closes or
/// the run finishes. Returns the gate when one opened.
pub fn propose_amend(
    inner: &Arc<Inner>,
    goal_id: GoalId,
    draft: NewWorkflow,
    by: Option<&str>,
) -> Result<Option<String>, EngineError> {
    let run = amendable_run(inner, goal_id)?;
    let candidate = amendment_of(&run, draft.clone());
    let problems = inner.ws.validate_workflow(&candidate)?;
    if !problems.is_empty() {
        return Err(bisa_store::StoreError::WorkflowInvalid(problems).into());
    }
    check_amendment_placement(inner, &run, &candidate)?;
    // Dry run: an amendment that would be refused at approval is refused now,
    // while the proposer can still fix it.
    let mut trial = run.clone();
    trial.apply(
        RunEvent::Amended {
            workflow: candidate.clone(),
        },
        now_secs(),
    )?;
    let goal = inner.ws.get_goal(goal_id)?;
    if goal.mode.adopts_alone() {
        let revision = candidate.revision;
        amend_run(inner, goal_id, draft, by)?;
        guided::note_proposed(
            inner,
            goal_id,
            &candidate,
            GuidancePhase::Repair,
            &guided::ProposalFate::Adopted,
        );
        add_note(
            inner,
            goal_id,
            format!("amendment (revision {revision}) applied — auto mode, no decision asked"),
            by,
        )?;
        return Ok(None);
    }
    let held = inner.ws.create_workflow(
        NewWorkflow {
            name: format!("{}{AMENDMENT_SUFFIX}", draft.name),
            ..draft
        },
        WorkflowOrigin::Goal { goal: goal_id },
    )?;
    guided::note_proposed(
        inner,
        goal_id,
        &candidate,
        GuidancePhase::Repair,
        &guided::ProposalFate::Gated { why: None },
    );
    let subject = format!("amend:{}@{}", run.id, held.id);
    let question = format!(
        "Amend the running workflow of \"{}\" to revision {}?",
        goal.title.unwrap_or_else(|| "this goal".into()),
        candidate.revision
    );
    let (gate_id, _rx) = inner.gates.open(
        Home::Goal { goal: goal_id },
        None,
        Gate::Approval,
        subject.clone(),
        question.clone(),
        AskKind::Decision,
    );
    let owner = inner.ws.owner_keys().clone();
    warn_on_err(
        inner.ws.append_journal(
            &Home::Goal { goal: goal_id },
            JournalPayload::Question {
                work_item: None,
                gate: subject,
                text: question.clone(),
                expects: AskKind::Decision,
            },
            &owner,
            None,
        ),
        "journaling an amendment question",
    );
    add_note(
        inner,
        goal_id,
        format!("proposed an amendment (revision {})", candidate.revision),
        by,
    )?;
    inner.emit(EngineEvent::scoped(
        goal_id,
        None,
        EnginePayload::GateOpened {
            gate_id: gate_id.clone(),
            gate: Gate::Approval,
            question,
        },
    ));
    Ok(Some(gate_id))
}

/// Apply (or drop) a held amendment once its gate is decided. The held copy
/// is dropped either way; an approval the run can no longer take — a step it
/// touches has started since, a project was detached, an input no longer
/// binds — says so and asks for a fresh proposal.
fn settle_amendment(
    inner: &Arc<Inner>,
    goal_id: GoalId,
    subject: &str,
    approve: bool,
) -> Result<(), EngineError> {
    let Some((_, held)) = subject
        .strip_prefix("amend:")
        .and_then(|s| s.split_once('@'))
    else {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-amendment-subject-names-no-held-workflow",
            subject = format!("{subject:?}")
        )));
    };
    let held: WorkflowId = held.parse().map_err(|_| {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-not-workflow-id",
            held = format!("{held:?}")
        ))
    })?;
    let stored = inner.ws.get_workflow(held)?;
    let outcome = if approve {
        amend_run(
            inner,
            goal_id,
            NewWorkflow {
                name: stored
                    .name
                    .strip_suffix(AMENDMENT_SUFFIX)
                    .unwrap_or(&stored.name)
                    .to_string(),
                ..NewWorkflow::from(&stored)
            },
            None,
        )
        .map(|_| ())
        .map_err(|e| {
            EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-amendment-can-no-longer-be-applied-propose",
                e = e.to_string()
            ))
        })
    } else {
        Ok(())
    };
    warn_on_err(
        inner.ws.delete_workflow(held),
        "dropping a held amendment after its decision",
    );
    outcome
}

/// Forget every amendment still held for `goal` and withdraw its gates: the
/// run they would have amended is finished or the goal is closed, so there
/// is nothing left for them to change. Best effort; nothing here fails the
/// caller.
pub(crate) fn drop_held_amendments(inner: &Arc<Inner>, goal: GoalId) {
    for gate in inner.gates.pending_for_goal(goal) {
        if gate.subject.starts_with("amend:") {
            inner.gates.withdraw(&gate.id);
        }
    }
    let designs = match inner.ws.list_workflows_in(WorkflowScope::Goal(goal)) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(%goal, "cannot list the goal's designs to drop held amendments: {e}");
            return;
        }
    };
    let pointed = inner.ws.get_goal(goal).ok().and_then(|g| g.workflow);
    for wf in designs {
        if wf.name.ends_with(AMENDMENT_SUFFIX) && pointed != Some(wf.id) {
            warn_on_err(
                inner.ws.delete_workflow(wf.id),
                "dropping a held amendment whose run is over",
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Closing
// ---------------------------------------------------------------------------

/// Close a goal: its queued runs and its live run are cancelled (live work
/// cancelled, questions withdrawn, waits disarmed — each settled as any run
/// ending is), every gate on it withdrawn, its workstreams closed as
/// records, a parent waiting on it moved on, and the bus hears it. The
/// release, the drop and the parent's release are made here as well as in
/// each run's settle: a closed goal holds nothing whatever it was running.
pub fn close_goal(
    inner: &Arc<Inner>,
    goal_id: GoalId,
    reason: bisa_core::ClosureReason,
) -> Result<Goal, EngineError> {
    let before = inner.ws.get_current_run(goal_id)?;
    let listened = inner
        .ws
        .listening(&ListenerHost::Goal { goal: goal_id })?
        .is_some();
    let (goal, cancelled) = inner.ws.set_goal_closed(goal_id, reason.clone())?;
    if listened {
        crate::listen::invalidate(inner);
        crate::listen::turn::announce(inner, ListenerHost::Goal { goal: goal_id }, false);
    }
    for (run, effects) in cancelled {
        let prev = before
            .as_ref()
            .filter(|b| b.id == run.id)
            .map(|b| b.steps.clone())
            .unwrap_or_default();
        emit_step_changes(inner, &prev, &run);
        effects::run_effects(inner, &run, effects);
    }
    for gate in inner.gates.pending_for_goal(goal_id) {
        inner.gates.withdraw(&gate.id);
    }
    drop_held_amendments(inner, goal_id);
    effects::release_goal_workstreams(inner, goal_id);
    waits::child_finished(inner, goal_id, None);
    inner.security.forget_home(&Home::Goal { goal: goal_id });
    // Whatever was running for the goal stops with it — its workers, its
    // design wake, the turns in its thread, the asks read for it — before
    // the wake's standing is forgotten, so the stop still finds it: a closed
    // goal holds nothing, whatever it was running.
    let workstreams = workstreams_of_goal(inner, goal_id).unwrap_or_default();
    sessions::stop_for(inner, Scope::Goal(goal_id), &workstreams);
    inner.guided.forget_goal(goal_id);
    inner.emit(EngineEvent::scoped(
        goal_id,
        None,
        EnginePayload::GoalClosed { reason },
    ));
    Ok(goal)
}

// ---------------------------------------------------------------------------
// Shared validation
// ---------------------------------------------------------------------------

/// Every named agent and team must exist.
///
/// `Workspace::set_goal_assignees` makes this check on its own path; a work
/// item's and a project's are made here — a project's where it is made and
/// where it is edited (`projects::create`, `projects::update`). An assignment
/// to a ghost is a typo that would otherwise fail silently at routing time —
/// the item would simply never find an agent, and nothing would say why.
pub fn check_assignees(inner: &Inner, assignees: &[Assignee]) -> Result<(), EngineError> {
    for a in assignees {
        match a {
            Assignee::Agent(id) => {
                let id = AgentId::new(id)?;
                inner.ws.get_agent(&id).map_err(|_| {
                    EngineError::Invalid(bisa_core::text!(
                        "error-engine-invalid-agent-not-found",
                        id = id.to_string()
                    ))
                })?;
            }
            Assignee::Team(id) => {
                let id = bisa_core::TeamId::new(id)?;
                inner.ws.get_team(&id).map_err(|_| {
                    EngineError::Invalid(bisa_core::text!(
                        "error-engine-invalid-team-not-found",
                        id = id.to_string()
                    ))
                })?;
            }
            // A human is a bare pubkey: valid whether or not that person has
            // joined this workspace yet.
            Assignee::Human(_) => {}
        }
    }
    Ok(())
}

/// **What its person gives a goal to work in is attached to it.** A person
/// who begins a goal's work with a project — an input of kind `project` —
/// says where the work is done, and saying it attaches it: the capture that
/// names one is the first the goal hears of it, and a run form offers every
/// project of the workspace. Attaching is additive, reversible and asks for
/// nothing, so it asks for nothing here.
///
/// Only the doors a person speaks through come here — a capture, a start by
/// hand, a test run, an adoption's inputs, what a goal listens with. What
/// an occurrence's payload or a step's mapping names is attached by nobody:
/// there the step's own check ([`check_project_attached`]) refuses, which is
/// what it is for.
///
/// `door` is the beginning itself. What it refuses leaves the goal attached
/// to nothing it was not.
pub(crate) fn begun_by_its_person<T>(
    inner: &Arc<Inner>,
    goal: GoalId,
    given: &BTreeMap<String, Value>,
    door: impl FnOnce() -> Result<T, EngineError>,
) -> Result<T, EngineError> {
    let attached = attach_given(inner, goal, given)?;
    door().inspect_err(|_| {
        for project in &attached {
            match inner.ws.detach(goal, *project) {
                Ok(()) => said_attached(inner, goal, *project, false),
                Err(e) => tracing::warn!(
                    target: "bisa_engine", %goal, %project,
                    "a project attached for a start that was refused could not be detached: {e}"
                ),
            }
        }
    })
}

/// Attach every project `given` names under an input of kind `project`,
/// and answer the ones this call attached. A word that is no project's id,
/// or names none of this workspace, is left for whoever reads it to refuse.
pub(crate) fn attach_given(
    inner: &Arc<Inner>,
    goal: GoalId,
    given: &BTreeMap<String, Value>,
) -> Result<Vec<ProjectId>, EngineError> {
    let Some(workflow) = inner.ws.get_goal(goal)?.workflow else {
        return Ok(Vec::new());
    };
    let mut attached = Vec::new();
    for def in &inner.ws.get_workflow(workflow)?.inputs {
        if def.kind != bisa_core::InputKind::Project {
            continue;
        }
        let named = given
            .get(def.name.as_str())
            .and_then(Value::as_str)
            .and_then(|word| word.parse::<ProjectId>().ok());
        let Some(project) = named else { continue };
        if inner.ws.get_project(project).is_err() || inner.ws.is_attached(goal, project)? {
            continue;
        }
        inner.ws.attach(goal, project)?;
        said_attached(inner, goal, project, true);
        attached.push(project);
    }
    Ok(attached)
}

fn said_attached(inner: &Arc<Inner>, goal: GoalId, project: ProjectId, attached: bool) {
    inner.emit(EngineEvent::scoped(
        goal,
        None,
        EnginePayload::AttachmentChanged { project, attached },
    ));
}

/// The goal must be **attached** to the project, not merely share a
/// workspace with it.
///
/// Attachment is the whole of the relationship, so it is the whole
/// of the check. Checking mere existence instead would let a step place work
/// in a folder belonging to unrelated work: the id is a ULID somebody could
/// have copied from anywhere. The refusal names the remedy, because attaching
/// is one call away.
pub fn check_project_attached(
    inner: &Arc<Inner>,
    goal: GoalId,
    project: ProjectId,
) -> Result<(), EngineError> {
    if inner.ws.is_attached(goal, project)? {
        return Ok(());
    }
    Err(EngineError::Invalid(bisa_core::text!(
        "error-engine-invalid-goal-not-attached-project-attach-project-goal",
        goal = goal.to_string(),
        project = project.to_string()
    )))
}

// ---------------------------------------------------------------------------
// Decisions
// ---------------------------------------------------------------------------

/// Decide a pending gate. Records the signed decision, resolves waiters, and
/// applies the consequence: an `approval` step is decided, a `human` step is
/// answered (an unsure answer re-asks), an adoption begins the goal's work
/// with `inputs` — a run, or listening — an amendment is applied, an agent's
/// own question wakes it.
pub fn decide(
    inner: &Arc<Inner>,
    gate_id: &str,
    approve: bool,
    rationale: Option<&str>,
    answer: Option<&Answer>,
    inputs: Option<BTreeMap<String, Value>>,
) -> Result<ApprovalId, EngineError> {
    decide_showing(inner, gate_id, approve, rationale, answer, inputs).map(|d| d.approval)
}

/// [`decide`], answering what the decision has to show as well: the public
/// hook secrets an adoption minted.
pub fn decide_showing(
    inner: &Arc<Inner>,
    gate_id: &str,
    approve: bool,
    rationale: Option<&str>,
    answer: Option<&Answer>,
    inputs: Option<BTreeMap<String, Value>>,
) -> Result<Decided, EngineError> {
    // The gate is this caller's alone from here to the last consequence: a
    // second decision arriving meanwhile is refused, never journaled beside
    // this one; a refusal below gives the gate back.
    let deciding = inner.gates.begin_decide(gate_id).map_err(|why| match why {
        crate::gates::DecideRefusal::Unknown => EngineError::UnknownGate(gate_id.into()),
        crate::gates::DecideRefusal::Decided => EngineError::GateAlreadyDecided(gate_id.into()),
    })?;
    let entry = &deciding.entry;
    entry.expects.validate_answer(answer)?;
    // An adoption starts the run with `inputs`: bind them *before* the gate is
    // resolved, so a missing input is a 400 that leaves the adoption pending
    // instead of a burnt gate and a goal nobody can start.
    // What only a goal has — an adoption, an amendment, a design's question
    // — is decided on its goal; a run of the workspace owes none of them.
    let goal = entry.home.goal();
    let adopting = entry.gate == Gate::Approval
        && entry.run.is_none()
        && entry.subject.starts_with("adopt:")
        && approve;
    if adopting {
        check_adoption_inputs(inner, owed_goal(goal, &entry.subject)?, inputs.as_ref())?;
    }

    let unsure = answer.map(|a| a.unsure).unwrap_or(false);
    let round = if unsure {
        Some(count_clarify_round(inner, entry.home))
    } else {
        inner.clarify_rounds.remove(&entry.home);
        None
    };
    let clarify_rounds_left = round.map(|r| r.left);
    // A human step's answer is the run's to take: ask the machine first, on
    // a copy, so a run that refuses — the step is no longer waiting — leaves
    // the gate open and the decision unrecorded instead of a burnt gate and
    // a step nobody can answer again.
    if let (Gate::Escalation, Some(run), Some(step)) = (entry.gate, entry.run, entry.step.as_ref())
    {
        if let Some(event) = step_answer_event(step, answer, round) {
            run_accepts(inner, run, event)?;
        }
    }

    let approval = inner.ws.record_decision(
        &entry.home,
        entry.gate,
        approve,
        &entry.subject,
        rationale,
        answer,
    )?;
    inner.gates.resolve(
        gate_id,
        approve,
        answer.cloned(),
        approval.0.clone(),
        clarify_rounds_left,
    );
    inner.emit(inner.home_scope(&entry.home).event(
        entry.work_item,
        EnginePayload::GateDecided {
            gate_id: gate_id.into(),
            gate: entry.gate,
            approve,
        },
    ));
    // The session that stopped on this gate goes on.
    if let Some(run) = inner.presence.by_gate(gate_id) {
        inner.presence.resumed(inner, run);
    }

    let mut secrets = Vec::new();
    match (entry.gate, entry.run, entry.step.as_ref()) {
        (Gate::Approval, Some(run), Some(step)) => {
            record_run_event(
                inner,
                run,
                RunEvent::Decided {
                    step: step.clone(),
                    approve,
                    approval: approval.clone(),
                },
            )?;
        }
        // Adopting a proposed workflow begins the goal's work with the
        // inputs the person gave — a run, or listening; declining leaves the
        // goal a draft, so it takes the no-op arm below.
        (Gate::Approval, None, None) if approve && entry.subject.starts_with("adopt:") => {
            let goal = owed_goal(goal, &entry.subject)?;
            let given = inputs.unwrap_or_default();
            secrets = begun_by_its_person(inner, goal, &given.clone(), || {
                begin_goal(inner, goal, given, Begin::Auto)
            })?
            .secrets();
        }
        (Gate::Approval, None, None) if entry.subject.starts_with("amend:") => {
            let goal = owed_goal(goal, &entry.subject)?;
            settle_amendment(inner, goal, &entry.subject, approve)?;
        }
        (Gate::Escalation, Some(run), Some(step)) => {
            answer_step(inner, run, step, answer, round)?;
        }
        // The Workflow Agent's own question on a goal it designs, whatever
        // its shape: the answer — or the verdict of a decision-shaped one —
        // resumes the design.
        (Gate::Escalation, _, _) => {
            let designs = goal
                .and_then(|g| inner.ws.get_goal(g).ok())
                .map(|g| g.mode.designs())
                .unwrap_or(false);
            if let (true, Some(goal)) = (designs, goal) {
                let said = answer_or_verdict(&entry.expects, approve, rationale, answer);
                guided::notify_answered(inner, goal, said, clarify_rounds_left);
            }
        }
        // A declined adoption leaves the design on the goal: still a draft,
        // still pointed at — edit and adopt, or choose another workflow. The
        // note is how a reader learns the person said no.
        (Gate::Approval, None, None) if entry.subject.starts_with("adopt:") => {
            add_note(
                inner,
                owed_goal(goal, &entry.subject)?,
                "adoption declined — the design stays on the goal; edit and adopt it, or choose another workflow".into(),
                None,
            )?;
        }
        // A publish decision, a permission: resolved, and nothing else moves.
        _ => {}
    }
    Ok(Decided { approval, secrets })
}

/// The event a `human` step's answer becomes: the answer, or — a *not sure*
/// past the clarify budget — the step's failure with the person's words.
/// `None` while a re-ask is still owed, which touches the run not at all.
fn step_answer_event(
    step: &StepId,
    answer: Option<&Answer>,
    round: Option<ClarifyRound>,
) -> Option<RunEvent> {
    let answer = answer?;
    if answer.unsure {
        return match round {
            None | Some(ClarifyRound { over: true, .. }) => Some(RunEvent::StepFailed {
                step: step.clone(),
                error: format!(
                    "the person is not sure{}",
                    answer
                        .text
                        .as_deref()
                        .map(|t| format!(": {t}"))
                        .unwrap_or_default()
                ),
            }),
            Some(_) => None,
        };
    }
    Some(RunEvent::Answered {
        step: step.clone(),
        answer: answer.clone(),
    })
}

/// Would the run take `event`? Asked of a copy, so nothing is written: the
/// machine's own refusal — the wrong kind, a step not live — comes back as
/// the error the caller would have had, before anything else is spent.
fn run_accepts(inner: &Arc<Inner>, run_id: RunId, event: RunEvent) -> Result<(), EngineError> {
    let mut probe = inner.ws.get_run(run_id)?;
    probe.apply(event, now_secs())?;
    Ok(())
}

/// A `human` step's answer. "I'm not sure" re-asks the same question — up to
/// the clarify budget, after which the step is failed with the person's
/// words, because a question that can be re-asked forever never finishes.
fn answer_step(
    inner: &Arc<Inner>,
    run: RunId,
    step: &StepId,
    answer: Option<&Answer>,
    round: Option<ClarifyRound>,
) -> Result<(), EngineError> {
    if answer.is_none() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-human-step-needs-answer"
        )));
    }
    match step_answer_event(step, answer, round) {
        Some(event) => {
            record_run_event(inner, run, event)?;
            Ok(())
        }
        None => reask(inner, run, step),
    }
}

/// Open the same question again for a `human` step still waiting.
fn reask(inner: &Arc<Inner>, run_id: RunId, step: &StepId) -> Result<(), EngineError> {
    let run = inner.ws.get_run(run_id)?;
    let Some(def) = run.workflow.step(step) else {
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
    let text = effects::render(inner, &run, prompt)?;
    effects::open_step_gate(
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

/// One *not sure* against the clarify budget of the goal — or run of the
/// workspace — asking: how many re-asks are left after it (`0` on the last
/// one), and whether it came past the budget — `max_clarify_rounds` is how
/// many times the same question is asked again, and the *not sure* after
/// them fails a `human` step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ClarifyRound {
    left: u8,
    over: bool,
}

fn count_clarify_round(inner: &Arc<Inner>, home: Home) -> ClarifyRound {
    let max = inner.config.max_clarify_rounds;
    let mut used = inner.clarify_rounds.entry(home).or_insert(0);
    *used = used.saturating_add(1);
    ClarifyRound {
        left: max.saturating_sub(*used),
        over: *used > max,
    }
}

/// The newest adoption or amendment question in the journal that no decision
/// has settled — the gate an unnamed decision is about.
fn newest_undecided_gate_question(journal: &[JournalEvent]) -> Option<String> {
    let subject = journal.iter().rev().find_map(|e| match &e.payload {
        JournalPayload::Question { gate, .. }
            if gate.starts_with("adopt:") || gate.starts_with("amend:") =>
        {
            Some(gate.clone())
        }
        _ => None,
    })?;
    let decided = journal.iter().any(
        |e| matches!(&e.payload, JournalPayload::Decision { subject: s, .. } if *s == subject),
    );
    (!decided).then_some(subject)
}

/// The goal an adoption or an amendment is owed on. Only a goal owes one: a
/// gate of either kind on a run of the workspace names nothing to act on.
fn owed_goal(goal: Option<GoalId>, subject: &str) -> Result<GoalId, EngineError> {
    goal.ok_or_else(|| {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-only-goal-owes-this-decision",
            subject = subject.to_string()
        ))
    })
}

/// Durable-mirror decide with no live engine gate: infer the pending gate
/// from the home's run (a waiting `approval`, `human` or held `wait` step) or
/// from its journal — for a goal an adoption, a held amendment or an agent's
/// question; for a run of the workspace an agent's question — record the
/// decision, and apply the consequence through the same funnel a live
/// decision uses. With several steps waiting, `step` names the one meant;
/// none named is refused rather than guessed.
pub fn decide_without_engine(
    inner: &Arc<Inner>,
    home: &Home,
    approve: bool,
    rationale: Option<&str>,
    answer: Option<&Answer>,
    inputs: Option<BTreeMap<String, Value>>,
    step: Option<&StepId>,
) -> Result<DecideOutcome, EngineError> {
    match home {
        Home::Goal { goal } => {
            decide_goal_without_engine(inner, *goal, approve, rationale, answer, inputs, step)
        }
        Home::Run { run } => {
            decide_run_without_engine(inner, *run, approve, rationale, answer, step)
        }
    }
}

/// A waiting step of a live run, decided durably — or `None` when no step
/// is the one meant. `owed_gate_question` says the home also owes a
/// decision on an adoption or an amendment: then an unnamed decision is
/// that one's, never the release of the one `wait` step merely holding.
#[allow(clippy::too_many_arguments)]
fn decide_waiting_step(
    inner: &Arc<Inner>,
    home: &Home,
    run: &WorkflowRun,
    approve: bool,
    rationale: Option<&str>,
    answer: Option<&Answer>,
    step: Option<&StepId>,
    owed_gate_question: bool,
) -> Result<Option<DecideOutcome>, EngineError> {
    let ws = &inner.ws;
    let waiting: Vec<&StepId> = run
        .steps
        .iter()
        .filter(|(id, record)| {
            record.state == StepState::Waiting
                && matches!(
                    run.workflow.step(id).map(|s| &s.kind),
                    Some(
                        StepKind::Approval { .. }
                            | StepKind::Human { .. }
                            | StepKind::Wait {
                                until: WaitFor::Release
                            }
                    )
                )
        })
        .map(|(id, _)| id)
        .collect();
    let chosen: Option<&StepId> = match (step, waiting.as_slice()) {
        (Some(named), _) => {
            if !waiting.contains(&named) {
                return Err(EngineError::Invalid(bisa_core::text!(
                    "error-engine-invalid-step-not-waiting-decision-answer",
                    named = named.to_string(),
                    home = home.to_string()
                )));
            }
            Some(named)
        }
        (None, [one])
            if owed_gate_question
                && matches!(
                    run.workflow.step(one).map(|s| &s.kind),
                    Some(StepKind::Wait {
                        until: WaitFor::Release
                    })
                ) =>
        {
            None
        }
        (None, [one]) => Some(one),
        (None, []) => None,
        (None, many) => {
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-steps-waiting-name-one-with",
                home = home.to_string(),
                a0 = (many.len()).to_string(),
                a1 = (many
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join(", "))
                .to_string()
            )));
        }
    };
    let Some(step) = chosen else {
        return Ok(None);
    };
    match run.workflow.step(step).map(|s| &s.kind) {
        // A held `wait` step: its one decision is the release. A decline
        // leaves it held — there is nothing to record.
        Some(StepKind::Wait {
            until: WaitFor::Release,
        }) => {
            if !approve {
                return Err(EngineError::Invalid(bisa_core::text!(
                    "error-engine-invalid-step-held-until-released-there-nothing",
                    step = step.to_string(),
                    home = home.to_string()
                )));
            }
            crate::waits::release(inner, run.id, step, None)?;
            Ok(Some(outcome_of(inner, home, Gate::Escalation, true)?))
        }
        Some(StepKind::Approval { .. }) => {
            AskKind::Decision.validate_answer(answer)?;
            let approval = ws.record_decision(
                home,
                Gate::Approval,
                approve,
                &approval_subject(run.id, step),
                rationale,
                answer,
            )?;
            record_run_event(
                inner,
                run.id,
                RunEvent::Decided {
                    step: step.clone(),
                    approve,
                    approval,
                },
            )?;
            Ok(Some(outcome_of(inner, home, Gate::Approval, approve)?))
        }
        Some(StepKind::Human { options, multi, .. }) => {
            let expects = AskKind::Answer {
                options: options.clone(),
                multi: *multi,
            };
            expects.validate_answer(answer)?;
            ws.record_decision(
                home,
                Gate::Escalation,
                approve,
                &format!("step:{}/{step}", run.id),
                rationale,
                answer,
            )?;
            let round = answer
                .filter(|a| a.unsure)
                .map(|_| count_clarify_round(inner, *home));
            answer_step(inner, run.id, step, answer, round)?;
            Ok(Some(outcome_of(inner, home, Gate::Escalation, approve)?))
        }
        _ => Ok(None),
    }
}

/// [`decide_without_engine`] for a goal: its live run's waiting step, then
/// the newest question its journal holds — an adoption, a held amendment, an
/// agent's own.
fn decide_goal_without_engine(
    inner: &Arc<Inner>,
    goal_id: GoalId,
    approve: bool,
    rationale: Option<&str>,
    answer: Option<&Answer>,
    inputs: Option<BTreeMap<String, Value>>,
    step: Option<&StepId>,
) -> Result<DecideOutcome, EngineError> {
    let ws = &inner.ws;
    let home = Home::Goal { goal: goal_id };
    let goal = ws.get_goal(goal_id)?;
    let run = ws.get_current_run(goal_id)?;
    let live = run.as_ref().filter(|r| !r.is_finished());
    let journal = ws.journal(&home)?;
    // An adoption or an amendment still owed a decision: what an unnamed
    // `approve` means while a `wait` step merely holds — a release is asked
    // for by naming its step, as the inbox row does.
    let owed_gate_question = newest_undecided_gate_question(&journal).is_some();

    // 1. A waiting step of the live run.
    if let Some(run) = live {
        if let Some(outcome) = decide_waiting_step(
            inner,
            &home,
            run,
            approve,
            rationale,
            answer,
            step,
            owed_gate_question,
        )? {
            return Ok(outcome);
        }
    }

    // 2. The newest journaled question: an adoption, a held amendment, or an
    //    agent's own.
    let question = journal.iter().rev().find_map(|e| match &e.payload {
        JournalPayload::Question { gate, expects, .. } => Some((gate.clone(), expects.clone())),
        _ => None,
    });
    let Some((subject, expects)) = question else {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-home-has-no-pending-gate",
            home = home.to_string()
        )));
    };
    if subject.starts_with("adopt:") {
        let decided = journal.iter().rev().any(
            |e| matches!(&e.payload, JournalPayload::Decision { subject: s, .. } if *s == subject),
        );
        if decided {
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-adoption-was-already-decided",
                subject = subject.to_string()
            )));
        }
        AskKind::Decision.validate_answer(answer)?;
        if approve {
            check_adoption_inputs(inner, goal_id, inputs.as_ref())?;
        }
        ws.record_decision(&home, Gate::Approval, approve, &subject, rationale, answer)?;
        let secrets = if approve {
            let given = inputs.unwrap_or_default();
            begun_by_its_person(inner, goal_id, &given.clone(), || {
                begin_goal(inner, goal_id, given, Begin::Auto)
            })?
            .secrets()
        } else {
            Vec::new()
        };
        return Ok(outcome_of(inner, &home, Gate::Approval, approve)?.showing(secrets));
    }
    if subject.starts_with("amend:") {
        let decided = journal.iter().rev().any(
            |e| matches!(&e.payload, JournalPayload::Decision { subject: s, .. } if *s == subject),
        );
        if decided {
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-amendment-was-already-decided",
                subject = subject.to_string()
            )));
        }
        AskKind::Decision.validate_answer(answer)?;
        ws.record_decision(&home, Gate::Approval, approve, &subject, rationale, answer)?;
        settle_amendment(inner, goal_id, &subject, approve)?;
        return outcome_of(inner, &home, Gate::Approval, approve);
    }
    expects.validate_answer(answer)?;
    ws.record_decision(
        &home,
        Gate::Escalation,
        approve,
        &subject,
        rationale,
        answer,
    )?;
    if goal.mode.designs() {
        let left = answer
            .filter(|a| a.unsure)
            .map(|_| count_clarify_round(inner, home).left);
        let said = answer_or_verdict(&expects, approve, rationale, answer);
        guided::notify_answered(inner, goal_id, said, left);
    }
    outcome_of(inner, &home, Gate::Escalation, approve)
}

/// [`decide_without_engine`] for a run of the workspace: its waiting step,
/// then the newest question its journal holds — an agent's own; a run of
/// the workspace owes no adoption and no amendment.
fn decide_run_without_engine(
    inner: &Arc<Inner>,
    run_id: RunId,
    approve: bool,
    rationale: Option<&str>,
    answer: Option<&Answer>,
    step: Option<&StepId>,
) -> Result<DecideOutcome, EngineError> {
    let ws = &inner.ws;
    let run = ws.get_run(run_id)?;
    let home = run.home();
    if !run.is_finished() {
        if let Some(outcome) =
            decide_waiting_step(inner, &home, &run, approve, rationale, answer, step, false)?
        {
            return Ok(outcome);
        }
    }
    let journal = ws.journal(&home)?;
    let Some((subject, expects)) = newest_open_ask(&journal) else {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-home-has-no-pending-gate",
            home = home.to_string()
        )));
    };
    expects.validate_answer(answer)?;
    ws.record_decision(
        &home,
        Gate::Escalation,
        approve,
        &subject,
        rationale,
        answer,
    )?;
    outcome_of(inner, &home, Gate::Escalation, approve)
}

/// The newest question an agent asked (`ask_human:`) that nothing has
/// settled since — no decision and no withdrawal on its subject. A step's
/// own question is decided through its step (`decide_waiting_step`), and a
/// question already answered is never answered twice.
fn newest_open_ask(journal: &[JournalEvent]) -> Option<(String, AskKind)> {
    let mut settled: HashSet<&str> = HashSet::new();
    for e in journal.iter().rev() {
        match &e.payload {
            JournalPayload::Decision { subject, .. }
            | JournalPayload::Withdrawn { subject, .. } => {
                settled.insert(subject.as_str());
            }
            JournalPayload::Question { gate, expects, .. }
                if gate.starts_with("ask_human:") && !settled.contains(gate.as_str()) =>
            {
                return Some((gate.clone(), expects.clone()));
            }
            _ => {}
        }
    }
    None
}

/// What the Workflow Agent is told the person said. A question shaped as an
/// answer carries the answer; one shaped as a decision — approve/decline with
/// an optional reason — carries the verdict as words, so a design that asked
/// a yes/no still continues.
fn answer_or_verdict(
    expects: &AskKind,
    approve: bool,
    rationale: Option<&str>,
    answer: Option<&Answer>,
) -> Option<Answer> {
    if expects.is_answer() {
        return answer.cloned();
    }
    let verdict = if approve {
        "Yes — approved."
    } else {
        "No — declined."
    };
    let said = match rationale.map(str::trim).filter(|r| !r.is_empty()) {
        Some(r) => format!("{verdict} {r}"),
        None => verdict.to_string(),
    };
    Some(Answer::text(said))
}

/// The inputs an adoption would begin the goal with, checked before
/// anything is recorded: bound against the workflow's declared inputs for a
/// run by hand, or — a workflow that begins on events — every input its
/// events need given, as turning it on asks.
fn check_adoption_inputs(
    inner: &Arc<Inner>,
    goal_id: GoalId,
    inputs: Option<&BTreeMap<String, Value>>,
) -> Result<(), EngineError> {
    let goal = inner.ws.get_goal(goal_id)?;
    let Some(workflow) = goal.workflow else {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-goal-points-no-workflow-adopt",
            goal_id = goal_id.to_string()
        )));
    };
    let wf = inner.ws.get_workflow(workflow)?;
    if !wf.event_starts().is_empty() {
        return crate::listen::turn::check(
            inner,
            &ListenerHost::Goal { goal: goal_id },
            &wf,
            &inputs.cloned().unwrap_or_default(),
        )
        .map_err(|e| {
            EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-adoption-needs-inputs",
                e = e.to_string()
            ))
        });
    }
    let bound = wf
        .bind_inputs(inputs.cloned().unwrap_or_default())
        .map_err(|e| {
            EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-adoption-needs-inputs",
                e = e.to_string()
            ))
        })?;
    let accounts = inner.ws.list_all_connector_accounts()?;
    let problems = wf.validate_bound(
        &RunScope::Goal { goal: goal_id },
        &bound,
        &accounts,
        &bisa_store::StoreSyntaxChecks,
    );
    if !problems.is_empty() {
        return Err(bisa_store::StoreError::WorkflowInvalid(problems).into());
    }
    Ok(())
}

/// Where `home` stands after a decision on it — the goal's status, or the
/// run of the workspace's — for the answer.
pub(crate) fn outcome_of(
    inner: &Arc<Inner>,
    home: &Home,
    gate: Gate,
    approve: bool,
) -> Result<DecideOutcome, EngineError> {
    let status = match home {
        Home::Goal { goal } => {
            let current = inner.ws.get_current_run(*goal)?;
            HomeStatus::Goal {
                status: inner.ws.get_goal(*goal)?.status(current.as_ref()),
            }
        }
        Home::Run { run } => HomeStatus::Run {
            status: inner.ws.get_run(*run)?.status(),
        },
    };
    Ok(DecideOutcome {
        home: *home,
        gate,
        approve,
        status,
        secrets: Vec::new(),
    })
}

/// Journal a note on a home — a goal, or a run of the workspace — as
/// `agent`, or as the owner when the session has no agent identity, like
/// every other unattributed write. Best effort: a note that cannot be written
/// is logged, never a reason to fail the work.
pub(crate) fn journal_note_as(inner: &Inner, agent: Option<&str>, home: Home, text: String) {
    let (signer, attestation) = signer_for(&inner.ws, agent);
    if let Err(e) =
        inner
            .ws
            .append_journal(&home, JournalPayload::Note { text }, &signer, attestation)
    {
        let who = agent.unwrap_or("the owner");
        tracing::warn!("{home}: could not journal {who}'s note: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_auto_goal_goes_alone_up_to_its_limit_and_not_one_failure_past_it() {
        for limit in [0u64, 1, 3, 10] {
            for failed in 0..=limit {
                assert!(
                    repairs_spent(failed, limit).is_ok(),
                    "{failed} failed of {limit}"
                );
            }
            let past = repairs_spent(limit + 1, limit).unwrap_err();
            assert!(past.contains(&format!("failed {}", limit + 1)), "{past}");
            assert!(repairs_spent(u64::MAX, limit).is_err());
        }
        // The words agree in number.
        assert!(repairs_spent(1, 0)
            .unwrap_err()
            .contains("failed 1 time, past the 0 repairs"));
        assert!(repairs_spent(2, 1)
            .unwrap_err()
            .contains("failed 2 times, past the 1 repair "));
        assert_eq!(
            DEFAULT_AUTO_REPAIR_LIMIT, 3,
            "the registry's default, said in the settings reference"
        );
    }
}
