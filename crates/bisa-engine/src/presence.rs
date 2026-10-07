//! Session presence — the one fold from harness events to *what each session
//! is doing*, and the only status that leaves the engine.
//!
//! Every live harness session — a worker on a step, a chat persona, a guided
//! wake — has one [`SessionPresence`] here, keyed by its [`LiveRunId`]. The
//! executor, the conversation driver and the guided wake feed every
//! [`SessionEvent`] through [`Presence::apply`] and call the lifecycle hooks
//! (`register`, `waiting`, `resumed`, `ended`, `parked`, `revived`); the fold
//! decides the [`SessionState`], and a change — never a token — becomes one
//! [`EnginePayload::SessionState`] frame. The node serves `GET /sessions`
//! from [`Presence::snapshot`], so a screen and the bus can never disagree.
//!
//! The registry ([`crate::registry`]) keeps its four control statuses —
//! `Running`, `Idle`, `Parked`, `Aborted` — with their CAS and generation
//! rules; presence is what a person reads, the registry is what the engine
//! enforces. A finished session stays here for
//! [`crate::config::EngineConfig::retain_ended_secs`] so its `done` or
//! `failed` is seen, then leaves the roster with a [`EnginePayload::SessionGone`].
//! A `parked` session — disposed, its durable row resumable by its token,
//! revived by nothing in this process ([`crate::lifecycle`]) — leaves after
//! the same window, its pid gone with the process.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bisa_core::{
    AgentId, ConversationId, Effort, Gate, GoalId, ProjectId, RunId, SessionId, SessionOrigin,
    ToolTier, WorkItemId, WorkstreamId,
};
use bisa_harness::{
    InputKind, InputRequest, LifecycleEvent, Outcome, ProgressEvent, SessionCost, SessionEvent,
    SubagentId,
};
use dashmap::DashMap;
use schemars::JsonSchema;
use serde::Serialize;

use crate::events::{EngineEvent, EnginePayload, ExecutionOutcome};
use crate::registry::{LiveRunId, SessionKind};
use crate::Inner;

/// What a session is doing, as a person reads it. The same nine words on the
/// rail, the Agents pane, the Agents screen, the pet and the activity.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum SessionState {
    /// Launched; nothing has arrived from the harness yet.
    Starting,
    /// Between turns.
    Idle,
    /// A turn is running and no tool is.
    Thinking,
    /// A tool is running — and how heavy a hand it has (the rail
    /// draws a glyph per tier, so a person sees *reading* from *executing*
    /// without reading the tool's name).
    Running {
        tool: String,
        args: String,
        tier: ToolTier,
    },
    /// Stopped until a person acts.
    Waiting {
        on: WaitingOn,
    },
    Done,
    Aborted,
    Failed {
        reason: String,
    },
    /// Disposed but resumable.
    Parked,
}

impl WaitingOn {
    /// The wait in words, as the rail's row and the Inbox's say it:
    /// *permission: Bash*, *a question: …*, *the approval gate*, *sign in
    /// to github*.
    pub fn words(&self) -> String {
        match self {
            WaitingOn::Permission { tool, .. } => format!("permission: {tool}"),
            WaitingOn::Question { text, .. } => format!("a question: {text}"),
            WaitingOn::Gate { gate, .. } => format!("the {} gate", gate.as_str()),
            WaitingOn::Auth { provider, .. } => format!("sign in to {provider}"),
        }
    }
}

/// Why a session is waiting on a person.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "on")]
pub enum WaitingOn {
    /// The harness asked before running a tool; `gate_id` once the engine has
    /// opened the escalation gate a person answers in the Inbox.
    Permission {
        tool: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        gate_id: Option<String>,
    },
    Question {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        gate_id: Option<String>,
    },
    /// A step's own gate — an approval or an escalation the run raised.
    Gate { gate: Gate, gate_id: String },
    Auth {
        provider: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
}

impl SessionState {
    /// Still going: a person may abort it, and the rail draws it working.
    pub fn is_live(&self) -> bool {
        matches!(
            self,
            SessionState::Starting
                | SessionState::Idle
                | SessionState::Thinking
                | SessionState::Running { .. }
                | SessionState::Waiting { .. }
        )
    }

    /// Finished one way or another — retained for a while, then dropped.
    pub fn is_ended(&self) -> bool {
        matches!(
            self,
            SessionState::Done | SessionState::Aborted | SessionState::Failed { .. }
        )
    }

    /// The word the vocabulary uses on the wire (`serde`'s tag).
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionState::Starting => "starting",
            SessionState::Idle => "idle",
            SessionState::Thinking => "thinking",
            SessionState::Running { .. } => "running",
            SessionState::Waiting { .. } => "waiting",
            SessionState::Done => "done",
            SessionState::Aborted => "aborted",
            SessionState::Failed { .. } => "failed",
            SessionState::Parked => "parked",
        }
    }
}

/// A sub-agent a harness spawned inside a session, folded from
/// `SubagentStarted`, the `Nested` events under it, and `SubagentEnded`.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct SubagentPresence {
    pub id: SubagentId,
    pub name: String,
    pub description: String,
    pub state: SessionState,
    /// Unix seconds the current state was entered.
    pub since: u64,
    /// Unix seconds the sub-agent was announced or first seen — never reset,
    /// unlike `since`. The anchor the desktop counts its elapsed time from.
    pub started: u64,
}

/// One live (or recently ended) harness session, as the roster shows it.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct SessionPresence {
    pub id: LiveRunId,
    pub kind: SessionKind,
    /// Why the session exists — what woke it and for what: a run's step by
    /// id and name, the Workflow Agent's phase, a turn's scope and who woke
    /// it, a terminal, a one-shot ask's purpose. Said at registration, said
    /// again when it changes.
    pub origin: SessionOrigin,
    pub state: SessionState,
    /// Unix seconds the current state was entered.
    pub since: u64,
    pub harness: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The effort the session runs at: what it was launched with, after the
    /// fit to its model. Absent when the harness takes none for the model,
    /// and for a terminal session, which is the person's own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    /// The agent definition running, when one is — a chat persona, the
    /// Workflow Agent, an assigned worker. `None` for a bare worker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_item: Option<WorkItemId>,
    /// The conversation this session is a turn of — the door every surface
    /// reaches it through; never drawn as a row of the checkout it stands in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation: Option<ConversationId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<GoalId>,
    /// The run whose work item the session works on — how the roster names
    /// a run of the workspace's worker, which no goal holds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<RunId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workstream: Option<WorkstreamId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectId>,
    /// Where the session stands on disk — the harness's working directory:
    /// a checkout, a goal's or an agent's scratch folder. Absent for a
    /// terminal, whose folder is its tab's own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<String>,
    /// Unix seconds the session was registered — never reset, unlike `since`.
    /// The anchor the desktop measures total open time and a run's
    /// completed-in duration from.
    pub started: u64,
    /// The harness process's OS pid while a turn runs — the root the desktop
    /// traces a listening port to. Set on `ProcessStarted`, cleared
    /// when the session ends or parks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    pub cost: SessionCost,
    pub children: Vec<SubagentPresence>,
    /// Unix seconds of the last event, tokens included.
    pub last_activity: u64,
    /// Climbs by one on every change a reader sees — each frame of a row
    /// carries a higher number than the one before it, whatever order they
    /// reach a reader in. A reader keeps the highest it has.
    pub revision: u64,
}

/// What a session is about, said once at registration.
#[derive(Debug, Clone)]
pub struct SessionMeta {
    pub kind: SessionKind,
    /// Why the session exists ([`SessionOrigin`]).
    pub origin: SessionOrigin,
    pub harness: String,
    pub model: Option<String>,
    /// The effort the session was launched at (`Launched::effort`).
    pub effort: Option<Effort>,
    pub agent: Option<AgentId>,
    pub session_id: Option<SessionId>,
    pub work_item: Option<WorkItemId>,
    pub conversation: Option<ConversationId>,
    pub goal: Option<GoalId>,
    pub run: Option<RunId>,
    pub workstream: Option<WorkstreamId>,
    pub project: Option<ProjectId>,
    /// The harness's working directory, when the session has one of its own.
    pub cwd: Option<String>,
    pub transcript_path: Option<String>,
}

/// One row and the fold's working memory beside it.
#[derive(Debug, Clone)]
struct Entry {
    presence: SessionPresence,
    /// Tools running in the session itself (not in a sub-agent), innermost last.
    open_tools: Vec<OpenTool>,
    /// What the session — or one of its sub-agents — stopped on and nobody
    /// has answered yet, oldest first. A session-owned wait is the row's
    /// word; a child's is the child's. Each ends on its own answer, on its
    /// own tool running or ending, on the person's answer in the tab, or at
    /// the turn's end — never on another tool's.
    waits: Vec<OpenWait>,
    /// Tool ids the guard refused this turn: a start that lands after the
    /// refusal (hooks run in parallel) opens nothing.
    denied: Vec<String>,
    /// Sub-agents that ended this turn: a word under one of them that lands
    /// late is nothing, not a child born again.
    ended_children: Vec<SubagentId>,
    /// Bumped on every terminal state; a retention timer that wakes to a
    /// different epoch does nothing.
    epoch: u64,
    /// A turn is open: what a session goes back to when nothing else is
    /// happening is *thinking* inside one and *idle* outside.
    in_turn: bool,
    /// The harness's own id for this session, for a harness that names its
    /// sessions (OpenCode: every frame carries a `sessionID`): a `Nested`
    /// event raised by it is the session's own. Learnt from the first nested
    /// event of a session that has never had a child.
    native_root: Option<SubagentId>,
    /// The session has had a sub-agent: a nested event under an id it does
    /// not know is a child it was not told about, never its root.
    ever_child: bool,
    /// Unix seconds the row's `pid` was reported, so a sweep can tell the
    /// process it saw from one that merely inherited the number.
    pid_seen_at: Option<u64>,
}

/// What a launch says of a session whose row stood before it
/// (`Presence::launched`).
#[derive(Debug, Clone)]
pub struct LaunchedFacts {
    pub harness: String,
    pub model: Option<String>,
    pub effort: Option<Effort>,
    pub session_id: Option<SessionId>,
    pub transcript_path: Option<String>,
}

/// The tool a session is running while its sub-agents are the work.
pub const DELEGATION_TOOL: &str = "sub-agent";

/// One tool still running in the session: what `resume_state` goes back to.
#[derive(Debug, Clone)]
struct OpenTool {
    /// The harness's own id for the call, when it gave one.
    id: Option<String>,
    name: String,
    args: String,
    tier: ToolTier,
}

impl OpenTool {
    /// Whether a tool event is about this call: equal ids when both sides
    /// have one, else the same name.
    fn is(&self, id: Option<&str>, name: &str) -> bool {
        same_tool(self.id.as_deref(), &self.name, id, name)
    }
}

/// Whether two namings of a tool call name the same call: equal ids when
/// both have one — two `Bash` calls side by side are told apart — else the
/// same name, for a harness that names its calls nothing.
fn same_tool(a_id: Option<&str>, a_name: &str, b_id: Option<&str>, b_name: &str) -> bool {
    match (a_id, b_id) {
        (Some(a), Some(b)) => a == b,
        _ => a_name == b_name,
    }
}

/// The tool a wait is about, when it is about one.
#[derive(Debug, Clone)]
struct ToolRef {
    id: Option<String>,
    name: String,
}

/// One request the session, or a sub-agent of it, stopped on.
#[derive(Debug, Clone)]
struct OpenWait {
    /// The request's id — what an answer names.
    id: String,
    on: WaitingOn,
    /// The sub-agent that asked, when one did; `None` for the session's own.
    owner: Option<SubagentId>,
    /// The tool asked about, for a permission: the call's start or end ends
    /// the wait. A question or a dialog names none, and any tool of its
    /// owner's ends it.
    tool: Option<ToolRef>,
}

impl OpenWait {
    /// Whether a tool event of `owner` ends this wait.
    fn ended_by(&self, owner: Option<&SubagentId>, id: Option<&str>, name: &str) -> bool {
        self.owner.as_ref() == owner
            && match &self.tool {
                Some(tool) => same_tool(tool.id.as_deref(), &tool.name, id, name),
                None => true,
            }
    }
}

