//! What the Inbox carries on the wire: a row per thing, its asks and its
//! notices. The node serves these as they are (`GET /inbox`, the goal page's
//! and the run page's asks); the command line prints them — one shape,
//! built in one place.

use crate::feed::PulseRow;
use crate::notices::{InboxKind, NoticeKind};
use bisa_core::{AskKind, ConversationOrigin, Gate, GoalStatus};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Clone, Debug, JsonSchema)]
pub struct NeedsAction {
    /// Where the decision is filed and decided through: the goal
    /// (`POST /goals/{id}/decide`), or a run of the workspace
    /// (`POST /runs/{rid}/decide`).
    pub home: bisa_core::Home,
    /// The live gate id. Absent when the pending decision was reconstructed
    /// from the run (a waiting `human` or `approval` step) or the journal (an
    /// adoption) — decide those through the home.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate_id: Option<String>,
    pub gate_kind: Gate,
    /// The run step this decision completes, when it completes one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
    pub question: String,
    /// What input this gate expects — and, for an answer, the options the
    /// asker offered. The escape hatches ("I'm not sure", free text) are the
    /// platform's, not the asker's: they are valid on every answer gate
    /// whether or not this list is empty.
    pub expects: AskKind,
    /// True when this was reconstructed from durable state rather than a live
    /// gate.
    pub durable: bool,
    /// What is being decided — `approval:<run>/<step>`, `adopt:<workflow>@…`,
    /// a branch and remote. Without it a row can only say "approval" and
    /// never *what* is being approved, which is the half a person needs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// Which work item is blocked. Five items run in parallel under one
    /// goal; "a session is waiting on you" does not say which.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_item: Option<String>,
    /// When the gate opened, or the step started waiting. Absent when neither
    /// is known — an invented clock is worse than none, because a gate waiting
    /// nine days would render as new and hide exactly the staleness worth
    /// seeing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opened_at: Option<u64>,
    /// For an adoption: the proposed workflow itself — description, every
    /// step with a one-line summary, edges, inputs — so the card that asks
    /// *adopt?* shows what is being adopted, not a name and a count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposal: Option<ProposalView>,
}

/// A proposed workflow as the ask card renders it.
#[derive(Serialize, JsonSchema, Clone, Debug)]
pub struct ProposalView {
    pub workflow: String,
    pub revision: u64,
    pub name: String,
    pub description: String,
    pub inputs: Vec<bisa_core::InputDef>,
    pub steps: Vec<ProposalStep>,
    pub edges: Vec<ProposalEdge>,
    /// What listening asks, when the design begins on events: the inputs its
    /// event starts read and do not map, with no default to fill them — what
    /// the card binds before adopting, in place of a run's inputs.
    pub listening_needs: Vec<String>,
    /// Only events begin it: adopting it makes the goal listen, never run.
    pub event_only: bool,
}

/// One step of a proposal, summarised for a reader.
#[derive(Serialize, JsonSchema, Clone, Debug)]
pub struct ProposalStep {
    pub id: String,
    pub name: String,
    /// The step's kind word (`agent`, `human`, …).
    pub kind: String,
    /// A `start` step's event word (`StartOn::as_str`); `null` for any
    /// other kind.
    pub event: Option<String>,
    /// One line a person can judge the step by (`Step::summary`), as data — the
    /// reader renders it in their language.
    pub summary: bisa_core::Text,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    /// How the step's incoming flows are counted: `all`, `any` or `one`.
    pub join: String,
    /// What happens on failure when it is not "the run fails": `skip`, or `then <step>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_fail: Option<String>,
    pub max_visits: u8,
}

/// One flow of a proposal.
#[derive(Serialize, JsonSchema, Clone, Debug)]
pub struct ProposalEdge {
    pub from: String,
    pub to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// `then` or `on_fail`.
    pub kind: String,
}

