//! The Goal and its satellite types.
//!
//! A goal is a stated want plus a workflow. It has no lifecycle of its own:
//! how it moves is the shape of the workflow it runs, and where it stands is a
//! projection of its current run and its listening ([`Goal::status`]) —
//! computed, never written. The one thing a goal does on its own is close.
//!
//! A goal whose workflow begins on events **listens** while it is open
//! ([`Goal::listening`]): each occurrence starts a run on the goal, queued
//! behind a live one. Between runs it reads *waiting* on the world; a failed
//! run pauses it, and it reads *failed* on you until a repair is adopted or a
//! person says listen again.

use crate::assignee::Assignee;
use crate::id::{GoalId, PrincipalId, RunId, WorkflowId};
use crate::listen::Listening;
use crate::run::{RunStatus, WorkflowRun};
use crate::tags::Tags;
use crate::workflow::StepId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// How many of a goal's runs its snapshot names: the queued ones and the
/// newest this many. A standing goal runs on every event for as long as it
/// is open; the index keeps the whole history.
pub const GOAL_RUNS_KEPT: usize = 50;

/// A signed, durable record of a wanted outcome. The current snapshot of the
/// addressable `kind:33400` event; history lives in the journal, and
/// `revision` is the snapshot authority (monotonic, never reused).
///
/// `deny_unknown_fields`: a goal written by the lifecycle-shaped code carried
/// `state`, `criteria`, `contract`, `blocked` and `parent`, and this build
/// refuses it rather than half-reading it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Goal {
    pub id: GoalId,
    /// The outcome, in the author's words. Sharpened before a run starts but
    /// never rewritten silently — each change carries a `kind:3400` note.
    pub statement: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub author: PrincipalId,
    /// The workflow the next run will use — or the one the current run is a
    /// copy of. `None` until one is picked, proposed or adopted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow: Option<WorkflowId>,
    /// The current run: the live one, else the latest that started. A goal
    /// may run several times over its life, one live at a time; a run made
    /// while one is live is queued and never the current run until it starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<RunId>,
    /// The queued runs and the newest [`GOAL_RUNS_KEPT`], oldest first — the
    /// queued ones at the tail. The index holds every run the goal has had.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<RunId>,
    /// The goal's standing while its workflow begins on events: the inputs
    /// its event runs bind, since when, and — after a failed run or a spent
    /// budget — why it paused. `None` for a goal whose work begins by hand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listening: Option<Listening>,
    /// Set once, by closing. The first of the two moves a goal makes on its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed: Option<Closure>,
    /// Put away: hidden from every list that does not ask, and only ever
    /// after closing — the second move, and the one that goes back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived: Option<crate::archive::Archived>,
    /// How this goal came to exist. Recorded at capture, never edited.
    pub origin: GoalOrigin,
    #[serde(default)]
    pub budget: Budget,
    /// How the goal moves: who designs its workflow, and who adopts, starts
    /// and repairs it. See [`GoalMode`].
    #[serde(default)]
    pub mode: GoalMode,
    /// Who carries this goal: agents take the work, humans may decide the
    /// gates, teams expand to both. Descendants inherit through `origin`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assignees: Vec<Assignee>,
    #[serde(default, skip_serializing_if = "Tags::is_empty")]
    pub tags: Tags,
    /// Monotonic snapshot revision. Only a snapshot with a higher revision may
    /// replace state.
    pub revision: u64,
    /// Unix seconds of creation (assigned at the store edge).
    pub created_at: u64,
}

/// How a goal moves — chosen at capture, from `goals.default_mode`.
///
/// - `Auto`: the Workflow Agent designs the workflow, and the platform adopts
///   it, starts the run, repairs a failed run and restarts by itself; the
///   run is unattended, so a tool permission above the step's tier ceiling
///   that no guard rule decides is read by the classifier
///   (`goals.auto.permissions`) rather than put to a person, and every
///   step's agent is told to decide rather than ask. Only what a person
///   alone can do waits for one: a `human`, `approval` or `wait { release }`
///   step the workflow declares, the guard's `ask` and `deny`, a call the
///   classifier finds harmful or gives no verdict on, a push or a pull
///   request under a gated project, a question an agent still chooses to
///   ask, a design whose required inputs have no default, and a goal
///   repaired more times than `goals.auto.repair_limit` allows.
/// - `Guided`: the Workflow Agent proposes; the person adopts, asks for
///   changes or declines, and approves every amendment.
/// - `Manual`: the person designs on the goal's Workflow tab, with the
///   Workflow Agent on request — a proposal it makes is the goal's own
///   draft, never a gate.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum GoalMode {
    #[default]
    Auto,
    Guided,
    Manual,
}