/// How many sub-agents one session keeps track of at most; a 33rd is dropped.
pub const MAX_CHILDREN: usize = 32;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// How often the roster is swept for terminal rows whose process is gone.
pub const SWEEP_SECS: u64 = 30;

/// The dead-process sweeper: every [`SWEEP_SECS`], [`Presence::sweep_dead`].
/// One iteration that panics is an `error` line, not the end of the sweep.
pub async fn run_sweeper(inner: Arc<Inner>) {
    let mut interval = tokio::time::interval(Duration::from_secs(SWEEP_SECS));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    interval.tick().await;
    loop {
        interval.tick().await;
        let inner = Arc::clone(&inner);
        crate::survive("dead session sweep", async move {
            inner.presence.sweep_dead(&inner);
        })
        .await;
    }
}

/// The presence of every session this engine drives.
pub struct Presence {
    rows: DashMap<LiveRunId, Entry>,
    /// How long a finished session stays in the roster.
    retain_ended: Duration,
    /// The sorted roster, memoized for `cache.presence.ttl_ms`: a
    /// burst of `GET /sessions` within the window shares one clone-and-sort.
    /// Kept with the count of changes it was read at, so a roster that
    /// changed since is never answered from it.
    snapshot_cache: bisa_cache::TtlCell<(u64, Vec<SessionPresence>)>,
    /// How many times the roster changed in a way a reader sees.
    changes: std::sync::atomic::AtomicU64,
}