impl ProposalView {
    pub fn of(wf: &bisa_core::Workflow) -> Self {
        use bisa_core::workflow::{OnFail, StepKind, ValueRef};
        let mut edges = Vec::new();
        let steps = wf
            .steps
            .iter()
            .map(|s| {
                for f in &s.then {
                    edges.push(ProposalEdge {
                        from: s.id.to_string(),
                        to: f.to.to_string(),
                        branch: f.branch.as_ref().map(|b| b.to_string()),
                        kind: "then".into(),
                    });
                }
                let on_fail = match &s.on_fail {
                    OnFail::Fail => None,
                    OnFail::Skip => Some("skip".to_string()),
                    OnFail::Then { step } => {
                        edges.push(ProposalEdge {
                            from: s.id.to_string(),
                            to: step.to_string(),
                            branch: None,
                            kind: "on_fail".into(),
                        });
                        Some(format!("then {step}"))
                    }
                };
                let assignee = match &s.kind {
                    StepKind::Agent { assignee, .. } | StepKind::Human { assignee, .. } => {
                        assignee.as_ref().map(ValueRef::word)
                    }
                    _ => None,
                };
                ProposalStep {
                    id: s.id.to_string(),
                    name: s.name.clone(),
                    kind: s.kind.as_str().to_string(),
                    event: s.start_on().map(|on| on.as_str().to_string()),
                    summary: s.summary(),
                    assignee,
                    join: s.join.as_str().to_string(),
                    on_fail,
                    max_visits: s.max_visits,
                }
            })
            .collect();
        Self {
            workflow: wf.id.to_string(),
            revision: wf.revision,
            name: wf.name.clone(),
            description: wf.description.clone(),
            inputs: wf.inputs.clone(),
            steps,
            edges,
            listening_needs: listening_needs(wf),
            event_only: wf.is_event_only(),
        }
    }
}

/// What a host must give when it begins listening with `wf`, by name
/// (`Workflow::listening_needs`), in the names' own order.
pub fn listening_needs(wf: &bisa_core::Workflow) -> Vec<String> {
    wf.listening_needs()
        .iter()
        .map(|name| name.to_string())
        .collect()
}

impl NeedsAction {
    /// A live gate, with everything the gate already knows carried across.
    /// Two surfaces build this — the inbox row and a goal's guidance strip
    /// — and a field dropped in one of them is a field that renders
    /// differently on two screens showing the same gate.
    pub fn from_gate(g: crate::gates::GateEntry) -> Self {
        Self {
            home: g.home,
            gate_id: Some(g.id),
            gate_kind: g.gate,
            run: g.run.map(|r| r.to_string()),
            step: g.step.map(|s| s.to_string()),
            question: g.question,
            expects: g.expects,
            durable: false,
            subject: Some(g.subject),
            work_item: g.work_item.map(|w| w.to_string()),
            opened_at: Some(g.opened_at),
            proposal: None,
        }
    }
}

/// A decision that has already been made on a conversation.
///
/// Read is per-person; *handled* is not. Whether a contract was approved is a
/// fact about the workspace, so the row says what was decided to everyone who
/// can see it rather than clearing itself for the one person who clicked.
#[derive(Serialize, Clone, JsonSchema)]
pub struct Decided {
    /// The journal stores a gate's kind as its Rust name. Parsing that back
    /// is not a round-trip anything guarantees, so an unrecognised label
    /// yields `None` — a generic glyph beats a confidently wrong one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate_kind: Option<Gate>,
    pub approve: bool,
    pub subject: String,
    pub actor: String,
    pub at: u64,
}

#[derive(Serialize, Clone, JsonSchema)]
pub struct Representative {
    pub author: String,
    pub snippet: String,
    pub at: u64,
}

/// The tab an Inbox row sits under — what the row is *about*, not what kind
/// of record it is: a conversation about a goal is the goals'. The wire's
/// `?source=` takes these words, and every row carries its own, so nothing
/// mirrors the rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InboxSource {
    Messages,
    Projects,
    Workflows,
    Goals,
    People,
}

impl InboxSource {
    /// Every source, in the order the tabs stand.
    pub const ALL: [InboxSource; 5] = [
        Self::Messages,
        Self::Projects,
        Self::Workflows,
        Self::Goals,
        Self::People,
    ];

    /// The source a row belongs under: its kind's — except a conversation,
    /// which sits under what it is about: a goal's, a workflow's, a
    /// project's or a checkout's tab. One about the node, the workspace, a
    /// drawing or a note is a message like any other.
    pub fn of(kind: InboxKind, origin: Option<&ConversationOrigin>) -> Self {
        match kind {
            InboxKind::Goal => Self::Goals,
            InboxKind::Workstream | InboxKind::Project => Self::Projects,
            InboxKind::Channel | InboxKind::Dm => Self::Messages,
            InboxKind::Conversation => match origin {
                Some(ConversationOrigin::Goal { .. }) => Self::Goals,
                Some(ConversationOrigin::Workflow { .. }) => Self::Workflows,
                Some(
                    ConversationOrigin::Project { .. } | ConversationOrigin::Workstream { .. },
                ) => Self::Projects,
                _ => Self::Messages,
            },
            InboxKind::Workflow => Self::Workflows,
            InboxKind::People => Self::People,
            // A harness in a terminal stands in a checkout.
            InboxKind::Session => Self::Projects,
        }
    }