impl GoalMode {
    pub const ALL: [GoalMode; 3] = [GoalMode::Auto, GoalMode::Guided, GoalMode::Manual];

    pub fn as_str(self) -> &'static str {
        match self {
            GoalMode::Auto => "auto",
            GoalMode::Guided => "guided",
            GoalMode::Manual => "manual",
        }
    }

    /// The Workflow Agent designs the workflow at capture and repairs it
    /// after a failed run. A manual goal wakes nobody: its person designs.
    pub fn designs(self) -> bool {
        matches!(self, GoalMode::Auto | GoalMode::Guided)
    }

    /// The platform adopts a proposal, starts the run and applies an
    /// amendment without a person — `Auto` alone.
    pub fn adopts_alone(self) -> bool {
        matches!(self, GoalMode::Auto)
    }

    /// The run is nobody's to watch: a permission above a step's ceiling is
    /// the classifier's to read, and the step's agent is told to decide
    /// rather than ask — `Auto` alone.
    pub fn unattended(self) -> bool {
        matches!(self, GoalMode::Auto)
    }
}

impl std::str::FromStr for GoalMode {
    type Err = crate::CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        GoalMode::ALL
            .into_iter()
            .find(|m| m.as_str() == s)
            .ok_or_else(|| crate::CoreError::UnknownGoalMode(s.to_string()))
    }
}

impl std::fmt::Display for GoalMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Goal {
    pub fn is_closed(&self) -> bool {
        self.closed.is_some()
    }

    pub fn is_archived(&self) -> bool {
        self.archived.is_some()
    }

    /// Where the goal stands, read off its current run and its listening.
    /// Never stored as truth: the run, `listening` and `closed` are, and this
    /// is a fold over them. Closed wins; a live run next; then a paused
    /// listening reads failed and a listening one waiting on its next event.
    pub fn status(&self, run: Option<&WorkflowRun>) -> GoalStatus {
        if self.closed.is_some() {
            return GoalStatus::Closed;
        }
        let live = run.is_some_and(|r| !r.is_finished());
        if let (Some(listening), false) = (&self.listening, live) {
            return if listening.is_paused() {
                GoalStatus::Failed
            } else {
                GoalStatus::Waiting
            };
        }
        match run.map(WorkflowRun::status) {
            // No run yet, or the last one was cancelled and the goal is still
            // open: ready for the next run.
            None | Some(RunStatus::Cancelled) => GoalStatus::Draft,
            Some(RunStatus::Running) => GoalStatus::Running,
            // A queued current run is only ever seen across a crash: the
            // store starts a run at once when nothing is live, and boot
            // advances the queue. The goal is waiting on the platform.
            Some(RunStatus::Waiting) | Some(RunStatus::Queued) => GoalStatus::Waiting,
            Some(RunStatus::Done) => GoalStatus::Done,
            Some(RunStatus::Failed) => GoalStatus::Failed,
        }
    }

    /// Who this goal waits on. `owed` is whether anything durable names the
    /// goal — a pending gate, a question — which the caller reads from the
    /// inbox: a goal somebody owes an answer is yours whatever the run does.
    /// The goal's own rungs are closed and no run yet; the rest are its
    /// run's ([`WorkflowRun::holder`]), so a run of the workspace reads the
    /// same word.
    pub fn holder(&self, run: Option<&WorkflowRun>, owed: bool) -> Holder {
        if self.closed.is_some() {
            return Holder::Finished;
        }
        let live = run.is_some_and(|r| !r.is_finished());
        if let (Some(listening), false) = (&self.listening, live) {
            // Between runs a listening goal waits on the world — unless a
            // failure paused it, or somebody owes it an answer.
            return if owed || listening.is_paused() {
                Holder::You
            } else {
                Holder::World
            };
        }
        match run {
            Some(run) => run.holder(owed),
            None if owed => Holder::You,
            None => match self.workflow {
                Some(_) => Holder::You,
                None => Holder::Design,
            },
        }
    }

    /// Whether the goal hears its workflow's start events right now.
    pub fn is_listening(&self) -> bool {
        self.listening.as_ref().is_some_and(|l| !l.is_paused()) && !self.is_closed()
    }