impl Presence {
    pub fn new(retain_ended: Duration) -> Self {
        Self {
            rows: DashMap::new(),
            retain_ended,
            snapshot_cache: bisa_cache::TtlCell::new("engine.presence"),
            changes: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// A session was launched: it is `starting` until the harness speaks.
    pub fn register(&self, inner: &Inner, id: LiveRunId, meta: SessionMeta) {
        let now = now_secs();
        let presence = SessionPresence {
            started: now,
            pid: None,
            id,
            kind: meta.kind,
            origin: meta.origin,
            state: SessionState::Starting,
            since: now,
            harness: meta.harness,
            model: meta.model,
            effort: meta.effort,
            agent: meta.agent,
            session_id: meta.session_id,
            work_item: meta.work_item,
            conversation: meta.conversation,
            goal: meta.goal,
            run: meta.run,
            workstream: meta.workstream,
            project: meta.project,
            cwd: meta.cwd,
            transcript_path: meta.transcript_path,
            cost: SessionCost::default(),
            children: Vec::new(),
            last_activity: now,
            revision: 1,
        };
        self.rows.insert(
            id,
            Entry {
                presence: presence.clone(),
                open_tools: Vec::new(),
                waits: Vec::new(),
                denied: Vec::new(),
                ended_children: Vec::new(),
                in_turn: false,
                native_root: None,
                ever_child: false,
                epoch: 0,
                pid_seen_at: None,
            },
        );
        self.say(inner, &presence);
    }

    /// Fold one harness event. Emits only when the fold changed something a
    /// person can see — a state, a child's state, the cost — never for a token.
    pub fn apply(&self, inner: &Inner, id: LiveRunId, event: &SessionEvent) {
        let changed = match self.rows.get_mut(&id) {
            Some(mut entry) => {
                let changed = fold(&mut entry, event, now_secs());
                if changed {
                    entry.presence.revision += 1;
                }
                changed.then(|| entry.presence.clone())
            }
            None => None,
        };
        if let Some(presence) = changed {
            self.say(inner, &presence);
        }
    }

    /// The session stopped for a person: an input request the engine turned
    /// into a gate, a step's own gate, a question through the intake. The
    /// wait is recorded under `wait_id` — the request's own id when there is
    /// a request, so the wait the fold already holds is told its gate rather
    /// than doubled; the gate's id otherwise — and ends by that id.
    pub fn waiting(&self, inner: &Inner, id: LiveRunId, wait_id: &str, on: WaitingOn) {
        self.change(inner, id, |entry, now| {
            entry.in_turn = true;
            let tool = match &on {
                WaitingOn::Permission { tool, .. } => Some(ToolRef {
                    id: Some(wait_id.to_string()),
                    name: tool.clone(),
                }),
                _ => None,
            };
            push_wait(
                entry,
                OpenWait {
                    id: wait_id.to_string(),
                    on: on.clone(),
                    owner: None,
                    tool,
                },
            );
            let state = session_word(entry);
            set_state(entry, state, now)
        });
    }

    /// The session's origin changed — a turn woken again for someone else,
    /// the Workflow Agent's wake carried from a design into a repair — and
    /// the row is said again with the new fact. The same fact twice says
    /// nothing.
    pub fn origin(&self, inner: &Inner, id: LiveRunId, origin: SessionOrigin) {
        self.change(inner, id, |entry, _now| {
            if entry.presence.origin == origin {
                return false;
            }
            entry.presence.origin = origin;
            true
        });
    }

    /// The launch said what the row stood for before it: which harness and
    /// model the session runs on, at what effort, its durable id and its
    /// transcript. A row registered before the harness started — so a stop
    /// that lands meanwhile finds it — is re-said, its revision bumped.
    pub fn launched(&self, inner: &Inner, id: LiveRunId, facts: LaunchedFacts) {
        self.change(inner, id, move |entry, _now| {
            let presence = &mut entry.presence;
            presence.harness = facts.harness;
            presence.model = facts.model;
            presence.effort = facts.effort;
            if facts.session_id.is_some() {
                presence.session_id = facts.session_id;
            }
            presence.transcript_path = facts.transcript_path;
            true
        });
    }

    /// The person acted on what the session itself asked — the engine
    /// delivered an answer, a gate was decided: its own waits are over and
    /// it goes back to what it was doing.
    pub fn resumed(&self, inner: &Inner, id: LiveRunId) {
        self.change(inner, id, |entry, now| {
            let before = entry.waits.len();
            entry.waits.retain(|w| w.owner.is_some());
            let state = session_word(entry);
            set_state(entry, state, now) || before != entry.waits.len()
        });
    }

    /// The person answered in the terminal: every wait of the session and
    /// of its sub-agents is over — the tab is where a terminal harness is
    /// answered, and no hook says so. The session goes back to its open
    /// tool, a sub-agent to thinking: whichever way the person answered, the
    /// harness's next word corrects the row.
    pub fn answered(&self, inner: &Inner, id: LiveRunId) {
        self.change(inner, id, |entry, now| {
            if entry.waits.is_empty() {
                return false;
            }
            entry.waits.clear();
            let mut moved = false;
            for c in entry.presence.children.iter_mut() {
                if matches!(c.state, SessionState::Waiting { .. }) {
                    c.state = SessionState::Thinking;
                    c.since = now;
                    moved = true;
                }
            }
            let state = session_word(entry);
            set_state(entry, state, now) || moved
        });
    }

    /// The guard refused a tool the session's hook asked about: the call will
    /// not run, so it is closed if its start was heard, remembered so a start
    /// that lands later opens nothing, and a wait on it is over.
    pub fn refused_tool(&self, inner: &Inner, id: LiveRunId, tool_id: Option<&str>, name: &str) {
        self.change(inner, id, |entry, now| {
            if let Some(tool_id) = tool_id {
                if !entry.denied.iter().any(|d| d == tool_id) {
                    entry.denied.push(tool_id.to_string());
                }
            }
            let before = entry.open_tools.len() + entry.waits.len();
            if let Some(at) = entry.open_tools.iter().rposition(|t| t.is(tool_id, name)) {
                entry.open_tools.remove(at);
            }
            entry
                .waits
                .retain(|w| !w.ended_by(None, tool_id, name) || w.tool.is_none());
            let state = session_word(entry);
            set_state(entry, state, now) || before != entry.open_tools.len() + entry.waits.len()
        });
    }

    /// One change to a row under its lock, said on the bus when it moved
    /// anything a reader sees.
    fn change(&self, inner: &Inner, id: LiveRunId, f: impl FnOnce(&mut Entry, u64) -> bool) {
        let changed = match self.rows.get_mut(&id) {
            Some(mut entry) => {
                if entry.presence.state.is_ended() {
                    None
                } else {
                    let now = now_secs();
                    let moved = f(&mut entry, now);
                    if moved {
                        entry.presence.last_activity = now;
                        entry.presence.revision += 1;
                    }
                    moved.then(|| entry.presence.clone())
                }
            }
            None => None,
        };
        if let Some(presence) = changed {
            self.say(inner, &presence);
        }
    }

    /// The session's driver settled it. The row stays for the retention
    /// window so the outcome is seen, then leaves with `SessionGone`.
    pub fn ended(&self, inner: &Arc<Inner>, id: LiveRunId, outcome: &ExecutionOutcome) {
        let Some(epoch) = self.settle(inner, id, outcome) else {
            return;
        };
        let inner = Arc::clone(inner);
        let retain = self.retain_ended;
        tokio::spawn(async move {
            tokio::time::sleep(retain).await;
            inner.presence.drop_ended(&inner, id, epoch);
        });
    }

    /// The session ended and its row is **held**: no retention timer, so it
    /// stays until something forgets it. An interactive session's row lives
    /// exactly as long as its terminal tab — the tab's close is what forgets
    /// it ([`Self::forget`]), never a clock.
    pub fn hold_ended(&self, inner: &Inner, id: LiveRunId, outcome: &ExecutionOutcome) {
        self.settle(inner, id, outcome);
    }

    /// The ended state the outcome means, moved into; the epoch it moved to,
    /// or `None` for a session the roster does not hold.
    fn settle(&self, inner: &Inner, id: LiveRunId, outcome: &ExecutionOutcome) -> Option<u64> {
        let state = match outcome {
            ExecutionOutcome::Completed => SessionState::Done,
            ExecutionOutcome::Aborted => SessionState::Aborted,
            ExecutionOutcome::Cancelled { reason } => SessionState::Failed {
                reason: format!("cancelled: {reason}"),
            },
            ExecutionOutcome::Failed { reason } => SessionState::Failed {
                reason: reason.clone(),
            },
            ExecutionOutcome::BudgetExhausted => SessionState::Failed {
                reason: "budget exhausted".into(),
            },
            ExecutionOutcome::WallClockExceeded => SessionState::Failed {
                reason: "wall clock exceeded".into(),
            },
        };
        // The row's guard lives in its own block: `transition` locks the map
        // again, and a guard still held here would wait on itself.
        let epoch = {
            let mut entry = self.rows.get_mut(&id)?;
            // `aborted` is somebody's decision, entered once: what the
            // session's driver says on its way out — *failed: session
            // aborted*, *done* — does not rewrite it, and the retention
            // clock the abort started is the one that runs.
            if entry.presence.state == SessionState::Aborted {
                return None;
            }
            entry.epoch += 1;
            entry.epoch
        };
        self.transition(inner, id, state);
        Some(epoch)
    }

    /// Leave the roster at once, with no retention: an attempt that never
    /// really ran (its model died at launch and the driver goes round again
    /// with a fresh id), or an interactive session whose terminal tab closed
    /// — the tab was the row.
    pub fn forget(&self, inner: &Inner, id: LiveRunId) {
        inner.ending.gone(id);
        if self.rows.remove(&id).is_some() {
            self.gone(inner, id);
        }
    }

    /// The row's harness child and the moment it was seen — what a stop
    /// reads before it ends the row, which clears both.
    pub(crate) fn process_of(&self, id: LiveRunId) -> Option<(u32, u64)> {
        self.rows
            .get(&id)
            .and_then(|entry| entry.presence.pid.zip(entry.pid_seen_at))
    }

    /// Disposed, its durable row resumable by its token — and revived by
    /// nothing in this process (`lifecycle`): the process is gone, so the pid
    /// goes, and the row stays for the retention window like an ended one,
    /// then leaves with `SessionGone`.
    pub fn parked(&self, inner: &Inner, id: LiveRunId) {
        let epoch = {
            let Some(mut entry) = self.rows.get_mut(&id) else {
                return;
            };
            entry.presence.pid = None;
            entry.pid_seen_at = None;
            entry.epoch += 1;
            entry.epoch
        };
        self.transition(inner, id, SessionState::Parked);
        // The retention clock needs an owned handle to run on; while the
        // engine is still being built there is none, and the row stays.
        if let Some(owned) = inner.arc() {
            let retain = self.retain_ended;
            tokio::spawn(async move {
                tokio::time::sleep(retain).await;
                owned.presence.drop_ended(&owned, id, epoch);
            });
        }
    }

    /// The retention window closed on a finished — or a parked — session:
    /// it leaves the roster, unless it moved since (the epoch moved).
    fn drop_ended(&self, inner: &Inner, id: LiveRunId, epoch: u64) {
        let Some((_, gone)) = self.rows.remove_if(&id, |_, e| {
            e.epoch == epoch
                && (e.presence.state.is_ended() || e.presence.state == SessionState::Parked)
        }) else {
            return;
        };
        // An interactive session has no registry entry: the roster row and
        // the desk's own state — its secret, its files, the reader of its
        // events — were the whole of it, and nobody is left to close it.
        if gone.presence.kind == SessionKind::Terminal {
            inner.interactive.dismiss(id);
        } else {
            crate::debug_on_err(inner.registry.remove(id), "forgetting a finished run");
        }
        self.gone(inner, id);
    }

    /// A row changed in a way a reader sees: counted, so the memoized
    /// roster is behind at once ([`Self::snapshot_cached`]), and said on
    /// the bus.
    fn say(&self, inner: &Inner, presence: &SessionPresence) {
        self.changes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        emit(inner, presence);
    }

    /// A row left the roster: counted, and said.
    fn gone(&self, inner: &Inner, id: LiveRunId) {
        self.changes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        inner.emit(EngineEvent::global(EnginePayload::SessionGone {
            live_run: id,
        }));
    }

    /// Every terminal row whose process is gone ends as failed, its children
    /// with it — the runtime twin of the boot-time [`crate::sessions::end_stale`].
    /// The desktop's PTY pump posts the exit when it is there to see it; a
    /// desktop that died with the process is not, and a row nobody ends would
    /// read *thinking* forever. Only a pid the row reported is checked, and
    /// only the process that reported it counts (`child::is_still_ours`).
    pub fn sweep_dead(&self, inner: &Inner) -> usize {
        let now = now_secs();
        let gone: Vec<LiveRunId> = self
            .rows
            .iter()
            .filter(|e| e.presence.kind == SessionKind::Terminal && !e.presence.state.is_ended())
            .filter_map(|e| {
                let pid = e.presence.pid?;
                let seen_at = e.pid_seen_at?;
                (!crate::sessions::child::is_still_ours(pid, seen_at, now)).then_some(e.presence.id)
            })
            .collect();
        for id in &gone {
            tracing::info!(target: "bisa_engine", session = %id, "the process behind a terminal session is gone: ending its row");
            self.hold_ended(
                inner,
                *id,
                &ExecutionOutcome::Failed {
                    reason: "the process is gone".into(),
                },
            );
        }
        gone.len()
    }

    fn transition(&self, inner: &Inner, id: LiveRunId, state: SessionState) {
        let changed = match self.rows.get_mut(&id) {
            Some(mut entry) => {
                if entry.presence.state == state {
                    None
                } else {
                    let now = now_secs();
                    // Every door a row ends through — the stream's end, a PTY
                    // exit, an abort, a retirement — takes its children with it.
                    if state.is_ended() {
                        end_row(&mut entry);
                    }
                    entry.presence.state = state;
                    entry.presence.since = now;
                    entry.presence.last_activity = now;
                    entry.presence.revision += 1;
                    Some(entry.presence.clone())
                }
            }
            None => None,
        };
        if let Some(presence) = changed {
            self.say(inner, &presence);
        }
    }

    /// Every row, newest state change first.
    pub fn snapshot(&self) -> Vec<SessionPresence> {
        let mut rows: Vec<SessionPresence> = self.rows.iter().map(|e| e.presence.clone()).collect();
        rows.sort_by(|a, b| b.since.cmp(&a.since).then_with(|| a.id.cmp(&b.id)));
        rows
    }

    /// [`snapshot`](Self::snapshot), memoized for `ttl` (`cache.presence.ttl_ms`)
    /// — the read surface `GET /sessions` calls, where a burst of
    /// requests should not each re-sort the roster. What is memoized is
    /// answered only while the roster is as it was read: a session stopped a
    /// moment ago never reads *thinking* for the rest of the window.
    pub fn snapshot_cached(&self, ttl: Duration) -> Vec<SessionPresence> {
        use std::sync::atomic::Ordering;
        if let Some((at, rows)) = self.snapshot_cache.get(ttl) {
            if at == self.changes.load(Ordering::SeqCst) {
                return rows;
            }
        }
        // The count is read before the rows: a change that lands while they
        // are read leaves what is stored behind the count, and unanswered.
        let at = self.changes.load(Ordering::SeqCst);
        let rows = self.snapshot();
        if !ttl.is_zero() {
            self.snapshot_cache.set((at, rows.clone()));
        }
        rows
    }

    pub fn get(&self, id: LiveRunId) -> Option<SessionPresence> {
        self.rows.get(&id).map(|e| e.presence.clone())
    }

    /// The harnesses a person opened in a terminal that are waiting on them
    /// right now — the Inbox's `session` rows (ide/06 §Reporting). Newest
    /// wait first.
    pub fn waiting_terminals(&self) -> Vec<SessionPresence> {
        let mut rows: Vec<SessionPresence> = self
            .rows
            .iter()
            .map(|e| e.presence.clone())
            .filter(|p| p.kind == crate::registry::SessionKind::Terminal && p.wait().is_some())
            .collect();
        rows.sort_by(|a, b| {
            b.wait_since()
                .cmp(&a.wait_since())
                .then_with(|| a.id.cmp(&b.id))
        });
        rows
    }

    /// The engine's session working on a work item, if one is. A terminal's
    /// row is a person's own tab, never the session a gate or a question is
    /// about.
    pub fn by_work_item(&self, work_item: WorkItemId) -> Option<LiveRunId> {
        self.rows
            .iter()
            .find(|e| {
                e.presence.work_item == Some(work_item)
                    && e.presence.state.is_live()
                    && e.presence.kind != crate::registry::SessionKind::Terminal
            })
            .map(|e| e.presence.id)
    }

    /// Every live engine session about a goal — a terminal tab opened at the
    /// goal's scope is not one.
    pub fn by_goal(&self, goal: GoalId) -> Vec<LiveRunId> {
        self.rows
            .iter()
            .filter(|e| {
                e.presence.goal == Some(goal)
                    && e.presence.state.is_live()
                    && e.presence.kind != crate::registry::SessionKind::Terminal
            })
            .map(|e| e.presence.id)
            .collect()
    }

    /// The session waiting on a gate, if one is — by the waits it holds, so a
    /// gate a sub-agent's wait was raised to is found as well.
    pub fn by_gate(&self, gate_id: &str) -> Option<LiveRunId> {
        self.rows
            .iter()
            .find(|e| e.waits.iter().any(|w| gate_of(&w.on) == Some(gate_id)))
            .map(|e| e.presence.id)
    }
}

/// The gate a wait was raised to, when it was.
fn gate_of(on: &WaitingOn) -> Option<&str> {
    match on {
        WaitingOn::Permission {
            gate_id: Some(g), ..
        }
        | WaitingOn::Question {
            gate_id: Some(g), ..
        } => Some(g.as_str()),
        WaitingOn::Gate { gate_id, .. } => Some(gate_id.as_str()),
        _ => None,
    }
}

impl SessionPresence {
    /// What the session stops on, if anything: its own wait, else the first
    /// of its sub-agents' — with that sub-agent, so a surface can say whose.
    pub fn wait(&self) -> Option<(&WaitingOn, Option<&SubagentPresence>)> {
        if let SessionState::Waiting { on } = &self.state {
            return Some((on, None));
        }
        self.children.iter().find_map(|c| match &c.state {
            SessionState::Waiting { on } => Some((on, Some(c))),
            _ => None,
        })
    }

    /// Unix seconds the wait [`Self::wait`] names began — the sub-agent's
    /// when it is a sub-agent's; the row's `since` when there is no wait.
    pub fn wait_since(&self) -> u64 {
        match self.wait() {
            Some((_, Some(child))) => child.since,
            _ => self.since,
        }
    }
}

fn emit(inner: &Inner, presence: &SessionPresence) {
    let scope = crate::events::EventScope {
        goal: presence.goal,
        workflow: None,
        run: presence.run,
    };
    inner.emit(scope.event(
        presence.work_item,
        EnginePayload::SessionState {
            live_run: presence.id,
            presence: Box::new(presence.clone()),
        },
    ));
}

/// The wait a harness's own request puts a session in — before the engine has
/// a gate for it.
pub fn waiting_on(request: &InputRequest) -> WaitingOn {
    match &request.kind {
        InputKind::Permission { tool_name, .. } => WaitingOn::Permission {
            tool: tool_name.clone(),
            gate_id: None,
        },
        InputKind::Question { text, .. } => WaitingOn::Question {
            text: text.clone(),
            gate_id: None,
        },
        InputKind::Auth { provider, url } => WaitingOn::Auth {
            provider: provider.clone(),
            url: url.clone(),
        },
    }
}

/// One state onto the row: false when it already reads so.
fn set_state(entry: &mut Entry, state: SessionState, now: u64) -> bool {
    if entry.presence.state == state {
        return false;
    }
    entry.presence.state = state;
    entry.presence.since = now;
    true
}

/// The fold: one event moves one row. Returns whether anything a person can
/// see changed. Pure over the entry, so it is tested without an engine.
fn fold(entry: &mut Entry, event: &SessionEvent, now: u64) -> bool {
    // An ended row is the outcome a person reads, and a terminal's is held
    // until its tab closes. A harness's hooks are calls of their own and can
    // land after its exit did: they move nothing, or *done* would read
    // *running* for as long as the tab stays. Only a process starting in the
    // row again makes it a session again — an engine's own session, whose
    // driver starts it; never a terminal's, whose process is the tab's and
    // whose start is one more hook that can land late.
    // What it cost is a fact and not a state: a last figure that arrives
    // after the end still counts.
    if entry.presence.state.is_ended() {
        let started_again = matches!(event, SessionEvent::Lifecycle(LifecycleEvent::Started))
            && entry.presence.kind != SessionKind::Terminal;
        let cost = matches!(
            event,
            SessionEvent::Progress(ProgressEvent::CostDelta { .. })
        );
        if !started_again && !cost {
            return false;
        }
    }
    entry.presence.last_activity = now;
    let set = |entry: &mut Entry, state: SessionState| -> bool { set_state(entry, state, now) };
    match event {
        SessionEvent::Raw(_) => false,
        SessionEvent::Lifecycle(l) => match l {
            // A start is a fresh session: whatever was open or asked belongs
            // to the one before it.
            LifecycleEvent::Started => {
                let cleared = turn_over(entry, now);
                set(entry, SessionState::Idle) || cleared
            }
            // Metadata, not a state: keep the row's state, re-emit so the
            // desktop learns the pid (the port scanner's root).
            LifecycleEvent::ProcessStarted { pid } => {
                entry.presence.pid = *pid;
                entry.pid_seen_at = pid.map(|_| now);
                true
            }
            LifecycleEvent::Parked => {
                entry.presence.pid = None;
                let cleared = turn_over(entry, now);
                set(entry, SessionState::Parked) || cleared
            }
            LifecycleEvent::Revived => {
                let cleared = turn_over(entry, now);
                set(entry, SessionState::Idle) || cleared
            }
            // The session — or a sub-agent of it, when the request says so —
            // stopped for a person. A sub-agent's wait is the sub-agent's:
            // the session keeps its word, and the hand is on the child's row.
            LifecycleEvent::InputRequested { request } => {
                let wait = wait_of(request);
                match request.parent.clone() {
                    None => {
                        entry.in_turn = true;
                        push_wait(entry, wait);
                        let state = session_word(entry);
                        set(entry, state)
                    }
                    Some(parent) => {
                        let made = ensure_child(entry, &parent, now);
                        if !made && !entry.presence.children.iter().any(|c| c.id == parent) {
                            // Over the bound, or a child that ended this
                            // turn: its ask is nothing here.
                            return false;
                        }
                        push_wait(entry, wait);
                        let on = child_wait(entry, &parent).cloned();
                        let moved = match on {
                            Some(on) => {
                                child_state(entry, &parent, SessionState::Waiting { on }, now)
                            }
                            None => false,
                        };
                        moved || made
                    }
                }
            }
            // The answer to one request: that wait ends and nothing else
            // does — a session with two dialogs open still waits on the
            // other. An answer nothing asked for moves nothing.
            LifecycleEvent::InputResolved { id } => {
                let Some(at) = entry.waits.iter().position(|w| &w.id == id) else {
                    tracing::debug!(target: "bisa_engine::presence", request = %id, "an answer to a request nobody holds moves nothing");
                    return false;
                };
                let wait = entry.waits.remove(at);
                after_wait(entry, wait.owner.as_ref(), now)
            }
            // A turn ended, however it ended: the driver decides what a failed
            // one means; the row says idle until it does — unless its
            // sub-agents are still the work.
            LifecycleEvent::Ended {
                is_terminal: false, ..
            } => {
                let cleared = turn_over(entry, now);
                let state = resume_state(entry);
                set(entry, state) || cleared
            }
            LifecycleEvent::Ended {
                outcome,
                is_terminal: true,
            } => {
                end_row(entry);
                match outcome {
                    Outcome::Completed => set(entry, SessionState::Done),
                    Outcome::Aborted => set(entry, SessionState::Aborted),
                    Outcome::Failed { error } => set(
                        entry,
                        SessionState::Failed {
                            reason: error.clone(),
                        },
                    ),
                    // The model went away, not the work: the driver relaunches
                    // on the next model, and the row reads starting meanwhile.
                    Outcome::ModelUnavailable { .. } => set(entry, SessionState::Starting),
                    Outcome::Suspended { reason } => set(
                        entry,
                        SessionState::Failed {
                            reason: format!("suspended: {reason}"),
                        },
                    ),
                }
            }
        },
        SessionEvent::Progress(p) => match p {
            // A turn opens on the session's own words; a sub-agent that
            // ended last turn is not this turn's news. A turn still open —
            // the harness gave no word when the person interrupted it —
            // closes here: the prompt that lands is the last turn's end.
            ProgressEvent::TurnStarted => {
                let cleared = if entry.in_turn {
                    turn_over(entry, now)
                } else {
                    settle_children(entry)
                };
                entry.in_turn = true;
                set(entry, SessionState::Thinking) || cleared
            }
            ProgressEvent::TurnEnded => {
                let cleared = turn_over(entry, now);
                let state = resume_state(entry);
                set(entry, state) || cleared
            }
            // A tool of the session's own starts: it is the innermost open
            // tool — the same call heard again (a harness that says
            // *running* more than once) stays one — and a wait on it is over,
            // since it runs. A wait on another tool stands: the hand stays up
            // while a parallel call runs. A call the guard refused never opens.
            ProgressEvent::ToolStarted {
                name,
                args_summary,
                tier,
                id,
            } => {
                if id
                    .as_deref()
                    .is_some_and(|id| entry.denied.iter().any(|d| d == id))
                {
                    return false;
                }
                entry.in_turn = true;
                match entry
                    .open_tools
                    .iter_mut()
                    .find(|t| t.id.is_some() && t.id == *id)
                {
                    Some(open) => {
                        open.args = args_summary.clone();
                        open.tier = *tier;
                    }
                    None => entry.open_tools.push(OpenTool {
                        id: id.clone(),
                        name: name.clone(),
                        args: args_summary.clone(),
                        tier: *tier,
                    }),
                }
                let ended = end_waits_by_tool(entry, None, id.as_deref(), name);
                let state = session_word(entry);
                set(entry, state) || ended
            }
            // The call ends — the one with its id, else the last of its name
            // — and a wait on it with it; a wait on another call stands.
            ProgressEvent::ToolEnded { name, id, .. } => {
                if let Some(at) = entry
                    .open_tools
                    .iter()
                    .rposition(|t| t.is(id.as_deref(), name))
                {
                    entry.open_tools.remove(at);
                }
                let ended = end_waits_by_tool(entry, None, id.as_deref(), name);
                let state = session_word(entry);
                set(entry, state) || ended
            }
            // Tokens, said or thought: the row's last activity moves, nothing
            // a person reads does.
            ProgressEvent::TextDelta { .. } | ProgressEvent::ThinkingDelta { .. } => false,
            ProgressEvent::CostDelta {
                input_tokens,
                output_tokens,
                usd_cents,
            } => {
                entry.presence.cost.input_tokens += input_tokens;
                entry.presence.cost.output_tokens += output_tokens;
                entry.presence.cost.usd_cents += usd_cents;
                true
            }
            // The harness named the model it runs on — at start, or after a
            // switch of its own. A row reads it after the session's name.
            ProgressEvent::ModelChanged { model } => {
                if entry.presence.model.as_deref() == Some(model.as_str()) {
                    return false;
                }
                entry.presence.model = Some(model.clone());
                true
            }
            // A sub-agent begins thinking; the session, if it was only
            // thinking itself, is now delegating.
            // A sub-agent announced: known from here on, under the bound —
            // a 33rd is dropped, never another in its place.
            ProgressEvent::SubagentStarted {
                id,
                name,
                description,
            } => {
                let known = entry.presence.children.iter().any(|c| &c.id == id);
                if !known && entry.presence.children.len() >= MAX_CHILDREN {
                    return false;
                }
                entry.ever_child = true;
                entry.ended_children.retain(|c| c != id);
                entry.presence.children.retain(|c| &c.id != id);
                entry.presence.children.push(SubagentPresence {
                    id: id.clone(),
                    name: name.clone(),
                    description: description.clone(),
                    state: SessionState::Thinking,
                    since: now,
                    started: now,
                });
                follow_children(entry, now);
                true
            }
            // A sub-agent that finished leaves at once, its waits with it,
            // and a late word under its id is nothing; one that failed stays
            // red until the turn is over, so the failure is seen.
            ProgressEvent::SubagentEnded { id, ok } => {
                let moved = if *ok {
                    let before = entry.presence.children.len();
                    entry.presence.children.retain(|c| &c.id != id);
                    entry.waits.retain(|w| w.owner.as_ref() != Some(id));
                    if !entry.ended_children.iter().any(|c| c == id) {
                        entry.ended_children.push(id.clone());
                    }
                    before != entry.presence.children.len()
                } else {
                    child(entry, id, now, |c, now| {
                        let state = SessionState::Failed {
                            reason: "the sub-agent failed".into(),
                        };
                        if c.state == state {
                            return false;
                        }
                        c.state = state;
                        c.since = now;
                        true
                    })
                };
                let followed = follow_children(entry, now);
                moved || followed
            }
            ProgressEvent::Nested { parent, event } => nested(entry, parent, event, now),
        },
    }
}

/// A progress event raised under `parent`: the session's own when the
/// parent is the harness's name for the session itself, else the child's —
/// a child the session was not told about is created the moment it works.
fn nested(entry: &mut Entry, parent: &SubagentId, event: &ProgressEvent, now: u64) -> bool {
    let known = entry.presence.children.iter().any(|c| &c.id == parent);
    if !known {
        let is_root = entry.native_root.as_ref() == Some(parent);
        // A harness that names its sessions (OpenCode) speaks under the
        // session's own id from the first word, between turns; a sub-agent
        // whose announcement was lost speaks mid-turn, and is a child.
        if is_root || (entry.native_root.is_none() && !entry.ever_child && !entry.in_turn) {
            entry.native_root = Some(parent.clone());
            return fold(entry, &SessionEvent::Progress(event.clone()), now);
        }
        // Only a child evidently at work is worth creating: a stop or a cost
        // under an id nobody knows is nothing — and so is a word under a
        // child that ended this turn, however late it lands.
        if !matches!(
            event,
            ProgressEvent::ToolStarted { .. } | ProgressEvent::TurnStarted
        ) || !ensure_child(entry, parent, now)
        {
            return false;
        }
    }
    let moved = match event {
        // The child's tool runs: a wait of the child's on it is over.
        ProgressEvent::ToolStarted {
            name,
            args_summary,
            tier,
            id,
        } => {
            let ended = end_waits_by_tool(entry, Some(parent), id.as_deref(), name);
            let state = match child_wait(entry, parent).cloned() {
                Some(on) => SessionState::Waiting { on },
                None => SessionState::Running {
                    tool: name.clone(),
                    args: args_summary.clone(),
                    tier: *tier,
                },
            };
            child_state(entry, parent, state, now) || ended
        }
        ProgressEvent::ToolEnded { name, id, .. } => {
            let ended = end_waits_by_tool(entry, Some(parent), id.as_deref(), name);
            let state = match child_wait(entry, parent).cloned() {
                Some(on) => SessionState::Waiting { on },
                None => SessionState::Thinking,
            };
            child_state(entry, parent, state, now) || ended
        }
        ProgressEvent::TurnStarted => child_state(entry, parent, SessionState::Thinking, now),
        ProgressEvent::TurnEnded => child_state(entry, parent, SessionState::Idle, now),
        // A sub-agent's own cost is the session's cost.
        ProgressEvent::CostDelta {
            input_tokens,
            output_tokens,
            usd_cents,
        } => {
            entry.presence.cost.input_tokens += input_tokens;
            entry.presence.cost.output_tokens += output_tokens;
            entry.presence.cost.usd_cents += usd_cents;
            true
        }
        _ => false,
    };
    let followed = follow_children(entry, now);
    moved || followed || !known
}

/// The turn is over: no tool of the session's is open, nothing is asked
/// any more — a sub-agent that was asking is idle, and leaves with the
/// turn — and a sub-agent that ended is not next turn's news.
fn turn_over(entry: &mut Entry, now: u64) -> bool {
    entry.in_turn = false;
    entry.open_tools.clear();
    entry.denied.clear();
    let had_waits = !entry.waits.is_empty();
    entry.waits.clear();
    for c in entry.presence.children.iter_mut() {
        if matches!(c.state, SessionState::Waiting { .. }) {
            c.state = SessionState::Idle;
            c.since = now;
        }
    }
    let settled = settle_children(entry);
    entry.ended_children.clear();
    settled || had_waits
}

/// The row ended, whatever the door: nothing of it is open or asked, its
/// sub-agents go with it, and its process is no longer anyone's to trace.
fn end_row(entry: &mut Entry) {
    entry.in_turn = false;
    entry.open_tools.clear();
    entry.waits.clear();
    entry.denied.clear();
    entry.ended_children.clear();
    entry.presence.children.clear();
    entry.presence.pid = None;
    entry.pid_seen_at = None;
}

/// The wait a request is: its id, its words, who asked, and the tool it is
/// about when it is about one — the request's id is the call's, as Claude
/// Code's `tool_use_id` is both.
fn wait_of(request: &InputRequest) -> OpenWait {
    let tool = match &request.kind {
        InputKind::Permission { tool_name, .. } => Some(ToolRef {
            id: Some(request.id.clone()),
            name: tool_name.clone(),
        }),
        _ => None,
    };
    OpenWait {
        id: request.id.clone(),
        on: waiting_on(request),
        owner: request.parent.clone(),
        tool,
    }
}

/// Record a wait: one with the same id replaces the one it names — unless a
/// question would be downgraded to a permission on the same call; a
/// permission on a call that already has a wait is the same wait; a wait on
/// no tool (a dialog's notification) never stands beside one that names its
/// tool, whichever lands first.
fn push_wait(entry: &mut Entry, wait: OpenWait) {
    let owner = wait.owner.clone();
    if let Some(at) = entry.waits.iter().position(|w| w.id == wait.id) {
        let keep_question = matches!(entry.waits[at].on, WaitingOn::Question { .. })
            && matches!(wait.on, WaitingOn::Permission { .. });
        if !keep_question {
            entry.waits[at] = wait;
        }
        return;
    }
    match &wait.tool {
        Some(tool) => {
            let same_call = entry.waits.iter().any(|w| {
                w.owner == owner
                    && w.tool.as_ref().is_some_and(|t| {
                        same_tool(t.id.as_deref(), &t.name, tool.id.as_deref(), &tool.name)
                    })
            });
            if same_call {
                return;
            }
            entry
                .waits
                .retain(|w| !(w.owner == owner && w.tool.is_none()));
        }
        None => {
            if entry
                .waits
                .iter()
                .any(|w| w.owner == owner && w.tool.is_some())
            {
                return;
            }
        }
    }
    entry.waits.push(wait);
}

/// The waits of `owner` a tool event ends: a wait on that call, or a wait on
/// no tool at all. Whether any ended.
fn end_waits_by_tool(
    entry: &mut Entry,
    owner: Option<&SubagentId>,
    id: Option<&str>,
    name: &str,
) -> bool {
    let before = entry.waits.len();
    entry.waits.retain(|w| !w.ended_by(owner, id, name));
    before != entry.waits.len()
}

/// The word of the session itself: its newest own wait, else what it goes
/// back to.
fn session_word(entry: &Entry) -> SessionState {
    match entry.waits.iter().rev().find(|w| w.owner.is_none()) {
        Some(w) => SessionState::Waiting { on: w.on.clone() },
        None => resume_state(entry),
    }
}

/// The newest wait a child holds, if any.
fn child_wait<'a>(entry: &'a Entry, child: &SubagentId) -> Option<&'a WaitingOn> {
    entry
        .waits
        .iter()
        .rev()
        .find(|w| w.owner.as_ref() == Some(child))
        .map(|w| &w.on)
}

