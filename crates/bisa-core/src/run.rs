//! `WorkflowRun`: one execution of a workflow — a goal's, or the workspace's
//! own — and the only place execution state lives.
//!
//! The shape is the one the lifecycle had: the run changes
//! only through a typed [`RunEvent`], [`WorkflowRun::apply`] is the one
//! function that may change it, and what it returns is *data* — a list of
//! [`RunEffect`]s the engine interprets. Nothing here launches, posts, waits or
//! signs; it decides what must happen and says so.
//!
//! A run holds a **frozen copy** of the workflow it started with. A later
//! revision of the definition never touches a run; an amendment is an event
//! on the run that replaces steps which have not started.
//!
//! A run has a [`RunScope`]: what it is for. A **goal's** run is made
//! *queued*: it holds its workflow and inputs and refuses every step event
//! until [`RunEvent::Start`], which the store applies at once when the goal
//! has no live run and the engine applies later, in order, when it has — a
//! goal has at most one live run. A run **of the workspace** — started from
//! the workflow itself, or by one of its start events while it is On — is
//! started the moment it is made, and any number may be live at once. A run
//! ends by an outcome or by [`RunEvent::Cancel`], which names its
//! [`CancelCause`]; both are absorbing.
//!
//! A run begins at one way in ([`RunEntry`]): the start step an event began
//! it at, or — by hand — the manual start, or the root of a workflow that
//! names none. The other starts are skipped, so their paths die and no join
//! waits on them. What a person asked for — a start named, an event sampled —
//! is read one way by every surface ([`WayIn::of`]): a person's start, a run
//! now at the start by hand, or a test run of an event start. While a step is
//! live its boundary events may fire ([`RunEvent::BoundaryFired`]): a divert
//! stops the step (`Diverted`) and takes the flows labelled with the
//! boundary's name; an act posts or signals beside it
//! ([`RunEffect::BoundaryAct`]).

use crate::ask::{Answer, AnswerError, AskKind};
use crate::boundary::may_carry_boundaries;
use crate::gate::ApprovalId;
use crate::goal::{Budget, ClosureReason, Holder};
use crate::home::Home;
use crate::id::{GoalId, RunId, WorkItemId};
use crate::listen::Chain;
use crate::signal::Signal;
use crate::template::{self, TemplateCtx, TemplateError};
use crate::workflow::{
    branch, Branch, Condition, ConditionCtx, Finish, Flow, InputError, Join, OnFail, Pick,
    RunOutcome, Step, StepId, StepKind, WaitFor, Workflow,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The addressable `kind:33413` snapshot, filed in its home's `state/`: its
/// goal's, or its own folder's for a run of the workspace
/// ([`WorkflowRun::home`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkflowRun {
    pub id: RunId,
    /// What the run is for. Set when it is made; a run never changes scope.
    pub scope: RunScope,
    /// Frozen at start; replaced only by [`RunEvent::Amended`].
    pub workflow: Workflow,
    #[serde(default)]
    pub inputs: BTreeMap<String, serde_json::Value>,
    /// The start step the run begins at. `None` for a run by hand while it is
    /// queued — it begins at the manual entry, and [`RunEvent::Start`] writes
    /// which step that was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<StepId>,
    /// The occurrence that began this run, whole — or a test run's sample —
    /// when an event did. Its start's input mapping read it; no step does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<Signal>,
    /// The id of the queued signal this run was dispatched from: one signal,
    /// one run, even across a crash between the two. A restart keeps the
    /// event and never this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatched: Option<String>,
    /// The listeners the causal line behind this run passed through: its
    /// event's chain, widened by every chained event it hears — what keeps
    /// a signal it raises from starting its own listener again.
    #[serde(default, skip_serializing_if = "Chain::is_empty")]
    pub chain: Chain,
    pub steps: BTreeMap<StepId, StepRecord>,
    /// When the run was made. The queue's order is `(queued_at, id)`.
    pub queued_at: u64,
    /// When [`RunEvent::Start`] was applied; `None` while the run is queued.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<RunOutcome>,
    /// Why the run was cancelled, when it was. A cancelled run has no outcome.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancelled: Option<CancelCause>,
    /// Orders step settlements: every accepted event **and every change to a
    /// step's record** advances it, and the record stamps it. The machine
    /// tells a flow that arrived *after* a step last finished from one that
    /// arrived before — the loop case — without trusting a clock. Per change,
    /// not per event, because one event settles a whole chain: a `decide`
    /// that finishes in the same pass as the step it loops back into must
    /// still read as *later* than that step.
    #[serde(default)]
    pub seq: u64,
    /// Snapshot authority, bumped by the store on every write.
    pub revision: u64,
}

/// What a run is for — the workspace itself, or a goal. The two differ in
/// three ways and in nothing else the machine decides: a goal's runs take
/// turns (one live, the rest queued) while the workspace's all start at
/// once; a goal carries the budget its runs spend against while a run of the
/// workspace carries its own; and a goal is the home a goal's run is filed
/// under while a run of the workspace is its own ([`WorkflowRun::home`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "scope")]
pub enum RunScope {
    /// A run of the workspace: started from its workflow — *Run…*,
    /// `bisa workflow run`, one of its start events while it is On — at once,
    /// beside any other. Its ceiling is frozen when it is made: the listening
    /// budget, else the workspace default (`budget.default.*`); unlimited is
    /// none of the three.
    Workspace {
        #[serde(default, skip_serializing_if = "Budget::is_unlimited")]
        budget: Budget,
    },
    /// A goal's run: one live at a time, the rest queued behind it.
    Goal { goal: GoalId },
}

impl RunScope {
    /// Every kind — the words the wire and the index share.
    pub const NAMES: [&'static str; 2] = ["workspace", "goal"];

    pub fn as_str(&self) -> &'static str {
        match self {
            RunScope::Workspace { .. } => "workspace",
            RunScope::Goal { .. } => "goal",
        }
    }

    /// The goal this run is for, when it is a goal's.
    pub fn goal(&self) -> Option<GoalId> {
        match self {
            RunScope::Workspace { .. } => None,
            RunScope::Goal { goal } => Some(*goal),
        }
    }

    pub fn is_workspace(&self) -> bool {
        matches!(self, RunScope::Workspace { .. })
    }

    /// The ceiling a run of the workspace carries. A goal's run has none of
    /// its own: it spends against its goal's.
    pub fn budget(&self) -> Option<&Budget> {
        match self {
            RunScope::Workspace { budget } => Some(budget),
            RunScope::Goal { .. } => None,
        }
    }
}

/// What has happened to one step in this run.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StepRecord {
    #[serde(default)]
    pub state: StepState,
    /// Times entered.
    #[serde(default)]
    pub visits: u8,
    /// The step was entered once more than its `max_visits` allows, and
    /// failed for it. A spent step takes no further arrival — a flow into it
    /// is dropped — so a ring whose steps pass over their own failures ends.
    /// Its visits come back with a loop's next iteration, which starts its
    /// body's count afresh, and with an amendment that raises the bound.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub spent: bool,
    /// Failed attempts in the current visit.
    #[serde(default)]
    pub attempts: u8,
    /// How many restarts cut this step short while it ran. Past
    /// [`MAX_INTERRUPTIONS`] the step fails instead of running again: a step
    /// that takes the node down with it is not resumed on every boot.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub interruptions: u8,
    /// The run's `seq` when this record last changed state.
    #[serde(default)]
    pub seq: u64,
    /// The run's `seq` when the step was last entered. What tells an `any`
    /// join a fresh round from the slower arm of the one it already took:
    /// a source entered after the join last changed is new; one entered
    /// before it is the same fan-out arriving late.
    #[serde(default)]
    pub entered: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<Answer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_item: Option<WorkItemId>,
    /// The decision that completed an `approval` step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Where a `for_each` or a `while` stands between iterations; `None`
    /// outside a loop. The one field an entry does not reset — save an
    /// entry from outside the loop's body, which starts afresh.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<LoopCursor>,
    /// The boundary events that fired during this visit, by name: how many
    /// times, and when last. A reminder re-arms from here — never more than
    /// its `max`, across restarts — and an entry resets it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fired: BTreeMap<Branch, Fired>,
}

/// One boundary event's fires in the current visit of its step.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Fired {
    pub count: u32,
    /// The run's `seq` at the last fire.
    pub seq: u64,
    pub at: u64,
}

/// A loop kind's place in its iterations.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoopCursor {
    /// The array `items` rendered to, parsed once when the loop started;
    /// `None` for a `while`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<serde_json::Value>>,
    /// Iterations dispatched so far — the index of the next one.
    pub index: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "state")]
pub enum StepState {
    #[default]
    Pending,
    Running,
    Waiting,
    /// Finished. A gateway names the branches it chose — one, or every one
    /// that held — and its labelled flows with those names are taken; with
    /// none named, the unlabelled flows are.
    Done {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        branches: Vec<Branch>,
    },
    Skipped,
    Failed,
    Cancelled,
    /// Stopped by the boundary event `by`: its work was cancelled, and only
    /// the flows labelled `by` are taken.
    Diverted {
        by: Branch,
    },
}

impl StepState {
    pub const NAMES: [&'static str; 8] = [
        "pending",
        "running",
        "waiting",
        "done",
        "skipped",
        "failed",
        "cancelled",
        "diverted",
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            StepState::Pending => "pending",
            StepState::Running => "running",
            StepState::Waiting => "waiting",
            StepState::Done { .. } => "done",
            StepState::Skipped => "skipped",
            StepState::Failed => "failed",
            StepState::Cancelled => "cancelled",
            StepState::Diverted { .. } => "diverted",
        }
    }

    /// Done with no branch named: what a task, an event or a `parallel`
    /// finishes as.
    pub fn done() -> Self {
        StepState::Done { branches: vec![] }
    }

    /// Done, choosing one branch.
    pub fn chose(branch: Branch) -> Self {
        StepState::Done {
            branches: vec![branch],
        }
    }

    pub fn is_live(&self) -> bool {
        matches!(self, StepState::Running | StepState::Waiting)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            StepState::Done { .. }
                | StepState::Skipped
                | StepState::Failed
                | StepState::Cancelled
                | StepState::Diverted { .. }
        )
    }
}

/// The only way a run changes. Constructing one is the proof the move exists;
/// `apply` decides whether it is legal *here*.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "event")]
pub enum RunEvent {
    Start,
    /// The engine bound a work item to a running `agent` step.
    StepStarted {
        step: StepId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        work_item: Option<WorkItemId>,
    },
    StepDone {
        step: StepId,
        #[serde(default)]
        output: serde_json::Value,
    },
    StepFailed {
        step: StepId,
        error: String,
    },
    /// The step failed and **must not be tried again**, whatever its
    /// `retries`: a `connector` write that may already have reached the
    /// platform — a timeout, a lost answer, a 5xx after sending — with no
    /// idempotency key to make a resend safe. The step fails at once and
    /// `on_fail` decides; the reason tells the person to look at the
    /// platform before running it again.
    StepStopped {
        step: StepId,
        error: String,
    },
    /// A restart found the step live with nothing behind it: the session,
    /// the command, the call or the post died with the last process. Not a
    /// failure of the step's own — `attempts` is untouched — and what
    /// follows is the kind's: an `agent` resumes on the work item it already
    /// has, a `check` runs again, the rest run again only within their
    /// `retries` and fail otherwise.
    StepInterrupted {
        step: StepId,
    },
    Answered {
        step: StepId,
        answer: Answer,
    },
    Decided {
        step: StepId,
        approve: bool,
        approval: ApprovalId,
    },
    /// A `wait` heard what it waits for — a named signal, a message, a
    /// project's change, a run's end, a platform topic. The payload is the
    /// step's output; the chain behind it widens the run's.
    Heard {
        step: StepId,
        #[serde(default)]
        payload: serde_json::Value,
        #[serde(default, skip_serializing_if = "Chain::is_empty")]
        chain: Chain,
    },
    /// A `wait`'s delay, moment or schedule came due.
    Elapsed {
        step: StepId,
    },
    /// A person — or a call — released a `wait` or a `spawn` that waits; a
    /// payload, when given, is the step's output.
    Released {
        step: StepId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        payload: Option<serde_json::Value>,
    },
    /// One of a live step's boundary events fired, for the visit that began
    /// at `entered` (the step record's `entered`): a divert stops the step, an
    /// act runs beside it. A fire for an earlier visit is stale and refused.
    BoundaryFired {
        step: StepId,
        boundary: Branch,
        entered: u64,
        #[serde(default)]
        payload: serde_json::Value,
        #[serde(default, skip_serializing_if = "Chain::is_empty")]
        chain: Chain,
    },
    /// Replace the steps that have not started.
    Amended {
        workflow: Workflow,
    },
    /// End the run for the cause named: every live step is cancelled and
    /// the run is finished. Accepted on a queued run too, where nothing is
    /// live and the only effect is [`RunEffect::Cancelled`].
    Cancel {
        cause: CancelCause,
    },
}

/// Why a run was cancelled. The one word the run keeps, the journal
/// records and every screen prints; a `ClosureReason` is the goal's.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "cause")]
pub enum CancelCause {
    /// A person stopped it: a goal's live run, its queue withdrawn with it
    /// (the goal stays open, ready for a new run), or one run of the
    /// workspace.
    Stopped {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
    },
    /// A person restarted it: this run ended and a new run of the same
    /// workflow and inputs started in its place — ahead of a goal's queue,
    /// or beside the workspace's other runs.
    Restarted,
    /// A person took this run out of its goal's queue before it started.
    Withdrawn,
    /// The goal was closed while this run was live or queued. Nested rather
    /// than flattened: a flattened tagged enum inside a tagged enum is a
    /// schema the desktop's type generator cannot read.
    Closed { reason: ClosureReason },
    /// Its workflow was archived or deleted while this run of the workspace
    /// went: retiring a workflow retires its runs first.
    Retired,
}

impl CancelCause {
    pub fn as_str(&self) -> &'static str {
        match self {
            CancelCause::Stopped { .. } => "stopped",
            CancelCause::Restarted => "restarted",
            CancelCause::Withdrawn => "withdrawn",
            CancelCause::Closed { .. } => "closed",
            CancelCause::Retired => "retired",
        }
    }
}

/// The output key a `judge` step's pick is read from.
pub const JUDGE_CHOICE: &str = "choice";

/// The branch a finished `judge` takes: the option its output names, or
/// `otherwise` — for a pick that is no option of the step, and for none.
/// `None` for every other kind.
pub fn judged_branch(kind: &StepKind, output: &serde_json::Value) -> Option<Branch> {
    let StepKind::Judge {
        options, otherwise, ..
    } = kind
    else {
        return None;
    };
    let picked = output.get(JUDGE_CHOICE).and_then(|c| c.as_str());
    Some(
        options
            .iter()
            .find(|o| Some(o.branch.as_str()) == picked)
            .map(|o| o.branch.clone())
            .unwrap_or_else(|| otherwise.clone()),
    )
}

/// The one reason a restart writes on a step it interrupted.
pub const INTERRUPTED: &str = "interrupted by a restart";

/// How many restarts may cut one step short before it fails for it
/// ([`StepRecord::interruptions`]).
pub const MAX_INTERRUPTIONS: u8 = 3;

/// The reason a step fails once restarts cut it short past
/// [`MAX_INTERRUPTIONS`].
pub const INTERRUPTED_TOO_OFTEN: &str =
    "interrupted by a restart too many times; the step is not resumed again";

fn is_zero(n: &u8) -> bool {
    *n == 0
}

/// The reason an `end` step with `finish = "failed"` records on itself.
pub const ENDED_FAILED: &str = "the workflow ends here as failed";

/// How many rounds one settle may take before the run is failed for it
/// ([`WorkflowRun::apply`]). No graph a person draws comes near: a round
/// enters, skips or fails at least one step.
pub const SETTLE_ROUNDS: usize = 10_000;

/// The reason a run that reached [`SETTLE_ROUNDS`] records on the step it
/// was entering.
pub const UNSETTLED: &str = "the run did not settle";

/// The reason a `connector` write that may have reached the platform fails
/// with, instead of being sent again: on a retry, and at a restart.
pub const AMBIGUOUS_WRITE: &str = "the platform may have received it — check there before running this step again; an operation with an idempotency header would be sent again safely";

/// Which kinds run *in the engine's process* — a session, a command, a call,
/// a post, a spawn — and so die with it, which is what a restart interrupts.
/// The other kinds wait on the world and are re-armed or rebuilt instead.
/// Exhaustive on purpose: a new kind must say which side it is on.
pub fn dies_with_the_process(kind: &StepKind) -> bool {
    match kind {
        StepKind::Agent { .. }
        | StepKind::Check { .. }
        | StepKind::Connector { .. }
        | StepKind::Judge { .. }
        | StepKind::Notify { .. }
        | StepKind::Emit { .. }
        | StepKind::Spawn { .. } => true,
        StepKind::Start { .. }
        | StepKind::Human { .. }
        | StepKind::Approval { .. }
        | StepKind::Wait { .. }
        | StepKind::Decide { .. }
        | StepKind::If { .. }
        | StepKind::Switch { .. }
        | StepKind::Parallel
        | StepKind::ForEach { .. }
        | StepKind::While { .. }
        | StepKind::End { .. } => false,
    }
}

/// Who moves a step that is `Waiting`: a person, or the outside world. An
/// exhaustive match, so a new step kind fails to compile here rather than
/// silently reading as somebody's move.
fn waiting_holder(kind: &StepKind) -> Holder {
    match kind {
        StepKind::Human { .. } | StepKind::Approval { .. } => Holder::You,
        StepKind::Wait {
            until: WaitFor::Release,
        } => Holder::You,
        StepKind::Wait { .. } | StepKind::Spawn { .. } => Holder::World,
        // These kinds never wait; a record that claims so still gets a total
        // answer rather than a panic.
        StepKind::Start { .. }
        | StepKind::Parallel
        | StepKind::Emit { .. }
        | StepKind::Agent { .. }
        | StepKind::Check { .. }
        | StepKind::Decide { .. }
        | StepKind::If { .. }
        | StepKind::Switch { .. }
        | StepKind::Judge { .. }
        | StepKind::ForEach { .. }
        | StepKind::While { .. }
        | StepKind::Connector { .. }
        | StepKind::Notify { .. }
        | StepKind::End { .. } => Holder::Agents,
    }
}

impl RunEvent {
    pub fn name(&self) -> &'static str {
        match self {
            RunEvent::Start => "start",
            RunEvent::StepStarted { .. } => "step_started",
            RunEvent::StepDone { .. } => "step_done",
            RunEvent::StepFailed { .. } => "step_failed",
            RunEvent::StepStopped { .. } => "step_stopped",
            RunEvent::StepInterrupted { .. } => "step_interrupted",
            RunEvent::Answered { .. } => "answered",
            RunEvent::Decided { .. } => "decided",
            RunEvent::Heard { .. } => "heard",
            RunEvent::Elapsed { .. } => "elapsed",
            RunEvent::Released { .. } => "released",
            RunEvent::BoundaryFired { .. } => "boundary_fired",
            RunEvent::Amended { .. } => "amended",
            RunEvent::Cancel { .. } => "cancel",
        }
    }

    /// The step this event is about, when it is about one.
    pub fn step(&self) -> Option<&StepId> {
        match self {
            RunEvent::StepStarted { step, .. }
            | RunEvent::StepDone { step, .. }
            | RunEvent::StepFailed { step, .. }
            | RunEvent::StepStopped { step, .. }
            | RunEvent::StepInterrupted { step }
            | RunEvent::Answered { step, .. }
            | RunEvent::Decided { step, .. }
            | RunEvent::Heard { step, .. }
            | RunEvent::Elapsed { step }
            | RunEvent::Released { step, .. }
            | RunEvent::BoundaryFired { step, .. } => Some(step),
            RunEvent::Start | RunEvent::Amended { .. } | RunEvent::Cancel { .. } => None,
        }
    }
}

/// What the engine must do because the run moved. Data, not behaviour.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "effect")]
pub enum RunEffect {
    StartAgent {
        step: StepId,
    },
    /// Relaunch the work item an `agent` step already has — after a restart,
    /// in the checkout the item already holds — rather than mint another.
    ResumeAgent {
        step: StepId,
        work_item: WorkItemId,
    },
    Ask {
        step: StepId,
    },
    OpenGate {
        step: StepId,
    },
    RunCheck {
        step: StepId,
    },
    /// Call the connector operation a `connector` step names, as the run's
    /// account for it.
    CallConnector {
        step: StepId,
    },
    /// Put a `judge` step's question to the Decision-Making Agent.
    Judge {
        step: StepId,
    },
    Arm {
        step: StepId,
        until: WaitFor,
    },
    Post {
        step: StepId,
    },
    /// Raise the named signal an `emit` step names.
    Emit {
        step: StepId,
    },
    SpawnGoal {
        step: StepId,
    },
    /// Do what a boundary event that does not divert does, beside its live
    /// step: post, or raise a signal.
    BoundaryAct {
        step: StepId,
        boundary: Branch,
    },
    /// These steps were live and are no longer: cancel their work items,
    /// withdraw their questions, disarm their waits.
    CancelWork {
        steps: Vec<StepId>,
    },
    /// The run reached an outcome. Absorbing: nothing moves after it.
    Finished {
        outcome: RunOutcome,
    },
    /// The run was cancelled for this cause. Absorbing like `Finished`, and
    /// settled by the same engine path: what was held for the run is
    /// released, and — for a goal's run — the goal's next queued run starts.
    Cancelled {
        cause: CancelCause,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RunError {
    #[error("the run has not started")]
    NotStarted,
    /// The run has nowhere to begin: the start it names is gone or is no
    /// start, or — a run by hand — the workflow has no start by hand.
    #[error("the run has no way in: its start is gone, or the workflow has no start by hand")]
    NoStart,
    /// An amendment must be the same workflow at a later revision, never a
    /// different definition swapped in under the run.
    #[error("the amendment is workflow {got}, but this run is running {expected}")]
    AmendChangesWorkflow { expected: String, got: String },
    /// The amended definition cannot read the inputs the run was started
    /// with: a new required input has no default, or a kind changed.
    #[error("the amendment cannot bind this run's inputs: {0}")]
    AmendNeedsInput(#[from] InputError),
    #[error("the run has already started")]
    AlreadyStarted,
    #[error("the run is finished; nothing moves")]
    Finished,
    #[error("step `{step}` is not in this run")]
    UnknownStep { step: String },
    #[error("cannot {event} step `{step}`: it is {state}, not {expected}")]
    NotLive {
        step: String,
        event: &'static str,
        state: &'static str,
        expected: &'static str,
    },
    #[error("cannot {event} step `{step}`: it is {kind}, and only {expected} accepts that")]
    WrongKind {
        step: String,
        event: &'static str,
        kind: &'static str,
        expected: &'static str,
    },
    #[error("step `{step}`: {source}")]
    BadAnswer {
        step: String,
        #[source]
        source: AnswerError,
    },
    #[error("an amendment may not change step `{step}`, which has started")]
    AmendTouchesStartedStep { step: String },
    /// A boundary event fired for an earlier visit of its step, or a reminder
    /// past its `max`: it no longer applies.
    #[error("boundary event `{boundary}` of step `{step}` no longer applies")]
    StaleBoundary { step: String, boundary: String },
    #[error("step `{step}` has no boundary event `{boundary}`")]
    UnknownBoundary { step: String, boundary: String },
}

/// A projection of the records, never stored as truth.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Made behind its goal's live run; starts on its own, in order, when the
    /// goal has none. A run of the workspace is never queued.
    Queued,
    Running,
    Waiting,
    Done,
    Failed,
    Cancelled,
}

impl RunStatus {
    pub const ALL: [RunStatus; 6] = [
        RunStatus::Queued,
        RunStatus::Running,
        RunStatus::Waiting,
        RunStatus::Done,
        RunStatus::Failed,
        RunStatus::Cancelled,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            RunStatus::Queued => "queued",
            RunStatus::Running => "running",
            RunStatus::Waiting => "waiting",
            RunStatus::Done => "done",
            RunStatus::Failed => "failed",
            RunStatus::Cancelled => "cancelled",
        }
    }

    pub fn is_finished(self) -> bool {
        matches!(
            self,
            RunStatus::Done | RunStatus::Failed | RunStatus::Cancelled
        )
    }

    /// Started and not finished: a goal's one executing run, or any of the
    /// workspace's.
    pub fn is_live(self) -> bool {
        matches!(self, RunStatus::Running | RunStatus::Waiting)
    }
}

/// How an edge into a step stands right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edge {
    /// Its source finished the way the edge needs.
    Taken,
    /// Its source can never take it (skipped, cancelled, chose another branch).
    Dead,
    /// Its source is still live.
    Open,
}

/// Where a run begins and what began it — the constructor's parameter
/// object. By hand, it names neither: the run begins at the manual entry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunEntry {
    /// The start step it begins at.
    pub step: Option<StepId>,
    /// The occurrence that began it, or a test run's sample.
    pub event: Option<Signal>,
}

impl RunEntry {
    /// A run a person starts: at the manual entry, with no event.
    pub fn by_hand() -> Self {
        Self::default()
    }

    /// A run that begins at `step` on `event`.
    pub fn at(step: StepId, event: Option<Signal>) -> Self {
        Self {
            step: Some(step),
            event,
        }
    }
}

/// Which way in a run is asked through: how every surface that begins a run
/// reads the start and the event it was given — a route's body, a command's
/// flags — so a run begins the same way whoever asks.
#[derive(Clone, Debug, PartialEq)]
pub enum WayIn {
    /// No start named: a person's start. A goal whose workflow begins on
    /// events listens; anything else runs by hand.
    Start,
    /// The start by hand, named: a run now, whatever else the workflow
    /// begins on.
    ByHand,
    /// An event start, named: a test run, begun there as if its event had
    /// happened with `payload` — the sample payload itself, never a whole
    /// signal; nothing when none was given.
    Test {
        start: StepId,
        payload: serde_json::Value,
    },
}