    /// Forget the oldest finished runs past [`GOAL_RUNS_KEPT`]: never the
    /// current run, never a queued one (`queued`).
    pub fn trim_runs(&mut self, queued: &BTreeSet<RunId>) {
        let mut excess = self.runs.len().saturating_sub(GOAL_RUNS_KEPT);
        if excess == 0 {
            return;
        }
        let current = self.run;
        self.runs.retain(|r| {
            if excess > 0 && Some(*r) != current && !queued.contains(r) {
                excess -= 1;
                false
            } else {
                true
            }
        });
    }
}

/// Who a goal waits on right now — the one word the Goals screen, the CLI
/// and the inbox all print. A projection like [`GoalStatus`], never stored:
/// the caller gathers the run and whether anything durable names the goal
/// (`owed` — a pending gate, a question), and this decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Holder {
    /// A question, an approval, a release, an adoption — or a workflow chosen
    /// and not started. Your move.
    You,
    /// An agent or a check is working.
    Agents,
    /// A signal, a schedule, a delay, or a child goal: the outside world.
    World,
    /// The run finished, or the goal is closed.
    Finished,
    /// No workflow yet: the goal is still being designed.
    Design,
}

impl Holder {
    pub const ALL: [Holder; 5] = [
        Holder::You,
        Holder::Agents,
        Holder::World,
        Holder::Finished,
        Holder::Design,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Holder::You => "you",
            Holder::Agents => "agents",
            Holder::World => "world",
            Holder::Finished => "finished",
            Holder::Design => "design",
        }
    }
}

impl std::str::FromStr for Holder {
    type Err = crate::CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Holder::ALL
            .into_iter()
            .find(|h| h.as_str() == s)
            .ok_or_else(|| crate::CoreError::UnknownHolder(s.to_string()))
    }
}

/// Why and when a goal was closed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Closure {
    #[serde(flatten)]
    pub reason: ClosureReason,
    pub at: u64,
}

/// Why a goal was closed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "reason")]
pub enum ClosureReason {
    /// Given up on. No further work will be done.
    Abandoned {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
    },
    /// Replaced by another goal, which is named.
    Superseded { by: GoalId },
}

impl ClosureReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            ClosureReason::Abandoned { .. } => "abandoned",
            ClosureReason::Superseded { .. } => "superseded",
        }
    }
}

/// How a goal came to exist.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "origin")]
pub enum GoalOrigin {
    /// A person captured it.
    Captured,
    /// A `spawn` step of a goal's run, or an agent's `spawn_sub_goal`,
    /// refining a parent.
    Spawned { parent: GoalId },
    /// A `spawn` step of a run of the workspace: a goal of its own, refining
    /// nothing, that the step may wait on. History like every origin — the
    /// run may since be gone with its workflow.
    Run { run: RunId, step: StepId },
}

impl GoalOrigin {
    pub const NAMES: [&'static str; 3] = ["captured", "spawned", "run"];

    pub fn as_str(&self) -> &'static str {
        match self {
            GoalOrigin::Captured => "captured",
            GoalOrigin::Spawned { .. } => "spawned",
            GoalOrigin::Run { .. } => "run",
        }
    }

    /// The parent in the `refines` hierarchy, when there is one.
    pub fn parent(&self) -> Option<GoalId> {
        match self {
            GoalOrigin::Spawned { parent } => Some(*parent),
            _ => None,
        }
    }
}

/// Where a goal stands. A projection of its run and its closure — six words
/// for rows and screens, none of them a state anything writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    /// No run, or the last run was cancelled: a workflow can start.
    Draft,
    Running,
    /// Every live step is waiting — on a person, a signal or the clock — or
    /// the goal is listening for the event that starts its next run.
    Waiting,
    Done,
    Failed,
    Closed,
}

impl GoalStatus {
    pub const ALL: [GoalStatus; 6] = [
        GoalStatus::Draft,
        GoalStatus::Running,
        GoalStatus::Waiting,
        GoalStatus::Done,
        GoalStatus::Failed,
        GoalStatus::Closed,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            GoalStatus::Draft => "draft",
            GoalStatus::Running => "running",
            GoalStatus::Waiting => "waiting",
            GoalStatus::Done => "done",
            GoalStatus::Failed => "failed",
            GoalStatus::Closed => "closed",
        }
    }

    /// Still somebody's problem: a run may start or is under way.
    pub fn is_live(self) -> bool {
        matches!(
            self,
            GoalStatus::Draft | GoalStatus::Running | GoalStatus::Waiting
        )
    }
}