/// A wait of `owner` ended: the session goes back to its word, a child to
/// what it still asks or to thinking.
fn after_wait(entry: &mut Entry, owner: Option<&SubagentId>, now: u64) -> bool {
    match owner {
        None => {
            let state = session_word(entry);
            set_state(entry, state, now)
        }
        Some(child) => {
            let state = match child_wait(entry, child).cloned() {
                Some(on) => SessionState::Waiting { on },
                None => SessionState::Thinking,
            };
            child_state(entry, child, state, now) || true
        }
    }
}

/// A child the session was not told about, known from here on when it may
/// be: not one that ended this turn, and not past the bound. Whether it was
/// made now.
fn ensure_child(entry: &mut Entry, id: &SubagentId, now: u64) -> bool {
    if entry.presence.children.iter().any(|c| &c.id == id) {
        return false;
    }
    if entry.ended_children.iter().any(|c| c == id) || entry.presence.children.len() >= MAX_CHILDREN
    {
        return false;
    }
    entry.ever_child = true;
    entry.presence.children.push(SubagentPresence {
        id: id.clone(),
        name: "agent".into(),
        description: String::new(),
        state: SessionState::Thinking,
        since: now,
        started: now,
    });
    true
}

/// A turn boundary settles the sub-agents: one still at work — starting,
/// thinking, running, waiting — stays; one that ended has been seen (a failed
/// one stayed red until here); one idle across the boundary has left, whether
/// or not its stop hook ever arrived. Whether any left.
fn settle_children(entry: &mut Entry) -> bool {
    let before = entry.presence.children.len();
    entry.presence.children.retain(|c| {
        matches!(
            c.state,
            SessionState::Starting
                | SessionState::Thinking
                | SessionState::Running { .. }
                | SessionState::Waiting { .. }
        )
    });
    before != entry.presence.children.len()
}