/// Why a start and an event name no way in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoWayIn {
    /// An event, and no start to read it.
    EventWithoutStart,
    /// The start a person runs by hand reads no event.
    ByHandReadsNoEvent { step: StepId },
}

impl WayIn {
    /// What `start` and `event` ask of `workflow` — absent when the run's
    /// home has none yet, which whoever makes the run refuses in its own
    /// words. A step that is no start of the workflow is read as a test
    /// run's here, and refused where the run is made.
    pub fn of(
        workflow: Option<&Workflow>,
        start: Option<StepId>,
        event: Option<serde_json::Value>,
    ) -> Result<Self, NoWayIn> {
        let by_hand = |step: &StepId| {
            workflow
                .and_then(Workflow::manual_entry)
                .is_some_and(|entry| &entry.id == step)
        };
        match (start, event) {
            (None, None) => Ok(WayIn::Start),
            (None, Some(_)) => Err(NoWayIn::EventWithoutStart),
            (Some(step), None) if by_hand(&step) => Ok(WayIn::ByHand),
            (Some(step), Some(_)) if by_hand(&step) => Err(NoWayIn::ByHandReadsNoEvent { step }),
            (Some(start), event) => Ok(WayIn::Test {
                start,
                payload: event.unwrap_or_else(|| serde_json::json!({})),
            }),
        }
    }
}

impl WorkflowRun {
    /// The setting that bounds a workflow's run history in the workspace:
    /// how many finished runs of the workspace a workflow keeps. The oldest
    /// beyond it are put away — folder and rows — when a run of it ends;
    /// live and queued runs are never counted.
    pub const SETTING_KEPT: &'static str = "workflow.runs.keep";

    /// A run not started yet: every step `Pending`, `Start` still required.
    /// The store starts a run of the workspace at once and a goal's when the
    /// goal has nothing live. The run's chain is its event's.
    pub fn new(
        id: RunId,
        scope: RunScope,
        workflow: Workflow,
        inputs: BTreeMap<String, serde_json::Value>,
        entry: RunEntry,
        queued_at: u64,
    ) -> Self {
        let steps = workflow
            .steps
            .iter()
            .map(|s| (s.id.clone(), StepRecord::default()))
            .collect();
        let chain = entry
            .event
            .as_ref()
            .map(|e| e.chain.clone())
            .unwrap_or_default();
        Self {
            id,
            scope,
            workflow,
            inputs,
            start: entry.step,
            event: entry.event,
            dispatched: None,
            chain,
            steps,
            queued_at,
            started_at: None,
            finished_at: None,
            outcome: None,
            cancelled: None,
            seq: 0,
            revision: 0,
        }
    }

    pub fn is_finished(&self) -> bool {
        self.finished_at.is_some()
    }

    /// The step this run begins at: the start it names, or — a run by hand —
    /// the workflow's manual entry. `None` when there is no such way in.
    pub fn entry_step(&self) -> Option<&Step> {
        match &self.start {
            Some(id) => self
                .workflow
                .step(id)
                .filter(|_| self.workflow.is_entry(id)),
            None => self.workflow.manual_entry(),
        }
    }

    /// Whether `id` is a start this run did not begin at: skipped at `Start`,
    /// never run, free to change under an amendment.
    fn is_unused_start(&self, id: &StepId) -> bool {
        self.workflow
            .step(id)
            .is_some_and(|s| matches!(s.kind, StepKind::Start { .. }))
            && self.start.as_ref() != Some(id)
    }

    /// Where this run's truth is filed: its goal, or — a run of the
    /// workspace — itself.
    pub fn home(&self) -> Home {
        match &self.scope {
            RunScope::Goal { goal } => Home::Goal { goal: *goal },
            RunScope::Workspace { .. } => Home::Run { run: self.id },
        }
    }

    /// Not started yet: waiting its turn behind its goal's live run. A run of
    /// the workspace never is.
    pub fn is_queued(&self) -> bool {
        self.started_at.is_none() && !self.is_finished()
    }

    /// Who this run waits on — the run's rungs of the one ladder every
    /// surface prints ([`crate::Goal::holder`] adds the goal's own: closed,
    /// and no run yet). `owed` is whether anything durable names the run's
    /// home — a pending gate, a question — which the caller reads from the
    /// inbox: somebody owes an answer, so it is yours whatever the steps do.
    pub fn holder(&self, owed: bool) -> Holder {
        if owed {
            return Holder::You;
        }
        if self.outcome.is_some() || self.cancelled.is_some() {
            return Holder::Finished;
        }
        if self.is_queued() {
            // Not started: the platform's move next.
            return Holder::Agents;
        }
        let mut any_running = false;
        let mut any_world = false;
        for step in &self.workflow.steps {
            let Some(record) = self.steps.get(&step.id) else {
                continue;
            };
            match record.state {
                StepState::Waiting => match waiting_holder(&step.kind) {
                    Holder::You => return Holder::You,
                    Holder::World => any_world = true,
                    _ => any_running = true,
                },
                StepState::Running => any_running = true,
                _ => {}
            }
        }
        if any_running {
            Holder::Agents
        } else if any_world {
            Holder::World
        } else {
            // Nothing live and not finished: the run is settling between
            // events; the next move is the engine's.
            Holder::Agents
        }
    }

    /// Started and not finished.
    pub fn is_live(&self) -> bool {
        self.started_at.is_some() && !self.is_finished()
    }

    pub fn status(&self) -> RunStatus {
        if self.cancelled.is_some() {
            return RunStatus::Cancelled;
        }
        match self.outcome {
            Some(RunOutcome::Done) => RunStatus::Done,
            Some(RunOutcome::Failed) => RunStatus::Failed,
            None if self.started_at.is_none() => RunStatus::Queued,
            None => {
                if self.steps.values().any(|r| r.state == StepState::Running) {
                    RunStatus::Running
                } else {
                    RunStatus::Waiting
                }
            }
        }
    }

    /// The steps that are live right now.
    pub fn live_steps(&self) -> Vec<&StepId> {
        self.steps
            .iter()
            .filter(|(_, r)| r.state.is_live())
            .map(|(id, _)| id)
            .collect()
    }

    fn started(&self) -> bool {
        self.started_at.is_some()
    }

    /// Which step-bound events a kind accepts. The table `apply` agrees with;
    /// non-step events are always accepted here (their own checks are on the
    /// run).
    pub fn accepts(kind: &StepKind, event: &RunEvent) -> bool {
        match event {
            RunEvent::Start | RunEvent::Amended { .. } | RunEvent::Cancel { .. } => true,
            RunEvent::StepStarted { .. } => matches!(kind, StepKind::Agent { .. }),
            RunEvent::StepDone { .. } | RunEvent::StepInterrupted { .. } => {
                dies_with_the_process(kind)
            }
            // A human step fails too: the person's last *not sure*. So does a
            // wait: one that cannot be armed, or a schedule with no next time.
            RunEvent::StepFailed { .. } => {
                dies_with_the_process(kind)
                    || matches!(kind, StepKind::Human { .. } | StepKind::Wait { .. })
            }
            RunEvent::StepStopped { .. } => matches!(kind, StepKind::Connector { .. }),
            RunEvent::Answered { .. } => matches!(kind, StepKind::Human { .. }),
            RunEvent::Decided { .. } => matches!(kind, StepKind::Approval { .. }),
            RunEvent::Heard { .. } => {
                matches!(kind, StepKind::Wait { until } if until.is_catch())
            }
            RunEvent::Elapsed { .. } => {
                matches!(kind, StepKind::Wait { until } if until.is_timer())
            }
            RunEvent::Released { .. } => {
                matches!(
                    kind,
                    StepKind::Wait { .. } | StepKind::Spawn { wait: true, .. }
                )
            }
            RunEvent::BoundaryFired { .. } => may_carry_boundaries(kind),
        }
    }

    /// The state a kind is in while it is live.
    pub fn live_state(kind: &StepKind) -> StepState {
        match kind {
            StepKind::Human { .. }
            | StepKind::Approval { .. }
            | StepKind::Wait { .. }
            | StepKind::Spawn { wait: true, .. } => StepState::Waiting,
            _ => StepState::Running,
        }
    }