impl std::str::FromStr for GoalStatus {
    type Err = crate::CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        GoalStatus::ALL
            .into_iter()
            .find(|g| g.as_str() == s)
            .ok_or_else(|| crate::CoreError::UnknownGoalStatus(s.to_string()))
    }
}

impl std::fmt::Display for GoalStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Hard ceilings. `None` = unlimited (still tracked in the ledger).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_usd_cents: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_wall_clock_secs: Option<u64>,
}

impl Budget {
    /// The settings a goal, or a run of the workspace, made without a budget
    /// of its own reads its ceilings from — `budget.default.*`, zero meaning
    /// no ceiling.
    pub const SETTING_TOKENS: &'static str = "budget.default.max_tokens";
    pub const SETTING_USD_CENTS: &'static str = "budget.default.max_usd_cents";
    pub const SETTING_WALL_CLOCK_SECS: &'static str = "budget.default.max_wall_clock_secs";

    pub fn allows(&self, spent: &BudgetSpent) -> bool {
        fn under(limit: Option<u64>, used: u64) -> bool {
            limit.map(|l| used < l).unwrap_or(true)
        }
        under(self.max_tokens, spent.tokens)
            && under(self.max_usd_cents, spent.usd_cents)
            && under(self.max_wall_clock_secs, spent.wall_clock_secs)
    }

    pub fn is_unlimited(&self) -> bool {
        self.max_tokens.is_none()
            && self.max_usd_cents.is_none()
            && self.max_wall_clock_secs.is_none()
    }
}

/// Accumulated spend against a budget.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BudgetSpent {
    pub tokens: u64,
    pub usd_cents: u64,
    pub wall_clock_secs: u64,
}

/// Typed edge in the goal graph. One kind: `refines`. The three that had no
/// writer (`depends_on`, `informs`, `supersedes`) are gone; `Superseded { by }`
/// on the closure reason carries the last one's relation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GoalEdge {
    pub from: GoalId,
    pub to: GoalId,
    pub kind: GoalEdgeKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GoalEdgeKind {
    /// `from` is a sub-goal decomposing `to`.
    Refines,
}