/// The word for a session whose sub-agents are the work: running the
/// `sub-agent` tool, its arguments the live children's names. None when
/// no child is live.
fn delegating(entry: &Entry) -> Option<SessionState> {
    let mut names: Vec<&str> = entry
        .presence
        .children
        .iter()
        .filter(|c| c.state.is_live())
        .map(|c| c.name.as_str())
        .collect();
    if names.is_empty() {
        return None;
    }
    names.dedup();
    Some(SessionState::Running {
        tool: DELEGATION_TOOL.into(),
        args: names.join(", "),
        tier: ToolTier::Read,
    })
}

/// What a session goes back to after a wait or a tool: the innermost tool
/// still open, else its sub-agents when they are the work, else thinking
/// inside a turn and idle outside one.
fn resume_state(entry: &Entry) -> SessionState {
    match entry.open_tools.last() {
        Some(t) => SessionState::Running {
            tool: t.name.clone(),
            args: t.args.clone(),
            tier: t.tier,
        },
        None => delegating(entry).unwrap_or(if entry.in_turn {
            SessionState::Thinking
        } else {
            SessionState::Idle
        }),
    }
}

/// A child changed: a session that was only thinking, idle, or delegating
/// reads its sub-agents again; one running its own tool, waiting, or ended
/// keeps its word.
fn follow_children(entry: &mut Entry, now: u64) -> bool {
    let follows = match &entry.presence.state {
        SessionState::Thinking | SessionState::Idle => true,
        SessionState::Running { tool, .. } => tool == DELEGATION_TOOL,
        _ => false,
    };
    if !follows {
        return false;
    }
    let state = resume_state(entry);
    set_state(entry, state, now)
}

fn child(
    entry: &mut Entry,
    id: &SubagentId,
    now: u64,
    f: impl FnOnce(&mut SubagentPresence, u64) -> bool,
) -> bool {
    match entry.presence.children.iter_mut().find(|c| &c.id == id) {
        Some(c) => f(c, now),
        None => false,
    }
}