    /// The wire's word — the same one `?source=` takes.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Messages => "messages",
            Self::Projects => "projects",
            Self::Workflows => "workflows",
            Self::Goals => "goals",
            Self::People => "people",
        }
    }
}

#[derive(Serialize, JsonSchema)]
pub struct InboxRow {
    /// The thing's own id: a goal, channel, conversation, workstream,
    /// project or workflow ULID. A row is a thing, never an event.
    pub key: String,
    pub kind: InboxKind,
    /// The tab the row sits under (`?source=`): its kind's, except a
    /// conversation, which sits under what it is about.
    pub source: InboxSource,
    /// What a conversation row is about — its origin — so the row can say
    /// so; absent on every other kind.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<ConversationOrigin>,
    pub title: String,
    pub latest_at: u64,
    pub unread_count: u64,
    /// Whether **you** have read this row: your local watermark is at or past
    /// its latest activity. Deliberately not derived from `unread_count`,
    /// which counts messages and so cannot tell a conversation you have read
    /// from one that never had a message in it.
    pub read: bool,
    /// Whether the decision this row was asking for has been made. A
    /// workspace fact, so it reads the same for everyone — unlike `read`.
    pub handled: bool,
    /// You were named here, from the durable `p`-tag index. Computed on every
    /// row rather than only inside the mentions filter: "someone addressed
    /// me" is a property of a row, and a filter is one use of it.
    pub mentioned: bool,
    pub needs_action: Vec<NeedsAction>,
    /// What happened to this thing that concerns a person — the newest
    /// first, from the activity index (`notices::notice_of`), bounded.
    pub notices: Vec<NoticeDto>,
    /// The notices after your read watermark.
    pub unread_notices: usize,
    /// The most recent decision on record, so a handled row can say what was
    /// decided instead of merely going quiet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided: Option<Decided>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub representative: Option<Representative>,
    /// A goal row's status, read from its current run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<GoalStatus>,
    /// A people row's open ask: somebody claimed an invitation under
    /// `collab.join = ask` and waits to be admitted or refused here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub join: Option<JoinRequest>,
    /// A `session` row's harness, waiting on you in its terminal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waiting: Option<WaitingSession>,
}

/// The claim waiting on the owner, on a people row.
#[derive(Serialize, Clone, PartialEq, Eq, JsonSchema)]
pub struct JoinRequest {
    pub invite: String,
    pub role: bisa_core::MemberRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub at: u64,
}

/// A harness a person opened in the IDE's terminal, waiting on them at its
/// own prompt (ide/06 §Reporting) — the `session` row's one fact. The row
/// is there while the wait is and gone when the prompt is answered or the
/// tab closes; its door is the workstream's terminal in the Project IDE,
/// where the wait is answered — nothing here decides it.
#[derive(Serialize, Clone, PartialEq, JsonSchema)]
pub struct WaitingSession {
    /// The session's id on the roster — the row's key too.
    pub session: String,
    /// The harness id, as the catalog names it.
    pub harness: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workstream: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    /// What it waits on, as the roster says it.
    pub on: crate::WaitingOn,
    /// The wait in words — *permission: Bash*, *a question: …*.
    pub words: String,
    /// The sub-agent whose wait it is, by name, when a sub-agent asks and
    /// not the session itself — the card reads *↳ explore · permission:
    /// Bash*. Absent for the session's own wait.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagent: Option<String>,
    /// Unix seconds the wait began — the sub-agent's when it is its wait.
    pub since: u64,
}

/// One notice on a row: which kind of thing happened, and the fact itself
/// as the Pulse carries it — so the desktop words it with the same function
/// a Pulse row gets, and a door opens the same record.
#[derive(Serialize, JsonSchema)]
pub struct NoticeDto {
    pub notice: NoticeKind,
    #[serde(flatten)]
    pub row: PulseRow,
}