impl GoalEdgeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            GoalEdgeKind::Refines => "refines",
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::run::{CancelCause, RunEntry, RunEvent, RunScope, WorkflowRun};
    use crate::workflow::tests::{agent, workflow};
    use std::collections::BTreeMap;

    pub(crate) fn goal() -> Goal {
        Goal {
            id: GoalId::from_ulid(ulid::Ulid::from_parts(1, 1)),
            statement: "Build a CLI that syncs bookmarks".into(),
            title: Some("Bookmark sync".into()),
            author: PrincipalId::new("ab".repeat(32)).unwrap(),
            workflow: None,
            run: None,
            runs: vec![],
            closed: None,
            listening: None,
            origin: GoalOrigin::Captured,
            budget: Budget::default(),
            mode: GoalMode::Guided,
            assignees: vec![Assignee::Team("01TEAM".into())],
            tags: Tags::default(),
            revision: 1,
            archived: None,
            created_at: 0,
        }
    }

    #[test]
    fn budget_allows_boundaries() {
        let b = Budget {
            max_tokens: Some(100),
            max_usd_cents: None,
            max_wall_clock_secs: None,
        };
        assert!(b.allows(&BudgetSpent {
            tokens: 99,
            ..Default::default()
        }));
        assert!(!b.allows(&BudgetSpent {
            tokens: 100,
            ..Default::default()
        }));
        assert!(Budget::default().is_unlimited());
        assert!(!b.is_unlimited());
    }

    #[test]
    fn goal_json_roundtrip_and_status_projection() {
        let g = goal();
        let json = serde_json::to_value(&g).unwrap();
        assert_eq!(json["origin"]["origin"], "captured");
        assert!(json.get("state").is_none(), "a goal has no lifecycle state");
        assert_eq!(serde_json::from_value::<Goal>(json).unwrap(), g);
        assert_eq!(g.status(None), GoalStatus::Draft);

        let wf = workflow(vec![agent("a", &[])]);
        let mut run = WorkflowRun::new(
            RunId::from_ulid(ulid::Ulid::from_parts(4, 1)),
            RunScope::Goal { goal: g.id },
            wf,
            BTreeMap::new(),
            RunEntry::by_hand(),
            10,
        );
        run.apply(RunEvent::Start, 10).unwrap();
        assert_eq!(g.status(Some(&run)), GoalStatus::Running);
        run.apply(
            RunEvent::Cancel {
                cause: CancelCause::Stopped { rationale: None },
            },
            11,
        )
        .unwrap();
        assert_eq!(
            g.status(Some(&run)),
            GoalStatus::Draft,
            "a cancelled run leaves an open goal ready to run again"
        );

        let closed = Goal {
            closed: Some(Closure {
                reason: ClosureReason::Abandoned {
                    rationale: Some("out of scope".into()),
                },
                at: 12,
            }),
            ..g
        };
        let json = serde_json::to_value(&closed).unwrap();
        assert_eq!(json["closed"]["reason"], "abandoned");
        assert_eq!(json["closed"]["at"], 12);
        assert_eq!(serde_json::from_value::<Goal>(json).unwrap(), closed);
        assert_eq!(closed.status(Some(&run)), GoalStatus::Closed);
        assert_eq!(
            GoalStatus::ALL
                .iter()
                .map(|s| s.as_str().parse::<GoalStatus>().unwrap())
                .collect::<Vec<_>>(),
            GoalStatus::ALL.to_vec()
        );
        assert!("shaping".parse::<GoalStatus>().is_err());
    }

    #[test]
    fn a_queued_current_run_reads_waiting_and_agents() {
        let g = goal();
        let run = WorkflowRun::new(
            RunId::from_ulid(ulid::Ulid::from_parts(4, 2)),
            RunScope::Goal { goal: g.id },
            workflow(vec![agent("a", &[])]),
            BTreeMap::new(),
            RunEntry::by_hand(),
            10,
        );
        assert_eq!(run.status(), RunStatus::Queued);
        assert_eq!(
            g.status(Some(&run)),
            GoalStatus::Waiting,
            "a queued current run is the platform's to start"
        );
        assert_eq!(g.holder(Some(&run), false), Holder::Agents);
        assert_eq!(g.holder(Some(&run), true), Holder::You, "owed still wins");
    }

    #[test]
    fn an_old_goal_shape_is_refused() {
        let old = serde_json::json!({
            "id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            "statement": "x",
            "author": "ab".repeat(32),
            "state": "shaping",
            "criteria": [],
            "budget": {},
            "mode": "guided",
            "revision": 1,
            "created_at": 0
        });
        let err = serde_json::from_value::<Goal>(old).unwrap_err().to_string();
        assert!(err.contains("state") || err.contains("origin"), "{err}");
    }

    /// A goal written before modes carried `guided`; this build refuses it
    /// rather than guessing which of the three it meant.
    #[test]
    fn a_goal_with_the_guided_flag_is_refused() {
        let mut v = serde_json::to_value(goal()).unwrap();
        v.as_object_mut().unwrap().remove("mode");
        v["guided"] = serde_json::json!(true);
        let err = serde_json::from_value::<Goal>(v).unwrap_err().to_string();
        assert!(err.contains("guided"), "{err}");
    }

    #[test]
    fn goal_modes_round_trip_and_say_who_designs_and_who_adopts() {
        for m in GoalMode::ALL {
            let wire = serde_json::to_value(m).unwrap();
            assert_eq!(wire, serde_json::json!(m.as_str()));
            assert_eq!(serde_json::from_value::<GoalMode>(wire).unwrap(), m);
            assert_eq!(m.as_str().parse::<GoalMode>().unwrap(), m);
        }
        assert!("interactive".parse::<GoalMode>().is_err());
        assert_eq!(
            GoalMode::default(),
            GoalMode::Auto,
            "a goal that says nothing is auto"
        );
        assert!(GoalMode::Auto.designs() && GoalMode::Guided.designs());
        assert!(
            !GoalMode::Manual.designs(),
            "a manual goal's person designs"
        );
        assert!(GoalMode::Auto.adopts_alone());
        assert!(!GoalMode::Guided.adopts_alone() && !GoalMode::Manual.adopts_alone());
        assert!(
            GoalMode::Auto.unattended(),
            "an auto run has nobody watching"
        );
        assert!(
            !GoalMode::Guided.unattended() && !GoalMode::Manual.unattended(),
            "a guided or manual goal's person answers above the ceiling"
        );
        // A goal that omits its mode is auto, and the field is always written.
        let mut v = serde_json::to_value(goal()).unwrap();
        assert_eq!(v["mode"], serde_json::json!("guided"));
        v.as_object_mut().unwrap().remove("mode");
        assert_eq!(
            serde_json::from_value::<Goal>(v).unwrap().mode,
            GoalMode::Auto
        );
    }

    #[test]
    fn origins_carry_their_parent() {
        let parent = GoalId::from_ulid(ulid::Ulid::from_parts(1, 9));
        assert_eq!(GoalOrigin::Spawned { parent }.parent(), Some(parent));
        assert_eq!(GoalOrigin::Captured.parent(), None);
        let old = serde_json::json!({"origin": "trigger", "trigger": "t", "signal": "s"});
        assert!(
            serde_json::from_value::<GoalOrigin>(old).is_err(),
            "no goal is made by a trigger any more"
        );
        assert_eq!(
            serde_json::to_value(GoalOrigin::Spawned { parent }).unwrap()["origin"],
            "spawned"
        );
    }

    /// A goal a run of the workspace spawned names the run and the step, and
    /// refines nothing: there is no parent goal to walk to.
    #[test]
    fn a_goal_a_workspace_run_spawned_names_the_run_and_refines_nothing() {
        let origin = GoalOrigin::Run {
            run: RunId::from_ulid(ulid::Ulid::from_parts(4, 4)),
            step: StepId::new("escalate").unwrap(),
        };
        assert_eq!(origin.as_str(), "run");
        assert_eq!(origin.parent(), None, "no refines edge to walk");
        let wire = serde_json::to_value(&origin).unwrap();
        assert_eq!(wire["origin"], "run");
        assert_eq!(wire["step"], "escalate");
        assert_eq!(serde_json::from_value::<GoalOrigin>(wire).unwrap(), origin);
        assert_eq!(GoalOrigin::NAMES, ["captured", "spawned", "run"]);
    }
}