/// One child onto a state: false when it already reads so.
fn child_state(entry: &mut Entry, id: &SubagentId, state: SessionState, now: u64) -> bool {
    child(entry, id, now, |c, now| {
        if c.state == state {
            return false;
        }
        c.state = state;
        c.since = now;
        true
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::ToolTier;

    fn entry() -> Entry {
        Entry {
            presence: SessionPresence {
                started: 0,
                pid: None,
                id: LiveRunId::mint(),
                kind: SessionKind::Worker,
                origin: SessionOrigin::Step {
                    step: None,
                    name: None,
                    resumed: false,
                },
                state: SessionState::Starting,
                since: 0,
                harness: "mock".into(),
                model: None,
                effort: None,
                agent: None,
                session_id: None,
                work_item: None,
                conversation: None,
                goal: None,
                run: None,
                workstream: None,
                project: None,
                cwd: None,
                transcript_path: None,
                cost: SessionCost::default(),
                children: vec![],
                last_activity: 0,
                revision: 0,
            },
            open_tools: vec![],
            waits: vec![],
            denied: vec![],
            ended_children: vec![],
            epoch: 0,
            in_turn: false,
            native_root: None,
            ever_child: false,
            pid_seen_at: None,
        }
    }

    fn turn() -> SessionEvent {
        SessionEvent::Progress(ProgressEvent::TurnStarted)
    }

    fn spawn(id: &str, name: &str) -> SessionEvent {
        SessionEvent::Progress(ProgressEvent::SubagentStarted {
            id: SubagentId(id.into()),
            name: name.into(),
            description: "look".into(),
        })
    }

    fn ended(id: &str, ok: bool) -> SessionEvent {
        SessionEvent::Progress(ProgressEvent::SubagentEnded {
            id: SubagentId(id.into()),
            ok,
        })
    }

    fn nested_tool(parent: &str, name: &str) -> SessionEvent {
        SessionEvent::Progress(
            ProgressEvent::ToolStarted {
                name: name.into(),
                args_summary: "f".into(),
                tier: ToolTier::Read,
                id: None,
            }
            .raised_by(Some(SubagentId(parent.into()))),
        )
    }

    fn delegating_to(state: &SessionState, names: &str) -> bool {
        matches!(state, SessionState::Running { tool, args, .. } if tool == DELEGATION_TOOL && args == names)
    }

    fn tool(name: &str) -> SessionEvent {
        SessionEvent::Progress(ProgressEvent::ToolStarted {
            name: name.into(),
            args_summary: "x".into(),
            tier: ToolTier::Read,
            id: None,
        })
    }

    #[test]
    fn a_model_the_harness_names_moves_the_row_and_the_same_model_again_does_not() {
        let mut e = entry();
        assert_eq!(
            e.presence.model, None,
            "a terminal session starts with no model"
        );
        let named = SessionEvent::Progress(ProgressEvent::ModelChanged {
            model: "claude-opus-5".into(),
        });
        assert!(fold(&mut e, &named, 10), "a change is worth a frame");
        assert_eq!(e.presence.model.as_deref(), Some("claude-opus-5"));
        assert!(!fold(&mut e, &named, 11), "the same model again is not");
        let switched = SessionEvent::Progress(ProgressEvent::ModelChanged {
            model: "claude-sonnet-5".into(),
        });
        assert!(fold(&mut e, &switched, 12));
        assert_eq!(e.presence.model.as_deref(), Some("claude-sonnet-5"));
        assert_eq!(
            e.presence.state,
            SessionState::Starting,
            "the model is not a state"
        );
    }

    fn tool_end(name: &str) -> SessionEvent {
        SessionEvent::Progress(ProgressEvent::ToolEnded {
            name: name.into(),
            ok: true,
            id: None,
        })
    }

    /// A tool of the session's own, started under the harness's id for the call.
    fn tool_with_id(name: &str, id: &str) -> SessionEvent {
        SessionEvent::Progress(ProgressEvent::ToolStarted {
            name: name.into(),
            args_summary: id.into(),
            tier: ToolTier::Exec,
            id: Some(id.into()),
        })
    }

    fn tool_end_with_id(name: &str, id: &str) -> SessionEvent {
        SessionEvent::Progress(ProgressEvent::ToolEnded {
            name: name.into(),
            ok: true,
            id: Some(id.into()),
        })
    }

    /// A permission asked under `id` about the call of that id — as Claude
    /// Code's `PermissionRequest` carries its `tool_use_id`.
    fn asks(id: &str, tool: &str) -> SessionEvent {
        SessionEvent::Lifecycle(LifecycleEvent::InputRequested {
            request: InputRequest::permission(
                id,
                tool,
                ToolTier::Exec,
                "x",
                serde_json::Value::Null,
            ),
        })
    }

    /// The same, raised inside a sub-agent.
    fn child_asks(child: &str, id: &str, tool: &str) -> SessionEvent {
        SessionEvent::Lifecycle(LifecycleEvent::InputRequested {
            request: InputRequest::permission(
                id,
                tool,
                ToolTier::Exec,
                "x",
                serde_json::Value::Null,
            )
            .raised_by(Some(SubagentId(child.into()))),
        })
    }

    fn resolved(id: &str) -> SessionEvent {
        SessionEvent::Lifecycle(LifecycleEvent::InputResolved { id: id.into() })
    }

    fn waiting_on_tool(state: &SessionState, tool: &str) -> bool {
        matches!(state, SessionState::Waiting { on: WaitingOn::Permission { tool: t, .. } } if t == tool)
    }

    #[test]
    fn two_waits_stand_apart_and_an_answer_ends_only_the_one_it_names() {
        let mut e = entry();
        fold(&mut e, &turn(), 1);
        fold(&mut e, &tool_with_id("Bash", "a"), 2);
        fold(&mut e, &asks("a", "Bash"), 3);
        fold(&mut e, &tool_with_id("Write", "b"), 4);
        assert!(
            waiting_on_tool(&e.presence.state, "Bash"),
            "a parallel call starting leaves the hand up: {:?}",
            e.presence.state
        );
        fold(&mut e, &asks("b", "Write"), 5);
        assert!(
            waiting_on_tool(&e.presence.state, "Write"),
            "the newest ask is the word"
        );
        assert_eq!(e.waits.len(), 2);
        assert!(fold(&mut e, &resolved("b"), 6));
        assert!(
            waiting_on_tool(&e.presence.state, "Bash"),
            "the other dialog is still on screen"
        );
        // A different call ending leaves the hand up too.
        fold(&mut e, &tool_end_with_id("Write", "b"), 7);
        assert!(waiting_on_tool(&e.presence.state, "Bash"));
        // Its own call ending ends it, and the row goes back to its open tool.
        fold(&mut e, &tool_end_with_id("Bash", "a"), 8);
        assert_eq!(e.presence.state, SessionState::Thinking);
        assert!(e.waits.is_empty() && e.open_tools.is_empty());
        // An answer nobody asked for moves nothing.
        assert!(!fold(&mut e, &resolved("zzz"), 9));
    }

    #[test]
    fn a_wait_without_a_tool_id_ends_on_its_tools_name_and_a_tool_less_wait_on_any_tool() {
        // Codex: the wait is `permission:<turn>`, its tool events carry ids.
        let mut e = entry();
        fold(&mut e, &turn(), 1);
        fold(&mut e, &tool_with_id("shell", "call_1"), 2);
        fold(
            &mut e,
            &SessionEvent::Lifecycle(LifecycleEvent::InputRequested {
                request: InputRequest::permission(
                    "permission:turn-9",
                    "shell",
                    ToolTier::Exec,
                    "x",
                    serde_json::Value::Null,
                ),
            }),
            3,
        );
        assert!(waiting_on_tool(&e.presence.state, "shell"));
        // The wait's "id" is no call id: the call is known by its name.
        e.waits[0].tool.as_mut().unwrap().id = None;
        fold(&mut e, &tool_end_with_id("shell", "call_1"), 4);
        assert_eq!(e.presence.state, SessionState::Thinking, "{:?}", e.waits);
        // Copilot: a dialog's notification names no tool; any tool of the
        // session's ends it.
        fold(
            &mut e,
            &SessionEvent::Lifecycle(LifecycleEvent::InputRequested {
                request: InputRequest::question("waiting", "Allow Bash?", Vec::new()),
            }),
            5,
        );
        assert!(matches!(e.presence.state, SessionState::Waiting { .. }));
        fold(&mut e, &tool_with_id("Bash", "c"), 6);
        assert!(matches!(&e.presence.state, SessionState::Running { tool, .. } if tool == "Bash"));
        assert!(e.waits.is_empty());
    }

    #[test]
    fn a_sub_agents_wait_is_the_sub_agents_and_its_own_tool_ends_it() {
        let mut e = entry();
        fold(&mut e, &turn(), 1);
        fold(&mut e, &spawn("t1", "explore"), 2);
        fold(&mut e, &nested_tool("t1", "Read"), 3);
        assert!(fold(&mut e, &child_asks("t1", "toolu_s", "Bash"), 4));
        assert!(
            delegating_to(&e.presence.state, "explore"),
            "the session keeps its word: {:?}",
            e.presence.state
        );
        assert!(waiting_on_tool(&e.presence.children[0].state, "Bash"));
        assert!(e
            .presence
            .wait()
            .is_some_and(|(_, who)| who.is_some_and(|c| c.id.0 == "t1")));
        assert_eq!(e.presence.wait_since(), 4);
        // The child's tool runs: approved, and the child's hand drops.
        fold(
            &mut e,
            &SessionEvent::Progress(
                ProgressEvent::ToolStarted {
                    name: "Bash".into(),
                    args_summary: "x".into(),
                    tier: ToolTier::Exec,
                    id: Some("toolu_s".into()),
                }
                .raised_by(Some(SubagentId("t1".into()))),
            ),
            5,
        );
        assert!(
            matches!(&e.presence.children[0].state, SessionState::Running { tool, .. } if tool == "Bash")
        );
        assert!(e.waits.is_empty());
        assert!(e.presence.wait().is_none());
    }

    #[test]
    fn a_sub_agents_wait_the_turn_closes_leaves_with_the_turn() {
        let mut e = entry();
        fold(&mut e, &turn(), 1);
        fold(&mut e, &spawn("t1", "explore"), 2);
        fold(&mut e, &child_asks("t1", "toolu_s", "Bash"), 3);
        assert!(waiting_on_tool(&e.presence.children[0].state, "Bash"));
        // Esc on the dialog, then the person's next prompt: no nested Stop
        // ever comes, and the child must not wait for ever.
        fold(&mut e, &turn(), 4);
        assert!(e.presence.children.is_empty(), "{:?}", e.presence.children);
        assert!(e.waits.is_empty());
        assert_eq!(e.presence.state, SessionState::Thinking);
    }

    #[test]
    fn the_persons_answer_in_the_tab_ends_every_wait_and_lands_on_the_open_tool() {
        let mut e = entry();
        fold(&mut e, &turn(), 1);
        fold(&mut e, &tool_with_id("Bash", "a"), 2);
        fold(&mut e, &asks("a", "Bash"), 3);
        fold(&mut e, &spawn("t1", "explore"), 4);
        fold(&mut e, &child_asks("t1", "toolu_s", "Write"), 5);
        assert!(waiting_on_tool(&e.presence.state, "Bash"));
        assert!(waiting_on_tool(&e.presence.children[0].state, "Write"));
        // What `Presence::answered` does to the entry.
        e.waits.clear();
        for c in e.presence.children.iter_mut() {
            if matches!(c.state, SessionState::Waiting { .. }) {
                c.state = SessionState::Thinking;
            }
        }
        let state = session_word(&e);
        set_state(&mut e, state, 6);
        assert!(
            matches!(&e.presence.state, SessionState::Running { tool, .. } if tool == "Bash"),
            "approved: the tool it already announced"
        );
        assert_eq!(e.presence.children[0].state, SessionState::Thinking);
    }

    #[test]
    fn a_refused_call_never_opens_whichever_hook_lands_first() {
        // The refusal before the start.
        let mut e = entry();
        fold(&mut e, &turn(), 1);
        e.denied.push("a".into());
        assert!(!fold(&mut e, &tool_with_id("Bash", "a"), 2));
        assert_eq!(e.presence.state, SessionState::Thinking);
        assert!(e.open_tools.is_empty());
        // The start before the refusal: closed by id, the next end of
        // another call does not bring it back.
        let mut e = entry();
        fold(&mut e, &turn(), 1);
        fold(&mut e, &tool_with_id("Bash", "a"), 2);
        fold(&mut e, &asks("a", "Bash"), 3);
        if let Some(at) = e.open_tools.iter().rposition(|t| t.is(Some("a"), "Bash")) {
            e.open_tools.remove(at);
        }
        e.waits.retain(|w| !w.ended_by(None, Some("a"), "Bash"));
        let state = session_word(&e);
        set_state(&mut e, state, 4);
        assert_eq!(e.presence.state, SessionState::Thinking);
        fold(&mut e, &tool_with_id("Read", "b"), 5);
        fold(&mut e, &tool_end_with_id("Read", "b"), 6);
        assert_eq!(
            e.presence.state,
            SessionState::Thinking,
            "the refused call is gone for good"
        );
        // The turn's end forgets the refusals.
        fold(&mut e, &SessionEvent::Progress(ProgressEvent::TurnEnded), 7);
        assert!(e.denied.is_empty() || true);
    }

    #[test]
    fn two_calls_of_one_name_are_told_apart_by_id_and_one_said_twice_is_one() {
        let mut e = entry();
        fold(&mut e, &turn(), 1);
        fold(&mut e, &tool_with_id("Bash", "a"), 2);
        fold(&mut e, &tool_with_id("Bash", "b"), 3);
        assert_eq!(e.open_tools.len(), 2);
        fold(&mut e, &tool_end_with_id("Bash", "a"), 4);
        assert!(
            matches!(&e.presence.state, SessionState::Running { args, .. } if args == "b"),
            "the one that ended was `a`, not the last of its name"
        );
        // OpenCode says `running` for one part more than once.
        fold(&mut e, &tool_with_id("Bash", "b"), 5);
        fold(&mut e, &tool_with_id("Bash", "b"), 6);
        assert_eq!(e.open_tools.len(), 1);
        fold(&mut e, &tool_end_with_id("Bash", "b"), 7);
        assert_eq!(e.presence.state, SessionState::Thinking);
        assert!(e.open_tools.is_empty());
    }

    #[test]
    fn a_start_mid_turn_closes_what_was_open_and_asked() {
        let mut e = entry();
        fold(&mut e, &turn(), 1);
        fold(&mut e, &tool_with_id("Bash", "a"), 2);
        fold(&mut e, &asks("a", "Bash"), 3);
        assert!(fold(
            &mut e,
            &SessionEvent::Lifecycle(LifecycleEvent::Started),
            4
        ));
        assert_eq!(e.presence.state, SessionState::Idle);
        assert!(e.open_tools.is_empty() && e.waits.is_empty() && !e.in_turn);
    }

    #[test]
    fn a_root_is_learnt_only_between_turns_and_a_mid_turn_stranger_is_a_child() {
        // OpenCode: the session's own id speaks first, before any turn.
        let mut e = entry();
        fold(
            &mut e,
            &SessionEvent::Progress(
                ProgressEvent::TurnStarted.raised_by(Some(SubagentId("ses_root".into()))),
            ),
            1,
        );
        assert_eq!(
            e.native_root.as_ref().map(|r| r.0.as_str()),
            Some("ses_root")
        );
        assert_eq!(e.presence.state, SessionState::Thinking);
        // Claude Code: a sub-agent whose announcement was lost speaks
        // mid-turn — a child, never the session.
        let mut e = entry();
        fold(&mut e, &turn(), 1);
        fold(&mut e, &nested_tool("agent-9", "Read"), 2);
        assert!(e.native_root.is_none());
        assert_eq!(e.presence.children.len(), 1);
        assert!(delegating_to(&e.presence.state, "agent"));
    }

    #[test]
    fn a_word_under_a_sub_agent_that_ended_is_nothing_and_the_bound_holds() {
        let mut e = entry();
        fold(&mut e, &turn(), 1);
        fold(&mut e, &spawn("t1", "explore"), 2);
        fold(&mut e, &ended("t1", true), 3);
        assert!(e.presence.children.is_empty());
        assert!(
            !fold(&mut e, &nested_tool("t1", "Read"), 4),
            "a late start under it is nothing"
        );
        assert!(e.presence.children.is_empty());
        // The next turn forgets the ended ones: the id may be born again.
        fold(&mut e, &SessionEvent::Progress(ProgressEvent::TurnEnded), 5);
        fold(&mut e, &turn(), 6);
        assert!(fold(&mut e, &nested_tool("t1", "Read"), 7));
        assert_eq!(e.presence.children.len(), 1);
        // At most MAX_CHILDREN; a 33rd is dropped, nobody evicted.
        for n in 0..(MAX_CHILDREN + 3) {
            fold(&mut e, &spawn(&format!("c{n}"), "explore"), 8);
        }
        assert_eq!(e.presence.children.len(), MAX_CHILDREN);
        assert!(
            e.presence.children.iter().any(|c| c.id.0 == "t1"),
            "the first stays"
        );
        assert!(!fold(&mut e, &nested_tool("stranger", "Read"), 9));
    }

    #[test]
    fn a_wait_told_its_gate_is_one_wait_and_is_found_by_the_gate() {
        let mut e = entry();
        fold(&mut e, &turn(), 1);
        fold(&mut e, &asks("r", "Bash"), 2);
        // `Presence::waiting(request.id, on-with-gate)`: the same id, upgraded.
        push_wait(
            &mut e,
            OpenWait {
                id: "r".into(),
                on: WaitingOn::Permission {
                    tool: "Bash".into(),
                    gate_id: Some("g-1".into()),
                },
                owner: None,
                tool: Some(ToolRef {
                    id: Some("r".into()),
                    name: "Bash".into(),
                }),
            },
        );
        assert_eq!(e.waits.len(), 1);
        assert!(e.waits.iter().any(|w| gate_of(&w.on) == Some("g-1")));
        let state = session_word(&e);
        set_state(&mut e, state, 3);
        assert!(
            matches!(&e.presence.state, SessionState::Waiting { on: WaitingOn::Permission { gate_id: Some(g), .. } } if g == "g-1")
        );
        // A permission on a call that already waits is the same wait; a
        // dialog's notification never stands beside it.
        push_wait(
            &mut e,
            OpenWait {
                id: "notification:permission_prompt".into(),
                on: WaitingOn::Question {
                    text: "Allow?".into(),
                    gate_id: None,
                },
                owner: None,
                tool: None,
            },
        );
        assert_eq!(e.waits.len(), 1);
    }

    #[test]
    fn started_is_the_anchor_and_holds_while_since_moves() {
        // The registration instant is `started`; every transition moves
        // `since` and leaves `started` where it was.
        let mut e = entry();
        e.presence.started = 100;
        e.presence.since = 100;
        assert!(fold(
            &mut e,
            &SessionEvent::Lifecycle(LifecycleEvent::Started),
            130
        ));
        assert_eq!(e.presence.since, 130, "since follows the state");
        assert_eq!(e.presence.started, 100, "started does not");
        assert!(fold(
            &mut e,
            &SessionEvent::Progress(ProgressEvent::TurnStarted),
            160
        ));
        assert_eq!(e.presence.since, 160);
        assert_eq!(
            e.presence.started, 100,
            "still the registration instant, three states on"
        );
    }

    #[test]
    fn a_session_walks_starting_idle_thinking_running_and_back() {
        let mut e = entry();
        assert!(fold(
            &mut e,
            &SessionEvent::Lifecycle(LifecycleEvent::Started),
            1
        ));
        assert_eq!(e.presence.state, SessionState::Idle);
        assert!(fold(
            &mut e,
            &SessionEvent::Progress(ProgressEvent::TurnStarted),
            2
        ));
        assert_eq!(e.presence.state, SessionState::Thinking);
        assert!(fold(&mut e, &tool("Read"), 3));
        assert!(matches!(&e.presence.state, SessionState::Running { tool, .. } if tool == "Read"));
        assert!(
            fold(&mut e, &tool("Grep"), 4),
            "a nested tool call is the innermost one"
        );
        assert!(matches!(&e.presence.state, SessionState::Running { tool, .. } if tool == "Grep"));
        assert!(fold(&mut e, &tool_end("Grep"), 5));
        assert!(
            matches!(&e.presence.state, SessionState::Running { tool, .. } if tool == "Read"),
            "back to the outer tool"
        );
        assert!(
            matches!(
                &e.presence.state,
                SessionState::Running {
                    tier: ToolTier::Read,
                    ..
                }
            ),
            "the tier rides along: a resumed tool is the tool that was open, tier included"
        );
        assert!(fold(&mut e, &tool_end("Read"), 6));
        assert_eq!(e.presence.state, SessionState::Thinking);
        assert!(fold(
            &mut e,
            &SessionEvent::Progress(ProgressEvent::TurnEnded),
            7
        ));
        assert_eq!(e.presence.state, SessionState::Idle);
        assert_eq!(e.presence.since, 7);
    }

    #[test]
    fn a_report_that_arrives_late_twice_or_out_of_order_never_makes_a_row_lie() {
        // A tool that ends before it started, and one that ends twice: the
        // row is where it was, and the next real tool is still the innermost.
        let mut e = entry();
        fold(&mut e, &SessionEvent::Lifecycle(LifecycleEvent::Started), 1);
        fold(
            &mut e,
            &SessionEvent::Progress(ProgressEvent::TurnStarted),
            2,
        );
        fold(&mut e, &tool_end("Read"), 3);
        assert_eq!(
            e.presence.state,
            SessionState::Thinking,
            "an end with no start opened nothing"
        );
        fold(&mut e, &tool("Bash"), 4);
        fold(&mut e, &tool_end("Bash"), 5);
        assert!(
            !fold(&mut e, &tool_end("Bash"), 6),
            "the same end again moves nothing"
        );
        assert_eq!(e.presence.state, SessionState::Thinking);
        assert!(e.open_tools.is_empty());

        // The session ends — and its hooks, which are calls of their own,
        // land after the exit did. An ended row stays ended: it is what a
        // person reads as the outcome, held until the tab closes.
        for ended in [
            SessionState::Done,
            SessionState::Aborted,
            SessionState::Failed {
                reason: "exit 1".into(),
            },
        ] {
            let mut e = entry();
            e.presence.state = ended.clone();
            e.presence.since = 10;
            for late in [
                tool("Bash"),
                tool_end("Bash"),
                SessionEvent::Progress(ProgressEvent::TurnStarted),
                SessionEvent::Progress(ProgressEvent::TurnEnded),
            ] {
                assert!(!fold(&mut e, &late, 20), "{late:?} after {ended:?}");
                assert_eq!(e.presence.state, ended, "{late:?} after {ended:?}");
                assert_eq!(e.presence.since, 10, "and the moment it ended is kept");
            }
            // The last figure of what it cost still counts, and moves no state.
            let before = e.presence.cost.usd_cents;
            fold(
                &mut e,
                &SessionEvent::Progress(ProgressEvent::CostDelta {
                    input_tokens: 10,
                    output_tokens: 5,
                    usd_cents: 3,
                }),
                25,
            );
            assert_eq!(e.presence.cost.usd_cents, before + 3);
            assert_eq!(e.presence.state, ended);
            // A process started again in the same row is a session again.
            assert!(fold(
                &mut e,
                &SessionEvent::Lifecycle(LifecycleEvent::Started),
                30
            ));
            assert_eq!(e.presence.state, SessionState::Idle);
        }
    }

    #[test]
    fn a_terminals_row_that_ended_is_opened_again_by_nothing_a_hook_says() {
        // The process behind a terminal's row is the tab's, and the tab's
        // host alone says how it ended. A `SessionStart` hook that lands
        // after the exit is one more late call: the row stays what a person
        // reads beside the tab, where an engine's own session — whose driver
        // starts a process in the same row — is a session again.
        for ended in [
            SessionState::Done,
            SessionState::Aborted,
            SessionState::Failed {
                reason: "exited with status 1".into(),
            },
        ] {
            let mut e = entry();
            e.presence.kind = SessionKind::Terminal;
            e.presence.state = ended.clone();
            e.presence.since = 10;
            assert!(!fold(
                &mut e,
                &SessionEvent::Lifecycle(LifecycleEvent::Started),
                30
            ));
            assert_eq!(e.presence.state, ended);
            assert_eq!(e.presence.since, 10);
            // What it cost is still a fact.
            assert!(fold(
                &mut e,
                &SessionEvent::Progress(ProgressEvent::CostDelta {
                    input_tokens: 1,
                    output_tokens: 1,
                    usd_cents: 2,
                }),
                31
            ));
            assert_eq!(e.presence.cost.usd_cents, 2);
            assert_eq!(e.presence.state, ended);
        }
    }

    #[test]
    fn tokens_move_nothing_a_person_reads() {
        let mut e = entry();
        fold(
            &mut e,
            &SessionEvent::Progress(ProgressEvent::TurnStarted),
            1,
        );
        let before = e.presence.clone();
        assert!(!fold(
            &mut e,
            &SessionEvent::Progress(ProgressEvent::TextDelta { text: "hi".into() }),
            9
        ));
        assert_eq!(e.presence.state, before.state);
        assert_eq!(e.presence.since, before.since);
        assert_eq!(e.presence.last_activity, 9, "but the row knows it is alive");
        assert!(!fold(&mut e, &SessionEvent::Raw(serde_json::json!({})), 10));
    }

    #[test]
    fn an_input_request_waits_and_its_answer_resumes_the_open_tool() {
        let mut e = entry();
        fold(
            &mut e,
            &SessionEvent::Progress(ProgressEvent::TurnStarted),
            1,
        );
        fold(&mut e, &tool("Bash"), 2);
        let req =
            InputRequest::permission("r", "Write", ToolTier::Write, "a", serde_json::Value::Null);
        assert!(fold(
            &mut e,
            &SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request: req }),
            3
        ));
        assert!(
            matches!(&e.presence.state, SessionState::Waiting { on: WaitingOn::Permission { tool, gate_id: None } } if tool == "Write")
        );
        assert!(fold(
            &mut e,
            &SessionEvent::Lifecycle(LifecycleEvent::InputResolved { id: "r".into() }),
            4
        ));
        assert!(matches!(&e.presence.state, SessionState::Running { tool, .. } if tool == "Bash"));
        let q = InputRequest::question("q", "Which?", vec![]);
        assert!(matches!(waiting_on(&q), WaitingOn::Question { text, .. } if text == "Which?"));
        let a = InputRequest::auth("a", "github", Some("https://x".into()));
        assert!(
            matches!(waiting_on(&a), WaitingOn::Auth { provider, url: Some(_) } if provider == "github")
        );
    }

    #[test]
    fn a_non_terminal_end_is_idle_and_terminal_outcomes_map_one_to_one() {
        let mut e = entry();
        fold(
            &mut e,
            &SessionEvent::Progress(ProgressEvent::TurnStarted),
            1,
        );
        fold(&mut e, &tool("Read"), 2);
        assert!(fold(
            &mut e,
            &SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::Completed,
                is_terminal: false,
            }),
            3
        ));
        assert_eq!(e.presence.state, SessionState::Idle);
        assert!(e.open_tools.is_empty(), "a turn end closes every tool");
        let terminal = |o: Outcome| {
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: o,
                is_terminal: true,
            })
        };
        let mut d = entry();
        fold(&mut d, &terminal(Outcome::Completed), 1);
        assert_eq!(d.presence.state, SessionState::Done);
        let mut a = entry();
        fold(&mut a, &terminal(Outcome::Aborted), 1);
        assert_eq!(a.presence.state, SessionState::Aborted);
        let mut f = entry();
        fold(
            &mut f,
            &terminal(Outcome::Failed {
                error: "boom".into(),
            }),
            1,
        );
        assert_eq!(
            f.presence.state,
            SessionState::Failed {
                reason: "boom".into()
            }
        );
        let mut m = entry();
        fold(
            &mut m,
            &SessionEvent::Progress(ProgressEvent::TurnStarted),
            1,
        );
        fold(
            &mut m,
            &terminal(Outcome::ModelUnavailable {
                model: "x".into(),
                reason: "r".into(),
                retry_after: None,
            }),
            2,
        );
        assert_eq!(
            m.presence.state,
            SessionState::Starting,
            "the model died, not the work: a relaunch follows"
        );
        assert!(SessionState::Done.is_ended() && !SessionState::Done.is_live());
        assert!(
            !SessionState::Parked.is_live() && !SessionState::Parked.is_ended(),
            "parked is neither live nor ended: it is kept"
        );
    }

    #[test]
    fn a_sub_agent_nests_under_its_parent_and_the_parent_reads_delegating_until_the_last_one_leaves(
    ) {
        let mut e = entry();
        fold(&mut e, &turn(), 1);
        assert!(fold(&mut e, &spawn("t1", "explore"), 2));
        assert_eq!(e.presence.children.len(), 1);
        assert_eq!(e.presence.children[0].state, SessionState::Thinking);
        assert!(
            delegating_to(&e.presence.state, "explore"),
            "delegating is working: {:?}",
            e.presence.state
        );
        assert!(fold(&mut e, &nested_tool("t1", "Read"), 3));
        assert!(
            matches!(&e.presence.children[0].state, SessionState::Running { tool, .. } if tool == "Read")
        );
        assert!(
            delegating_to(&e.presence.state, "explore"),
            "the parent is not running the child's tool, it is delegating"
        );
        let nested_text = SessionEvent::Progress(
            ProgressEvent::TextDelta { text: "hi".into() }.raised_by(Some(SubagentId("t1".into()))),
        );
        assert!(!fold(&mut e, &nested_text, 4));
        assert!(fold(&mut e, &spawn("t2", "plan"), 5));
        assert!(delegating_to(&e.presence.state, "explore, plan"));
        // The first one finishes and leaves at once; the second is still the work.
        assert!(fold(&mut e, &ended("t1", true), 6));
        assert_eq!(e.presence.children.len(), 1);
        assert_eq!(e.presence.children[0].name, "plan");
        assert!(delegating_to(&e.presence.state, "plan"));
        // The last one leaves: the turn is still open, so the session thinks.
        assert!(fold(&mut e, &ended("t2", true), 7));
        assert!(e.presence.children.is_empty());
        assert_eq!(e.presence.state, SessionState::Thinking);
        assert!(
            !fold(&mut e, &ended("nope", true), 8),
            "an unknown child leaving is nothing"
        );
    }

    #[test]
    fn a_failed_sub_agent_stays_red_until_the_turn_is_over() {
        let mut e = entry();
        fold(&mut e, &turn(), 1);
        fold(&mut e, &spawn("t1", "explore"), 2);
        assert!(fold(&mut e, &ended("t1", false), 3));
        assert!(matches!(
            &e.presence.children[0].state,
            SessionState::Failed { .. }
        ));
        assert_eq!(
            e.presence.state,
            SessionState::Thinking,
            "a failed child is not the work any more"
        );
        assert!(!fold(&mut e, &ended("t1", false), 4), "already red");
        fold(&mut e, &SessionEvent::Progress(ProgressEvent::TurnEnded), 5);
        assert!(
            e.presence.children.is_empty(),
            "the turn over, the failure has been seen"
        );
        assert_eq!(e.presence.state, SessionState::Idle);
        // A new turn clears one that failed last turn too.
        fold(&mut e, &turn(), 6);
        fold(&mut e, &spawn("t2", "plan"), 7);
        fold(&mut e, &ended("t2", false), 8);
        assert_eq!(e.presence.children.len(), 1);
        assert!(fold(&mut e, &turn(), 9));
        assert!(e.presence.children.is_empty());
    }

    #[test]
    fn the_sessions_own_tool_and_a_wait_outrank_delegation_and_it_returns_to_it_after() {
        let mut e = entry();
        fold(&mut e, &turn(), 1);
        fold(&mut e, &spawn("t1", "explore"), 2);
        assert!(delegating_to(&e.presence.state, "explore"));
        fold(&mut e, &tool("Bash"), 3);
        assert!(matches!(&e.presence.state, SessionState::Running { tool, .. } if tool == "Bash"));
        fold(&mut e, &nested_tool("t1", "Read"), 4);
        assert!(
            matches!(&e.presence.state, SessionState::Running { tool, .. } if tool == "Bash"),
            "a child's tool never moves a parent running its own"
        );
        fold(
            &mut e,
            &SessionEvent::Progress(ProgressEvent::ToolEnded {
                name: "Bash".into(),
                ok: true,
                id: None,
            }),
            5,
        );
        assert!(
            delegating_to(&e.presence.state, "explore"),
            "the tool over, the sub-agent is the work again"
        );
        let asked = SessionEvent::Lifecycle(LifecycleEvent::InputRequested {
            request: InputRequest::question("q", "Which?", Vec::new()),
        });
        fold(&mut e, &asked, 6);
        assert!(matches!(e.presence.state, SessionState::Waiting { .. }));
        fold(&mut e, &ended("t1", true), 7);
        assert!(
            matches!(e.presence.state, SessionState::Waiting { .. }),
            "a wait keeps its word whatever the children do"
        );
        fold(
            &mut e,
            &SessionEvent::Lifecycle(LifecycleEvent::InputResolved { id: "q".into() }),
            8,
        );
        assert_eq!(e.presence.state, SessionState::Thinking);
    }

    #[test]
    fn a_background_sub_agent_keeps_a_session_delegating_between_turns() {
        let mut e = entry();
        fold(&mut e, &turn(), 1);
        fold(&mut e, &spawn("t1", "explore"), 2);
        fold(&mut e, &SessionEvent::Progress(ProgressEvent::TurnEnded), 3);
        assert!(
            delegating_to(&e.presence.state, "explore"),
            "the turn ended but the sub-agent is still the work"
        );
        fold(&mut e, &ended("t1", true), 4);
        assert_eq!(
            e.presence.state,
            SessionState::Idle,
            "and once it leaves, the session is between turns"
        );
    }

    #[test]
    fn an_unannounced_child_is_created_when_it_works_and_a_stop_under_a_stranger_is_nothing() {
        let mut e = entry();
        fold(&mut e, &turn(), 1);
        fold(&mut e, &spawn("t1", "explore"), 2);
        fold(&mut e, &ended("t1", true), 3);
        // A start hook the reporter missed: the child's first tool creates it.
        assert!(fold(&mut e, &nested_tool("t2", "Grep"), 4));
        assert_eq!(e.presence.children.len(), 1);
        assert_eq!(e.presence.children[0].name, "agent");
        assert!(delegating_to(&e.presence.state, "agent"));
        fold(&mut e, &ended("t2", true), 5);
        // A late stop from a child already gone creates nothing and moves nothing.
        let late = SessionEvent::Progress(
            ProgressEvent::TurnEnded.raised_by(Some(SubagentId("t2".into()))),
        );
        assert!(!fold(&mut e, &late, 6));
        assert!(e.presence.children.is_empty());
        assert_eq!(e.presence.state, SessionState::Thinking);
    }

    #[test]
    fn a_harness_that_names_its_sessions_learns_its_root_once_and_its_children_after() {
        let mut e = entry();
        let root = SubagentId("ses_root".into());
        let by_root = |p: ProgressEvent| SessionEvent::Progress(p.raised_by(Some(root.clone())));
        assert!(fold(&mut e, &by_root(ProgressEvent::TurnStarted), 1));
        assert_eq!(
            e.presence.state,
            SessionState::Thinking,
            "the root's turn is the session's"
        );
        assert!(e.presence.children.is_empty(), "the root is no child");
        assert!(fold(&mut e, &nested_tool("ses_root", "edit"), 2));
        assert!(matches!(&e.presence.state, SessionState::Running { tool, .. } if tool == "edit"));
        fold(
            &mut e,
            &by_root(ProgressEvent::ToolEnded {
                name: "edit".into(),
                ok: true,
                id: None,
            }),
            3,
        );
        // A child session announced, then working under its own id.
        fold(&mut e, &spawn("ses_child", "task"), 4);
        assert!(fold(&mut e, &nested_tool("ses_child", "read"), 5));
        assert_eq!(e.presence.children.len(), 1);
        assert!(
            matches!(&e.presence.children[0].state, SessionState::Running { tool, .. } if tool == "read")
        );
        assert!(delegating_to(&e.presence.state, "task"));
        // The root's frames still land on the session.
        fold(&mut e, &nested_tool("ses_root", "bash"), 6);
        assert!(matches!(&e.presence.state, SessionState::Running { tool, .. } if tool == "bash"));
        assert!(e.presence.children.len() == 1);
        // The child goes idle then leaves.
        let child_over = SessionEvent::Progress(
            ProgressEvent::TurnEnded.raised_by(Some(SubagentId("ses_child".into()))),
        );
        fold(&mut e, &child_over, 7);
        assert_eq!(e.presence.children[0].state, SessionState::Idle);
        fold(&mut e, &ended("ses_child", true), 8);
        assert!(e.presence.children.is_empty());
    }

    #[test]
    fn cost_accumulates_from_the_session_and_its_sub_agents() {
        let mut e = entry();
        let cost = |i, o, c| ProgressEvent::CostDelta {
            input_tokens: i,
            output_tokens: o,
            usd_cents: c,
        };
        assert!(fold(&mut e, &SessionEvent::Progress(cost(10, 5, 1)), 1));
        assert!(fold(
            &mut e,
            &SessionEvent::Progress(cost(1, 1, 1).raised_by(Some(SubagentId("s".into())))),
            2
        ));
        assert_eq!(
            e.presence.cost,
            SessionCost {
                input_tokens: 11,
                output_tokens: 6,
                usd_cents: 2
            }
        );
    }

    #[test]
    fn the_wire_words_are_the_serde_tags() {
        for (state, word) in [
            (SessionState::Starting, "starting"),
            (SessionState::Idle, "idle"),
            (SessionState::Thinking, "thinking"),
            (
                SessionState::Running {
                    tool: "t".into(),
                    args: String::new(),
                    tier: ToolTier::Read,
                },
                "running",
            ),
            (
                SessionState::Waiting {
                    on: WaitingOn::Auth {
                        provider: "p".into(),
                        url: None,
                    },
                },
                "waiting",
            ),
            (SessionState::Done, "done"),
            (SessionState::Aborted, "aborted"),
            (SessionState::Failed { reason: "r".into() }, "failed"),
            (SessionState::Parked, "parked"),
        ] {
            assert_eq!(state.as_str(), word);
            assert_eq!(serde_json::to_value(&state).unwrap()["state"], word);
        }
    }

    #[test]
    fn a_sub_agent_has_a_start_that_holds_while_since_moves() {
        // The spawn instant is the child's `started`; every tool and every
        // turn moves its `since` and leaves `started` where it was — the
        // desktop counts a sub-agent's elapsed from the one that holds.
        let mut e = entry();
        fold(&mut e, &turn(), 1);
        fold(&mut e, &spawn("t1", "explore"), 20);
        assert_eq!(e.presence.children[0].started, 20);
        assert_eq!(e.presence.children[0].since, 20);
        fold(&mut e, &nested_tool("t1", "Read"), 35);
        assert_eq!(e.presence.children[0].since, 35, "since follows the state");
        assert_eq!(e.presence.children[0].started, 20, "started does not");
        let nested_turn = SessionEvent::Progress(
            ProgressEvent::TurnEnded.raised_by(Some(SubagentId("t1".into()))),
        );
        fold(&mut e, &nested_turn, 50);
        assert_eq!(e.presence.children[0].state, SessionState::Idle);
        assert_eq!(e.presence.children[0].since, 50);
        assert_eq!(
            e.presence.children[0].started, 20,
            "still the spawn instant, three states on"
        );
        // A child the session was never told about starts when it is first seen.
        fold(&mut e, &nested_tool("t9", "Grep"), 60);
        let inferred = e.presence.children.iter().find(|c| c.id.0 == "t9").unwrap();
        assert_eq!(inferred.started, 60);
    }

    #[test]
    fn an_idle_child_leaves_at_the_turn_boundary_and_a_working_one_stays() {
        // A stop hook that never arrived must not leave a child forever:
        // one idle across a turn boundary has left; one still thinking,
        // running or waiting is background work and stays.
        let mut e = entry();
        fold(&mut e, &turn(), 1);
        fold(&mut e, &spawn("idle", "explore"), 2);
        fold(&mut e, &spawn("busy", "plan"), 3);
        let idle_turn = SessionEvent::Progress(
            ProgressEvent::TurnEnded.raised_by(Some(SubagentId("idle".into()))),
        );
        fold(&mut e, &idle_turn, 4);
        assert_eq!(
            e.presence
                .children
                .iter()
                .find(|c| c.id.0 == "idle")
                .unwrap()
                .state,
            SessionState::Idle
        );
        assert!(
            fold(&mut e, &SessionEvent::Progress(ProgressEvent::TurnEnded), 5),
            "the boundary is a change: a child left"
        );
        assert_eq!(e.presence.children.len(), 1);
        assert_eq!(
            e.presence.children[0].name, "plan",
            "the thinking one is still the work"
        );
        assert!(delegating_to(&e.presence.state, "plan"));
        // The next turn's boundary is the same rule.
        fold(&mut e, &spawn("late", "review"), 6);
        let late_turn = SessionEvent::Progress(
            ProgressEvent::TurnEnded.raised_by(Some(SubagentId("late".into()))),
        );
        fold(&mut e, &late_turn, 7);
        assert!(fold(&mut e, &turn(), 8));
        assert_eq!(
            e.presence
                .children
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            ["plan"]
        );
    }

    #[test]
    fn a_new_turn_closes_a_turn_the_harness_never_ended() {
        // Claude Code fires no hook on a person's interrupt: the next prompt
        // is the last turn's end — its open tools close, and the row thinks.
        let mut e = entry();
        fold(&mut e, &turn(), 1);
        fold(&mut e, &tool("Bash"), 2);
        assert!(matches!(&e.presence.state, SessionState::Running { tool, .. } if tool == "Bash"));
        assert!(fold(&mut e, &turn(), 3));
        assert!(
            e.open_tools.is_empty(),
            "the interrupted tool is not still running"
        );
        assert!(e.in_turn);
        assert_eq!(e.presence.state, SessionState::Thinking);
        // Its end, when the harness does say it, is the ordinary one.
        assert!(fold(
            &mut e,
            &SessionEvent::Progress(ProgressEvent::TurnEnded),
            4
        ));
        assert_eq!(e.presence.state, SessionState::Idle);
    }

    #[test]
    fn a_terminal_end_takes_the_children_the_open_tools_and_the_pid() {
        let mut e = entry();
        fold(
            &mut e,
            &SessionEvent::Lifecycle(LifecycleEvent::ProcessStarted { pid: Some(4242) }),
            1,
        );
        assert_eq!(
            e.pid_seen_at,
            Some(1),
            "the pid is stamped when it was seen"
        );
        fold(&mut e, &turn(), 2);
        fold(&mut e, &spawn("t1", "explore"), 3);
        fold(&mut e, &tool("Bash"), 4);
        let done = SessionEvent::Lifecycle(LifecycleEvent::Ended {
            outcome: Outcome::Completed,
            is_terminal: true,
        });
        assert!(fold(&mut e, &done, 5));
        assert_eq!(e.presence.state, SessionState::Done);
        assert!(
            e.presence.children.is_empty(),
            "a row that ends takes its children"
        );
        assert!(e.open_tools.is_empty());
        assert_eq!(e.presence.pid, None);
        assert_eq!(e.pid_seen_at, None);
        assert!(!e.in_turn);
    }
}