    /// The names of the kinds that accept `event`, for the error message.
    fn accepting_kinds(event: &RunEvent) -> &'static str {
        match event {
            RunEvent::StepStarted { .. } => "agent",
            RunEvent::StepDone { .. } | RunEvent::StepInterrupted { .. } => {
                "agent, check, connector, judge, notify, emit or spawn"
            }
            RunEvent::StepFailed { .. } => {
                "agent, check, connector, judge, notify, emit, spawn, human or wait"
            }
            RunEvent::StepStopped { .. } => "connector",
            RunEvent::Answered { .. } => "human",
            RunEvent::Decided { .. } => "approval",
            RunEvent::Heard { .. } => "wait (signal, message, project, run or platform)",
            RunEvent::Elapsed { .. } => "wait (delay, time or schedule)",
            RunEvent::Released { .. } => "wait or spawn",
            RunEvent::BoundaryFired { .. } => "agent, human, approval, wait or a spawn that waits",
            RunEvent::Start | RunEvent::Amended { .. } | RunEvent::Cancel { .. } => "the run",
        }
    }

    /// The one exhaustive entry point. On `Err` the run is unchanged.
    pub fn apply(&mut self, event: RunEvent, now: u64) -> Result<Vec<RunEffect>, RunError> {
        if self.is_finished() {
            return Err(RunError::Finished);
        }
        // Check everything before touching anything, so an error leaves the
        // run byte-identical.
        //
        // A withdrawal takes a run out of a queue: one that has started is
        // no longer in one. The rule is here, where the run moves under its
        // one writer, so a look before the cancel is never what decides it.
        let withdrawal = matches!(
            event,
            RunEvent::Cancel {
                cause: CancelCause::Withdrawn
            }
        );
        if withdrawal && self.started() {
            return Err(RunError::AlreadyStarted);
        }
        let step_def = match event.step() {
            Some(id) => {
                if !self.started() {
                    return Err(RunError::NotStarted);
                }
                let def = self
                    .workflow
                    .step(id)
                    .ok_or_else(|| RunError::UnknownStep {
                        step: id.to_string(),
                    })?
                    .clone();
                if !Self::accepts(&def.kind, &event) {
                    return Err(RunError::WrongKind {
                        step: id.to_string(),
                        event: event.name(),
                        kind: def.kind.as_str(),
                        expected: Self::accepting_kinds(&event),
                    });
                }
                let expected = Self::live_state(&def.kind);
                let record = self.steps.get(id).ok_or_else(|| RunError::UnknownStep {
                    step: id.to_string(),
                })?;
                if record.state != expected {
                    return Err(RunError::NotLive {
                        step: id.to_string(),
                        event: event.name(),
                        state: record.state.as_str(),
                        expected: expected.as_str(),
                    });
                }
                Some(def)
            }
            None => None,
        };
        if let RunEvent::Answered { step, answer } = &event {
            if let Some(StepKind::Human { options, multi, .. }) = step_def.as_ref().map(|d| &d.kind)
            {
                let kind = AskKind::Answer {
                    options: options.clone(),
                    multi: *multi,
                };
                kind.validate_answer(Some(answer))
                    .map_err(|source| RunError::BadAnswer {
                        step: step.to_string(),
                        source,
                    })?;
            }
        }
        if let RunEvent::BoundaryFired {
            step,
            boundary,
            entered,
            ..
        } = &event
        {
            // Checked above: the step exists, may carry boundaries, and is live.
            let refused = |stale: bool| {
                let (step, boundary) = (step.to_string(), boundary.to_string());
                if stale {
                    RunError::StaleBoundary { step, boundary }
                } else {
                    RunError::UnknownBoundary { step, boundary }
                }
            };
            let def = step_def
                .as_ref()
                .and_then(|d| d.boundary(boundary))
                .ok_or_else(|| refused(false))?;
            let record = self.steps.get(step);
            let fired = record
                .and_then(|r| r.fired.get(boundary))
                .map_or(0, |f| f.count);
            if record.map(|r| r.entered) != Some(*entered) || !def.on.may_fire_again(fired) {
                return Err(refused(true));
            }
        }
        let amended_inputs = match &event {
            RunEvent::Amended { workflow } => Some(self.check_amendment(workflow)?),
            _ => None,
        };
        if matches!(event, RunEvent::Start) {
            if self.started() {
                return Err(RunError::AlreadyStarted);
            }
            if self.entry_step().is_none() {
                return Err(RunError::NoStart);
            }
        }
        if !matches!(
            event,
            RunEvent::Start | RunEvent::Cancel { .. } | RunEvent::Amended { .. }
        ) && !self.started()
        {
            return Err(RunError::NotStarted);
        }

        self.seq += 1;
        let mut effects = Vec::new();
        match event {
            RunEvent::Start => {
                self.started_at = Some(now);
                // Checked above: the run has a way in. The other starts are
                // skipped before it is entered, so no join waits on them.
                if let Some(entry) = self.entry_step().cloned() {
                    self.start = Some(entry.id.clone());
                    self.skip_unused_starts(now);
                    self.enter(&entry, now, &mut effects);
                }
                self.settle(now, &mut effects);
            }
            RunEvent::StepStarted { step, work_item } => {
                if let Some(r) = self.steps.get_mut(&step) {
                    if work_item.is_some() {
                        r.work_item = work_item;
                    }
                }
            }
            RunEvent::StepDone { step, output } => {
                // A judge's output names the option the Decision-Making
                // Agent took; the branch is read from it against the
                // definition, so the machine stays a function of its events.
                let branches = step_def
                    .as_ref()
                    .and_then(|def| judged_branch(&def.kind, &output))
                    .into_iter()
                    .collect();
                self.mark(&step, StepState::Done { branches }, now, |r| {
                    r.output = Some(output);
                    r.error = None;
                });
                self.settle(now, &mut effects);
            }
            RunEvent::StepFailed { step, error } => {
                let def = step_def.unwrap_or_else(|| Step {
                    id: step.clone(),
                    name: String::new(),
                    kind: StepKind::End {
                        finish: Finish::Failed,
                    },
                    then: vec![],
                    boundaries: vec![],
                    join: Join::All,
                    on_fail: OnFail::Fail,
                    retries: 0,
                    max_visits: 1,
                    position: None,
                });
                let attempts = self
                    .steps
                    .get_mut(&step)
                    .map(|r| {
                        r.attempts = r.attempts.saturating_add(1);
                        r.error = Some(error.clone());
                        r.attempts
                    })
                    .unwrap_or(u8::MAX);
                if attempts <= def.retries {
                    // Try again: the same effect, the same live state.
                    if let Some(e) = Self::effect_for(&def) {
                        effects.push(e);
                    }
                } else {
                    self.mark(&step, StepState::Failed, now, |_| {});
                    self.fail_out(&def, now, &mut effects);
                    self.settle(now, &mut effects);
                }
            }
            RunEvent::StepStopped { step, error } => {
                // No retry, whatever the author allowed: the attempt counts,
                // the step fails, `on_fail` decides.
                let def = step_def.clone().unwrap_or_else(|| Step {
                    id: step.clone(),
                    name: String::new(),
                    kind: StepKind::End {
                        finish: Finish::Failed,
                    },
                    then: vec![],
                    boundaries: vec![],
                    join: Join::All,
                    on_fail: OnFail::Fail,
                    retries: 0,
                    max_visits: 1,
                    position: None,
                });
                if let Some(r) = self.steps.get_mut(&step) {
                    r.attempts = r.attempts.saturating_add(1);
                    r.error = Some(error.clone());
                }
                self.mark(&step, StepState::Failed, now, |_| {});
                self.fail_out(&def, now, &mut effects);
                self.settle(now, &mut effects);
            }
            RunEvent::StepInterrupted { step } => {
                // Checked above: the step exists, accepts the event, is live.
                let def = step_def.clone().unwrap_or_else(|| Step {
                    id: step.clone(),
                    name: String::new(),
                    kind: StepKind::End {
                        finish: Finish::Failed,
                    },
                    then: vec![],
                    boundaries: vec![],
                    join: Join::All,
                    on_fail: OnFail::Fail,
                    retries: 0,
                    max_visits: 1,
                    position: None,
                });
                let (attempts, work_item, too_often) = self
                    .steps
                    .get_mut(&step)
                    .map(|r| {
                        r.interruptions = r.interruptions.saturating_add(1);
                        let too_often = r.interruptions > MAX_INTERRUPTIONS;
                        r.error = Some(
                            if too_often {
                                INTERRUPTED_TOO_OFTEN
                            } else {
                                INTERRUPTED
                            }
                            .to_string(),
                        );
                        (r.attempts, r.work_item, too_often)
                    })
                    .unwrap_or((u8::MAX, None, false));
                match &def.kind {
                    // Cut short once too often: whatever its kind, the step
                    // fails rather than running again on every boot.
                    _ if too_often => {
                        self.mark(&step, StepState::Failed, now, |_| {});
                        self.fail_out(&def, now, &mut effects);
                        self.settle(now, &mut effects);
                    }
                    // The work is in the item's checkout; the session is not
                    // the work. Resume on the item, or start one when the
                    // restart came before an item existed.
                    StepKind::Agent { .. } => effects.push(match work_item {
                        Some(work_item) => RunEffect::ResumeAgent {
                            step: step.clone(),
                            work_item,
                        },
                        None => RunEffect::StartAgent { step: step.clone() },
                    }),
                    // A check is a read of the world: running it again costs
                    // nothing it did not already cost.
                    StepKind::Check { .. } => {
                        effects.push(RunEffect::RunCheck { step: step.clone() })
                    }
                    // An emit raised again is the same signal: its key is the
                    // run, the step and the visit.
                    StepKind::Emit { .. } => effects.push(RunEffect::Emit { step: step.clone() }),
                    // A call, a post, a spawn may have gone out: only an
                    // author who allowed a repeat gets one, and the
                    // interruption is not counted against them.
                    _ if attempts < def.retries => {
                        if let Some(e) = Self::effect_for(&def) {
                            effects.push(e);
                        }
                    }
                    _ => {
                        self.mark(&step, StepState::Failed, now, |_| {});
                        self.fail_out(&def, now, &mut effects);
                        self.settle(now, &mut effects);
                    }
                }
            }
            RunEvent::Answered { step, answer } => {
                self.mark(&step, StepState::done(), now, |r| {
                    r.answer = Some(answer);
                });
                self.settle(now, &mut effects);
            }
            RunEvent::Decided {
                step,
                approve,
                approval,
            } => {
                if approve {
                    self.mark(&step, StepState::done(), now, |r| {
                        r.gate = Some(approval.0);
                    });
                    self.settle(now, &mut effects);
                } else {
                    self.mark(&step, StepState::Failed, now, |r| {
                        r.gate = Some(approval.0);
                        r.error = Some("declined".into());
                    });
                    if let Some(def) = step_def {
                        self.fail_out(&def, now, &mut effects);
                    }
                    self.settle(now, &mut effects);
                }
            }
            RunEvent::Heard {
                step,
                payload,
                chain,
            } => {
                self.chain.absorb(&chain);
                self.mark(&step, StepState::done(), now, |r| {
                    r.output = Some(payload);
                });
                self.settle(now, &mut effects);
            }
            RunEvent::Elapsed { step } => {
                self.mark(&step, StepState::done(), now, |_| {});
                self.settle(now, &mut effects);
            }
            RunEvent::Released { step, payload } => {
                self.mark(&step, StepState::done(), now, |r| {
                    if payload.is_some() {
                        r.output = payload;
                    }
                });
                self.settle(now, &mut effects);
            }
            RunEvent::BoundaryFired {
                step,
                boundary,
                chain,
                ..
            } => {
                // Checked above: the boundary is the step's and fires for
                // this visit.
                self.chain.absorb(&chain);
                self.seq += 1;
                let seq = self.seq;
                if let Some(r) = self.steps.get_mut(&step) {
                    let fired = r.fired.entry(boundary.clone()).or_default();
                    fired.count = fired.count.saturating_add(1);
                    fired.seq = seq;
                    fired.at = now;
                }
                let diverts = step_def
                    .as_ref()
                    .and_then(|d| d.boundary(&boundary))
                    .is_some_and(|b| b.diverts());
                if diverts {
                    self.mark(
                        &step,
                        StepState::Diverted {
                            by: boundary.clone(),
                        },
                        now,
                        |r| r.error = None,
                    );
                    effects.push(RunEffect::CancelWork { steps: vec![step] });
                    self.settle(now, &mut effects);
                } else {
                    effects.push(RunEffect::BoundaryAct { step, boundary });
                }
            }
            RunEvent::Amended { workflow } => {
                let mut steps = BTreeMap::new();
                for s in &workflow.steps {
                    let record = self.steps.remove(&s.id).unwrap_or_default();
                    steps.insert(s.id.clone(), record);
                }
                self.steps = steps;
                self.workflow = workflow;
                if let Some(inputs) = amended_inputs {
                    self.inputs = inputs;
                }
                // A queued run has not begun: its steps wait for `Start`, and
                // settling them now would read as a stall.
                if self.started() {
                    self.skip_unused_starts(now);
                    self.settle(now, &mut effects);
                }
            }
            RunEvent::Cancel { cause } => {
                let live = self.cancel_live(now);
                if !live.is_empty() {
                    effects.push(RunEffect::CancelWork { steps: live });
                }
                self.cancelled = Some(cause.clone());
                self.finished_at = Some(now);
                effects.push(RunEffect::Cancelled { cause });
            }
        }
        Ok(effects)
    }

    /// An amendment is the same workflow at a later revision; it may add,
    /// edit or remove steps that have not started — the starts the run did not
    /// begin at among them; a step that has must keep its id, its kind and its
    /// boundary events, and a step that has finished must keep its flows —
    /// history is frozen. The start the run began at is such a step. The
    /// run's inputs must still bind: a new input needs a default (which is
    /// filled in), and no kind may change under a value. Returns the inputs
    /// the run holds once amended.
    fn check_amendment(
        &self,
        next: &Workflow,
    ) -> Result<BTreeMap<String, serde_json::Value>, RunError> {
        if next.id != self.workflow.id {
            return Err(RunError::AmendChangesWorkflow {
                expected: self.workflow.id.to_string(),
                got: next.id.to_string(),
            });
        }
        let inputs = next.bind_inputs(self.inputs.clone())?;
        for (id, record) in &self.steps {
            if record.state == StepState::Pending || self.is_unused_start(id) {
                continue;
            }
            let Some(old) = self.workflow.step(id) else {
                continue;
            };
            let refused = || RunError::AmendTouchesStartedStep {
                step: id.to_string(),
            };
            let new = next.step(id).ok_or_else(refused)?;
            if new.kind != old.kind || new.boundaries != old.boundaries {
                return Err(refused());
            }
            if record.state.is_terminal() && new.then != old.then {
                return Err(refused());
            }
        }
        Ok(inputs)
    }

    /// Skip every start the run did not begin at that has not settled: its
    /// paths die, and no join waits on them.
    fn skip_unused_starts(&mut self, now: u64) {
        let unused: Vec<StepId> = self
            .workflow
            .steps
            .iter()
            .filter(|s| self.is_unused_start(&s.id))
            .filter(|s| {
                self.steps
                    .get(&s.id)
                    .is_some_and(|r| r.state == StepState::Pending)
            })
            .map(|s| s.id.clone())
            .collect();
        for id in unused {
            self.mark(&id, StepState::Skipped, now, |_| {});
        }
    }

    fn mark<F: FnOnce(&mut StepRecord)>(
        &mut self,
        step: &StepId,
        state: StepState,
        now: u64,
        fill: F,
    ) {
        self.seq += 1;
        let seq = self.seq;
        if let Some(r) = self.steps.get_mut(step) {
            r.state = state;
            r.seq = seq;
            r.finished_at = Some(now);
            fill(r);
        }
    }

    /// The effect a kind produces on entry, if any.
    fn effect_for(step: &Step) -> Option<RunEffect> {
        let id = step.id.clone();
        Some(match &step.kind {
            StepKind::Agent { .. } => RunEffect::StartAgent { step: id },
            StepKind::Human { .. } => RunEffect::Ask { step: id },
            StepKind::Approval { .. } => RunEffect::OpenGate { step: id },
            StepKind::Check { .. } => RunEffect::RunCheck { step: id },
            StepKind::Connector { .. } => RunEffect::CallConnector { step: id },
            StepKind::Judge { .. } => RunEffect::Judge { step: id },
            StepKind::Wait { until } => RunEffect::Arm {
                step: id,
                until: until.clone(),
            },
            StepKind::Notify { .. } => RunEffect::Post { step: id },
            StepKind::Emit { .. } => RunEffect::Emit { step: id },
            StepKind::Spawn { .. } => RunEffect::SpawnGoal { step: id },
            StepKind::Start { .. }
            | StepKind::Decide { .. }
            | StepKind::If { .. }
            | StepKind::Switch { .. }
            | StepKind::Parallel
            | StepKind::ForEach { .. }
            | StepKind::While { .. }
            | StepKind::End { .. } => return None,
        })
    }

    /// Enter a step: count the visit, and either finish it at once (a start,
    /// a gateway, a loop, an end) or make it live and say what the engine
    /// must do. A loop kind re-entered with a cursor is taking an iteration,
    /// not a visit: neither counted nor bounded here — `max_iterations` is.
    fn enter(&mut self, step: &Step, now: u64, effects: &mut Vec<RunEffect>) {
        self.seq += 1;
        let seq = self.seq;
        let exhausted = {
            let r = self.steps.entry(step.id.clone()).or_default();
            let first = !step.kind.is_loop() || r.cursor.is_none();
            if first && r.visits >= step.max_visits {
                true
            } else {
                if first {
                    r.visits = r.visits.saturating_add(1);
                }
                r.spent = false;
                r.attempts = 0;
                r.seq = seq;
                r.entered = seq;
                r.started_at = Some(now);
                r.finished_at = None;
                r.output = None;
                r.answer = None;
                r.error = None;
                r.gate = None;
                r.work_item = None;
                r.fired.clear();
                false
            }
        };
        if exhausted {
            // Once: a spent step takes no further arrival ([`Self::settle`]),
            // so it fails out here one time and is not entered again.
            let max = step.max_visits;
            self.mark(&step.id, StepState::Failed, now, |r| {
                r.spent = true;
                r.error = Some(format!("entered {max} times; max_visits is {max}"));
            });
            self.fail_out(step, now, effects);
            return;
        }
        match &step.kind {
            // A start is the way in, and a `parallel` every way out at once:
            // both finish on entry and take their unlabelled flows.
            StepKind::Start { .. } | StepKind::Parallel => {
                self.mark(&step.id, StepState::done(), now, |_| {});
            }
            StepKind::Decide {
                rules,
                otherwise,
                pick,
            } => {
                let mut branches: Vec<Branch> = Vec::new();
                for rule in rules {
                    if branches.contains(&rule.branch) || !self.holds(&rule.when, now) {
                        continue;
                    }
                    branches.push(rule.branch.clone());
                    if *pick == Pick::First {
                        break;
                    }
                }
                if branches.is_empty() {
                    branches.push(otherwise.clone());
                }
                self.mark(&step.id, StepState::Done { branches }, now, |_| {});
            }
            StepKind::If { when } => {
                let word = if self.holds(when, now) {
                    branch::YES
                } else {
                    branch::NO
                };
                self.mark(&step.id, StepState::chose(branch::of(word)), now, |_| {});
            }
            StepKind::Switch {
                on,
                cases,
                otherwise,
            } => match self.render(on) {
                Err(e) => self.fail_step(step, format!("cannot render `on`: {e}"), now, effects),
                Ok(text) => {
                    let branch = cases
                        .iter()
                        .find(|c| c.value == text)
                        .map(|c| c.branch.clone())
                        .unwrap_or_else(|| otherwise.clone());
                    self.mark(&step.id, StepState::chose(branch), now, |r| {
                        r.output = Some(serde_json::json!({ "value": text }))
                    });
                }
            },
            StepKind::ForEach {
                items,
                max_iterations,
            } => self.enter_for_each(step, items, *max_iterations, now, effects),
            StepKind::While {
                when,
                max_iterations,
            } => self.enter_while(step, when, *max_iterations, now, effects),
            StepKind::End { finish } => match finish {
                // The path ends here; the run ends when every path has.
                Finish::Path => self.mark(&step.id, StepState::done(), now, |_| {}),
                Finish::Done => {
                    self.mark(&step.id, StepState::done(), now, |_| {});
                    self.finish(RunOutcome::Done, now, effects);
                }
                Finish::Failed => {
                    self.mark(&step.id, StepState::Failed, now, |r| {
                        r.error = Some(ENDED_FAILED.to_string());
                    });
                    self.finish(RunOutcome::Failed, now, effects);
                }
            },
            kind => {
                let state = Self::live_state(kind);
                if let Some(r) = self.steps.get_mut(&step.id) {
                    r.state = state;
                }
                if let Some(e) = Self::effect_for(step) {
                    effects.push(e);
                }
            }
        }
    }

    /// What a condition reads: the run's inputs and records, and the hour
    /// `now` falls in. The machine never reads a clock; the caller passes the
    /// moment.
    fn condition_ctx(&self, now: u64) -> ConditionCtx<'_> {
        ConditionCtx {
            inputs: &self.inputs,
            steps: &self.steps,
            hour: ((now / 3600) % 24) as u8,
        }
    }

    fn holds(&self, when: &Condition, now: u64) -> bool {
        when.holds(&self.condition_ctx(now))
    }

    /// Render a step string inside the machine — a switch's subject, a
    /// loop's items — against the run alone: the machine reads no goal, and
    /// validation refuses `{goal.…}` in those two strings for that reason.
    fn render(&self, tmpl: &str) -> Result<String, TemplateError> {
        let ctx = TemplateCtx {
            inputs: &self.inputs,
            steps: &self.steps,
            event: None,
            goal: None,
            params: template::no_values(),
            account: template::no_values(),
        };
        template::render(tmpl, &ctx)
    }

    /// Fail a step the machine itself judged — a subject that would not
    /// render, a loop past its bound — and apply its `on_fail`.
    fn fail_step(&mut self, step: &Step, error: String, now: u64, effects: &mut Vec<RunEffect>) {
        self.mark(&step.id, StepState::Failed, now, |r| r.error = Some(error));
        self.fail_out(step, now, effects);
    }

    /// A loop dispatched an iteration: its body's steps start their visit
    /// count afresh, so `max_visits` bounds rework inside one iteration and
    /// not the loop itself.
    fn reset_body(&mut self, loop_id: &StepId) {
        for id in self.workflow.loop_body(loop_id) {
            if let Some(r) = self.steps.get_mut(&id) {
                r.visits = 0;
                r.spent = false;
            }
        }
    }

    /// A `for_each`: parse the list on the first entry, then one item per
    /// entry until the list is spent.
    fn enter_for_each(
        &mut self,
        step: &Step,
        items: &str,
        max: u16,
        now: u64,
        effects: &mut Vec<RunEffect>,
    ) {
        let started = self.steps.get(&step.id).is_some_and(|r| r.cursor.is_some());
        if !started {
            // `render` writes a non-string JSON value as compact JSON, so
            // the list comes back through the parser.
            let parsed: Result<Vec<serde_json::Value>, String> = self
                .render(items)
                .map_err(|e| format!("cannot render `items`: {e}"))
                .and_then(|text| {
                    serde_json::from_str::<serde_json::Value>(&text)
                        .map_err(|e| format!("`items` did not render to JSON: {e}"))
                })
                .and_then(|value| {
                    value.as_array().cloned().ok_or_else(|| {
                        format!(
                            "`items` must render to a JSON array; got {}",
                            crate::workflow::first_sentence(&value.to_string(), 80)
                        )
                    })
                });
            let list = match parsed {
                Ok(list) => list,
                Err(e) => return self.fail_step(step, e, now, effects),
            };
            if list.len() > usize::from(max) {
                let n = list.len();
                return self.fail_step(
                    step,
                    format!("{n} items; max_iterations is {max}"),
                    now,
                    effects,
                );
            }
            if let Some(r) = self.steps.get_mut(&step.id) {
                r.cursor = Some(LoopCursor {
                    items: Some(list),
                    index: 0,
                });
            }
        }
        let (next, index, count) = {
            let cursor = self.steps.get(&step.id).and_then(|r| r.cursor.as_ref());
            let items: &[serde_json::Value] =
                cursor.and_then(|c| c.items.as_deref()).unwrap_or(&[]);
            let index = cursor.map(|c| c.index).unwrap_or(0);
            (items.get(index as usize).cloned(), index, items.len())
        };
        match next {
            Some(item) => {
                if let Some(c) = self.steps.get_mut(&step.id).and_then(|r| r.cursor.as_mut()) {
                    c.index += 1;
                }
                self.reset_body(&step.id);
                self.mark(
                    &step.id,
                    StepState::chose(branch::of(branch::EACH)),
                    now,
                    |r| {
                        r.output = Some(serde_json::json!({
                            "item": item,
                            "index": index,
                            "count": count,
                        }));
                    },
                );
            }
            None => {
                if let Some(r) = self.steps.get_mut(&step.id) {
                    r.cursor = None;
                }
                self.mark(
                    &step.id,
                    StepState::chose(branch::of(branch::DONE)),
                    now,
                    |r| {
                        r.output = Some(serde_json::json!({ "index": count, "count": count }));
                    },
                );
            }
        }
    }

    /// A `while`: test the condition on every entry, the first included;
    /// `loop` while it holds and the bound is not reached, else `done`.
    fn enter_while(
        &mut self,
        step: &Step,
        when: &Condition,
        max: u16,
        now: u64,
        effects: &mut Vec<RunEffect>,
    ) {
        let index = {
            let r = self.steps.entry(step.id.clone()).or_default();
            let cursor = r.cursor.get_or_insert(LoopCursor {
                items: None,
                index: 0,
            });
            cursor.index
        };
        if self.holds(when, now) {
            if index >= u32::from(max) {
                if let Some(r) = self.steps.get_mut(&step.id) {
                    r.cursor = None;
                }
                return self.fail_step(
                    step,
                    format!("ran {index} iterations; max_iterations is {max}"),
                    now,
                    effects,
                );
            }
            if let Some(c) = self.steps.get_mut(&step.id).and_then(|r| r.cursor.as_mut()) {
                c.index += 1;
            }
            self.reset_body(&step.id);
            self.mark(
                &step.id,
                StepState::chose(branch::of(branch::LOOP)),
                now,
                |r| r.output = Some(serde_json::json!({ "index": index })),
            );
        } else {
            if let Some(r) = self.steps.get_mut(&step.id) {
                r.cursor = None;
            }
            self.mark(
                &step.id,
                StepState::chose(branch::of(branch::DONE)),
                now,
                |r| r.output = Some(serde_json::json!({ "index": index })),
            );
        }
    }

    /// A step failed for good — its retries are spent, a person declined,
    /// its visits ran out: what `on_fail` says. `fail` ends the run here.
    /// `skip` and `then` end nothing: the failure is a state the flows out
    /// of the step are read from ([`Self::edge`], [`Self::fail_edge`]), and
    /// the settle that follows takes them. It never settles itself — the
    /// fixpoint is one loop ([`Self::settle`]), whatever fails inside it.
    fn fail_out(&mut self, step: &Step, now: u64, effects: &mut Vec<RunEffect>) {
        if self.is_finished() {
            return;
        }
        if matches!(step.on_fail, OnFail::Fail) {
            self.finish(RunOutcome::Failed, now, effects);
        }
    }

    /// How the edge `from --flow--> …` stands. A step done with no branch
    /// named takes its unlabelled flows; one that named branches takes the
    /// flows labelled with them; a diverted step takes only the flows
    /// labelled with the boundary event that diverted it.
    fn edge(&self, from: &Step, flow: &Flow) -> Edge {
        let Some(record) = self.steps.get(&from.id) else {
            return Edge::Dead;
        };
        match &record.state {
            StepState::Pending | StepState::Running | StepState::Waiting => Edge::Open,
            StepState::Skipped | StepState::Cancelled => Edge::Dead,
            StepState::Done { branches } => match &flow.branch {
                None if branches.is_empty() => Edge::Taken,
                Some(want) if branches.contains(want) => Edge::Taken,
                _ => Edge::Dead,
            },
            StepState::Diverted { by } => match &flow.branch {
                Some(want) if want == by => Edge::Taken,
                _ => Edge::Dead,
            },
            StepState::Failed => match &from.on_fail {
                OnFail::Skip if flow.branch.is_none() => Edge::Taken,
                OnFail::Then { step } if &flow.to == step => Edge::Taken,
                _ => Edge::Dead,
            },
        }
    }

    /// How the implicit `on_fail: then` edge out of `from` stands.
    fn fail_edge(&self, from: &Step) -> Edge {
        let Some(record) = self.steps.get(&from.id) else {
            return Edge::Dead;
        };
        match &record.state {
            StepState::Pending | StepState::Running | StepState::Waiting => Edge::Open,
            StepState::Failed => Edge::Taken,
            _ => Edge::Dead,
        }
    }

    /// Run the graph to a fixpoint after a change: skip what can no longer be
    /// reached, enter what is ready, and finish when nothing is live.
    ///
    /// A **loop edge** — a flow or fail route back into a step it leads to
    /// ([`Workflow::loop_edges`]) — never holds an `all` or a `one` join: a
    /// step enters on its forward edges, and the loop edge re-enters it when
    /// it is taken. A `one` join is judged on the arrivals since the step
    /// last finished, so a loop target with one is re-entered rather than
    /// failed for the forward edge it already counted. When
    /// nothing is live and no step is pending, every branch drained and the
    /// run is done. A pending step left behind is a **stall**, never a quiet
    /// success: it is failed with the flows it waited on, and so is the run.
    ///
    /// **One loop, never a call to itself.** A step that fails while the
    /// graph settles — its visits spent, a `switch` that cannot render, a
    /// `one` join with two arrivals — is a change like any other: the loop
    /// goes round again and reads the flows out of it. The stack a settle
    /// takes does not grow with the graph, the visits or the failures.
    ///
    /// **A spent step takes no further arrival.** A step entered once more
    /// than `max_visits` allows fails for it once ([`Self::enter`]); a flow
    /// into it afterwards is dropped, so a ring whose steps pass over their
    /// own failures (`on_fail: skip`) ends when its visits are spent rather
    /// than failing round and round.
    ///
    /// **The rounds are bounded** ([`SETTLE_ROUNDS`]): every round enters,
    /// skips or fails a step, and visits and iterations are all bounded, so
    /// the loop ends — but loops inside loops of steps that never wait can
    /// make that a very long time. A settle that reaches the bound fails the
    /// run and says so on the step it was entering; it never leaves a run
    /// going with nothing moving it.
    fn settle(&mut self, now: u64, effects: &mut Vec<RunEffect>) {
        let loops = self.workflow.loop_edges();
        let mut rounds = 0usize;
        let mut last_entered: Option<StepId> = None;
        loop {
            if self.is_finished() {
                return;
            }
            rounds += 1;
            if rounds > SETTLE_ROUNDS {
                if let Some(id) = last_entered {
                    self.mark(&id, StepState::Failed, now, |r| {
                        r.error = Some(format!("{UNSETTLED} after {SETTLE_ROUNDS} rounds"));
                    });
                }
                self.finish(RunOutcome::Failed, now, effects);
                return;
            }
            let mut changed = false;
            let steps: Vec<Step> = self.workflow.steps.clone();
            for step in &steps {
                if self.is_finished() {
                    return;
                }
                let incoming = self.workflow.incoming(&step.id);
                let fail_routes = self.workflow.fail_routes_into(&step.id);
                if incoming.is_empty() && fail_routes.is_empty() {
                    continue; // the start step: entered by `Start`, never by settle
                }
                let Some(record) = self.steps.get(&step.id) else {
                    continue;
                };
                if record.state.is_live() {
                    continue;
                }
                // Spent, and the bound still says so: an amendment that
                // raised it gave the step its visits back.
                if record.spent && record.visits >= step.max_visits {
                    continue;
                }
                let my_seq = record.seq;
                let terminal = record.state.is_terminal();

                let mut open = false;
                let mut taken = 0usize;
                let mut new_taken = 0usize;
                // Whether an arrival that counts came along a forward edge —
                // what tells a loop entered anew from one taking its next
                // iteration, which comes back along a loop edge.
                let mut forward_arrival = false;
                let join = step.join;
                let mut consider = |edge: Edge, from: &StepRecord, is_loop: bool| match edge {
                    // A loop edge still to come never holds this step; it
                    // re-enters the step when it is taken.
                    Edge::Open => {
                        if !is_loop {
                            open = true;
                        }
                    }
                    Edge::Taken => {
                        taken += 1;
                        // A fresh arrival: one that settled after this
                        // step last changed — or, for an `any` join, one
                        // whose source was *entered* after it, so the
                        // slower arm of the fan-out it already took is not
                        // a new round.
                        let fresh = match join {
                            Join::Any => from.entered > my_seq,
                            Join::All | Join::One => from.seq > my_seq,
                        };
                        if !terminal || fresh {
                            new_taken += 1;
                            forward_arrival |= !is_loop;
                        }
                    }
                    Edge::Dead => {}
                };
                let none = StepRecord::default();
                for (from, flow) in &incoming {
                    let record = self.steps.get(&from.id).unwrap_or(&none);
                    let is_loop = loops.contains(&(from.id.clone(), step.id.clone()));
                    consider(self.edge(from, flow), record, is_loop);
                }
                for from in &fail_routes {
                    let record = self.steps.get(&from.id).unwrap_or(&none);
                    let is_loop = loops.contains(&(from.id.clone(), step.id.clone()));
                    consider(self.fail_edge(from), record, is_loop);
                }

                if record.state == StepState::Pending && !open && taken == 0 {
                    // Every way in is dead: this branch was not taken — for
                    // now. A loop that brings the run round again re-enters
                    // it the moment a flow into it is taken.
                    self.mark(&step.id, StepState::Skipped, now, |_| {});
                    changed = true;
                    continue;
                }
                let ready = match step.join {
                    // `one` waits for every forward edge to resolve before
                    // it judges, like `all`.
                    Join::All | Join::One => !open && taken > 0,
                    Join::Any => taken > 0,
                };
                if ready && new_taken > 0 {
                    if step.join == Join::One && new_taken > 1 {
                        let n = new_taken;
                        self.mark(&step.id, StepState::Failed, now, |r| {
                            r.error = Some(format!("exactly one flow may arrive; {n} did"));
                        });
                        changed = true;
                        self.fail_out(step, now, effects);
                        continue;
                    }
                    // A loop entered along a forward edge — from before it, or
                    // round an outer loop after a divert left its body
                    // mid-iteration — begins afresh rather than resuming a
                    // list it abandoned. Its next iteration comes back along
                    // a loop edge and keeps the cursor.
                    if step.kind.is_loop() && forward_arrival {
                        if let Some(r) = self.steps.get_mut(&step.id) {
                            r.cursor = None;
                        }
                    }
                    self.enter(step, now, effects);
                    last_entered = Some(step.id.clone());
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        if !self.is_finished() && self.live_steps().is_empty() {
            let stalled = self.stalled_steps(&loops);
            if stalled.is_empty() {
                // Every branch drained: the implicit end.
                self.finish(RunOutcome::Done, now, effects);
            } else {
                // Nothing can move and something is still waiting: the graph
                // stalled. Say which flows never settled, on the steps that
                // waited for them, and fail the run — never report it done.
                for (id, waited_on) in stalled {
                    let error = if waited_on.is_empty() {
                        "stalled: nothing flows into it".to_string()
                    } else {
                        format!(
                            "stalled: waited on {} which never settled",
                            waited_on.join(", ")
                        )
                    };
                    self.mark(&id, StepState::Failed, now, |r| r.error = Some(error));
                }
                self.finish(RunOutcome::Failed, now, effects);
            }
        }
    }

    /// Pending steps with no live step left to move them, each with the
    /// forward flows (`from → to`) it was still waiting on. Only meaningful
    /// once a settle reached its fixpoint with nothing live.
    fn stalled_steps(&self, loops: &BTreeSet<(StepId, StepId)>) -> Vec<(StepId, Vec<String>)> {
        let mut out = Vec::new();
        for step in &self.workflow.steps {
            let pending = self
                .steps
                .get(&step.id)
                .is_some_and(|r| r.state == StepState::Pending);
            if !pending {
                continue;
            }
            let mut waited_on: Vec<String> = Vec::new();
            for (from, flow) in self.workflow.incoming(&step.id) {
                if loops.contains(&(from.id.clone(), step.id.clone())) {
                    continue;
                }
                if self.edge(from, flow) == Edge::Open {
                    waited_on.push(format!("{} → {}", from.id, step.id));
                }
            }
            for from in self.workflow.fail_routes_into(&step.id) {
                if loops.contains(&(from.id.clone(), step.id.clone())) {
                    continue;
                }
                if self.fail_edge(from) == Edge::Open {
                    waited_on.push(format!("{} → {} (on failure)", from.id, step.id));
                }
            }
            out.push((step.id.clone(), waited_on));
        }
        out
    }

    fn cancel_live(&mut self, now: u64) -> Vec<StepId> {
        let seq = self.seq;
        let mut live = Vec::new();
        for (id, r) in self.steps.iter_mut() {
            if r.state.is_live() {
                live.push(id.clone());
            }
            if !r.state.is_terminal() {
                r.state = StepState::Cancelled;
                r.seq = seq;
                r.finished_at = Some(now);
            }
        }
        live
    }

    fn finish(&mut self, outcome: RunOutcome, now: u64, effects: &mut Vec<RunEffect>) {
        if self.is_finished() {
            return;
        }
        let live = self.cancel_live(now);
        if !live.is_empty() {
            effects.push(RunEffect::CancelWork { steps: live });
        }
        self.outcome = Some(outcome);
        self.finished_at = Some(now);
        effects.push(RunEffect::Finished { outcome });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ask::AskOption;
    use crate::assignee::Assignee;
    use crate::workflow::tests::{agent, sid, step, workflow};
    use crate::workflow::{
        CheckKind, Condition, InputDef, InputKind, InputName, Rule, ValueRef, DEFAULT_MAX_VISITS,
    };
    use serde_json::json;

    fn goal_scope() -> RunScope {
        RunScope::Goal {
            goal: GoalId::from_ulid(ulid::Ulid::from_parts(1, 1)),
        }
    }

    fn run(wf: Workflow) -> WorkflowRun {
        WorkflowRun::new(
            RunId::from_ulid(ulid::Ulid::from_parts(4, 1)),
            goal_scope(),
            wf,
            BTreeMap::new(),
            RunEntry::by_hand(),
            100,
        )
    }

    fn started(wf: Workflow) -> (WorkflowRun, Vec<RunEffect>) {
        let mut r = run(wf);
        let e = r.apply(RunEvent::Start, 100).unwrap();
        (r, e)
    }

    fn done(step: &str) -> RunEvent {
        RunEvent::StepDone {
            step: sid(step),
            output: json!({"ok": true}),
        }
    }

    fn state<'a>(r: &'a WorkflowRun, step: &str) -> &'a StepState {
        &r.steps[&sid(step)].state
    }

    fn end(id: &str, finish: Finish) -> Step {
        step(id, StepKind::End { finish }, vec![])
    }

    fn check(id: &str, then: &[&str]) -> Step {
        step(
            id,
            StepKind::Check {
                check: CheckKind::Command {
                    command: "true".into(),
                },
            },
            then.iter().map(|t| Flow::to(sid(t))).collect(),
        )
    }

    fn decide(id: &str, rules: Vec<(&str, Condition)>, otherwise: &str, then: Vec<Flow>) -> Step {
        step(
            id,
            StepKind::Decide {
                rules: rules
                    .into_iter()
                    .map(|(b, when)| Rule {
                        when,
                        branch: Branch::new(b).unwrap(),
                    })
                    .collect(),
                otherwise: Branch::new(otherwise).unwrap(),
                pick: Pick::First,
            },
            then,
        )
    }

    fn labelled(to: &str, branch: &str) -> Flow {
        Flow::branch(sid(to), Branch::new(branch).unwrap())
    }

    #[test]
    fn start_enters_the_start_step_and_returns_its_effect() {
        let (r, e) = started(workflow(vec![agent("a", &["b"]), agent("b", &[])]));
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("a") }]);
        assert_eq!(state(&r, "a"), &StepState::Running);
        assert_eq!(state(&r, "b"), &StepState::Pending);
        assert_eq!(r.status(), RunStatus::Running);
        assert_eq!(r.steps[&sid("a")].visits, 1);
    }

    #[test]
    fn sequential_steps_run_one_after_another() {
        let (mut r, _) = started(workflow(vec![agent("a", &["b"]), agent("b", &[])]));
        let e = r.apply(done("a"), 101).unwrap();
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("b") }]);
        assert_eq!(r.steps[&sid("a")].output, Some(json!({"ok": true})));
        let e = r.apply(done("b"), 102).unwrap();
        assert_eq!(
            e,
            vec![RunEffect::Finished {
                outcome: RunOutcome::Done
            }]
        );
        assert_eq!(r.status(), RunStatus::Done);
        assert_eq!(r.finished_at, Some(102));
    }

    /// A `connector` step goes live like a `check`: an effect the engine
    /// runs, settled by the step's result or failure.
    #[test]
    fn a_connector_step_is_called_and_settles_on_its_result() {
        let (mut run, effects) = started(workflow(vec![
            step(
                "call",
                StepKind::Connector {
                    connector: Some(crate::id::ConnectorId::new("slack").unwrap()),
                    operation: Some(crate::id::OperationId::new("post_message").unwrap()),
                    account: None,
                    params: BTreeMap::from([("text".to_string(), "hi".to_string())]),
                    output_schema: None,
                    unattended: false,
                },
                vec![Flow::to(sid("after"))],
            ),
            agent("after", &[]),
        ]));
        assert_eq!(
            effects,
            vec![RunEffect::CallConnector { step: sid("call") }]
        );
        assert_eq!(run.steps[&sid("call")].state, StepState::Running);
        let e = run
            .apply(
                RunEvent::StepDone {
                    step: sid("call"),
                    output: json!({"ok": true, "ts": "1.2"}),
                },
                20,
            )
            .unwrap();
        assert_eq!(run.steps[&sid("call")].state, StepState::done());
        assert_eq!(
            run.steps[&sid("call")].output,
            Some(json!({"ok": true, "ts": "1.2"}))
        );
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("after") }]);
        assert!(
            !WorkflowRun::accepts(
                &StepKind::Connector {
                    connector: Some(crate::id::ConnectorId::new("slack").unwrap()),
                    operation: Some(crate::id::OperationId::new("post_message").unwrap()),
                    account: None,
                    params: BTreeMap::new(),
                    output_schema: None,
                    unattended: false,
                },
                &RunEvent::Answered {
                    step: sid("call"),
                    answer: crate::ask::Answer::text("x"),
                },
            ),
            "a person cannot answer a connector call"
        );
    }

    #[test]
    fn parallel_fan_out_and_all_join() {
        let (mut r, e) = started(workflow(vec![
            agent("a", &["b", "c"]),
            agent("b", &["d"]),
            agent("c", &["d"]),
            agent("d", &[]),
        ]));
        assert_eq!(e.len(), 1);
        let e = r.apply(done("a"), 101).unwrap();
        assert_eq!(
            e,
            vec![
                RunEffect::StartAgent { step: sid("b") },
                RunEffect::StartAgent { step: sid("c") }
            ]
        );
        let e = r.apply(done("b"), 102).unwrap();
        assert!(e.is_empty(), "d waits for c: {e:?}");
        assert_eq!(state(&r, "d"), &StepState::Pending);
        let e = r.apply(done("c"), 103).unwrap();
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("d") }]);
    }

    /// The slower arm of the same fan-out is not a new round: the `any`
    /// join took the first arrival and moved on, and the second arrival
    /// changes nothing — it does not re-enter the join and everything
    /// after it.
    #[test]
    fn an_any_join_is_not_re_entered_by_the_slower_arm_of_the_same_round() {
        let mut d = agent("d", &["e"]);
        d.join = Join::Any;
        let (mut r, _) = started(workflow(vec![
            agent("a", &["b", "c"]),
            agent("b", &["d"]),
            agent("c", &["d"]),
            d,
            agent("e", &[]),
        ]));
        r.apply(done("a"), 101).unwrap();
        assert_eq!(
            r.apply(done("b"), 102).unwrap(),
            vec![RunEffect::StartAgent { step: sid("d") }]
        );
        assert_eq!(
            r.apply(done("d"), 103).unwrap(),
            vec![RunEffect::StartAgent { step: sid("e") }],
            "d is done and e runs"
        );
        let e = r.apply(done("c"), 104).unwrap();
        assert!(e.is_empty(), "the slower arm re-enters nothing: {e:?}");
        assert_eq!(r.steps[&sid("d")].visits, 1);
        assert_eq!(state(&r, "d"), &StepState::done());
        assert_eq!(
            r.apply(done("e"), 105).unwrap(),
            vec![RunEffect::Finished {
                outcome: RunOutcome::Done
            }]
        );
    }

    /// A loop back into an `any` join is a new round — its source was
    /// entered after the join last ran — so the rework still fires.
    #[test]
    fn an_any_join_at_a_loop_head_still_re_enters_on_the_loop() {
        let mut head = agent("head", &["x"]);
        head.join = Join::Any;
        let (mut r, e) = started(workflow(vec![
            head,
            agent("x", &["d"]),
            decide(
                "d",
                vec![(
                    "again",
                    Condition::Between {
                        from_hour: 0,
                        to_hour: 23,
                    },
                )],
                "stop",
                vec![labelled("head", "again"), labelled("done", "stop")],
            ),
            end("done", Finish::Done),
        ]));
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("head") }]);
        r.apply(done("head"), 101).unwrap();
        let e = r.apply(done("x"), 102).unwrap();
        assert_eq!(
            e,
            vec![RunEffect::StartAgent { step: sid("head") }],
            "the decide looped back: the head re-enters"
        );
        assert_eq!(r.steps[&sid("head")].visits, 2);
    }

    /// The person's last *not sure* on a human step is a failure of the
    /// step, and the step's `on_fail` decides the run.
    #[test]
    fn a_human_step_fails_on_the_last_not_sure() {
        let (mut r, e) = started(workflow(vec![
            human("q", &["yes", "no"], &["after"]),
            agent("after", &[]),
        ]));
        assert_eq!(e, vec![RunEffect::Ask { step: sid("q") }]);
        assert_eq!(state(&r, "q"), &StepState::Waiting);
        let e = r
            .apply(
                RunEvent::StepFailed {
                    step: sid("q"),
                    error: "the person is not sure: which colour?".into(),
                },
                101,
            )
            .unwrap();
        assert_eq!(state(&r, "q"), &StepState::Failed);
        assert_eq!(
            r.steps[&sid("q")].error.as_deref(),
            Some("the person is not sure: which colour?")
        );
        assert_eq!(
            e,
            vec![RunEffect::Finished {
                outcome: RunOutcome::Failed
            }]
        );
    }

    /// The incident shape: triage, mitigate, tell the stakeholders, a person
    /// confirms, and *no — still broken* loops back to triage. The end is
    /// reached through the follow-up alone, so a *no* re-enters triage and
    /// the run goes on; it does not finish under the loop's feet.
    #[test]
    fn an_incident_loop_that_says_not_yet_re_enters_triage_and_does_not_finish() {
        let announce = step(
            "announce",
            StepKind::Notify {
                scope: None,
                template: "update".into(),
                mentions: vec![],
                author: None,
            },
            vec![Flow::to(sid("confirm"))],
        );
        let (mut r, e) = started(workflow(vec![
            agent("triage", &["mitigate"]),
            agent("mitigate", &["announce"]),
            announce,
            human("confirm", &["yes", "no"], &["held"]),
            decide(
                "held",
                vec![(
                    "yes",
                    Condition::Answered {
                        step: sid("confirm"),
                        option: "yes".into(),
                    },
                )],
                "no",
                vec![labelled("cause", "yes"), labelled("triage", "no")],
            ),
            agent("cause", &["followup"]),
            agent("followup", &["done"]),
            end("done", Finish::Done),
        ]));
        assert_eq!(
            e,
            vec![RunEffect::StartAgent {
                step: sid("triage")
            }]
        );
        r.apply(done("triage"), 101).unwrap();
        r.apply(done("mitigate"), 102).unwrap();
        r.apply(done("announce"), 103).unwrap();
        assert_eq!(state(&r, "confirm"), &StepState::Waiting);
        let e = r
            .apply(
                RunEvent::Answered {
                    step: sid("confirm"),
                    answer: Answer::selecting(["no"]),
                },
                104,
            )
            .unwrap();
        assert_eq!(
            e,
            vec![RunEffect::StartAgent {
                step: sid("triage")
            }],
            "still broken: back to triage, and nothing finished"
        );
        assert!(!r.is_finished());
        assert_eq!(r.steps[&sid("triage")].visits, 2);
        // The yes side is skipped for now — every way into it is dead this
        // round — and re-enters the moment a later round says yes.
        assert_eq!(state(&r, "done"), &StepState::Skipped);
        assert_eq!(
            state(&r, "cause"),
            &StepState::Skipped,
            "the yes side waits, skipped, for a yes"
        );
        // The second round says yes: the cause, the follow-up, the end.
        r.apply(done("triage"), 105).unwrap();
        r.apply(done("mitigate"), 106).unwrap();
        r.apply(done("announce"), 107).unwrap();
        let e = r
            .apply(
                RunEvent::Answered {
                    step: sid("confirm"),
                    answer: Answer::selecting(["yes"]),
                },
                108,
            )
            .unwrap();
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("cause") }]);
        r.apply(done("cause"), 109).unwrap();
        let e = r.apply(done("followup"), 110).unwrap();
        assert_eq!(
            e,
            vec![RunEffect::Finished {
                outcome: RunOutcome::Done
            }]
        );
    }

    #[test]
    fn any_join_starts_on_the_first_arrival_and_ignores_the_second() {
        let mut d = agent("d", &[]);
        d.join = Join::Any;
        let (mut r, _) = started(workflow(vec![
            agent("a", &["b", "c"]),
            agent("b", &["d"]),
            agent("c", &["d"]),
            d,
        ]));
        r.apply(done("a"), 101).unwrap();
        let e = r.apply(done("b"), 102).unwrap();
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("d") }]);
        let e = r.apply(done("c"), 103).unwrap();
        assert!(e.is_empty(), "d is already running: {e:?}");
        assert_eq!(r.steps[&sid("d")].visits, 1);
        // d finishes; c is done too; the run drains.
        let e = r.apply(done("d"), 104).unwrap();
        assert_eq!(
            e,
            vec![RunEffect::Finished {
                outcome: RunOutcome::Done
            }]
        );
    }

    fn branching() -> Workflow {
        workflow(vec![
            check("c", &["d"]),
            decide(
                "d",
                vec![(
                    "yes",
                    Condition::Outcome {
                        step: sid("c"),
                        passed: true,
                    },
                )],
                "no",
                vec![labelled("y", "yes"), labelled("n", "no")],
            ),
            agent("y", &["z"]),
            agent("n", &["z"]),
            agent("z", &[]),
        ])
    }

    #[test]
    fn decide_takes_one_branch_and_skips_only_the_exclusive_successors() {
        let (mut r, e) = started(branching());
        assert_eq!(e, vec![RunEffect::RunCheck { step: sid("c") }]);
        let e = r.apply(done("c"), 101).unwrap();
        assert_eq!(
            state(&r, "d"),
            &StepState::chose(Branch::new("yes").unwrap())
        );
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("y") }]);
        assert_eq!(state(&r, "n"), &StepState::Skipped);
        // z is reachable from y and from n: skipped n does not skip z.
        assert_eq!(state(&r, "z"), &StepState::Pending);
        let e = r.apply(done("y"), 102).unwrap();
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("z") }]);
    }

    #[test]
    fn a_failed_check_is_a_failed_step_and_outcome_reads_it_as_not_passed() {
        let mut wf = branching();
        wf.steps[0].on_fail = OnFail::Skip;
        let (mut r, _) = started(wf);
        let e = r
            .apply(
                RunEvent::StepFailed {
                    step: sid("c"),
                    error: "exit 1".into(),
                },
                101,
            )
            .unwrap();
        assert_eq!(state(&r, "c"), &StepState::Failed);
        assert_eq!(r.steps[&sid("c")].error.as_deref(), Some("exit 1"));
        // on_fail: skip carried the unlabelled flow into the decide, which
        // read the check as not passed.
        assert_eq!(
            state(&r, "d"),
            &StepState::chose(Branch::new("no").unwrap())
        );
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("n") }]);
        assert_eq!(state(&r, "y"), &StepState::Skipped);
    }

    #[test]
    fn retries_re_emit_then_on_fail_decides() {
        let mut a = agent("a", &["b"]);
        a.retries = 1;
        let (mut r, _) = started(workflow(vec![a, agent("b", &[])]));
        let fail = |e: &str| RunEvent::StepFailed {
            step: sid("a"),
            error: e.into(),
        };
        let e = r.apply(fail("boom"), 101).unwrap();
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("a") }], "retry");
        assert_eq!(state(&r, "a"), &StepState::Running);
        assert_eq!(r.steps[&sid("a")].attempts, 1);
        let e = r.apply(fail("boom again"), 102).unwrap();
        assert_eq!(state(&r, "a"), &StepState::Failed);
        assert_eq!(
            e,
            vec![RunEffect::Finished {
                outcome: RunOutcome::Failed
            }]
        );
        assert_eq!(state(&r, "b"), &StepState::Cancelled);
        assert_eq!(r.status(), RunStatus::Failed);
    }

    /// A restart may cut a step short three times; the fourth fails it with
    /// its own reason, so a step that takes the node down is not resumed on
    /// every boot — and none of it costs an attempt.
    #[test]
    fn the_fourth_interruption_fails_the_step_with_the_reason() {
        let (mut r, _) = started(workflow(vec![agent("a", &["b"]), agent("b", &[])]));
        let item = WorkItemId::from_ulid(ulid::Ulid::from_parts(7, 7));
        r.apply(
            RunEvent::StepStarted {
                step: sid("a"),
                work_item: Some(item),
            },
            101,
        )
        .unwrap();
        for n in 1..=MAX_INTERRUPTIONS {
            let e = r
                .apply(
                    RunEvent::StepInterrupted { step: sid("a") },
                    101 + u64::from(n),
                )
                .unwrap();
            assert_eq!(
                e,
                vec![RunEffect::ResumeAgent {
                    step: sid("a"),
                    work_item: item
                }],
                "cut short {n} times: resumed"
            );
            assert_eq!(state(&r, "a"), &StepState::Running);
            assert_eq!(r.steps[&sid("a")].interruptions, n);
            assert_eq!(r.steps[&sid("a")].error.as_deref(), Some(INTERRUPTED));
        }
        let e = r
            .apply(RunEvent::StepInterrupted { step: sid("a") }, 110)
            .unwrap();
        assert_eq!(state(&r, "a"), &StepState::Failed);
        assert_eq!(
            r.steps[&sid("a")].error.as_deref(),
            Some(INTERRUPTED_TOO_OFTEN)
        );
        assert_eq!(
            e,
            vec![RunEffect::Finished {
                outcome: RunOutcome::Failed
            }]
        );
        assert_eq!(
            r.steps[&sid("a")].attempts,
            0,
            "an interruption costs no attempt"
        );
        // The count rides the snapshot, and a record that never was cut
        // short does not spell it.
        let wire = serde_json::to_value(&r.steps[&sid("a")]).unwrap();
        assert_eq!(wire["interruptions"], serde_json::json!(4));
        let fresh = serde_json::to_value(&r.steps[&sid("b")]).unwrap();
        assert!(fresh.get("interruptions").is_none(), "{fresh}");
    }

    #[test]
    fn an_interrupted_agent_resumes_on_its_item_and_the_interruption_costs_no_attempt() {
        let (mut r, _) = started(workflow(vec![agent("a", &["b"]), agent("b", &[])]));
        let item = WorkItemId::from_ulid(ulid::Ulid::from_parts(7, 7));
        r.apply(
            RunEvent::StepStarted {
                step: sid("a"),
                work_item: Some(item),
            },
            101,
        )
        .unwrap();
        let e = r
            .apply(RunEvent::StepInterrupted { step: sid("a") }, 102)
            .unwrap();
        assert_eq!(
            e,
            vec![RunEffect::ResumeAgent {
                step: sid("a"),
                work_item: item
            }]
        );
        assert_eq!(
            state(&r, "a"),
            &StepState::Running,
            "still live, on the same item"
        );
        assert_eq!(
            r.steps[&sid("a")].attempts,
            0,
            "a restart is not the step's failure"
        );
        assert_eq!(r.steps[&sid("a")].error.as_deref(), Some(INTERRUPTED));
        assert_eq!(r.steps[&sid("a")].work_item, Some(item));
        // Interrupted again before an item was bound: a fresh start.
        let (mut r, _) = started(workflow(vec![agent("a", &[])]));
        let e = r
            .apply(RunEvent::StepInterrupted { step: sid("a") }, 102)
            .unwrap();
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("a") }]);
    }

    #[test]
    fn an_interrupted_check_runs_again_and_a_call_repeats_only_within_its_retries() {
        let (mut r, _) = started(workflow(vec![check("c", &["e"]), end("e", Finish::Done)]));
        let e = r
            .apply(RunEvent::StepInterrupted { step: sid("c") }, 101)
            .unwrap();
        assert_eq!(e, vec![RunEffect::RunCheck { step: sid("c") }]);
        assert_eq!(r.steps[&sid("c")].attempts, 0);

        let notify = |id: &str, retries: u8, then: &[&str]| {
            let mut s = step(
                id,
                StepKind::Notify {
                    scope: None,
                    template: "hi".into(),
                    mentions: vec![],
                    author: None,
                },
                then.iter().map(|t| Flow::to(sid(t))).collect(),
            );
            s.retries = retries;
            s
        };
        // One retry allowed: the post goes again, and the interruption did
        // not spend it.
        let (mut r, _) = started(workflow(vec![
            notify("n", 1, &["e"]),
            end("e", Finish::Done),
        ]));
        let e = r
            .apply(RunEvent::StepInterrupted { step: sid("n") }, 101)
            .unwrap();
        assert_eq!(e, vec![RunEffect::Post { step: sid("n") }]);
        assert_eq!(r.steps[&sid("n")].attempts, 0);
        let e = r
            .apply(
                RunEvent::StepFailed {
                    step: sid("n"),
                    error: "refused".into(),
                },
                102,
            )
            .unwrap();
        assert_eq!(
            e,
            vec![RunEffect::Post { step: sid("n") }],
            "the author's retry is still whole"
        );
        // No retries: a repeat nobody allowed is a failure, through on_fail.
        let (mut r, _) = started(workflow(vec![
            notify("n", 0, &["e"]),
            end("e", Finish::Done),
        ]));
        let e = r
            .apply(RunEvent::StepInterrupted { step: sid("n") }, 101)
            .unwrap();
        assert_eq!(state(&r, "n"), &StepState::Failed);
        assert_eq!(r.steps[&sid("n")].error.as_deref(), Some(INTERRUPTED));
        assert_eq!(
            e,
            vec![RunEffect::Finished {
                outcome: RunOutcome::Failed
            }]
        );
        // A step that is not live refuses the event like any other.
        let (mut r, _) = started(workflow(vec![agent("a", &["b"]), agent("b", &[])]));
        let err = r
            .apply(RunEvent::StepInterrupted { step: sid("b") }, 101)
            .unwrap_err();
        assert!(matches!(err, RunError::NotLive { .. }), "{err:?}");
        assert!(dies_with_the_process(&StepKind::Agent {
            instructions: String::new(),
            assignee: None,
            project: None,
            harness: vec![],
            model: None,
            effort: None,
            output_schema: None,
            tier_ceiling: crate::ToolTier::Read,
        }));
        assert!(!dies_with_the_process(&StepKind::End {
            finish: Finish::Done
        }));
    }

    #[test]
    fn on_fail_then_takes_only_the_implicit_edge() {
        let mut a = agent("a", &["b"]);
        a.on_fail = OnFail::Then { step: sid("fix") };
        let (mut r, _) = started(workflow(vec![a, agent("b", &[]), agent("fix", &["b"])]));
        let e = r
            .apply(
                RunEvent::StepFailed {
                    step: sid("a"),
                    error: "x".into(),
                },
                101,
            )
            .unwrap();
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("fix") }]);
        assert_eq!(state(&r, "b"), &StepState::Pending, "b waits on fix");
        let e = r.apply(done("fix"), 102).unwrap();
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("b") }]);
    }

    #[test]
    fn a_loop_is_bounded_by_max_visits_and_fails_out() {
        let mut fix = agent("fix", &["c"]);
        fix.max_visits = 2;
        let (mut r, _) = started(workflow(vec![
            agent("start", &["fix"]),
            fix,
            check("c", &["d"]),
            decide(
                "d",
                vec![(
                    "ok",
                    Condition::Outcome {
                        step: sid("c"),
                        passed: true,
                    },
                )],
                "again",
                vec![labelled("e", "ok"), labelled("fix", "again")],
            ),
            end("e", Finish::Done),
        ]));
        r.apply(done("start"), 101).unwrap();
        assert_eq!(state(&r, "fix"), &StepState::Running);
        let fail_check = |r: &mut WorkflowRun, t: u64| {
            r.apply(done("fix"), t).unwrap();
            r.apply(
                RunEvent::StepFailed {
                    step: sid("c"),
                    error: "no".into(),
                },
                t + 1,
            )
            .unwrap()
        };
        // First failure: the check has on_fail: fail by default, so make it skip.
        let mut wf = r.workflow.clone();
        wf.steps[2].on_fail = OnFail::Skip;
        r.apply(RunEvent::Amended { workflow: wf }, 101).unwrap();
        let e = fail_check(&mut r, 102);
        assert_eq!(
            e,
            vec![RunEffect::StartAgent { step: sid("fix") }],
            "second visit"
        );
        assert_eq!(r.steps[&sid("fix")].visits, 2);
        assert_eq!(state(&r, "e"), &StepState::Skipped);
        let e = fail_check(&mut r, 104);
        // Third entry refused: fix fails out with on_fail: fail.
        assert_eq!(state(&r, "fix"), &StepState::Failed);
        assert!(r.steps[&sid("fix")]
            .error
            .as_deref()
            .unwrap()
            .contains("max_visits"));
        assert_eq!(
            e,
            vec![RunEffect::Finished {
                outcome: RunOutcome::Failed
            }]
        );
    }

    #[test]
    fn a_loop_that_passes_re_enters_a_skipped_branch() {
        let (mut r, _) = started(workflow(vec![
            agent("start", &["fix"]),
            agent("fix", &["c"]),
            {
                let mut c = check("c", &["d"]);
                c.on_fail = OnFail::Skip;
                c
            },
            decide(
                "d",
                vec![(
                    "ok",
                    Condition::Outcome {
                        step: sid("c"),
                        passed: true,
                    },
                )],
                "again",
                vec![labelled("ship", "ok"), labelled("fix", "again")],
            ),
            agent("ship", &[]),
        ]));
        r.apply(done("start"), 101).unwrap();
        r.apply(done("fix"), 102).unwrap();
        r.apply(
            RunEvent::StepFailed {
                step: sid("c"),
                error: "no".into(),
            },
            103,
        )
        .unwrap();
        assert_eq!(state(&r, "ship"), &StepState::Skipped);
        assert_eq!(state(&r, "fix"), &StepState::Running);
        r.apply(done("fix"), 104).unwrap();
        let e = r.apply(done("c"), 105).unwrap();
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("ship") }]);
        assert_eq!(state(&r, "ship"), &StepState::Running);
        assert_eq!(r.steps[&sid("ship")].visits, 1);
    }

    /// What `work` does, on a thread whose stack is a quarter of a test's
    /// own: a machine that settles by calling itself overflows it, and one
    /// that settles in a loop does not notice.
    fn on_a_small_stack<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
        std::thread::Builder::new()
            .stack_size(512 * 1024)
            .spawn(work)
            .expect("a thread")
            .join()
            .expect("the work ends")
    }

    fn passing_over(mut step: Step) -> Step {
        step.on_fail = OnFail::Skip;
        step
    }

    #[test]
    fn a_ring_of_steps_that_pass_over_their_failures_ends_when_its_visits_are_spent() {
        // a → b → a, both done the moment they are entered, both passing
        // over their own failure: nothing in the ring ever waits, so the
        // whole of it is walked inside one event.
        let ring = workflow(vec![
            passing_over(step("a", StepKind::Parallel, vec![Flow::to(sid("b"))])),
            passing_over(step("b", StepKind::Parallel, vec![Flow::to(sid("a"))])),
        ]);
        let (r, effects) = on_a_small_stack(move || started(ring));
        assert_eq!(
            effects,
            vec![RunEffect::Finished {
                outcome: RunOutcome::Done
            }],
            "every failure was passed over, and the ring drained"
        );
        for id in ["a", "b"] {
            let record = &r.steps[&sid(id)];
            assert_eq!(record.state, StepState::Failed, "{id}");
            assert_eq!(record.visits, DEFAULT_MAX_VISITS, "{id}");
            assert!(record.spent, "{id} takes no further arrival");
            assert_eq!(
                record.error.as_deref(),
                Some("entered 3 times; max_visits is 3"),
                "{id}"
            );
        }
    }

    #[test]
    fn a_ring_of_agents_that_pass_over_their_failures_ends_when_its_visits_are_spent() {
        let (mut r, _) = started(workflow(vec![
            passing_over(agent("a", &["b"])),
            passing_over(agent("b", &["a"])),
        ]));
        let mut last = Vec::new();
        for round in 0..u64::from(DEFAULT_MAX_VISITS) {
            assert_eq!(state(&r, "a"), &StepState::Running, "round {round}");
            r.apply(done("a"), 101 + round).unwrap();
            assert_eq!(state(&r, "b"), &StepState::Running, "round {round}");
            last = r.apply(done("b"), 101 + round).unwrap();
        }
        // The fourth entry of `a` is refused and passed over, then `b`'s,
        // and the flow back into `a` finds a step whose visits are spent.
        assert_eq!(
            last,
            vec![RunEffect::Finished {
                outcome: RunOutcome::Done
            }]
        );
        assert!(r.steps[&sid("a")].spent && r.steps[&sid("b")].spent);
        assert_eq!(state(&r, "a"), &StepState::Failed);
        assert_eq!(state(&r, "b"), &StepState::Failed);
    }

    #[test]
    fn a_spent_step_that_routes_its_failure_leaves_the_ring_by_its_route() {
        // a → b → a, and `a` routes its failure to `out`: when `a`'s visits
        // are spent the ring is left by the route, once.
        let mut a = agent("a", &["b"]);
        a.on_fail = OnFail::Then { step: sid("out") };
        let (mut r, _) = started(workflow(vec![
            a,
            passing_over(agent("b", &["a"])),
            agent("out", &[]),
        ]));
        let mut last = Vec::new();
        for round in 0..u64::from(DEFAULT_MAX_VISITS) {
            r.apply(done("a"), 101 + round).unwrap();
            last = r.apply(done("b"), 101 + round).unwrap();
        }
        assert_eq!(last, vec![RunEffect::StartAgent { step: sid("out") }]);
        assert_eq!(state(&r, "a"), &StepState::Failed);
        assert_eq!(r.steps[&sid("out")].visits, 1);
        assert_eq!(
            state(&r, "b"),
            &StepState::Done { branches: vec![] },
            "the flow a → b died with the route, and b is as it was"
        );
        let e = r.apply(done("out"), 110).unwrap();
        assert_eq!(
            e,
            vec![RunEffect::Finished {
                outcome: RunOutcome::Done
            }]
        );
    }

    #[test]
    fn a_run_settles_in_a_stack_that_does_not_grow_with_its_failures() {
        // Two switches that cannot render what they read, each passing over
        // its failure, entered two hundred times each: four hundred failures
        // inside one event.
        let failing = |id: &str, to: &str| {
            let mut s = passing_over(step(
                id,
                StepKind::Switch {
                    on: "{steps.nobody.output.word}".into(),
                    cases: vec![],
                    otherwise: Branch::new("other").unwrap(),
                },
                vec![Flow::to(sid(to))],
            ));
            s.max_visits = 200;
            s
        };
        let ring = workflow(vec![failing("a", "b"), failing("b", "a")]);
        let (r, effects) = on_a_small_stack(move || started(ring));
        assert_eq!(
            effects,
            vec![RunEffect::Finished {
                outcome: RunOutcome::Done
            }]
        );
        for id in ["a", "b"] {
            let record = &r.steps[&sid(id)];
            assert_eq!(record.visits, 200, "{id}");
            assert!(record.spent, "{id}");
        }
    }

    #[test]
    fn an_amendment_that_raises_the_bound_gives_a_spent_step_its_visits_back() {
        let (mut r, _) = started(workflow(vec![
            agent("a", &["b"]),
            passing_over(agent("b", &["c", "a"])),
            agent("c", &[]),
        ]));
        // `a` is entered three times; its fourth entry is refused with
        // `on_fail: fail`… so make it pass over, and keep `c` to hold the run.
        let mut wf = r.workflow.clone();
        wf.steps[0].on_fail = OnFail::Skip;
        wf.steps[2].join = Join::Any;
        r.apply(RunEvent::Amended { workflow: wf }, 100).unwrap();
        for round in 0..u64::from(DEFAULT_MAX_VISITS) {
            r.apply(done("a"), 101 + round).unwrap();
            r.apply(done("b"), 101 + round).unwrap();
        }
        assert!(r.steps[&sid("a")].spent);
        assert!(!r.is_finished(), "c is still going: {:?}", r.steps);

        let mut wider = r.workflow.clone();
        wider.revision += 1;
        wider.steps[0].max_visits = 5;
        let e = r.apply(RunEvent::Amended { workflow: wider }, 120).unwrap();
        assert_eq!(
            e,
            vec![RunEffect::StartAgent { step: sid("a") }],
            "the arrival that was dropped is taken now"
        );
        assert_eq!(r.steps[&sid("a")].visits, 4);
        assert!(!r.steps[&sid("a")].spent);
    }

    fn always() -> Condition {
        Condition::Between {
            from_hour: 0,
            to_hour: 23,
        }
    }

    fn if_step(id: &str, when: Condition, yes: &str, no: &str) -> Step {
        step(
            id,
            StepKind::If { when },
            vec![labelled(yes, "yes"), labelled(no, "no")],
        )
    }

    /// `a → {b, c} → d`, with `d`'s join and `on_fail` as given.
    fn diamond_into(join: Join, on_fail: OnFail, after: &[&str]) -> Workflow {
        let mut d = agent("d", after);
        d.join = join;
        d.on_fail = on_fail;
        workflow(vec![
            agent("a", &["b", "c"]),
            agent("b", &["d"]),
            agent("c", &["d"]),
            d,
            agent("e", &[]),
        ])
    }

    #[test]
    fn one_join_enters_on_a_single_arrival_and_fails_on_two() {
        // `e` hangs off `d`: with nothing after the join it would be a second
        // start, and a run with two starts never starts.
        let (mut r, _) = started(diamond_into(Join::One, OnFail::Fail, &["e"]));
        r.apply(done("a"), 101).unwrap();
        r.apply(done("b"), 102).unwrap();
        assert_eq!(state(&r, "d"), &StepState::Pending, "c is still open");
        let e = r.apply(done("c"), 103).unwrap();
        assert_eq!(state(&r, "d"), &StepState::Failed);
        assert_eq!(
            r.steps[&sid("d")].error.as_deref(),
            Some("exactly one flow may arrive; 2 did")
        );
        assert_eq!(
            e,
            vec![RunEffect::Finished {
                outcome: RunOutcome::Failed
            }]
        );

        // One arm alive, the other dead: exactly one arrives.
        let mut j = agent("j", &[]);
        j.join = Join::One;
        let wf = workflow(vec![
            check("c", &["d"]),
            decide(
                "d",
                vec![("yes", always())],
                "no",
                vec![labelled("y", "yes"), labelled("n", "no")],
            ),
            agent("y", &["j"]),
            agent("n", &["j"]),
            j,
        ]);
        assert_eq!(
            wf.start_steps()
                .iter()
                .map(|s| s.id.to_string())
                .collect::<Vec<_>>(),
            vec!["c".to_string()],
            "one start: the check"
        );
        let (mut r, _) = started(wf);
        r.apply(done("c"), 101).unwrap();
        assert_eq!(state(&r, "n"), &StepState::Skipped);
        let e = r.apply(done("y"), 102).unwrap();
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("j") }]);
        assert_eq!(r.steps[&sid("j")].visits, 1);
    }

    #[test]
    fn one_join_with_on_fail_skip_continues_past_the_violation() {
        let (mut r, _) = started(diamond_into(Join::One, OnFail::Skip, &["e"]));
        r.apply(done("a"), 101).unwrap();
        r.apply(done("b"), 102).unwrap();
        let e = r.apply(done("c"), 103).unwrap();
        assert_eq!(state(&r, "d"), &StepState::Failed);
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("e") }]);
        assert_eq!(state(&r, "e"), &StepState::Running);
        assert_eq!(r.status(), RunStatus::Running);
    }

    #[test]
    fn one_join_on_a_loop_target_judges_only_new_arrivals() {
        let mut x = agent("x", &["c"]);
        x.join = Join::One;
        let (mut r, _) = started(workflow(vec![
            agent("start", &["x"]),
            x,
            {
                let mut c = check("c", &["d"]);
                c.on_fail = OnFail::Skip;
                c
            },
            decide(
                "d",
                vec![(
                    "ok",
                    Condition::Outcome {
                        step: sid("c"),
                        passed: true,
                    },
                )],
                "again",
                vec![labelled("e", "ok"), labelled("x", "again")],
            ),
            end("e", Finish::Done),
        ]));
        r.apply(done("start"), 101).unwrap();
        r.apply(done("x"), 102).unwrap();
        let e = r
            .apply(
                RunEvent::StepFailed {
                    step: sid("c"),
                    error: "no".into(),
                },
                103,
            )
            .unwrap();
        // The forward edge from `start` was counted on the first pass; only
        // the loop edge is news, so `x` re-enters rather than failing.
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("x") }]);
        assert_eq!(state(&r, "x"), &StepState::Running);
        assert_eq!(r.steps[&sid("x")].visits, 2);
    }

    #[test]
    fn if_takes_yes_when_its_condition_holds_and_no_otherwise() {
        let wf = || {
            workflow(vec![
                {
                    let mut c = check("c", &["i"]);
                    c.on_fail = OnFail::Skip;
                    c
                },
                if_step(
                    "i",
                    Condition::Outcome {
                        step: sid("c"),
                        passed: true,
                    },
                    "y",
                    "n",
                ),
                agent("y", &[]),
                agent("n", &[]),
            ])
        };
        let (mut r, _) = started(wf());
        let e = r.apply(done("c"), 101).unwrap();
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("y") }]);
        assert_eq!(
            state(&r, "i"),
            &StepState::chose(Branch::new("yes").unwrap())
        );
        assert_eq!(state(&r, "n"), &StepState::Skipped);
        assert_eq!(r.steps[&sid("i")].output, None, "an if records no output");

        let (mut r, _) = started(wf());
        let e = r
            .apply(
                RunEvent::StepFailed {
                    step: sid("c"),
                    error: "no".into(),
                },
                101,
            )
            .unwrap();
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("n") }]);
        assert_eq!(state(&r, "y"), &StepState::Skipped);
    }

    fn switch_wf(on: &str) -> Workflow {
        workflow(vec![
            agent("a", &["s"]),
            step(
                "s",
                StepKind::Switch {
                    on: on.into(),
                    cases: vec![crate::workflow::Case {
                        value: "prod".into(),
                        branch: Branch::new("live").unwrap(),
                    }],
                    otherwise: Branch::new("other").unwrap(),
                },
                vec![labelled("live", "live"), labelled("other", "other")],
            ),
            agent("live", &[]),
            agent("other", &[]),
        ])
    }

    #[test]
    fn switch_matches_the_rendered_value_or_otherwise() {
        let (mut r, _) = started(switch_wf("{steps.a.output.env}"));
        let e = r
            .apply(done_with("a", json!({"env": "prod"})), 101)
            .unwrap();
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("live") }]);
        assert_eq!(
            state(&r, "s"),
            &StepState::chose(Branch::new("live").unwrap())
        );
        assert_eq!(r.steps[&sid("s")].output, Some(json!({"value": "prod"})));
        assert_eq!(state(&r, "other"), &StepState::Skipped);

        let (mut r, _) = started(switch_wf("{steps.a.output.env}"));
        let e = r.apply(done_with("a", json!({"env": "qa"})), 101).unwrap();
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("other") }]);
        assert_eq!(r.steps[&sid("s")].output, Some(json!({"value": "qa"})));
    }

    fn judge_wf() -> Workflow {
        let option = |b: &str| crate::workflow::JudgeOption {
            branch: Branch::new(b).unwrap(),
            meaning: format!("what {b} means"),
        };
        workflow(vec![
            agent("a", &["j"]),
            step(
                "j",
                StepKind::Judge {
                    state: "{steps.a.output.report}".into(),
                    instructions: "Is it good news?".into(),
                    options: vec![option("good"), option("bad")],
                    otherwise: Branch::new("unsure").unwrap(),
                    min_confidence: None,
                },
                vec![
                    labelled("good", "good"),
                    labelled("bad", "bad"),
                    labelled("unsure", "unsure"),
                ],
            ),
            agent("good", &[]),
            agent("bad", &[]),
            agent("unsure", &[]),
        ])
    }

    #[test]
    fn a_judge_asks_the_engine_and_takes_the_branch_its_output_names() {
        let (mut r, _) = started(judge_wf());
        let e = r
            .apply(done_with("a", json!({"report": "all green"})), 101)
            .unwrap();
        assert_eq!(e, vec![RunEffect::Judge { step: sid("j") }]);
        assert_eq!(state(&r, "j"), &StepState::Running);
        assert!(dies_with_the_process(&judge_wf().steps[1].kind));

        let e = r
            .apply(
                done_with(
                    "j",
                    json!({"choice": "good", "confidence": 0.9, "judged": true}),
                ),
                102,
            )
            .unwrap();
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("good") }]);
        assert_eq!(
            state(&r, "j"),
            &StepState::chose(Branch::new("good").unwrap())
        );
        assert_eq!(state(&r, "bad"), &StepState::Skipped);
        assert_eq!(state(&r, "unsure"), &StepState::Skipped);
    }

    #[test]
    fn a_judge_with_no_pick_or_a_pick_it_never_offered_takes_otherwise() {
        for output in [
            json!({"choice": null, "confidence": 0.2, "judged": false}),
            json!({"choice": "elsewhere", "confidence": 0.99, "judged": true}),
            json!({}),
        ] {
            let (mut r, _) = started(judge_wf());
            r.apply(done_with("a", json!({"report": "?"})), 101)
                .unwrap();
            let e = r.apply(done_with("j", output.clone()), 102).unwrap();
            assert_eq!(
                e,
                vec![RunEffect::StartAgent {
                    step: sid("unsure")
                }],
                "{output}"
            );
        }
        // Only a judge reads a branch off its output.
        assert_eq!(
            judged_branch(
                &StepKind::End {
                    finish: Finish::Done
                },
                &json!({"choice": "good"})
            ),
            None
        );
    }

    #[test]
    fn switch_fails_when_its_subject_cannot_render() {
        let (mut r, _) = started(switch_wf("{steps.a.output.env}"));
        let e = r.apply(done_with("a", json!({"x": 1})), 101).unwrap();
        assert_eq!(state(&r, "s"), &StepState::Failed);
        let why = r.steps[&sid("s")].error.clone().unwrap_or_default();
        assert!(why.starts_with("cannot render `on`"), "{why}");
        assert_eq!(
            e,
            vec![RunEffect::Finished {
                outcome: RunOutcome::Failed
            }]
        );
    }

    /// `list → each → body → each`, `each → done`; the body may be re-entered
    /// once per iteration and no more.
    fn for_each_wf(max_iterations: u16) -> Workflow {
        let mut body = agent("body", &["each"]);
        body.max_visits = 1;
        workflow(vec![
            agent("list", &["each"]),
            step(
                "each",
                StepKind::ForEach {
                    items: "{steps.list.output.items}".into(),
                    max_iterations,
                },
                vec![labelled("body", "each"), labelled("done", "done")],
            ),
            body,
            agent("done", &[]),
        ])
    }

    #[test]
    fn for_each_dispatches_one_item_per_iteration_then_done() {
        let (mut r, _) = started(for_each_wf(100));
        let e = r
            .apply(done_with("list", json!({"items": ["a", "b", "c"]})), 101)
            .unwrap();
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("body") }]);
        assert_eq!(state(&r, "done"), &StepState::Skipped);
        for (n, item) in ["a", "b", "c"].iter().enumerate() {
            assert_eq!(
                r.steps[&sid("each")].output,
                Some(json!({"item": item, "index": n, "count": 3})),
                "iteration {n}"
            );
            assert_eq!(
                state(&r, "each"),
                &StepState::chose(Branch::new("each").unwrap())
            );
            assert_eq!(
                r.steps[&sid("each")].cursor.as_ref().map(|c| c.index),
                Some(n as u32 + 1)
            );
            assert_eq!(state(&r, "body"), &StepState::Running);
            assert_eq!(r.steps[&sid("body")].visits, 1, "reset per iteration");
            let e = r.apply(done("body"), 102 + n as u64).unwrap();
            if n < 2 {
                assert_eq!(e, vec![RunEffect::StartAgent { step: sid("body") }]);
            } else {
                assert_eq!(e, vec![RunEffect::StartAgent { step: sid("done") }]);
            }
        }
        assert_eq!(
            state(&r, "each"),
            &StepState::chose(Branch::new("done").unwrap())
        );
        assert_eq!(
            r.steps[&sid("each")].output,
            Some(json!({"index": 3, "count": 3}))
        );
        assert_eq!(r.steps[&sid("each")].cursor, None);
        assert_eq!(r.steps[&sid("each")].visits, 1, "iterations are not visits");
        assert_eq!(state(&r, "done"), &StepState::Running);
        assert_eq!(r.steps[&sid("done")].visits, 1);
        let e = r.apply(done("done"), 110).unwrap();
        assert_eq!(
            e,
            vec![RunEffect::Finished {
                outcome: RunOutcome::Done
            }]
        );
    }

    /// [`for_each_wf`] with a body that never waits: a `parallel`, done the
    /// moment it is entered, so the whole list is walked inside one event.
    fn for_each_that_never_waits() -> Workflow {
        let mut wf = for_each_wf(u16::MAX);
        wf.steps[2] = step("body", StepKind::Parallel, vec![Flow::to(sid("each"))]);
        wf
    }

    #[test]
    fn a_loop_that_never_waits_is_walked_whole_inside_one_event() {
        let (mut r, _) = started(for_each_that_never_waits());
        let items: Vec<u32> = (0..4_000).collect();
        let e = r
            .apply(done_with("list", json!({ "items": items })), 101)
            .unwrap();
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("done") }]);
        assert_eq!(
            r.steps[&sid("each")].output,
            Some(json!({"index": 4_000, "count": 4_000}))
        );
    }

    #[test]
    fn a_settle_that_reaches_its_bound_fails_the_run_and_says_so() {
        let (mut r, _) = started(for_each_that_never_waits());
        let items: Vec<u32> = (0..20_000).collect();
        let e = r
            .apply(done_with("list", json!({ "items": items })), 101)
            .unwrap();
        assert_eq!(
            e,
            vec![RunEffect::Finished {
                outcome: RunOutcome::Failed
            }],
            "never a run left going with nothing moving it"
        );
        assert!(r.is_finished());
        let said: Vec<&str> = r
            .steps
            .values()
            .filter_map(|record| record.error.as_deref())
            .filter(|error| error.starts_with(UNSETTLED))
            .collect();
        assert_eq!(
            said,
            vec!["the run did not settle after 10000 rounds"],
            "on the step it was entering"
        );
    }

    #[test]
    fn for_each_with_no_items_exits_at_once() {
        let (mut r, _) = started(for_each_wf(100));
        let e = r
            .apply(done_with("list", json!({"items": []})), 101)
            .unwrap();
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("done") }]);
        assert_eq!(
            state(&r, "each"),
            &StepState::chose(Branch::new("done").unwrap())
        );
        assert_eq!(
            r.steps[&sid("each")].output,
            Some(json!({"index": 0, "count": 0}))
        );
        assert_eq!(state(&r, "body"), &StepState::Skipped);
    }

    #[test]
    fn for_each_refuses_a_non_array_and_too_many_items() {
        let (mut r, _) = started(for_each_wf(3));
        let e = r
            .apply(done_with("list", json!({"items": {"k": 1}})), 101)
            .unwrap();
        assert_eq!(state(&r, "each"), &StepState::Failed);
        let why = r.steps[&sid("each")].error.clone().unwrap_or_default();
        assert!(
            why.starts_with("`items` must render to a JSON array"),
            "{why}"
        );
        assert_eq!(
            e,
            vec![RunEffect::Finished {
                outcome: RunOutcome::Failed
            }]
        );

        let (mut r, _) = started(for_each_wf(3));
        r.apply(done_with("list", json!({"items": "x"})), 101)
            .unwrap();
        let why = r.steps[&sid("each")].error.clone().unwrap_or_default();
        assert!(why.starts_with("`items` did not render to JSON"), "{why}");

        let (mut r, _) = started(for_each_wf(3));
        r.apply(done_with("list", json!({"items": [1, 2, 3, 4]})), 101)
            .unwrap();
        assert_eq!(
            r.steps[&sid("each")].error.as_deref(),
            Some("4 items; max_iterations is 3")
        );
        assert_eq!(r.status(), RunStatus::Failed);
    }

    #[test]
    fn for_each_re_entry_is_not_a_visit() {
        let mut wf = for_each_wf(100);
        wf.steps[1].max_visits = 1;
        let (mut r, _) = started(wf);
        r.apply(done_with("list", json!({"items": [1, 2, 3]})), 101)
            .unwrap();
        for t in 0..3 {
            r.apply(done("body"), 102 + t).unwrap();
        }
        assert_eq!(state(&r, "done"), &StepState::Running);
        assert_eq!(r.steps[&sid("each")].visits, 1);
    }

    fn while_wf(when: Condition, max_iterations: u16) -> Workflow {
        workflow(vec![
            agent("a", &["w"]),
            step(
                "w",
                StepKind::While {
                    when,
                    max_iterations,
                },
                vec![labelled("body", "loop"), labelled("done", "done")],
            ),
            agent("body", &["w"]),
            agent("done", &[]),
        ])
    }

    #[test]
    fn while_loops_until_its_condition_fails() {
        let until_done = Condition::Not {
            of: Box::new(Condition::OutputEquals {
                step: sid("body"),
                path: "done".into(),
                value: json!(true),
            }),
        };
        let (mut r, _) = started(while_wf(until_done, 10));
        r.apply(done("a"), 101).unwrap();
        for n in 0..3u64 {
            assert_eq!(r.steps[&sid("w")].output, Some(json!({"index": n})));
            assert_eq!(state(&r, "body"), &StepState::Running);
            let last = n == 2;
            let e = r
                .apply(done_with("body", json!({"done": last})), 102 + n)
                .unwrap();
            if last {
                assert_eq!(e, vec![RunEffect::StartAgent { step: sid("done") }]);
            } else {
                assert_eq!(e, vec![RunEffect::StartAgent { step: sid("body") }]);
            }
        }
        assert_eq!(
            state(&r, "w"),
            &StepState::chose(Branch::new("done").unwrap())
        );
        assert_eq!(r.steps[&sid("w")].output, Some(json!({"index": 3})));
        assert_eq!(r.steps[&sid("w")].cursor, None);
        assert_eq!(r.steps[&sid("w")].visits, 1);
    }

    #[test]
    fn while_fails_when_max_iterations_is_reached() {
        let (mut r, _) = started(while_wf(always(), 2));
        r.apply(done("a"), 101).unwrap();
        r.apply(done("body"), 102).unwrap();
        assert_eq!(r.steps[&sid("w")].output, Some(json!({"index": 1})));
        let e = r.apply(done("body"), 103).unwrap();
        assert_eq!(state(&r, "w"), &StepState::Failed);
        assert_eq!(
            r.steps[&sid("w")].error.as_deref(),
            Some("ran 2 iterations; max_iterations is 2")
        );
        assert_eq!(
            e,
            vec![RunEffect::Finished {
                outcome: RunOutcome::Failed
            }]
        );
    }

    #[test]
    fn a_loop_restarted_by_an_outer_loop_starts_a_fresh_cursor() {
        // `done` decides: `again` back to `list` once, then `stop`.
        let mut wf = for_each_wf(100);
        wf.steps[3] = decide(
            "done",
            vec![(
                "again",
                Condition::Not {
                    of: Box::new(Condition::OutputEquals {
                        step: sid("list"),
                        path: "last".into(),
                        value: json!(true),
                    }),
                },
            )],
            "stop",
            vec![labelled("list", "again"), labelled("e", "stop")],
        );
        wf.steps.push(end("e", Finish::Done));
        let (mut r, _) = started(wf);
        r.apply(
            done_with("list", json!({"items": [1, 2], "last": false})),
            101,
        )
        .unwrap();
        r.apply(done("body"), 102).unwrap();
        r.apply(done("body"), 103).unwrap();
        // The loop drained, `done` chose `again`, and `list` runs a second time.
        assert_eq!(state(&r, "list"), &StepState::Running);
        assert_eq!(r.steps[&sid("list")].visits, 2);
        r.apply(done_with("list", json!({"items": [7], "last": true})), 104)
            .unwrap();
        assert_eq!(
            r.steps[&sid("each")].output,
            Some(json!({"item": 7, "index": 0, "count": 1})),
            "a fresh cursor over the new list"
        );
        assert_eq!(r.steps[&sid("each")].visits, 2, "a second start is a visit");
        let e = r.apply(done("body"), 105).unwrap();
        assert_eq!(
            e,
            vec![RunEffect::Finished {
                outcome: RunOutcome::Done
            }]
        );
    }

    fn approval_wf() -> Workflow {
        workflow(vec![
            step(
                "ok",
                StepKind::Approval {
                    prompt: "Ship?".into(),
                },
                vec![Flow::to(sid("b"))],
            ),
            agent("b", &[]),
        ])
    }

    #[test]
    fn a_declined_approval_fails_the_step() {
        let (mut r, e) = started(approval_wf());
        assert_eq!(e, vec![RunEffect::OpenGate { step: sid("ok") }]);
        assert_eq!(state(&r, "ok"), &StepState::Waiting);
        assert_eq!(r.status(), RunStatus::Waiting);
        let e = r
            .apply(
                RunEvent::Decided {
                    step: sid("ok"),
                    approve: false,
                    approval: ApprovalId("dec-1".into()),
                },
                101,
            )
            .unwrap();
        assert_eq!(state(&r, "ok"), &StepState::Failed);
        assert_eq!(r.steps[&sid("ok")].gate.as_deref(), Some("dec-1"));
        assert_eq!(
            e,
            vec![RunEffect::Finished {
                outcome: RunOutcome::Failed
            }]
        );

        let (mut r, _) = started(approval_wf());
        let e = r
            .apply(
                RunEvent::Decided {
                    step: sid("ok"),
                    approve: true,
                    approval: ApprovalId("dec-2".into()),
                },
                101,
            )
            .unwrap();
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("b") }]);
    }

    #[test]
    fn answers_are_validated_against_the_question() {
        let (mut r, e) = started(workflow(vec![
            step(
                "q",
                StepKind::Human {
                    prompt: "?".into(),
                    options: vec![AskOption::new("a", "A")],
                    multi: false,
                    assignee: None,
                },
                vec![Flow::to(sid("b"))],
            ),
            agent("b", &[]),
        ]));
        assert_eq!(e, vec![RunEffect::Ask { step: sid("q") }]);
        let before = r.clone();
        assert_eq!(
            r.apply(
                RunEvent::Answered {
                    step: sid("q"),
                    answer: Answer::selecting(["zzz"])
                },
                101
            ),
            Err(RunError::BadAnswer {
                step: "q".into(),
                source: AnswerError::UnknownOption("zzz".into())
            })
        );
        assert_eq!(r, before, "a refused event leaves the run untouched");
        assert!(r
            .apply(
                RunEvent::Answered {
                    step: sid("q"),
                    answer: Answer::unsure()
                },
                101
            )
            .is_ok());
        assert_eq!(r.steps[&sid("q")].answer, Some(Answer::unsure()));
    }

    #[test]
    fn wait_steps_accept_their_own_event_and_release() {
        let wf = |until: WaitFor| {
            workflow(vec![
                step("w", StepKind::Wait { until }, vec![Flow::to(sid("b"))]),
                agent("b", &[]),
            ])
        };
        let (mut r, e) = started(wf(WaitFor::Delay {
            secs: ValueRef::Fixed(5),
        }));
        assert_eq!(
            e,
            vec![RunEffect::Arm {
                step: sid("w"),
                until: WaitFor::Delay {
                    secs: ValueRef::Fixed(5)
                }
            }]
        );
        assert!(matches!(
            r.apply(
                RunEvent::Heard {
                    step: sid("w"),
                    payload: json!({}),
                    chain: Chain::default(),
                },
                101
            ),
            Err(RunError::WrongKind { .. })
        ));
        assert!(r.apply(RunEvent::Elapsed { step: sid("w") }, 101).is_ok());

        let (mut r, _) = started(wf(WaitFor::Signal {
            filter: crate::listen::SignalFilter {
                name: "deploy.finished".into(),
                fields: BTreeMap::new(),
            },
        }));
        assert!(matches!(
            r.apply(RunEvent::Elapsed { step: sid("w") }, 101),
            Err(RunError::WrongKind { .. })
        ));
        let e = r
            .apply(
                RunEvent::Heard {
                    step: sid("w"),
                    payload: json!({"env": "prod"}),
                    chain: Chain::default(),
                },
                101,
            )
            .unwrap();
        assert_eq!(r.steps[&sid("w")].output, Some(json!({"env": "prod"})));
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("b") }]);

        let (mut r, _) = started(wf(WaitFor::Release));
        assert!(r
            .apply(
                RunEvent::Released {
                    step: sid("w"),
                    payload: None,
                },
                101
            )
            .is_ok());
    }

    #[test]
    fn a_spawn_that_waits_is_released_by_its_child_or_a_person() {
        let (mut r, e) = started(workflow(vec![
            step(
                "s",
                StepKind::Spawn {
                    statement_template: "child".into(),
                    workflow: None,
                    assignees: vec![ValueRef::Fixed(Assignee::Agent("developer".into()))],
                    inputs: Default::default(),
                    wait: true,
                },
                vec![Flow::to(sid("b"))],
            ),
            agent("b", &[]),
        ]));
        assert_eq!(e, vec![RunEffect::SpawnGoal { step: sid("s") }]);
        assert_eq!(state(&r, "s"), &StepState::Waiting);
        assert!(r
            .apply(
                RunEvent::StepDone {
                    step: sid("s"),
                    output: json!({"child": "01X"})
                },
                101
            )
            .is_ok());
    }

    #[test]
    fn end_finishes_and_cancels_the_rest() {
        let (mut r, _) = started(workflow(vec![
            agent("a", &["b", "e"]),
            agent("b", &[]),
            end("e", Finish::Failed),
        ]));
        let e = r.apply(done("a"), 101).unwrap();
        // b starts, then e ends the run and cancels b.
        assert_eq!(
            e,
            vec![
                RunEffect::StartAgent { step: sid("b") },
                RunEffect::CancelWork {
                    steps: vec![sid("b")]
                },
                RunEffect::Finished {
                    outcome: RunOutcome::Failed
                }
            ]
        );
        assert_eq!(state(&r, "b"), &StepState::Cancelled);
        assert_eq!(r.status(), RunStatus::Failed);
    }

    #[test]
    fn draining_every_branch_finishes_done() {
        let (mut r, _) = started(workflow(vec![
            agent("a", &["b", "c"]),
            agent("b", &[]),
            agent("c", &[]),
        ]));
        r.apply(done("a"), 101).unwrap();
        assert!(r.apply(done("b"), 102).unwrap().is_empty());
        assert_eq!(r.status(), RunStatus::Running);
        let e = r.apply(done("c"), 103).unwrap();
        assert_eq!(
            e,
            vec![RunEffect::Finished {
                outcome: RunOutcome::Done
            }]
        );
    }

    #[test]
    fn amended_keeps_started_history_and_enters_new_successors_when_their_source_settles() {
        let (mut r, _) = started(workflow(vec![agent("a", &["b"]), agent("b", &[])]));
        r.apply(done("a"), 101).unwrap();
        // b is running: a started step keeps its id and kind but may gain a
        // flow — add c after b, and d after c. Nothing is enterable yet, so
        // the amendment itself has no effect; history (a done) is untouched.
        let mut wf = r.workflow.clone();
        wf.steps[1].then.push(Flow::to(sid("c")));
        wf.steps.push(agent("c", &["d"]));
        wf.steps.push(agent("d", &[]));
        let e = r.apply(RunEvent::Amended { workflow: wf }, 102).unwrap();
        assert!(e.is_empty(), "{e:?}");
        assert_eq!(state(&r, "a"), &StepState::done());
        assert_eq!(state(&r, "b"), &StepState::Running);
        assert_eq!(state(&r, "c"), &StepState::Pending);
        assert_eq!(r.workflow.steps.len(), 4);
        // The new successors enter as their sources settle.
        assert_eq!(
            r.apply(done("b"), 103).unwrap(),
            vec![RunEffect::StartAgent { step: sid("c") }]
        );
        assert_eq!(
            r.apply(done("c"), 104).unwrap(),
            vec![RunEffect::StartAgent { step: sid("d") }]
        );
        assert_eq!(
            r.apply(done("d"), 105).unwrap(),
            vec![RunEffect::Finished {
                outcome: RunOutcome::Done
            }]
        );
    }

    #[test]
    fn amended_cannot_add_a_flow_out_of_a_finished_step() {
        // a is done: its flows are history. Adding `a → d` would start d from
        // a past that is frozen, so the amendment is refused whole.
        let (mut r, _) = started(workflow(vec![agent("a", &["b"]), agent("b", &[])]));
        r.apply(done("a"), 101).unwrap();
        let before = r.clone();
        let mut wf = r.workflow.clone();
        wf.steps[0].then.push(Flow::to(sid("d")));
        wf.steps.push(agent("d", &[]));
        assert_eq!(
            r.apply(RunEvent::Amended { workflow: wf }, 102),
            Err(RunError::AmendTouchesStartedStep { step: "a".into() })
        );
        assert_eq!(r, before);
    }

    #[test]
    fn amended_refuses_to_touch_a_started_step() {
        let (mut r, _) = started(workflow(vec![agent("a", &["b"]), agent("b", &[])]));
        r.apply(done("a"), 101).unwrap();
        let before = r.clone();
        // Change a finished step's flows.
        let mut wf = r.workflow.clone();
        wf.steps[0].then = vec![];
        assert_eq!(
            r.apply(RunEvent::Amended { workflow: wf }, 102),
            Err(RunError::AmendTouchesStartedStep { step: "a".into() })
        );
        // Change a running step's kind.
        let mut wf = r.workflow.clone();
        wf.steps[1].kind = StepKind::Approval { prompt: "?".into() };
        assert_eq!(
            r.apply(RunEvent::Amended { workflow: wf }, 102),
            Err(RunError::AmendTouchesStartedStep { step: "b".into() })
        );
        // Remove a running step.
        let mut wf = r.workflow.clone();
        wf.steps.pop();
        wf.steps[0].then = vec![];
        assert!(matches!(
            r.apply(RunEvent::Amended { workflow: wf }, 102),
            Err(RunError::AmendTouchesStartedStep { .. })
        ));
        assert_eq!(r, before);
    }

    #[test]
    fn cancel_cancels_every_live_step_and_is_absorbing() {
        let (mut r, _) = started(workflow(vec![
            agent("a", &["b", "c"]),
            agent("b", &[]),
            agent("c", &[]),
        ]));
        r.apply(done("a"), 101).unwrap();
        let cause = CancelCause::Closed {
            reason: ClosureReason::Abandoned { rationale: None },
        };
        let e = r
            .apply(
                RunEvent::Cancel {
                    cause: cause.clone(),
                },
                102,
            )
            .unwrap();
        assert_eq!(
            e,
            vec![
                RunEffect::CancelWork {
                    steps: vec![sid("b"), sid("c")]
                },
                RunEffect::Cancelled {
                    cause: cause.clone()
                },
            ],
            "the work is cancelled first, then the run is settled"
        );
        assert_eq!(r.status(), RunStatus::Cancelled);
        assert_eq!(r.cancelled, Some(cause.clone()));
        assert_eq!(r.finished_at, Some(102));
        assert_eq!(r.outcome, None, "a cancelled run has no outcome");
        assert_eq!(state(&r, "a"), &StepState::done(), "history stays");
        for ev in [RunEvent::Cancel { cause }, done("b"), RunEvent::Start] {
            assert_eq!(r.apply(ev, 103), Err(RunError::Finished));
        }
    }

    #[test]
    fn a_new_run_is_queued_until_start() {
        let mut r = run(workflow(vec![agent("a", &["b"]), agent("b", &[])]));
        assert_eq!(r.status(), RunStatus::Queued);
        assert!(r.is_queued());
        assert!(!r.is_live());
        assert!(!r.is_finished());
        assert_eq!(r.queued_at, 100);
        assert_eq!(r.started_at, None);
        let before = r.clone();
        for ev in [
            done("a"),
            RunEvent::StepStarted {
                step: sid("a"),
                work_item: None,
            },
            RunEvent::StepFailed {
                step: sid("a"),
                error: "x".into(),
            },
            RunEvent::StepInterrupted { step: sid("a") },
            RunEvent::Released {
                step: sid("a"),
                payload: None,
            },
            RunEvent::Elapsed { step: sid("a") },
        ] {
            assert_eq!(r.apply(ev, 101), Err(RunError::NotStarted));
            assert_eq!(r, before, "a refused event leaves the run byte-identical");
        }
        let e = r.apply(RunEvent::Start, 105).unwrap();
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("a") }]);
        assert_eq!(r.started_at, Some(105));
        assert_eq!(
            r.queued_at, 100,
            "the queue stamp is the making, not the start"
        );
        assert_eq!(r.status(), RunStatus::Running);
        assert!(r.is_live());
        assert!(!r.is_queued());
    }

    /// A withdrawal takes a run out of a queue, and only that: a run that
    /// has started is no longer in one, and refuses it — so a run that starts
    /// between somebody's look and their withdrawal is never ended as
    /// *withdrawn* with its work left going and nobody stopping it.
    #[test]
    fn a_withdrawal_is_refused_by_a_run_that_has_started() {
        let mut r = run(workflow(vec![agent("a", &[])]));
        r.apply(RunEvent::Start, 101).unwrap();
        let before = r.clone();
        assert_eq!(
            r.apply(
                RunEvent::Cancel {
                    cause: CancelCause::Withdrawn,
                },
                102,
            ),
            Err(RunError::AlreadyStarted)
        );
        assert_eq!(r, before, "refused, it moved nothing");
        // Every other cause ends a started run as it always did.
        r.apply(
            RunEvent::Cancel {
                cause: CancelCause::Stopped { rationale: None },
            },
            103,
        )
        .unwrap();
        assert_eq!(r.status(), RunStatus::Cancelled);
    }

    #[test]
    fn a_queued_run_cancels_with_nothing_to_cancel() {
        let mut r = run(workflow(vec![agent("a", &[])]));
        let e = r
            .apply(
                RunEvent::Cancel {
                    cause: CancelCause::Withdrawn,
                },
                101,
            )
            .unwrap();
        assert_eq!(
            e,
            vec![RunEffect::Cancelled {
                cause: CancelCause::Withdrawn
            }],
            "nothing was live, so no work to cancel"
        );
        assert_eq!(r.status(), RunStatus::Cancelled);
        assert_eq!(r.started_at, None, "it never started");
        assert_eq!(r.finished_at, Some(101));
        assert!(!r.is_queued() && !r.is_live());
        assert_eq!(
            state(&r, "a"),
            &StepState::Cancelled,
            "a pending step of a withdrawn run reads cancelled"
        );
        assert_eq!(r.apply(RunEvent::Start, 102), Err(RunError::Finished));
    }

    #[test]
    fn a_cancel_on_a_live_run_emits_cancel_work_then_cancelled() {
        let (mut r, _) = started(workflow(vec![agent("a", &[])]));
        let e = r
            .apply(
                RunEvent::Cancel {
                    cause: CancelCause::Stopped {
                        rationale: Some("enough".into()),
                    },
                },
                101,
            )
            .unwrap();
        assert_eq!(
            e,
            vec![
                RunEffect::CancelWork {
                    steps: vec![sid("a")]
                },
                RunEffect::Cancelled {
                    cause: CancelCause::Stopped {
                        rationale: Some("enough".into())
                    }
                },
            ]
        );
        assert_eq!(r.status(), RunStatus::Cancelled);
        let e = {
            let (mut r, _) = started(workflow(vec![agent("a", &[])]));
            r.apply(
                RunEvent::Cancel {
                    cause: CancelCause::Restarted,
                },
                101,
            )
            .unwrap()
        };
        assert_eq!(
            e.last(),
            Some(&RunEffect::Cancelled {
                cause: CancelCause::Restarted
            })
        );
    }

    #[test]
    fn start_twice_is_already_started() {
        let (mut r, _) = started(workflow(vec![agent("a", &["b"]), agent("b", &[])]));
        let before = r.clone();
        assert_eq!(r.apply(RunEvent::Start, 101), Err(RunError::AlreadyStarted));
        assert_eq!(r, before);
        assert_eq!(r.started_at, Some(100), "the first start's stamp stays");
    }

    #[test]
    fn run_status_matrix() {
        let queued = run(approval_wf());
        assert_eq!(queued.status(), RunStatus::Queued);
        let (waiting, _) = started(approval_wf());
        assert_eq!(waiting.status(), RunStatus::Waiting);
        let (running, _) = started(workflow(vec![agent("a", &[])]));
        assert_eq!(running.status(), RunStatus::Running);
        let (mut done_run, _) = started(workflow(vec![agent("a", &[])]));
        done_run.apply(done("a"), 101).unwrap();
        assert_eq!(done_run.status(), RunStatus::Done);
        let (mut failed, _) = started(workflow(vec![agent("a", &[])]));
        failed
            .apply(
                RunEvent::StepFailed {
                    step: sid("a"),
                    error: "red".into(),
                },
                101,
            )
            .unwrap();
        assert_eq!(failed.status(), RunStatus::Failed);
        let mut cancelled = run(workflow(vec![agent("a", &[])]));
        cancelled
            .apply(
                RunEvent::Cancel {
                    cause: CancelCause::Withdrawn,
                },
                101,
            )
            .unwrap();
        assert_eq!(cancelled.status(), RunStatus::Cancelled);
        for s in RunStatus::ALL {
            assert_eq!(
                s.is_live(),
                matches!(s, RunStatus::Running | RunStatus::Waiting)
            );
            assert_eq!(
                s.is_finished(),
                matches!(
                    s,
                    RunStatus::Done | RunStatus::Failed | RunStatus::Cancelled
                )
            );
            assert_eq!(
                serde_json::to_value(s).unwrap(),
                serde_json::Value::String(s.as_str().into())
            );
        }
        for r in [&queued, &waiting, &running, &done_run, &failed, &cancelled] {
            assert_eq!(r.is_live(), r.status().is_live());
            assert_eq!(r.is_queued(), r.status() == RunStatus::Queued);
            assert_eq!(r.is_finished(), r.status().is_finished());
        }
    }

    #[test]
    fn cancel_cause_words_round_trip() {
        let causes = [
            CancelCause::Stopped { rationale: None },
            CancelCause::Stopped {
                rationale: Some("enough".into()),
            },
            CancelCause::Restarted,
            CancelCause::Withdrawn,
            CancelCause::Closed {
                reason: ClosureReason::Superseded {
                    by: GoalId::from_ulid(ulid::Ulid::from_parts(2, 2)),
                },
            },
            CancelCause::Retired,
        ];
        for c in &causes {
            let json = serde_json::to_value(c).unwrap();
            assert_eq!(json["cause"], c.as_str(), "{c:?}");
            assert_eq!(serde_json::from_value::<CancelCause>(json).unwrap(), *c);
        }
        assert_eq!(
            serde_json::to_string(&CancelCause::Retired).unwrap(),
            r#"{"cause":"retired"}"#,
            "a retired run names no rationale and no reason"
        );
        let json = serde_json::to_value(&causes[4]).unwrap();
        assert_eq!(
            json["reason"]["reason"], "superseded",
            "the goal's reason nests"
        );
        assert_eq!(
            serde_json::to_string(&RunEvent::Cancel {
                cause: CancelCause::Restarted
            })
            .unwrap(),
            r#"{"event":"cancel","cause":{"cause":"restarted"}}"#
        );
        assert_eq!(
            serde_json::to_string(&RunEffect::Cancelled {
                cause: CancelCause::Withdrawn
            })
            .unwrap(),
            r#"{"effect":"cancelled","cause":{"cause":"withdrawn"}}"#
        );
    }

    #[test]
    fn a_workspace_run_carries_its_ceiling_names_no_goal_and_is_its_own_home() {
        let budget = Budget {
            max_usd_cents: Some(500),
            ..Budget::default()
        };
        let id = RunId::from_ulid(ulid::Ulid::from_parts(4, 9));
        let r = WorkflowRun::new(
            id,
            RunScope::Workspace {
                budget: budget.clone(),
            },
            workflow(vec![agent("a", &[])]),
            BTreeMap::new(),
            RunEntry::by_hand(),
            100,
        );
        assert!(r.scope.is_workspace());
        assert_eq!(r.scope.goal(), None);
        assert_eq!(r.scope.budget(), Some(&budget));
        assert_eq!(
            r.home(),
            Home::Run { run: id },
            "a run of the workspace is its own home"
        );
        let goals = run(workflow(vec![agent("a", &[])]));
        assert!(!goals.scope.is_workspace());
        assert_eq!(
            goals.scope.budget(),
            None,
            "a goal's run spends against its goal's ceiling"
        );
        assert_eq!(
            goals.home(),
            Home::Goal {
                goal: goals.scope.goal().unwrap()
            }
        );
    }

    #[test]
    fn a_run_scope_round_trips_and_a_snapshot_naming_its_goal_at_the_top_is_refused() {
        let ceiling = Budget {
            max_tokens: Some(9),
            ..Budget::default()
        };
        for scope in [
            goal_scope(),
            RunScope::Workspace {
                budget: Budget::default(),
            },
            RunScope::Workspace { budget: ceiling },
        ] {
            let wire = serde_json::to_value(&scope).unwrap();
            assert_eq!(wire["scope"], scope.as_str());
            assert_eq!(serde_json::from_value::<RunScope>(wire).unwrap(), scope);
        }
        assert_eq!(
            serde_json::to_value(RunScope::Workspace {
                budget: Budget::default()
            })
            .unwrap(),
            json!({"scope": "workspace"}),
            "an unlimited ceiling is absent"
        );
        assert_eq!(RunScope::NAMES, ["workspace", "goal"]);
        // The shape before scopes named its goal at the top level: this build
        // refuses it rather than guessing which scope it meant.
        let mut old = serde_json::to_value(run(workflow(vec![agent("a", &[])]))).unwrap();
        let goal = old["scope"]["goal"].clone();
        old.as_object_mut().unwrap().remove("scope");
        old["goal"] = goal;
        let err = serde_json::from_value::<WorkflowRun>(old)
            .unwrap_err()
            .to_string();
        assert!(err.contains("goal") || err.contains("scope"), "{err}");
    }

    #[test]
    fn a_runs_holder_is_read_off_its_live_steps() {
        let ask = step(
            "ask",
            StepKind::Human {
                prompt: "Which way?".into(),
                options: vec![],
                multi: false,
                assignee: None,
            },
            vec![],
        );
        let (asking, _) = started(workflow(vec![ask]));
        assert_eq!(
            asking.holder(false),
            Holder::You,
            "a person answers a human step"
        );
        let (mut working, _) = started(workflow(vec![agent("a", &[])]));
        assert_eq!(working.holder(false), Holder::Agents);
        assert_eq!(
            working.holder(true),
            Holder::You,
            "owed wins over the steps"
        );
        working.apply(done("a"), 101).unwrap();
        assert_eq!(working.holder(false), Holder::Finished);
        let queued = run(workflow(vec![agent("a", &[])]));
        assert_eq!(
            queued.holder(false),
            Holder::Agents,
            "a goal's queued run is the platform's to start"
        );
    }

    #[test]
    fn events_before_start_and_twice_started_are_refused() {
        let mut r = run(workflow(vec![agent("a", &[])]));
        assert_eq!(r.apply(done("a"), 100), Err(RunError::NotStarted));
        r.apply(RunEvent::Start, 100).unwrap();
        assert_eq!(r.apply(RunEvent::Start, 101), Err(RunError::AlreadyStarted));
        assert_eq!(
            r.apply(done("zzz"), 101),
            Err(RunError::UnknownStep { step: "zzz".into() })
        );
        assert!(matches!(
            r.apply(
                RunEvent::Answered {
                    step: sid("a"),
                    answer: Answer::text("x")
                },
                101
            ),
            Err(RunError::WrongKind { .. })
        ));
        r.apply(done("a"), 101).unwrap();
    }

    #[test]
    fn a_step_that_is_not_live_refuses_its_event() {
        let (mut r, _) = started(workflow(vec![agent("a", &["b"]), agent("b", &[])]));
        assert_eq!(
            r.apply(done("b"), 101),
            Err(RunError::NotLive {
                step: "b".into(),
                event: "step_done",
                state: "pending",
                expected: "running"
            })
        );
    }

    #[test]
    fn status_is_a_projection() {
        let (mut r, _) = started(approval_wf());
        assert_eq!(r.status(), RunStatus::Waiting);
        r.apply(
            RunEvent::Decided {
                step: sid("ok"),
                approve: true,
                approval: ApprovalId("d".into()),
            },
            101,
        )
        .unwrap();
        assert_eq!(r.status(), RunStatus::Running);
        r.apply(done("b"), 102).unwrap();
        assert_eq!(r.status(), RunStatus::Done);
        assert!(r.status().is_finished());
    }

    #[test]
    fn a_decide_reads_what_the_start_mapped_from_the_event() {
        // The event is read once, by the start's mapping, into a typed input;
        // the decide tests the input like any other.
        let mut ticket = step(
            "ticket",
            StepKind::Start {
                on: crate::start::StartOn::Hook { public: false },
                inputs: BTreeMap::from([("env".to_string(), "{event.payload.env}".to_string())]),
                guard: crate::start::Guard::default(),
            },
            vec![Flow::to(sid("d"))],
        );
        ticket.name = "A deploy is called".into();
        let mut wf = workflow(vec![
            ticket,
            decide(
                "d",
                vec![(
                    "prod",
                    Condition::InputEquals {
                        input: InputName::new("env").unwrap(),
                        value: json!("prod"),
                    },
                )],
                "other",
                vec![labelled("p", "prod"), labelled("o", "other")],
            ),
            agent("p", &[]),
            agent("o", &[]),
        ]);
        wf.inputs = vec![InputDef {
            name: InputName::new("env").unwrap(),
            label: "Env".into(),
            kind: InputKind::Text,
            default: None,
            required: false,
        }];
        let event = signal(json!({"env": "prod"}));
        let StepKind::Start {
            inputs: mapping, ..
        } = &wf.steps[0].kind
        else {
            unreachable!("the first step is the start")
        };
        let inputs =
            crate::start::map_event(mapping, &wf.inputs, &serde_json::to_value(&event).unwrap())
                .unwrap();
        let mut r = WorkflowRun::new(
            RunId::from_ulid(ulid::Ulid::from_parts(4, 2)),
            goal_scope(),
            wf,
            inputs,
            RunEntry::at(sid("ticket"), Some(event)),
            100,
        );
        let e = r.apply(RunEvent::Start, 100).unwrap();
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("p") }]);
        assert_eq!(state(&r, "o"), &StepState::Skipped);
        assert_eq!(state(&r, "ticket"), &StepState::done());
    }

    #[test]
    fn start_with_two_roots_is_refused_and_a_cycle_starts_at_the_first_step() {
        // Two roots — nothing flows into either — is no *single* start, so the
        // run refuses and is untouched.
        let mut r = run(workflow(vec![agent("a", &[]), agent("b", &[])]));
        let before = r.clone();
        assert_eq!(r.apply(RunEvent::Start, 100), Err(RunError::NoStart));
        assert_eq!(r, before);
        assert!(!r.started());
        // Two steps flowing into each other: every step is a loop's target,
        // so the first in display order is the start (`Workflow::start_steps`).
        let mut r = run(workflow(vec![agent("a", &["b"]), agent("b", &["a"])]));
        assert_eq!(
            r.apply(RunEvent::Start, 100).unwrap(),
            vec![RunEffect::StartAgent { step: sid("a") }]
        );
        assert_eq!(state(&r, "a"), &StepState::Running);
        assert_eq!(state(&r, "b"), &StepState::Pending);
        // An empty workflow has no start either.
        let mut r = run(workflow(vec![]));
        assert_eq!(r.apply(RunEvent::Start, 100), Err(RunError::NoStart));
    }

    #[test]
    fn amendment_with_another_id_is_refused() {
        let (mut r, _) = started(workflow(vec![agent("a", &["b"]), agent("b", &[])]));
        let mut other = r.workflow.clone();
        other.id = crate::id::WorkflowId::from_ulid(ulid::Ulid::from_parts(9, 77));
        let before = r.clone();
        assert_eq!(
            r.apply(
                RunEvent::Amended {
                    workflow: other.clone()
                },
                101
            ),
            Err(RunError::AmendChangesWorkflow {
                expected: r.workflow.id.to_string(),
                got: other.id.to_string(),
            })
        );
        assert_eq!(r, before);
    }

    fn text_input(name: &str, default: Option<serde_json::Value>, required: bool) -> InputDef {
        InputDef {
            name: InputName::new(name).unwrap(),
            label: name.to_uppercase(),
            kind: InputKind::Text,
            default,
            required,
        }
    }

    #[test]
    fn amendment_adding_required_input_without_default_is_refused() {
        let (mut r, _) = started(workflow(vec![agent("a", &["b"]), agent("b", &[])]));
        let mut wf = r.workflow.clone();
        wf.inputs.push(text_input("x", None, true));
        wf.steps[1].kind = StepKind::Agent {
            instructions: "use {inputs.x}".into(),
            assignee: None,
            project: None,
            harness: vec![],
            model: None,
            effort: None,
            output_schema: None,
            tier_ceiling: crate::caps::ToolTier::Write,
        };
        let before = r.clone();
        assert_eq!(
            r.apply(RunEvent::Amended { workflow: wf }, 101),
            Err(RunError::AmendNeedsInput(InputError::Missing {
                input: "x".into()
            }))
        );
        assert_eq!(r, before);
    }

    #[test]
    fn amendment_adding_input_with_default_fills_it() {
        let (mut r, _) = started(workflow(vec![agent("a", &["b"]), agent("b", &[])]));
        let mut wf = r.workflow.clone();
        wf.inputs.push(text_input("x", Some(json!("v")), false));
        wf.steps[1].kind = StepKind::Agent {
            instructions: "use {inputs.x}".into(),
            assignee: None,
            project: None,
            harness: vec![],
            model: None,
            effort: None,
            output_schema: None,
            tier_ceiling: crate::caps::ToolTier::Write,
        };
        r.apply(RunEvent::Amended { workflow: wf }, 101).unwrap();
        assert_eq!(r.inputs.get("x"), Some(&json!("v")));
        assert_eq!(r.workflow.inputs.len(), 1);
    }

    #[test]
    fn step_started_binds_the_work_item() {
        let (mut r, _) = started(workflow(vec![agent("a", &[])]));
        let item = WorkItemId::from_ulid(ulid::Ulid::from_parts(2, 2));
        assert!(r
            .apply(
                RunEvent::StepStarted {
                    step: sid("a"),
                    work_item: Some(item)
                },
                101
            )
            .unwrap()
            .is_empty());
        assert_eq!(r.steps[&sid("a")].work_item, Some(item));
        assert_eq!(state(&r, "a"), &StepState::Running);
    }

    #[test]
    fn run_json_roundtrip() {
        let (mut r, _) = started(branching());
        r.apply(done("c"), 101).unwrap();
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["queued_at"], 100);
        assert_eq!(json["started_at"], 100);
        assert_eq!(json.get("cancelled"), None, "absent until cancelled");
        let queued = serde_json::to_value(run(branching())).unwrap();
        assert_eq!(queued["queued_at"], 100);
        assert_eq!(queued.get("started_at"), None, "absent while queued");
        // A record's state is one tagged object, `{state, branches?}` — the
        // shape the desktop reads (`rec.state.state`, `rec.state.branches`).
        assert_eq!(json["steps"]["d"]["state"]["state"], "done");
        assert_eq!(json["steps"]["d"]["state"]["branches"], json!(["yes"]));
        assert_eq!(json["steps"]["n"]["state"]["state"], "skipped");
        assert_eq!(
            json["steps"]["n"]["state"].get("branches"),
            None,
            "no branch on a skip"
        );
        assert_eq!(
            json["steps"]["c"]["state"].get("branches"),
            None,
            "none named by a step that chose nothing"
        );
        assert_eq!(serde_json::from_value::<WorkflowRun>(json).unwrap(), r);
        assert_eq!(
            serde_json::to_string(&RunEvent::Elapsed { step: sid("w") }).unwrap(),
            r#"{"event":"elapsed","step":"w"}"#
        );

        // A loop mid-flight carries its cursor, and a `one` join its word.
        let mut wf = for_each_wf(100);
        wf.steps[3].join = Join::One;
        let (mut r, _) = started(wf);
        r.apply(done_with("list", json!({"items": [1, 2]})), 101)
            .unwrap();
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["steps"]["each"]["cursor"]["index"], 1);
        assert_eq!(json["steps"]["each"]["cursor"]["items"], json!([1, 2]));
        assert_eq!(json["workflow"]["steps"][3]["join"], "one");
        assert_eq!(serde_json::from_value::<WorkflowRun>(json).unwrap(), r);
    }

    // -----------------------------------------------------------------------
    // Property tests: totality and the kind table.
    // -----------------------------------------------------------------------

    fn human(id: &str, options: &[&str], then: &[&str]) -> Step {
        step(
            id,
            StepKind::Human {
                prompt: "?".into(),
                options: options.iter().map(|o| AskOption::new(*o, *o)).collect(),
                multi: false,
                assignee: None,
            },
            then.iter().map(|t| Flow::to(sid(t))).collect(),
        )
    }

    fn done_with(step: &str, output: serde_json::Value) -> RunEvent {
        RunEvent::StepDone {
            step: sid(step),
            output,
        }
    }

    /// The shape the Workflow Agent designs for almost every goal: build,
    /// check, a person reviews, a decide sends it back to rework or on to the
    /// end. The loop edge `rework → renders` used to hold `renders` forever
    /// after `build`, and the run was reported done with five steps cancelled.
    fn rework_loop() -> Workflow {
        let mut renders = check("renders", &["review"]);
        renders.on_fail = OnFail::Then {
            step: sid("rework"),
        };
        let mut rework = agent("rework", &["renders"]);
        rework.max_visits = 2;
        workflow(vec![
            agent("build", &["renders"]),
            renders,
            human("review", &["accept", "revise"], &["verdict"]),
            decide(
                "verdict",
                vec![(
                    "accept",
                    Condition::Answered {
                        step: sid("review"),
                        option: "accept".into(),
                    },
                )],
                "revise",
                vec![labelled("done", "accept"), labelled("rework", "revise")],
            ),
            rework,
            end("done", Finish::Done),
        ])
    }

    #[test]
    fn a_rework_loop_enters_the_join_target_on_first_pass() {
        let (mut r, e) = started(rework_loop());
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("build") }]);
        let e = r.apply(done("build"), 101).unwrap();
        assert_eq!(
            e,
            vec![RunEffect::RunCheck {
                step: sid("renders")
            }],
            "{r:?}"
        );
        assert_eq!(state(&r, "renders"), &StepState::Running);
        assert!(!r.is_finished());
        for id in ["review", "verdict", "rework", "done"] {
            assert_eq!(
                state(&r, id),
                &StepState::Pending,
                "{id} waits, not cancelled"
            );
        }
        // The check passes, the person asks for a revision: the loop fires.
        r.apply(done("renders"), 102).unwrap();
        let e = r
            .apply(
                RunEvent::Answered {
                    step: sid("review"),
                    answer: Answer::selecting(["revise"]),
                },
                103,
            )
            .unwrap();
        assert_eq!(
            e,
            vec![RunEffect::StartAgent {
                step: sid("rework")
            }]
        );
        assert_eq!(
            state(&r, "done"),
            &StepState::Skipped,
            "the other branch is skipped for now"
        );
        // Rework done: the loop edge re-enters the check — its second visit.
        let e = r.apply(done("rework"), 104).unwrap();
        assert_eq!(
            e,
            vec![RunEffect::RunCheck {
                step: sid("renders")
            }]
        );
        assert_eq!(r.steps[&sid("renders")].visits, 2);
        assert_eq!(
            r.steps[&sid("build")].visits,
            1,
            "a forward edge already taken is not re-taken"
        );
        r.apply(done("renders"), 105).unwrap();
        assert_eq!(r.steps[&sid("review")].visits, 2);
        let e = r
            .apply(
                RunEvent::Answered {
                    step: sid("review"),
                    answer: Answer::selecting(["accept"]),
                },
                106,
            )
            .unwrap();
        assert_eq!(
            e,
            vec![RunEffect::Finished {
                outcome: RunOutcome::Done
            }]
        );
        assert_eq!(r.steps[&sid("rework")].visits, 1);
        assert_eq!(
            r.steps[&sid("renders")].visits,
            2,
            "the loop edge fired once"
        );
    }

    #[test]
    fn a_fail_route_loop_edge_does_not_hold_the_join() {
        let mut tests = check("tests", &["review"]);
        tests.on_fail = OnFail::Then {
            step: sid("implement"),
        };
        let (mut r, _) = started(workflow(vec![
            agent("implement", &["tests"]),
            tests,
            agent("review", &[]),
        ]));
        let e = r.apply(done("implement"), 101).unwrap();
        assert_eq!(e, vec![RunEffect::RunCheck { step: sid("tests") }]);
        // The check fails: its fail route re-enters the start step.
        let e = r
            .apply(
                RunEvent::StepFailed {
                    step: sid("tests"),
                    error: "red".into(),
                },
                102,
            )
            .unwrap();
        assert_eq!(
            e,
            vec![RunEffect::StartAgent {
                step: sid("implement")
            }]
        );
        assert_eq!(r.steps[&sid("implement")].visits, 2);
        let e = r.apply(done("implement"), 103).unwrap();
        assert_eq!(e, vec![RunEffect::RunCheck { step: sid("tests") }]);
        let e = r.apply(done("tests"), 104).unwrap();
        assert_eq!(
            e,
            vec![RunEffect::StartAgent {
                step: sid("review")
            }]
        );
    }

    #[test]
    fn a_workflow_whose_first_step_is_a_loop_target_starts() {
        let (mut r, e) = started(workflow(vec![
            agent("survey", &["sound"]),
            decide(
                "sound",
                vec![(
                    "ok",
                    Condition::OutputEquals {
                        step: sid("survey"),
                        path: "ok".into(),
                        value: json!(true),
                    },
                )],
                "again",
                vec![labelled("write", "ok"), labelled("survey", "again")],
            ),
            agent("write", &[]),
        ]));
        assert_eq!(
            e,
            vec![RunEffect::StartAgent {
                step: sid("survey")
            }]
        );
        let e = r
            .apply(done_with("survey", json!({"ok": false})), 101)
            .unwrap();
        assert_eq!(
            e,
            vec![RunEffect::StartAgent {
                step: sid("survey")
            }],
            "the loop re-enters the start"
        );
        assert_eq!(r.steps[&sid("survey")].visits, 2);
        let e = r
            .apply(done_with("survey", json!({"ok": true})), 102)
            .unwrap();
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("write") }]);
        assert_eq!(
            r.apply(done("write"), 103).unwrap(),
            vec![RunEffect::Finished {
                outcome: RunOutcome::Done
            }]
        );
    }

    #[test]
    fn a_join_inside_a_loop_body_still_waits_for_every_forward_edge() {
        // implement → {tests, lint} → review(all) → verdict(ship | again → implement)
        let (mut r, _) = started(workflow(vec![
            agent("implement", &["tests", "lint"]),
            agent("tests", &["review"]),
            agent("lint", &["review"]),
            agent("review", &["verdict"]),
            decide(
                "verdict",
                vec![(
                    "ship",
                    Condition::OutputEquals {
                        step: sid("review"),
                        path: "ok".into(),
                        value: json!(true),
                    },
                )],
                "again",
                vec![labelled("end", "ship"), labelled("implement", "again")],
            ),
            end("end", Finish::Done),
        ]));
        r.apply(done("implement"), 101).unwrap();
        assert_eq!(state(&r, "tests"), &StepState::Running);
        assert_eq!(state(&r, "lint"), &StepState::Running);
        r.apply(done("tests"), 102).unwrap();
        assert_eq!(
            state(&r, "review"),
            &StepState::Pending,
            "the arms are forward edges: review waits for lint"
        );
        let e = r.apply(done("lint"), 103).unwrap();
        assert_eq!(
            e,
            vec![RunEffect::StartAgent {
                step: sid("review")
            }]
        );
        // Second pass around the loop: the join waits again.
        let e = r
            .apply(done_with("review", json!({"ok": false})), 104)
            .unwrap();
        assert_eq!(
            e,
            vec![RunEffect::StartAgent {
                step: sid("implement")
            }]
        );
        r.apply(done("implement"), 105).unwrap();
        r.apply(done("tests"), 106).unwrap();
        // Still both arms, on the second pass: review keeps its first-pass
        // record (done, one visit) and is not re-entered until lint settles.
        assert_eq!(state(&r, "review"), &StepState::done());
        assert_eq!(
            r.steps[&sid("review")].visits,
            1,
            "not re-entered on one arm"
        );
        assert_eq!(state(&r, "lint"), &StepState::Running);
        let e = r.apply(done("lint"), 107).unwrap();
        assert_eq!(
            e,
            vec![RunEffect::StartAgent {
                step: sid("review")
            }]
        );
        assert_eq!(r.steps[&sid("review")].visits, 2);
        let e = r
            .apply(done_with("review", json!({"ok": true})), 108)
            .unwrap();
        assert_eq!(
            e,
            vec![RunEffect::Finished {
                outcome: RunOutcome::Done
            }]
        );
    }

    #[test]
    fn a_stall_finishes_failed_and_names_the_flows() {
        // a → b, then an amendment slips in an orphan `z → b` (the store
        // validates amendments; the machine itself does not, so the shape
        // is reachable). `b` waits on `z`, which nothing ever enters.
        let (mut r, _) = started(workflow(vec![agent("a", &["b"]), agent("b", &[])]));
        let mut wf = r.workflow.clone();
        wf.steps.push(agent("z", &["b"]));
        r.apply(RunEvent::Amended { workflow: wf }, 101).unwrap();
        let e = r.apply(done("a"), 102).unwrap();
        assert_eq!(
            e,
            vec![RunEffect::Finished {
                outcome: RunOutcome::Failed
            }],
            "a stall is a failure, never a quiet success"
        );
        assert_eq!(r.outcome, Some(RunOutcome::Failed));
        assert_eq!(state(&r, "b"), &StepState::Failed);
        let why = r.steps[&sid("b")].error.clone().unwrap_or_default();
        assert!(why.starts_with("stalled:"), "{why}");
        assert!(why.contains("z → b"), "{why}");
        assert_eq!(state(&r, "z"), &StepState::Failed);
        assert!(r.steps[&sid("z")]
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("nothing flows into it"));
        assert!(r.steps.values().all(|s| s.state != StepState::Pending));
    }

    #[test]
    fn a_stalled_fail_route_is_named_as_such() {
        // a → b; c's failure would route to b, but nothing ever enters c.
        let (mut r, _) = started(workflow(vec![agent("a", &["b"]), agent("b", &[])]));
        let mut wf = r.workflow.clone();
        let mut c = agent("c", &[]);
        c.on_fail = OnFail::Then { step: sid("b") };
        wf.steps.push(c);
        r.apply(RunEvent::Amended { workflow: wf }, 101).unwrap();
        r.apply(done("a"), 102).unwrap();
        assert_eq!(r.outcome, Some(RunOutcome::Failed));
        let why = r.steps[&sid("b")].error.clone().unwrap_or_default();
        assert!(why.contains("c → b (on failure)"), "{why}");
    }

    // -------------------------------------------------------------------
    // Events and gateways
    // -------------------------------------------------------------------

    use crate::boundary::{Boundary, BoundaryAct, BoundaryOn};
    use crate::listen::{ListenerHost, ListenerKey, MessageFilter, SignalFilter};
    use crate::start::{Guard, Schedule, StartOn};

    fn signal(payload: serde_json::Value) -> Signal {
        Signal {
            id: "01SIGNAL".into(),
            listener: None,
            source: crate::listen::SignalSource::Hook,
            name: None,
            at: 90,
            payload,
            scope: crate::listen::SignalScope::Workspace,
            chain: Chain::default(),
            dedupe_key: None,
        }
    }

    fn begins(id: &str, on: StartOn, then: &[&str]) -> Step {
        step(
            id,
            StepKind::Start {
                on,
                inputs: BTreeMap::new(),
                guard: Guard::default(),
            },
            then.iter().map(|t| Flow::to(sid(t))).collect(),
        )
    }

    fn by_hand(id: &str, then: &[&str]) -> Step {
        begins(id, StartOn::Manual, then)
    }

    fn weekly(id: &str, then: &[&str]) -> Step {
        begins(
            id,
            StartOn::Schedule {
                schedule: Schedule::cron("0 9 * * 1", None),
            },
            then,
        )
    }

    fn timeout(name: &str) -> Boundary {
        Boundary {
            name: Branch::new(name).unwrap(),
            on: BoundaryOn::After {
                secs: ValueRef::Fixed(60),
            },
            act: BoundaryAct::Divert,
        }
    }

    fn reminder(name: &str, max: u32) -> Boundary {
        Boundary {
            name: Branch::new(name).unwrap(),
            on: BoundaryOn::Every {
                secs: ValueRef::Fixed(60),
                max,
            },
            act: BoundaryAct::Notify {
                scope: None,
                template: "still waiting".into(),
                mentions: vec![],
                author: None,
            },
        }
    }

    /// A boundary event firing for the step's current visit.
    fn fire(r: &WorkflowRun, step: &str, boundary: &str) -> RunEvent {
        RunEvent::BoundaryFired {
            step: sid(step),
            boundary: Branch::new(boundary).unwrap(),
            entered: r.steps[&sid(step)].entered,
            payload: json!({}),
            chain: Chain::default(),
        }
    }

    fn run_at(wf: Workflow, entry: RunEntry) -> WorkflowRun {
        WorkflowRun::new(
            RunId::from_ulid(ulid::Ulid::from_parts(4, 7)),
            goal_scope(),
            wf,
            BTreeMap::new(),
            entry,
            100,
        )
    }

    #[test]
    fn a_run_enters_the_start_it_began_at_and_skips_the_others() {
        let wf = workflow(vec![
            by_hand("hand", &["a"]),
            weekly("weekly", &["b"]),
            agent("a", &["join"]),
            agent("b", &["join"]),
            agent("join", &[]),
        ]);
        let mut r = run_at(
            wf.clone(),
            RunEntry::at(sid("weekly"), Some(signal(json!({})))),
        );
        assert_eq!(
            r.apply(RunEvent::Start, 100).unwrap(),
            vec![RunEffect::StartAgent { step: sid("b") }]
        );
        assert_eq!(state(&r, "weekly"), &StepState::done());
        assert_eq!(state(&r, "hand"), &StepState::Skipped);
        assert_eq!(state(&r, "a"), &StepState::Skipped);
        assert_eq!(r.start, Some(sid("weekly")));
        assert_eq!(
            r.apply(done("b"), 101).unwrap(),
            vec![RunEffect::StartAgent { step: sid("join") }],
            "the join waits on nothing a skipped start leads to"
        );

        let mut r = run_at(wf.clone(), RunEntry::by_hand());
        assert_eq!(
            r.apply(RunEvent::Start, 100).unwrap(),
            vec![RunEffect::StartAgent { step: sid("a") }]
        );
        assert_eq!(
            r.start,
            Some(sid("hand")),
            "`Start` writes down where it began"
        );
        assert_eq!(state(&r, "weekly"), &StepState::Skipped);

        let mut r = run_at(wf, RunEntry::at(sid("a"), None));
        assert_eq!(r.apply(RunEvent::Start, 100), Err(RunError::NoStart));

        let events_only = workflow(vec![weekly("weekly", &["b"]), agent("b", &[])]);
        let mut r = run_at(events_only, RunEntry::by_hand());
        assert_eq!(
            r.apply(RunEvent::Start, 100),
            Err(RunError::NoStart),
            "a workflow only events begin has no way in by hand"
        );
    }

    #[test]
    fn a_way_in_is_what_a_start_and_an_event_name() {
        let wf = workflow(vec![
            by_hand("hand", &["a"]),
            begins("ticket", StartOn::Hook { public: false }, &["a"]),
            agent("a", &[]),
        ]);
        let way = |start: Option<&str>, event: Option<serde_json::Value>| {
            WayIn::of(Some(&wf), start.map(sid), event)
        };
        assert_eq!(way(None, None), Ok(WayIn::Start));
        assert_eq!(way(Some("hand"), None), Ok(WayIn::ByHand));
        // An event start is a test run: of the sample, or of an empty one.
        assert_eq!(
            way(Some("ticket"), Some(json!({"subject": "help"}))),
            Ok(WayIn::Test {
                start: sid("ticket"),
                payload: json!({"subject": "help"}),
            })
        );
        assert_eq!(
            way(Some("ticket"), None),
            Ok(WayIn::Test {
                start: sid("ticket"),
                payload: json!({}),
            })
        );
        // The start by hand reads no event, and an event needs its start.
        assert_eq!(
            way(Some("hand"), Some(json!({}))),
            Err(NoWayIn::ByHandReadsNoEvent { step: sid("hand") })
        );
        assert_eq!(way(None, Some(json!({}))), Err(NoWayIn::EventWithoutStart));
        // A step that is no start is refused where the run is made.
        assert_eq!(
            way(Some("a"), None),
            Ok(WayIn::Test {
                start: sid("a"),
                payload: json!({}),
            })
        );

        // A workflow that names no start begins by hand at its root.
        let plain = workflow(vec![agent("build", &[])]);
        assert_eq!(
            WayIn::of(Some(&plain), Some(sid("build")), None),
            Ok(WayIn::ByHand)
        );
        // A home with no workflow yet is judged by nothing here.
        assert_eq!(WayIn::of(None, None, None), Ok(WayIn::Start));
        assert_eq!(
            WayIn::of(None, Some(sid("ticket")), None),
            Ok(WayIn::Test {
                start: sid("ticket"),
                payload: json!({}),
            })
        );
    }

    #[test]
    fn an_amendment_may_add_or_change_a_start_the_run_did_not_begin_at() {
        let wf = workflow(vec![
            by_hand("hand", &["a"]),
            weekly("weekly", &["a"]),
            agent("a", &["b"]),
            agent("b", &[]),
        ]);
        let mut r = run_at(wf.clone(), RunEntry::by_hand());
        r.apply(RunEvent::Start, 100).unwrap();
        let mut next = r.workflow.clone();
        next.steps[1] = weekly("weekly", &["a", "b"]);
        next.steps
            .push(begins("hook", StartOn::Hook { public: false }, &["b"]));
        let e = r.apply(RunEvent::Amended { workflow: next }, 101).unwrap();
        assert!(e.is_empty(), "{e:?}");
        assert_eq!(
            state(&r, "hook"),
            &StepState::Skipped,
            "a start added later is skipped"
        );
        assert_eq!(
            r.apply(done("a"), 102).unwrap(),
            vec![RunEffect::StartAgent { step: sid("b") }]
        );
        assert_eq!(
            r.apply(done("b"), 103).unwrap(),
            vec![RunEffect::Finished {
                outcome: RunOutcome::Done
            }],
            "no stall on a start that never ran"
        );

        let mut r = run_at(wf, RunEntry::by_hand());
        r.apply(RunEvent::Start, 100).unwrap();
        let mut next = r.workflow.clone();
        next.steps[0] = begins("hand", StartOn::Hook { public: false }, &["a"]);
        assert!(matches!(
            r.apply(RunEvent::Amended { workflow: next }, 101),
            Err(RunError::AmendTouchesStartedStep { .. })
        ));
    }

    #[test]
    fn a_queued_run_amended_waits_for_its_start() {
        let mut r = run(workflow(vec![agent("a", &["b"]), agent("b", &[])]));
        let mut next = r.workflow.clone();
        next.steps.push(agent("c", &[]));
        next.steps[1].then.push(Flow::to(sid("c")));
        assert!(r
            .apply(RunEvent::Amended { workflow: next }, 101)
            .unwrap()
            .is_empty());
        assert!(
            r.is_queued(),
            "an amendment never settles a run that has not begun"
        );
        assert!(r.steps.values().all(|s| s.state == StepState::Pending));
    }

    #[test]
    fn a_parallel_takes_every_flow_and_its_join_merges() {
        let (mut r, e) = started(workflow(vec![
            step(
                "fan",
                StepKind::Parallel,
                vec![Flow::to(sid("a")), Flow::to(sid("b"))],
            ),
            agent("a", &["join"]),
            agent("b", &["join"]),
            agent("join", &[]),
        ]));
        assert_eq!(
            e,
            vec![
                RunEffect::StartAgent { step: sid("a") },
                RunEffect::StartAgent { step: sid("b") }
            ]
        );
        assert_eq!(state(&r, "fan"), &StepState::done());
        assert!(r.apply(done("a"), 101).unwrap().is_empty());
        assert_eq!(
            r.apply(done("b"), 102).unwrap(),
            vec![RunEffect::StartAgent { step: sid("join") }]
        );
    }

    #[test]
    fn decide_every_takes_each_rule_that_holds_else_otherwise() {
        let always = || Condition::Between {
            from_hour: 0,
            to_hour: 23,
        };
        let never = || Condition::Not {
            of: Box::new(always()),
        };
        let wf = |first: Condition, second: Condition| {
            let mut d = decide(
                "d",
                vec![("mail", first), ("chat", second)],
                "none",
                vec![
                    labelled("m", "mail"),
                    labelled("c", "chat"),
                    labelled("n", "none"),
                ],
            );
            if let StepKind::Decide { pick, .. } = &mut d.kind {
                *pick = Pick::Every;
            }
            workflow(vec![d, agent("m", &[]), agent("c", &[]), agent("n", &[])])
        };
        let (r, e) = started(wf(always(), always()));
        assert_eq!(
            e,
            vec![
                RunEffect::StartAgent { step: sid("m") },
                RunEffect::StartAgent { step: sid("c") }
            ]
        );
        assert_eq!(
            state(&r, "d"),
            &StepState::Done {
                branches: vec![Branch::new("mail").unwrap(), Branch::new("chat").unwrap()]
            }
        );
        assert_eq!(state(&r, "n"), &StepState::Skipped);
        let (_, e) = started(wf(never(), always()));
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("c") }]);
        let (r, e) = started(wf(never(), never()));
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("n") }]);
        assert_eq!(
            state(&r, "d"),
            &StepState::chose(Branch::new("none").unwrap())
        );
    }

    #[test]
    fn an_end_ends_its_path_the_run_or_fails_it() {
        let (mut r, _) = started(workflow(vec![
            agent("a", &["b", "e"]),
            agent("b", &[]),
            end("e", Finish::Path),
        ]));
        assert_eq!(
            r.apply(done("a"), 101).unwrap(),
            vec![RunEffect::StartAgent { step: sid("b") }]
        );
        assert_eq!(state(&r, "e"), &StepState::done());
        assert!(
            !r.is_finished(),
            "a path's end leaves the other paths going"
        );
        assert_eq!(
            r.apply(done("b"), 102).unwrap(),
            vec![RunEffect::Finished {
                outcome: RunOutcome::Done
            }]
        );

        let (mut r, _) = started(workflow(vec![
            agent("a", &["b", "e"]),
            agent("b", &[]),
            end("e", Finish::Done),
        ]));
        assert_eq!(
            r.apply(done("a"), 101).unwrap(),
            vec![
                RunEffect::StartAgent { step: sid("b") },
                RunEffect::CancelWork {
                    steps: vec![sid("b")]
                },
                RunEffect::Finished {
                    outcome: RunOutcome::Done
                }
            ]
        );

        let (mut r, _) = started(workflow(vec![agent("a", &["e"]), end("e", Finish::Failed)]));
        assert_eq!(
            r.apply(done("a"), 101).unwrap(),
            vec![RunEffect::Finished {
                outcome: RunOutcome::Failed
            }]
        );
        assert_eq!(state(&r, "e"), &StepState::Failed);
        assert_eq!(r.steps[&sid("e")].error.as_deref(), Some(ENDED_FAILED));
    }

    #[test]
    fn a_divert_stops_a_live_step_and_takes_only_its_flows() {
        let kinds: Vec<StepKind> = vec![
            agent("x", &[]).kind,
            StepKind::Human {
                prompt: "?".into(),
                options: vec![],
                multi: false,
                assignee: None,
            },
            StepKind::Approval { prompt: "?".into() },
            StepKind::Wait {
                until: WaitFor::Release,
            },
            StepKind::Spawn {
                statement_template: "c".into(),
                workflow: None,
                assignees: vec![],
                inputs: Default::default(),
                wait: true,
            },
        ];
        for kind in kinds {
            let word = kind.as_str();
            let mut s = step(
                "s",
                kind,
                vec![Flow::to(sid("next")), labelled("late", "late")],
            );
            s.boundaries = vec![timeout("late")];
            let (mut r, _) = started(workflow(vec![s, agent("next", &[]), agent("late", &[])]));
            assert!(r.steps[&sid("s")].state.is_live(), "{word}");
            let e = r.apply(fire(&r, "s", "late"), 101).unwrap();
            assert_eq!(
                e,
                vec![
                    RunEffect::CancelWork {
                        steps: vec![sid("s")]
                    },
                    RunEffect::StartAgent { step: sid("late") }
                ],
                "{word}"
            );
            assert_eq!(
                state(&r, "s"),
                &StepState::Diverted {
                    by: Branch::new("late").unwrap()
                },
                "{word}"
            );
            assert_eq!(state(&r, "next"), &StepState::Skipped, "{word}");
            assert_eq!(
                r.steps[&sid("s")].fired[&Branch::new("late").unwrap()].count,
                1
            );
        }
    }

    #[test]
    fn a_reminder_acts_beside_its_step_up_to_its_max() {
        let mut gate = step(
            "g",
            StepKind::Approval { prompt: "?".into() },
            vec![Flow::to(sid("next"))],
        );
        gate.boundaries = vec![reminder("nudge", 2)];
        let (mut r, _) = started(workflow(vec![gate, agent("next", &[])]));
        for n in 1..=2u32 {
            let e = r.apply(fire(&r, "g", "nudge"), 100 + u64::from(n)).unwrap();
            assert_eq!(
                e,
                vec![RunEffect::BoundaryAct {
                    step: sid("g"),
                    boundary: Branch::new("nudge").unwrap()
                }]
            );
            assert_eq!(state(&r, "g"), &StepState::Waiting);
            assert_eq!(
                r.steps[&sid("g")].fired[&Branch::new("nudge").unwrap()].count,
                n
            );
        }
        let before = r.clone();
        assert!(matches!(
            r.apply(fire(&r, "g", "nudge"), 110),
            Err(RunError::StaleBoundary { .. })
        ));
        assert_eq!(r, before, "a refusal leaves the run untouched");
    }

    #[test]
    fn a_boundary_of_another_visit_or_name_is_refused() {
        let mut gate = step(
            "g",
            StepKind::Approval { prompt: "?".into() },
            vec![Flow::to(sid("next")), labelled("next", "late")],
        );
        gate.boundaries = vec![timeout("late")];
        let (mut r, _) = started(workflow(vec![gate, agent("next", &[])]));
        let stale = RunEvent::BoundaryFired {
            step: sid("g"),
            boundary: Branch::new("late").unwrap(),
            entered: r.steps[&sid("g")].entered + 7,
            payload: json!({}),
            chain: Chain::default(),
        };
        assert!(matches!(
            r.apply(stale, 101),
            Err(RunError::StaleBoundary { .. })
        ));
        assert!(matches!(
            r.apply(fire(&r, "g", "nope"), 101),
            Err(RunError::UnknownBoundary { .. })
        ));
        let (mut r, _) = started(workflow(vec![check("c", &[])]));
        assert!(
            matches!(
                r.apply(fire(&r, "c", "late"), 101),
                Err(RunError::WrongKind { .. })
            ),
            "a check cannot be stopped mid-flight"
        );
    }

    #[test]
    fn a_race_is_a_wait_whose_first_event_wins() {
        let mut wait = step(
            "w",
            StepKind::Wait {
                until: WaitFor::Signal {
                    filter: SignalFilter {
                        name: "paid".into(),
                        fields: BTreeMap::new(),
                    },
                },
            },
            vec![
                Flow::to(sid("ship")),
                labelled("chase", "late"),
                labelled("refund", "cancelled"),
            ],
        );
        wait.boundaries = vec![
            timeout("late"),
            Boundary {
                name: Branch::new("cancelled").unwrap(),
                on: BoundaryOn::Message {
                    filter: MessageFilter {
                        contains: Some("cancel".into()),
                        ..MessageFilter::default()
                    },
                },
                act: BoundaryAct::Divert,
            },
        ];
        let wf = workflow(vec![
            wait,
            agent("ship", &[]),
            agent("chase", &[]),
            agent("refund", &[]),
        ]);
        let (mut r, _) = started(wf.clone());
        let e = r
            .apply(
                RunEvent::Heard {
                    step: sid("w"),
                    payload: json!({"amount": 5}),
                    chain: Chain::default(),
                },
                101,
            )
            .unwrap();
        assert_eq!(e, vec![RunEffect::StartAgent { step: sid("ship") }]);
        assert_eq!(state(&r, "chase"), &StepState::Skipped);
        assert_eq!(state(&r, "refund"), &StepState::Skipped);

        let (mut r, _) = started(wf);
        let e = r.apply(fire(&r, "w", "cancelled"), 101).unwrap();
        assert_eq!(
            e,
            vec![
                RunEffect::CancelWork {
                    steps: vec![sid("w")]
                },
                RunEffect::StartAgent {
                    step: sid("refund")
                }
            ]
        );
        assert_eq!(state(&r, "ship"), &StepState::Skipped);
        assert!(
            r.apply(
                RunEvent::Heard {
                    step: sid("w"),
                    payload: json!({}),
                    chain: Chain::default(),
                },
                102
            )
            .is_err(),
            "the losing event arrives to a step already over"
        );
    }

    #[test]
    fn a_release_carries_its_payload_and_a_heard_event_widens_the_chain() {
        let w = |until| step("w", StepKind::Wait { until }, vec![Flow::to(sid("b"))]);
        let (mut r, _) = started(workflow(vec![w(WaitFor::Release), agent("b", &[])]));
        r.apply(
            RunEvent::Released {
                step: sid("w"),
                payload: Some(json!({"ok": 1})),
            },
            101,
        )
        .unwrap();
        assert_eq!(r.steps[&sid("w")].output, Some(json!({"ok": 1})));

        let (mut r, _) = started(workflow(vec![
            w(WaitFor::Signal {
                filter: SignalFilter {
                    name: "x".into(),
                    fields: BTreeMap::new(),
                },
            }),
            agent("b", &[]),
        ]));
        let key = ListenerKey {
            host: ListenerHost::Workspace {
                workflow: r.workflow.id,
            },
            step: sid("s"),
        };
        let chain = Chain::default().extend(&key);
        r.apply(
            RunEvent::Heard {
                step: sid("w"),
                payload: json!({}),
                chain,
            },
            101,
        )
        .unwrap();
        assert_eq!(r.chain.listeners, vec![key]);
        assert_eq!(r.chain.depth, 1);
    }

    #[test]
    fn an_emit_raises_its_signal_and_raises_it_again_after_a_restart() {
        let (mut r, e) = started(workflow(vec![
            step(
                "e",
                StepKind::Emit {
                    signal: "report.ready".into(),
                    payload: BTreeMap::new(),
                },
                vec![Flow::to(sid("b"))],
            ),
            agent("b", &[]),
        ]));
        assert_eq!(e, vec![RunEffect::Emit { step: sid("e") }]);
        assert_eq!(
            r.apply(RunEvent::StepInterrupted { step: sid("e") }, 101)
                .unwrap(),
            vec![RunEffect::Emit { step: sid("e") }]
        );
        assert_eq!(
            r.apply(
                RunEvent::StepDone {
                    step: sid("e"),
                    output: json!({"signal": "01S"}),
                },
                102
            )
            .unwrap(),
            vec![RunEffect::StartAgent { step: sid("b") }]
        );
    }

    #[test]
    fn a_loop_entered_anew_after_a_divert_left_its_body_starts_its_list_again() {
        let mut work = agent("work", &["each"]);
        work.then.push(labelled("prep", "late"));
        work.boundaries = vec![timeout("late")];
        let wf = workflow(vec![
            agent("prep", &["each"]),
            step(
                "each",
                StepKind::ForEach {
                    items: "[1, 2, 3]".into(),
                    max_iterations: 5,
                },
                vec![labelled("work", "each"), labelled("fin", "done")],
            ),
            work,
            agent("fin", &[]),
        ]);
        let (mut r, _) = started(wf);
        r.apply(done("prep"), 101).unwrap();
        let first = Some(json!({"item": 1, "index": 0, "count": 3}));
        assert_eq!(r.steps[&sid("each")].output, first);
        r.apply(fire(&r, "work", "late"), 102).unwrap();
        assert_eq!(state(&r, "prep"), &StepState::Running);
        r.apply(done("prep"), 103).unwrap();
        assert_eq!(
            r.steps[&sid("each")].output,
            first,
            "entered along a forward edge, the loop begins its list again"
        );
        assert_eq!(state(&r, "work"), &StepState::Running);
    }

    #[test]
    fn a_run_round_trips_with_its_entry_event_dispatch_and_chain() {
        let wf = workflow(vec![weekly("weekly", &["a"]), agent("a", &[])]);
        let key = ListenerKey {
            host: ListenerHost::Workspace { workflow: wf.id },
            step: sid("weekly"),
        };
        let mut event = signal(json!({"x": 1}));
        event.chain = Chain::default().extend(&key);
        let mut r = run_at(wf, RunEntry::at(sid("weekly"), Some(event.clone())));
        r.dispatched = Some(event.id.clone());
        assert_eq!(r.chain, event.chain, "a run's chain is its event's");
        r.apply(RunEvent::Start, 100).unwrap();
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(serde_json::from_str::<WorkflowRun>(&json).unwrap(), r);
        let mut old = serde_json::to_value(&r).unwrap();
        old["signal"] = json!({});
        assert!(
            serde_json::from_value::<WorkflowRun>(old).is_err(),
            "a run of the trigger era is refused, never half-read"
        );
    }

    mod props {
        use super::*;
        use proptest::prelude::*;

        fn kind_strategy() -> impl Strategy<Value = StepKind> {
            prop_oneof![
                Just(StepKind::Agent {
                    instructions: "x".into(),
                    assignee: None,
                    project: None,
                    harness: vec![],
                    model: None,
                    effort: None,
                    output_schema: None,
                    tier_ceiling: crate::caps::ToolTier::Write,
                }),
                Just(StepKind::Human {
                    prompt: "?".into(),
                    options: vec![AskOption::new("a", "A")],
                    multi: false,
                    assignee: None,
                }),
                Just(StepKind::Approval { prompt: "?".into() }),
                Just(StepKind::Check {
                    check: CheckKind::Command {
                        command: "true".into()
                    }
                }),
                // A gate and a loop with unlabelled successors: their flows
                // die and the chain drains, and totality still holds.
                Just(StepKind::If { when: always() }),
                Just(StepKind::While {
                    when: always(),
                    max_iterations: 1
                }),
                Just(StepKind::Wait {
                    until: WaitFor::Release
                }),
                Just(StepKind::Wait {
                    until: WaitFor::Delay {
                        secs: ValueRef::Fixed(1)
                    }
                }),
                Just(StepKind::Wait {
                    until: WaitFor::Signal {
                        filter: crate::listen::SignalFilter {
                            name: "t".into(),
                            fields: BTreeMap::new()
                        }
                    }
                }),
                Just(StepKind::Wait {
                    until: WaitFor::Message {
                        filter: crate::listen::MessageFilter::default()
                    }
                }),
                Just(StepKind::Notify {
                    scope: None,
                    template: "hi".into(),
                    mentions: vec![],
                    author: None
                }),
                Just(StepKind::Emit {
                    signal: "report.ready".into(),
                    payload: BTreeMap::new()
                }),
                Just(StepKind::Spawn {
                    statement_template: "c".into(),
                    workflow: None,
                    assignees: vec![],
                    inputs: Default::default(),
                    wait: true
                }),
                // Gateways whose flows are unlabelled: a `parallel` takes them
                // all, an inclusive `decide` chooses `otherwise` and they die.
                Just(StepKind::Parallel),
                Just(StepKind::Decide {
                    rules: vec![],
                    otherwise: Branch::new("o").unwrap(),
                    pick: Pick::Every
                }),
                // A start, which the chain only keeps at its head.
                Just(StepKind::Start {
                    on: crate::start::StartOn::Manual,
                    inputs: BTreeMap::new(),
                    guard: crate::start::Guard::default()
                }),
                Just(StepKind::End {
                    finish: Finish::Done
                }),
                Just(StepKind::End {
                    finish: Finish::Path
                }),
                Just(StepKind::End {
                    finish: Finish::Failed
                }),
            ]
        }

        /// A chain of 1..=5 steps of random kinds, plus a random extra flow
        /// forward so some joins have two edges, and — one time in three — a
        /// flow back from the tail to an earlier step, so some graphs loop.
        /// An `end` only ever sits at the tail and a `start` at the head, so
        /// the chain has one start and is always valid enough to run. A step
        /// that may carry boundary events sometimes carries a divert `late`
        /// with its own flow on.
        fn workflow_strategy() -> impl Strategy<Value = Workflow> {
            (
                proptest::collection::vec(kind_strategy(), 1..=5),
                any::<u8>(),
                any::<u8>(),
            )
                .prop_map(|(kinds, extra, back)| {
                    let n = kinds.len();
                    let loop_to = (n >= 2 && back % 3 == 0).then(|| (back as usize) % (n - 1));
                    let steps: Vec<Step> = kinds
                        .into_iter()
                        .enumerate()
                        .map(|(i, kind)| {
                            let id = sid(&format!("s{i}"));
                            let is_tail = i + 1 == n;
                            let kind = match kind {
                                StepKind::End { .. } if !is_tail => {
                                    StepKind::Approval { prompt: "?".into() }
                                }
                                StepKind::Start { .. } if i != 0 => {
                                    StepKind::Approval { prompt: "?".into() }
                                }
                                other => other,
                            };
                            let is_end = matches!(kind, StepKind::End { .. });
                            let mut then = vec![];
                            if !is_tail {
                                then.push(Flow::to(sid(&format!("s{}", i + 1))));
                                let skip = i + 2;
                                if skip < n && (extra as usize) % n == i {
                                    then.push(Flow::to(sid(&format!("s{skip}"))));
                                }
                            } else if let (Some(to), false) = (loop_to, is_end) {
                                then.push(Flow::to(sid(&format!("s{to}"))));
                            }
                            let carries = crate::boundary::may_carry_boundaries(&kind)
                                && (extra as usize + i) % 3 == 1;
                            let mut s = step(id.as_str(), kind, then);
                            s.on_fail = if extra % 2 == 0 {
                                OnFail::Skip
                            } else {
                                OnFail::Fail
                            };
                            if carries {
                                let late = Branch::new("late").unwrap();
                                s.boundaries = vec![crate::boundary::Boundary {
                                    name: late.clone(),
                                    on: crate::boundary::BoundaryOn::After {
                                        secs: ValueRef::Fixed(60),
                                    },
                                    act: crate::boundary::BoundaryAct::Divert,
                                }];
                                if !is_tail {
                                    s.then.push(Flow::branch(sid(&format!("s{}", i + 1)), late));
                                }
                            }
                            s
                        })
                        .collect();
                    workflow(steps)
                })
        }

        fn event_strategy(n: usize) -> impl Strategy<Value = RunEvent> {
            (0..n).prop_flat_map(|i| {
                let id = sid(&format!("s{i}"));
                prop_oneof![
                    Just(RunEvent::Start),
                    Just(RunEvent::StepStarted {
                        step: id.clone(),
                        work_item: None
                    }),
                    Just(RunEvent::StepDone {
                        step: id.clone(),
                        output: json!({})
                    }),
                    Just(RunEvent::StepFailed {
                        step: id.clone(),
                        error: "e".into()
                    }),
                    Just(RunEvent::Answered {
                        step: id.clone(),
                        answer: Answer::text("t")
                    }),
                    Just(RunEvent::Decided {
                        step: id.clone(),
                        approve: true,
                        approval: ApprovalId("a".into())
                    }),
                    Just(RunEvent::Decided {
                        step: id.clone(),
                        approve: false,
                        approval: ApprovalId("a".into())
                    }),
                    Just(RunEvent::Heard {
                        step: id.clone(),
                        payload: json!({}),
                        chain: Chain::default(),
                    }),
                    Just(RunEvent::Elapsed { step: id.clone() }),
                    Just(RunEvent::Released {
                        step: id.clone(),
                        payload: None,
                    }),
                    (0u64..40).prop_map(move |entered| RunEvent::BoundaryFired {
                        step: id.clone(),
                        boundary: Branch::new("late").unwrap(),
                        entered,
                        payload: json!({}),
                        chain: Chain::default(),
                    }),
                    Just(RunEvent::Cancel {
                        cause: CancelCause::Stopped { rationale: None }
                    }),
                ]
            })
        }

        /// A workflow and a batch of events naming its steps.
        fn scenario() -> impl Strategy<Value = (Workflow, Vec<RunEvent>)> {
            workflow_strategy().prop_flat_map(|wf| {
                let n = wf.steps.len();
                (
                    Just(wf),
                    proptest::collection::vec(event_strategy(n), 1..=12),
                )
            })
        }

        proptest! {
            #[test]
            fn apply_is_total_and_never_panics((wf, events) in scenario()) {
                let mut r = run(wf);
                r.apply(RunEvent::Start, 100).unwrap();
                let mut t = 100u64;
                for ev in events {
                    t += 1;
                    let before = r.clone();
                    match r.apply(ev, t) {
                        Ok(_) => {}
                        Err(_) => prop_assert_eq!(&r, &before, "an error leaves the run untouched"),
                    }
                }
            }

            /// Nothing is left going with nothing moving it: after every
            /// event the machine took, the run is finished or a step of it
            /// is live.
            #[test]
            fn a_run_that_is_going_has_a_live_step((wf, events) in scenario()) {
                let mut r = run(wf);
                r.apply(RunEvent::Start, 100).unwrap();
                prop_assert!(r.is_finished() || !r.live_steps().is_empty(), "after the start: {:?}", r.steps);
                let mut t = 100u64;
                for ev in events {
                    t += 1;
                    let name = ev.name();
                    if r.apply(ev, t).is_ok() {
                        prop_assert!(
                            r.is_finished() || !r.live_steps().is_empty(),
                            "after {}: {:?}", name, r.steps
                        );
                    }
                }
            }

            #[test]
            fn the_kind_table_and_apply_agree((wf, events) in scenario()) {
                let mut r = run(wf.clone());
                r.apply(RunEvent::Start, 100).unwrap();
                for ev in events {
                    let Some(step) = ev.step().cloned() else { continue };
                    let Some(def) = wf.step(&step) else { continue };
                    let live = r.steps.get(&step).is_some_and(|s| s.state.is_live());
                    if !live {
                        continue;
                    }
                    let mut probe = r.clone();
                    let res = probe.apply(ev.clone(), 101);
                    let wrong_kind = matches!(res, Err(RunError::WrongKind { .. }));
                    prop_assert_eq!(wrong_kind, !WorkflowRun::accepts(&def.kind, &ev), "{:?}", ev);
                }
            }

            #[test]
            fn finished_is_absorbing_and_leaves_no_live_step(wf in workflow_strategy()) {
                let mut r = run(wf);
                r.apply(RunEvent::Start, 100).unwrap();
                let cancel = RunEvent::Cancel { cause: CancelCause::Restarted };
                if r.is_finished() {
                    // A workflow that drains at once (a lone `decide`, an `end`)
                    // is finished by its start: nothing is live, and even a
                    // cancel is refused as `Finished`.
                    prop_assert!(r.live_steps().is_empty());
                    prop_assert_ne!(r.status(), RunStatus::Running);
                    prop_assert_eq!(r.apply(cancel, 101), Err(RunError::Finished));
                } else {
                    let effects = r.apply(cancel, 101).unwrap();
                    prop_assert!(r.live_steps().is_empty());
                    prop_assert_eq!(r.status(), RunStatus::Cancelled);
                    prop_assert_eq!(
                        effects.last(),
                        Some(&RunEffect::Cancelled { cause: CancelCause::Restarted }),
                        "a cancel settles through one effect"
                    );
                }
                prop_assert_eq!(r.apply(RunEvent::Start, 102), Err(RunError::Finished));
                prop_assert!(r.steps.values().all(|s| !s.state.is_live()));
                prop_assert!(!r.is_live() && !r.is_queued());
            }

            #[test]
            fn a_queued_run_refuses_every_step_event_and_only_start_or_cancel_moves_it((wf, events) in scenario()) {
                let mut r = run(wf);
                let before = r.clone();
                for ev in events {
                    if ev.step().is_none() {
                        continue;
                    }
                    prop_assert_eq!(r.apply(ev, 101), Err(RunError::NotStarted));
                    prop_assert_eq!(&r, &before, "a queued run is byte-identical after a refusal");
                }
                prop_assert_eq!(r.status(), RunStatus::Queued);
                let mut withdrawn = r.clone();
                let effects = withdrawn.apply(RunEvent::Cancel { cause: CancelCause::Withdrawn }, 101).unwrap();
                prop_assert_eq!(effects, vec![RunEffect::Cancelled { cause: CancelCause::Withdrawn }]);
                prop_assert_eq!(withdrawn.status(), RunStatus::Cancelled);
                prop_assert_eq!(withdrawn.started_at, None);
                r.apply(RunEvent::Start, 102).unwrap();
                prop_assert_eq!(r.started_at, Some(102));
                prop_assert_ne!(r.status(), RunStatus::Queued);
            }

            #[test]
            fn status_is_queued_exactly_before_start((wf, events) in scenario()) {
                let mut r = run(wf);
                prop_assert_eq!(r.status(), RunStatus::Queued);
                prop_assert!(r.is_queued());
                r.apply(RunEvent::Start, 100).unwrap();
                let mut t = 100u64;
                for ev in events {
                    t += 1;
                    let _refused = r.apply(ev, t).is_err();
                    prop_assert_ne!(r.status(), RunStatus::Queued, "never queued again once started");
                    prop_assert!(!r.is_queued());
                    prop_assert_eq!(r.is_live(), r.status().is_live());
                }
            }

            #[test]
            fn a_run_never_finishes_done_with_a_pending_step((wf, events) in scenario()) {
                let mut r = run(wf);
                r.apply(RunEvent::Start, 100).unwrap();
                let mut t = 100u64;
                for ev in events {
                    t += 1;
                    // An event the step does not accept is refused and changes
                    // nothing; the property is about what was applied.
                    let _refused = r.apply(ev, t).is_err();
                }
                if r.outcome == Some(RunOutcome::Done) {
                    prop_assert!(r.steps.values().all(|s| s.state != StepState::Pending), "{:?}", r.steps);
                }
                if r.outcome == Some(RunOutcome::Failed) && r.cancelled.is_none() {
                    prop_assert!(r.steps.values().any(|s| s.state == StepState::Failed), "{:?}", r.steps);
                }
            }

            #[test]
            fn forward_edges_are_acyclic_and_there_is_one_start(wf in workflow_strategy()) {
                prop_assert_eq!(wf.start_steps().len(), 1);
                let loops = wf.loop_edges();
                // Kahn's walk over the forward edges must consume every step.
                let mut indegree: BTreeMap<&StepId, usize> = wf.steps.iter().map(|s| (&s.id, 0)).collect();
                let mut forward: Vec<(&StepId, &StepId)> = Vec::new();
                for s in &wf.steps {
                    for to in Workflow::successors(s) {
                        if loops.contains(&(s.id.clone(), to.clone())) {
                            continue;
                        }
                        forward.push((&s.id, to));
                        *indegree.entry(to).or_insert(0) += 1;
                    }
                }
                let mut ready: Vec<&StepId> = indegree.iter().filter(|(_, d)| **d == 0).map(|(id, _)| *id).collect();
                let mut seen = 0usize;
                while let Some(id) = ready.pop() {
                    seen += 1;
                    for (from, to) in &forward {
                        if *from == id {
                            let d = indegree.get_mut(to).map(|d| { *d -= 1; *d });
                            if d == Some(0) {
                                ready.push(to);
                            }
                        }
                    }
                }
                prop_assert_eq!(seen, wf.steps.len(), "the forward edges form a cycle");
            }

            #[test]
            fn status_never_says_running_when_no_step_is_running(wf in workflow_strategy()) {
                let mut r = run(wf);
                r.apply(RunEvent::Start, 100).unwrap();
                let running = r.steps.values().any(|s| s.state == StepState::Running);
                prop_assert_eq!(r.status() == RunStatus::Running, running);
                if r.is_finished() {
                    prop_assert!(r.status().is_finished());
                }
            }
        }
    }
}