#[cfg(test)]
mod holder_tests {
    use super::*;
    use crate::run::{RunScope, StepRecord, StepState, WorkflowRun};
    use crate::workflow::{Step, StepId, StepKind, ValueRef, WaitFor, Workflow};
    use crate::{ProjectOrigin, RunId, WorkflowId, WorkflowOrigin};
    use std::collections::{BTreeMap, BTreeSet};

    fn step(id: &str, kind: StepKind) -> Step {
        Step {
            id: StepId::new(id).unwrap(),
            name: id.to_string(),
            kind,
            then: vec![],
            boundaries: vec![],
            join: Default::default(),
            on_fail: Default::default(),
            retries: 0,
            max_visits: 3,
            position: None,
        }
    }

    fn goal_with(workflow: Option<WorkflowId>) -> Goal {
        Goal {
            id: GoalId::from_ulid(ulid::Ulid::from_parts(7, 1)),
            statement: "ship it".into(),
            title: None,
            author: PrincipalId::new("a".repeat(64)).unwrap(),
            workflow,
            run: None,
            runs: vec![],
            listening: None,
            closed: None,
            origin: GoalOrigin::Captured,
            budget: Budget::default(),
            mode: GoalMode::Guided,
            assignees: vec![],
            tags: Tags::default(),
            revision: 1,
            archived: None,
            created_at: 1,
        }
    }

    fn run_with(steps: Vec<(Step, StepState)>) -> WorkflowRun {
        let records: BTreeMap<_, _> = steps
            .iter()
            .map(|(s, state)| {
                (
                    s.id.clone(),
                    StepRecord {
                        state: state.clone(),
                        ..Default::default()
                    },
                )
            })
            .collect();
        WorkflowRun {
            id: RunId::from_ulid(ulid::Ulid::from_parts(7, 2)),
            scope: RunScope::Goal {
                goal: GoalId::from_ulid(ulid::Ulid::from_parts(7, 1)),
            },
            workflow: Workflow {
                id: WorkflowId::from_ulid(ulid::Ulid::from_parts(7, 3)),
                name: "wf".into(),
                description: String::new(),
                inputs: vec![],
                steps: steps.into_iter().map(|(s, _)| s).collect(),
                origin: WorkflowOrigin::Workspace,
                author: PrincipalId::new("a".repeat(64)).unwrap(),
                tags: Tags::default(),
                revision: 1,
                archived: None,
                decision_making: false,
                created_at: 1,
            },
            inputs: BTreeMap::new(),
            start: None,
            event: None,
            dispatched: None,
            chain: Default::default(),
            steps: records,
            queued_at: 1,
            started_at: Some(1),
            finished_at: None,
            outcome: None,
            cancelled: None,
            seq: 1,
            revision: 1,
        }
    }

    fn agent_kind() -> StepKind {
        StepKind::Agent {
            instructions: "do".into(),
            assignee: None,
            project: None,
            harness: vec![],
            model: None,
            effort: None,
            output_schema: None,
            tier_ceiling: crate::ToolTier::Exec,
        }
    }

    /// The ladder: closed beats everything; owed beats a finished run; a
    /// finished run beats the steps; no run is yours to start or to design.
    #[test]
    fn holder_precedence_owed_beats_finished() {
        let wf = WorkflowId::from_ulid(ulid::Ulid::from_parts(7, 3));
        let mut goal = goal_with(Some(wf));
        let mut run = run_with(vec![(step("a", agent_kind()), StepState::done())]);
        run.outcome = Some(crate::workflow::RunOutcome::Done);

        assert_eq!(goal.holder(Some(&run), true), Holder::You, "owed wins");
        assert_eq!(goal.holder(Some(&run), false), Holder::Finished);
        assert_eq!(goal.holder(None, false), Holder::You, "chosen, not started");
        goal.workflow = None;
        assert_eq!(goal.holder(None, false), Holder::Design);
        goal.closed = Some(Closure {
            reason: ClosureReason::Abandoned { rationale: None },
            at: 2,
        });
        assert_eq!(
            goal.holder(Some(&run), true),
            Holder::Finished,
            "closed wins over owed"
        );
    }

    /// One row per live kind, and the precedence you > agents > world when
    /// steps of several kinds are live at once.
    #[test]
    fn holder_of_each_live_kind() {
        let wf = WorkflowId::from_ulid(ulid::Ulid::from_parts(7, 3));
        let goal = goal_with(Some(wf));
        let waiting = |kind: StepKind| {
            let run = run_with(vec![(step("s", kind), StepState::Waiting)]);
            goal.holder(Some(&run), false)
        };
        assert_eq!(
            waiting(StepKind::Human {
                prompt: "?".into(),
                options: vec![],
                multi: false,
                assignee: None,
            }),
            Holder::You
        );
        assert_eq!(
            waiting(StepKind::Approval { prompt: "?".into() }),
            Holder::You
        );
        assert_eq!(
            waiting(StepKind::Wait {
                until: WaitFor::Release
            }),
            Holder::You
        );
        assert_eq!(
            waiting(StepKind::Wait {
                until: WaitFor::Delay {
                    secs: ValueRef::Fixed(5)
                }
            }),
            Holder::World
        );
        assert_eq!(
            waiting(StepKind::Spawn {
                statement_template: "x".into(),
                workflow: None,
                assignees: vec![],
                inputs: Default::default(),
                wait: true,
            }),
            Holder::World
        );
        let running = run_with(vec![(step("r", agent_kind()), StepState::Running)]);
        assert_eq!(goal.holder(Some(&running), false), Holder::Agents);

        // you > agents > world when all three are live.
        let mixed = run_with(vec![
            (step("a", agent_kind()), StepState::Running),
            (
                step(
                    "w",
                    StepKind::Wait {
                        until: WaitFor::Delay {
                            secs: ValueRef::Fixed(5),
                        },
                    },
                ),
                StepState::Waiting,
            ),
            (
                step("h", StepKind::Approval { prompt: "?".into() }),
                StepState::Waiting,
            ),
        ]);
        assert_eq!(goal.holder(Some(&mixed), false), Holder::You);
        let agents_and_world = run_with(vec![
            (step("a", agent_kind()), StepState::Running),
            (
                step(
                    "w",
                    StepKind::Wait {
                        until: WaitFor::Delay {
                            secs: ValueRef::Fixed(5),
                        },
                    },
                ),
                StepState::Waiting,
            ),
        ]);
        assert_eq!(goal.holder(Some(&agents_and_world), false), Holder::Agents);
    }

    fn listening(paused: bool) -> crate::listen::Listening {
        crate::listen::Listening {
            inputs: BTreeMap::new(),
            budget: None,
            since: 5,
            paused: paused.then(|| crate::listen::Paused {
                reason: crate::listen::PauseReason::RunFailed {
                    run: RunId::from_ulid(ulid::Ulid::from_parts(7, 2)),
                },
                at: 6,
            }),
        }
    }

    /// A listening goal between runs waits on the world; a paused one is
    /// yours; a live run and a closure win over both.
    #[test]
    fn a_listening_goal_waits_on_the_world_and_a_paused_one_on_you() {
        let wf = WorkflowId::from_ulid(ulid::Ulid::from_parts(7, 3));
        let mut goal = goal_with(Some(wf));
        goal.listening = Some(listening(false));
        assert!(goal.is_listening());
        assert_eq!(goal.status(None), GoalStatus::Waiting);
        assert_eq!(goal.holder(None, false), Holder::World);
        assert_eq!(goal.holder(None, true), Holder::You, "owed still wins");

        // Its last run finished: still listening for the next.
        let mut finished = run_with(vec![(step("a", agent_kind()), StepState::done())]);
        finished.outcome = Some(crate::workflow::RunOutcome::Done);
        finished.finished_at = Some(3);
        assert_eq!(goal.status(Some(&finished)), GoalStatus::Waiting);
        assert_eq!(goal.holder(Some(&finished), false), Holder::World);

        // A run under way reads as the run does.
        let live = run_with(vec![(step("r", agent_kind()), StepState::Running)]);
        assert_eq!(goal.status(Some(&live)), GoalStatus::Running);
        assert_eq!(goal.holder(Some(&live), false), Holder::Agents);

        // Paused by a failed run: failed, and yours.
        goal.listening = Some(listening(true));
        assert!(!goal.is_listening());
        assert_eq!(goal.status(Some(&finished)), GoalStatus::Failed);
        assert_eq!(goal.holder(Some(&finished), false), Holder::You);

        goal.closed = Some(Closure {
            reason: ClosureReason::Abandoned { rationale: None },
            at: 9,
        });
        assert!(!goal.is_listening());
        assert_eq!(goal.status(None), GoalStatus::Closed);
        assert_eq!(goal.holder(None, false), Holder::Finished);
    }

    #[test]
    fn a_standing_goal_names_the_newest_runs_and_every_queued_one() {
        let mut goal = goal_with(None);
        let id = |n: u64| RunId::from_ulid(ulid::Ulid::from_parts(n, 1));
        goal.runs = (1..=(GOAL_RUNS_KEPT as u64 + 3)).map(id).collect();
        goal.run = Some(id(2));
        let queued = BTreeSet::from([id(1)]);
        goal.trim_runs(&queued);
        assert_eq!(goal.runs.len(), GOAL_RUNS_KEPT);
        assert!(goal.runs.contains(&id(1)), "a queued run stays");
        assert!(goal.runs.contains(&id(2)), "the current run stays");
        assert!(!goal.runs.contains(&id(3)) && !goal.runs.contains(&id(5)));
        assert!(goal.runs.contains(&id(6)), "the oldest finished go first");
        let before = goal.runs.clone();
        goal.trim_runs(&queued);
        assert_eq!(goal.runs, before, "at the cap, nothing more goes");
    }

    #[test]
    fn holder_words_round_trip() {
        for h in Holder::ALL {
            assert_eq!(h.as_str().parse::<Holder>().unwrap(), h);
        }
        assert!("flight".parse::<Holder>().is_err());
    }

    #[test]
    fn a_projects_origin_is_reachable_from_goal_and_workflow() {
        // Placed here beside the holder because both are the projections the
        // desktop mirrors; keeps origin.rs free of Goal fixtures.
        let goal = GoalId::from_ulid(ulid::Ulid::from_parts(7, 9));
        let origin = ProjectOrigin::from_goal(goal);
        assert_eq!(origin.goal(), Some(goal));
    }

    #[test]
    fn an_unlimited_budget_has_no_ceiling_and_a_partial_one_is_not_unlimited() {
        assert!(Budget::default().is_unlimited());
        assert!(Budget::default().allows(&BudgetSpent {
            tokens: u64::MAX,
            usd_cents: u64::MAX,
            wall_clock_secs: u64::MAX
        }));
        let partial = Budget {
            max_usd_cents: Some(10),
            ..Budget::default()
        };
        assert!(!partial.is_unlimited());
        assert!(partial.allows(&BudgetSpent {
            tokens: 1_000_000,
            usd_cents: 9,
            wall_clock_secs: 1_000_000
        }));
        assert!(
            !partial.allows(&BudgetSpent {
                tokens: 0,
                usd_cents: 10,
                wall_clock_secs: 0
            }),
            "the ceiling itself is out"
        );
        let json = serde_json::to_value(&partial).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"max_usd_cents": 10}),
            "absent ceilings are absent"
        );
    }
}
