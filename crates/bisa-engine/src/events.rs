//! Engine-level event vocabulary, layered over the harness session events.
//!
//! Every payload has a **topic** — `run.finished`, `goal.closed`, one dotted
//! name per variant ([`EnginePayload::topic`]) — and flat **fields**
//! ([`EnginePayload::fields`]): what a `platform` start, wait or boundary
//! names and matches on, exactly. The topics are the runtime's contract with
//! every workflow that listens to it; validation reads [`TOPICS`].

use bisa_core::{
    AskKind, Branch, ClosureReason, ConversationId, ConversationOrigin, Gate, GuidancePhase,
    GuidanceStatus, ListenerHost, ListenerKey, RunOutcome, SessionId, SignalSource,
};
use bisa_core::{
    AttachmentRef, GoalId, ProjectId, RunId, StepId, WorkItemId, WorkflowId, WorkflowRun,
    WorkstreamId, WorkstreamState,
};
use bisa_harness::SessionEvent;
use schemars::JsonSchema;
use serde::Serialize;
use std::collections::BTreeMap;

use crate::presence::SessionPresence;
use crate::registry::LiveRunId;

/// How a work item's execution ended — typed, so a screen matches a tag and
/// never a sentence.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum ExecutionOutcome {
    Completed,
    Aborted,
    /// The step was cancelled or amended away while the item ran.
    Cancelled {
        reason: String,
    },
    Failed {
        reason: String,
    },
    BudgetExhausted,
    WallClockExceeded,
}

impl std::fmt::Display for ExecutionOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionOutcome::Cancelled { reason } => write!(f, "cancelled: {reason}"),
            ExecutionOutcome::Failed { reason } => write!(f, "failed: {reason}"),
            other => f.write_str(other.as_str()),
        }
    }
}

impl ExecutionOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionOutcome::Completed => "completed",
            ExecutionOutcome::Aborted => "aborted",
            ExecutionOutcome::Cancelled { .. } => "cancelled",
            ExecutionOutcome::Failed { .. } => "failed",
            ExecutionOutcome::BudgetExhausted => "budget_exhausted",
            ExecutionOutcome::WallClockExceeded => "wall_clock_exceeded",
        }
    }
}

/// One event on the engine's broadcast bus. `goal`/`workflow`/`run`/
/// `work_item` scope the payload when known; subscribers filter by scope and
/// payload kind.
#[derive(Debug, Clone, Serialize)]
pub struct EngineEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<GoalId>,
    /// The workflow of the run the payload is about, when a run is behind
    /// it — what a run of the workspace, which no goal holds, is filed
    /// under in the feed and the Inbox.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow: Option<WorkflowId>,
    /// The run behind the payload, when there is one. What keeps an event
    /// from feeding itself: a listener that hears an event a run caused reads
    /// that run's causal chain ([`bisa_core::Chain`]) and refuses when it is
    /// already in it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<RunId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_item: Option<WorkItemId>,
    pub payload: EnginePayload,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum EnginePayload {
    /// A harness session event, re-broadcast with goal/work-item scope.
    Session {
        event: SessionEvent,
    },
    /// A work-item was scheduled onto a harness.
    Scheduled {
        harness: String,
    },
    /// A work-item finished executing (before the run reads its result).
    ExecutionEnded {
        outcome: ExecutionOutcome,
    },
    /// What a live session is doing changed — a state, a wait, a sub-agent,
    /// its cost. Folded once in `presence.rs`; coalesced, never per token.
    SessionState {
        live_run: LiveRunId,
        /// Boxed: the roster row is the bus's largest payload by far, and
        /// every other variant would pay for it in every frame otherwise.
        presence: Box<SessionPresence>,
    },
    /// A finished session left the roster after its retention window.
    SessionGone {
        live_run: LiveRunId,
    },
    GateOpened {
        gate_id: String,
        gate: Gate,
        question: String,
    },
    /// A session asked the human a question (decision or free text).
    QuestionAsked {
        gate_id: String,
        text: String,
        expects: AskKind,
    },
    /// The Workflow Agent's standing on a guided goal moved. Mirrors the
    /// `Guidance` journal fact one for one; emitted by `guided::record` only.
    Guided {
        phase: GuidancePhase,
        status: GuidanceStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        session: Option<SessionId>,
    },
    /// A conversation was started: its origin says what it is about.
    ConversationCreated {
        id: ConversationId,
        origin: ConversationOrigin,
    },
    /// A conversation's record moved: renamed, archived or unarchived,
    /// deleted. A list re-reads on it.
    ConversationChanged {
        id: ConversationId,
        change: ConversationChange,
    },
    /// An agent is composing a reply in a message stream (the UI's "working"
    /// indicator). `scope` is a channel id, a goal id or a conversation id.
    AgentThinking {
        scope: String,
        agent: String,
    },
    /// The words and the thinking an agent's turn added since the last
    /// frame, in a message stream — a timeline appends them to the turn in
    /// flight. Coalesced by the engine, so a frame is many tokens; either
    /// part may be empty. `working` is the tool the agent runs *now*, as it
    /// stands — a frame replaces it, never appends, and a `null` is news:
    /// the tool ended. `AgentReplied` follows when the reply is posted.
    AgentStreamed {
        scope: String,
        agent: String,
        text: String,
        thinking: String,
        working: Option<String>,
    },
    /// An agent finished its turn in a message stream. `posted` is false when
    /// the turn produced no reply (it acted through tools only). `message` is
    /// the id of the message the reply landed as, so a timeline keeps the
    /// turn in flight on screen until that message is in its page — no blink
    /// between the last frame and the first read; `None` when nothing new
    /// was posted.
    AgentReplied {
        scope: String,
        agent: String,
        posted: bool,
        message: Option<String>,
    },
    GateDecided {
        gate_id: String,
        gate: Gate,
        approve: bool,
    },
    /// A work item's structured result landed and its step is done with it.
    ResultAccepted,
    /// A run of a workflow started — on a goal, at once or from its queue;
    /// or in the workspace, at once.
    RunStarted {
        run: RunId,
        workflow: WorkflowId,
    },
    /// A run was made behind the goal's live run; `position` is its place
    /// in the queue, 1 first. It starts on its own, announced by
    /// [`Self::RunStarted`], when its turn comes.
    RunQueued {
        run: RunId,
        workflow: WorkflowId,
        position: usize,
    },
    /// The run reached its end — every branch drained, an `end` step, or a
    /// failure nothing routed around. A cancelled run announces itself
    /// through [`Self::RunCancelled`] instead.
    RunFinished {
        run: RunId,
        workflow: WorkflowId,
        outcome: RunOutcome,
    },
    /// The run was cancelled, live or queued: stopped or restarted by a
    /// person, withdrawn from the queue, closed with its goal, or retired
    /// with its workflow.
    RunCancelled {
        run: RunId,
        workflow: WorkflowId,
        cause: bisa_core::CancelCause,
    },
    /// One step of a run changed state. `state` is the `StepState` tag
    /// (`pending` … `cancelled`), `kind` the step kind's tag, so a matcher can
    /// filter on either without parsing the snapshot.
    StepChanged {
        run: RunId,
        workflow: WorkflowId,
        step: StepId,
        state: String,
        kind: String,
    },
    /// The goal was closed: abandoned or superseded. Its live run and its
    /// queued runs were cancelled first, each announced by
    /// [`Self::RunCancelled`].
    GoalClosed {
        reason: ClosureReason,
    },
    /// A goal came into existence — captured by a person, spawned by a step
    /// or an agent. The workspace's shape changed, which is what the activity
    /// feed's *Workspace* concept is; the goal's own story starts in its
    /// journal. `goal` rides the payload as well as the envelope for the
    /// reason [`Self::ProjectCreated`] carries its fields: a `platform`
    /// filter reads its fields from the payload alone.
    GoalCreated {
        goal: GoalId,
        origin: bisa_core::GoalOrigin,
    },
    /// The Workflow Agent proposed a workflow for the goal. `gate_id` is the
    /// Adopt gate that opened for the person — absent when the platform
    /// adopted it alone (an auto goal) or recorded it as the person's draft
    /// (a manual goal).
    WorkflowProposed {
        workflow: WorkflowId,
        revision: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        gate_id: Option<String>,
    },
    /// A workflow definition was created or saved, or a goal was pointed at
    /// a different one. The designer and the library re-read on it.
    /// `designed` says whose hand wrote it: `true` when the Workflow Agent
    /// did — a proposal, an amendment applied alone on an auto goal — and
    /// the Inbox tells the workflow's row; `false` for a person's own create,
    /// draft, save, promote, design or choice, which is no news to them.
    WorkflowChanged {
        workflow: WorkflowId,
        revision: u64,
        designed: bool,
    },
    /// A workflow definition was deleted: out of the library and every
    /// picker; the runs that copied it keep their copy. The designer open on
    /// it leaves.
    WorkflowDeleted {
        workflow: WorkflowId,
    },
    /// A workflow was put away, or taken back out — a mark, never a status:
    /// out of the library and the pickers while archived, refused for a goal
    /// or a run.
    WorkflowArchived {
        workflow: WorkflowId,
        archived: bool,
    },
    /// A goal was put away, or taken back out — a mark, never a status: an
    /// archived goal is a closed goal the lists hide until asked.
    GoalArchived {
        goal: GoalId,
        archived: bool,
    },
    /// A goal was deleted: its folder, its runs, its work — gone from this node.
    GoalDeleted {
        goal: GoalId,
    },
    /// A project was put away, or taken back out: hidden from the rail and
    /// refused for attachment and placement; every session in it stopped.
    ProjectArchived {
        project: ProjectId,
        archived: bool,
    },
    /// A project's records went — and with them, when asked, its managed folder to the Trash.
    ProjectDeleted {
        project: ProjectId,
    },
    /// A goal ⇄ project attachment changed.
    AttachmentChanged {
        project: ProjectId,
        attached: bool,
    },
    /// A person gave the goal a document — a file kept under the goal's
    /// `documents/` folder as its context. The goal page's card and the
    /// activity feed move on it; the fact itself is the journal's.
    DocumentAdded {
        goal: GoalId,
        file: AttachmentRef,
    },
    /// A project came into existence — from the desktop, the CLI, an agent's
    /// `create_project` tool, or an intake op.
    ///
    /// Without it, a project made by anything other than the screen you were
    /// looking at would appear only on a remount. A `KIND_PROJECT` snapshot does ride the *conversation* channel, but
    /// a project is not a conversation: making the projects screen subscribe
    /// there to learn about itself would tie it to a channel whose whole
    /// subject is messages, and would leave every other engine subscriber
    /// (the CLI activity view, a `platform` start) still blind to the fact.
    ///
    /// The origin is carried in the payload as well as on the envelope
    /// because [`EnginePayload::fields`] reads the payload alone — a variant
    /// that hides the owner behind the envelope cannot be filtered on it.
    ProjectCreated {
        project: ProjectId,
        slug: String,
        /// Where it was born — the workspace, a goal, or a workflow's step.
        origin: bisa_core::ProjectOrigin,
    },
    /// Nobody is set to commit in a project and nothing resolved it — a
    /// person is asked. Raised once per project while the question
    /// is open: on creation, or when a commit was refused for it. `global` is
    /// the person's global git pair, offered so a screen can propose pinning
    /// it; `workstream` is the primary, where the answer is written.
    CommitterNeeded {
        project: ProjectId,
        slug: String,
        workstream: WorkstreamId,
        reason: crate::identity::CommitterReason,
        origin: bisa_core::ProjectOrigin,
        #[serde(skip_serializing_if = "Option::is_none")]
        global: Option<bisa_vcs::Ident>,
    },
    /// Who commits in a project was set — the repository's local identity.
    /// Every write funnels here, whichever door it came through.
    CommitterSet {
        project: ProjectId,
        workstream: WorkstreamId,
        identity: bisa_vcs::Ident,
    },
    /// A project's record was edited — name, publish policy, assignees, tags,
    /// group or photo. Not its tree: files have their own stream.
    ProjectChanged {
        project: ProjectId,
    },
    /// A workstream's person-editable fields changed — name, note, pinned.
    /// Its lifecycle has its own event ([`Self::WorkstreamChanged`]).
    WorkstreamEdited {
        workstream: WorkstreamId,
        project: ProjectId,
    },
    /// A note was created, edited, or appended to.
    ///
    /// One variant for all three, because the overlay's response to each is
    /// the same — re-read — and a surface that had to switch on *which* kind
    /// of change it was would be deciding something it does not need to know.
    ///
    /// `scope` and `scope_id` ride the payload rather than only the envelope
    /// for the reason [`Self::ProjectCreated`] carries its origin: a
    /// `platform` filter reads its fields from the payload alone, and so does
    /// the overlay, which is filtering for the one scope it is showing.
    NoteChanged {
        note: String,
        scope: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        scope_id: Option<String>,
    },
    /// A drawing was created, changed or deleted (19 — Drawings): the Draw
    /// overlay re-reads its list, and an open canvas compares `hash` with
    /// the scene it last saved to tell its own write from somebody else's.
    /// `scope` and `scope_id` ride the payload for the reason
    /// [`Self::NoteChanged`]'s do.
    DrawingChanged {
        drawing: String,
        scope: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        scope_id: Option<String>,
        hash: String,
    },
    /// An agent asked the canvas to draw (19): the desktop performs it in the
    /// live canvas or an offscreen one and answers
    /// `POST /drawings/requests/{id}`; the request waits on the engine's
    /// drawing desk until then.
    DrawingRequest {
        id: String,
        request: crate::drawings::DrawRequest,
        scope: crate::drawings::DrawScope,
    },
    /// A workstream was opened: a git worktree on a branch, or a copy of a
    /// non-git project. `branch` is `None` for a copy; `source` says where
    /// the branch came from in a line (`WorkstreamSource::describe` — *a new
    /// branch*, *the branch origin/x*, *pull request #12*, *a copy*).
    WorkstreamOpened {
        workstream: WorkstreamId,
        project: ProjectId,
        path: String,
        branch: Option<String>,
        source: String,
    },
    /// A workstream's state moved (dirty, committed, pushed, pr open, closed).
    /// One event for every step, so a surface can follow a branch's life
    /// without polling git.
    WorkstreamChanged {
        workstream: WorkstreamId,
        state: WorkstreamState,
    },
    /// The node started or stopped serving a folder (ide/18): the Browser
    /// menu and the footer read the list again. `workstream` names the
    /// checkout whose folder it is; an artifact's page has none.
    ServerChanged {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workstream: Option<WorkstreamId>,
    },
    /// An agent asked the embedded browser to do something (ide/18): the
    /// desktop performs it and answers `POST /browser/requests/{id}`; the
    /// request waits in `browser::BrowserRequests` until then.
    BrowserRequest {
        id: String,
        request: crate::browser::BrowserRequest,
        scope: crate::browser::BrowserScope,
        /// A tab this request opens is kept out of sight (ide/18 §Headless tabs).
        headless: bool,
    },
    /// This machine's mobile toolchain was examined again, or a device was
    /// booted, shut down or made (ide/19): Settings and the IDE read the
    /// list again.
    MobileDevelopmentChanged {
        what: crate::mobile_development::MobileDevelopmentChange,
    },
    /// An installed MCP server was probed (06 § MCP servers): Settings and
    /// the agent editor read its health again. `ok` is the answer in one
    /// word; the report is `GET /mcp/{id}`'s.
    McpProbed {
        id: bisa_core::McpId,
        ok: bool,
    },
    /// A workstream script ran — or was skipped for want of approval on this
    /// machine (ide/07 §Workstream scripts). `output` is the outcome in words
    /// followed by the tail of what the script printed; a failed post-create
    /// script is the one nobody is looking at, so the desktop toasts it.
    WorkstreamScriptRan {
        workstream: WorkstreamId,
        project: ProjectId,
        phase: crate::scripts::Phase,
        ok: bool,
        output: String,
    },
    /// The Tool & Commands Guard judged a tool call: allowed, refused or put
    /// to a person — by a rule, the classifier or the person. `subject` is
    /// the redacted command or input, so this is safe on every surface.
    GuardDecided {
        #[serde(skip_serializing_if = "Option::is_none")]
        session: Option<LiveRunId>,
        tool: String,
        subject: String,
        verdict: bisa_core::event::GuardVerdict,
        by: bisa_core::event::GuardJudge,
        #[serde(skip_serializing_if = "Option::is_none")]
        rule: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// The Decision-Making Agent was asked at a decision point: who answered,
    /// the questions as they were asked (redacted), the answers, and whether
    /// the point acted on them or ran its own rule. Every judgement is one of
    /// these, goal or no goal; one made on a goal is in its journal too.
    Judged {
        judgement: bisa_core::Judgement,
        /// The agent the judgement was about or for, when there was one.
        #[serde(skip_serializing_if = "Option::is_none")]
        agent: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        run: Option<bisa_core::RunId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        step: Option<bisa_core::workflow::StepId>,
    },
    /// What waits for a word in a conversation's review changed — a turn
    /// recorded an edit, a person kept or undid one, somebody else's write was
    /// folded away (ide/20). A frame for the pane to re-read on, not a fact.
    ChangesMoved {
        conversation: String,
        workstream: String,
        pending: usize,
    },
    /// A person's word on an agent's changes landed: `keep`, `undo` or
    /// `restore`, how many files it reached and how many it left alone.
    ChangesSettled {
        conversation: String,
        act: String,
        files: usize,
        skipped: usize,
    },
    /// A turn of a conversation about a checkout stopped to ask the person
    /// in the conversation itself — a command no rule decided, a rule that
    /// asks (ide/20). The question is redacted like a gate's.
    AskOpened {
        conversation: String,
        ask: crate::changes::asks::AskView,
    },
    /// The ask was answered, or its session went away.
    AskSettled {
        conversation: String,
        ask_id: String,
        allowed: bool,
    },
    /// Secrets were replaced by placeholders before text left for an agent —
    /// how many and of which kinds, never which. `at` names the seam:
    /// `prompt`, `steer`, `follow_up`, `answer`, `mcp_reply`.
    Redacted {
        count: usize,
        kinds: Vec<String>,
        at: String,
    },
    /// What a person approved did not go out: a push, a pull request, a merge
    /// or a branch's deletion that passed its `Publish` gate and then
    /// failed — the remote refused, the code host refused, the network was
    /// gone. Nobody waits on the call by then (the route answered `202`
    /// when the gate opened), so this is the one word of its end. `what` is
    /// the act as the gate asked it (*push work/checkout-ab12*); `reason`
    /// the refusal in words, redacted.
    WorkstreamPublishFailed {
        workstream: WorkstreamId,
        project: ProjectId,
        what: String,
        reason: String,
    },
    /// A workstream's changes landed in a commit. Carries the sha because the
    /// commit *is* the result for a git project — the patch file was what
    /// ephemeral isolation had to settle for.
    WorkstreamCommitted {
        workstream: WorkstreamId,
        branch: String,
        commit: String,
    },
    /// A run moved from one model to another after the first hit a wall —
    /// a quota, a rate limit, a model id the harness will not run.
    ///
    /// Emitted for every switch, at launch and mid-run alike, because a
    /// failover that nobody can see is indistinguishable from magic: the human
    /// reading a transcript has to be able to tell which model produced which
    /// half of it. `after_progress` says the dead session had already done
    /// real work, so a partial change may be sitting in the workstream.
    ModelSwitched {
        #[serde(skip_serializing_if = "Option::is_none")]
        work_item: Option<WorkItemId>,
        from: String,
        to: String,
        reason: String,
        /// Seconds until `from` comes out of cooldown.
        retry_in_secs: u64,
        after_progress: bool,
    },
    /// An occurrence was written down: a signal for the listener it is for,
    /// or — `listener` null — a named signal kept for the waits that replay
    /// it. Emitted **after** the enqueue, never before: an event that outran
    /// the queue would be a promise the platform might not keep.
    SignalReceived {
        signal: String,
        listener: Option<ListenerKey>,
        source: SignalSource,
    },
    /// A queued signal met its listener: it started a run — on the goal that
    /// listens, or in the workspace — or it was skipped, and why.
    ListenerFired {
        listener: ListenerKey,
        signal: String,
        outcome: FiredOutcome,
    },
    /// A listener could not be armed — its start does not resolve against
    /// the listening inputs, its host cannot start runs — or a signal of it
    /// could not start its run. Said once per cause, on the host's row.
    ListenerFailed {
        listener: ListenerKey,
        #[serde(skip_serializing_if = "Option::is_none")]
        signal: Option<String>,
        error: String,
    },
    /// A host began or stopped hearing its start events: a library workflow
    /// turned On or Off, a goal that listens, paused or stopped.
    ListeningChanged {
        host: ListenerHost,
        on: bool,
    },
    /// A boundary event of a live step fired: it diverted the step, or it
    /// acted beside it (a post, a signal).
    BoundaryFired {
        run: RunId,
        workflow: WorkflowId,
        step: StepId,
        boundary: Branch,
        diverts: bool,
    },
    /// A file under an open root changed — by an agent, a save, the explorer,
    /// or something outside the platform. `kind: rescan` means the OS queue
    /// overflowed and the client should refetch what it has open.
    FileChanged {
        scope: String,
        id: String,
        path: String,
        kind: FileChangeKind,
        /// The path that was renamed away, for `renamed`.
        from: Option<String>,
        /// Matched by the root's `.gitignore` or `.git/info/exclude`: the
        /// explorer dims it rather than hiding it. A write route's own frame
        /// says `false`; the watcher decides.
        ignored: bool,
        /// The path is a folder — a whole tree made, moved or copied in one
        /// frame, which a reader holding a list of *files* cannot patch from
        /// the name alone. `false` for a path that is gone: nobody can ask
        /// what it was, and a removal is by prefix either way.
        #[serde(default)]
        dir: bool,
    },
    /// A settings layer changed. `scope` is `machine`, `workspace` or
    /// `project`; an open editor re-reads its tab size on it.
    /// A language server spoke (ide/10): diagnostics for a document, or the
    /// server's own lifecycle (`bisa/serverStarted`, `…Failed`,
    /// `…Stopped`). URIs in `params` are root-relative.
    Lsp {
        scope: String,
        id: String,
        language: String,
        method: String,
        params: serde_json::Value,
    },
    SettingsChanged {
        scope: String,
        project: Option<ProjectId>,
        keys: Vec<String>,
    },
    /// A person on another node joined, left or changed role
    /// (14-collaboration). The People panel, the sidebar and the roster
    /// pickers re-read on it.
    PeopleChanged {
        pubkey: bisa_core::PrincipalId,
        change: bisa_store::PeopleChange,
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// An invitation moved — made, claimed, admitted, refused, withdrawn,
    /// expired. Carries the record, never a secret.
    InviteChanged {
        invite: bisa_core::Invite,
    },
    /// A message from a person on another node is held from agents, and
    /// why; the owner sees it with a caution and may release it.
    MessageHeld {
        scope: String,
        event: String,
        author: bisa_core::PrincipalId,
        reason: bisa_store::HeldReason,
    },
    /// A held message was let through: agents may hear it now.
    MessageReleased {
        scope: String,
        event: String,
    },
    /// The content screen read what an agent was about to read from outside
    /// (11-security §What an agent reads from outside): `source` in words —
    /// `example.com`, `github.com/org/repo#12` — and what became of it —
    /// `safe`, `allowed` (by the person), `withheld` (denied by the policy or
    /// the person), `unscreened` (the screen is off). Never the text.
    ContentScreened {
        #[serde(skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        agent: String,
        source: String,
        verdict: crate::content::ContentVerdict,
    },
    /// What this node holds of a workspace it is a guest of moved — joined,
    /// left, a channel or a message arrived. The hosted sections re-read.
    HostedChanged {
        host: bisa_core::PrincipalId,
        #[serde(skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
    },
    /// The relays this node talks to changed or a relay's health moved.
    RelaysChanged,
    /// The person's git setup changed through the platform (ide/04, ide/08):
    /// a profile was saved or removed, an SSH key generated or loaded, a code
    /// host account added, forgotten or made the default. The Settings panels
    /// and the Repository view's connection card re-read on it. Never carries
    /// a value — only which part moved.
    GitSetupChanged {
        what: GitSetup,
    },
    /// A connector definition or one of its accounts on this machine changed
    /// — added, edited, forgotten, connected, made the default, its tokens
    /// refreshed. Settings › Connectors and the designer's connector forms
    /// re-read on it. Never carries a value — only which part moved.
    ConnectorsChanged {
        what: ConnectorsChange,
    },
    /// An addon was installed, removed, enabled, disabled or re-granted
    /// (18 — Addons). Settings › Addons and the addon layer re-read on it.
    /// Never a value — which addon, and which part moved.
    AddonsChanged {
        #[serde(flatten)]
        what: crate::addons::AddonsChange,
    },
    Paused,
    Resumed,
}

/// What became of a signal at its listener, for [`EnginePayload::ListenerFired`].
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum FiredOutcome {
    /// It started this run — on `goal`, or in the workspace.
    Started {
        run: RunId,
        #[serde(skip_serializing_if = "Option::is_none")]
        goal: Option<GoalId>,
    },
    /// It started nothing, and why: the guard dropped it, the chain refused
    /// it, its host stopped listening.
    Skipped { reason: String },
}

/// What moved on a conversation, for [`EnginePayload::ConversationChanged`].
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ConversationChange {
    Renamed,
    Archived,
    Unarchived,
    /// The mode moved — `manual`, `auto` or `plan` (ide/20).
    Mode,
    Deleted,
}

/// Which part of the connectors a [`EnginePayload::ConnectorsChanged`] names.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorsChange {
    /// The definitions: one created, edited or removed.
    Definitions,
    /// This machine's accounts: one added, edited, forgotten, connected or
    /// made the default, or its tokens refreshed.
    Accounts,
}

impl ConnectorsChange {
    pub fn as_str(self) -> &'static str {
        match self {
            ConnectorsChange::Definitions => "definitions",
            ConnectorsChange::Accounts => "accounts",
        }
    }
}

/// Which part of the git setup a [`EnginePayload::GitSetupChanged`] names.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum GitSetup {
    Profiles,
    Keys,
    Accounts,
    /// The person's global git identity — `user.name` / `user.email` in
    /// their global config, written from Settings › Git & code hosts. Every
    /// repository that inherits it now resolves differently.
    Identity,
}

impl GitSetup {
    pub fn as_str(self) -> &'static str {
        match self {
            GitSetup::Profiles => "profiles",
            GitSetup::Keys => "keys",
            GitSetup::Accounts => "accounts",
            GitSetup::Identity => "identity",
        }
    }
}

/// What happened to a file, as the watcher and the write routes report it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileChangeKind {
    Created,
    Modified,
    Removed,
    Renamed,
    Rescan,
}

impl EngineEvent {
    pub fn scoped(goal: GoalId, work_item: Option<WorkItemId>, payload: EnginePayload) -> Self {
        EventScope::of_goal(goal).event(work_item, payload)
    }

    /// An event about a run — a goal's or the workspace's: the goal when it
    /// has one, and its workflow either way.
    pub fn of_run(
        run: &WorkflowRun,
        work_item: Option<WorkItemId>,
        payload: EnginePayload,
    ) -> Self {
        EventScope::of_run(run).event(work_item, payload)
    }

    pub fn global(payload: EnginePayload) -> Self {
        EventScope::default().event(None, payload)
    }
}

/// What an event's envelope says it is about: the goal, the workflow and the
/// run behind it — or none, for the workspace's own news. Worked out once
/// where a run's work starts, and stamped on everything it emits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventScope {
    pub goal: Option<GoalId>,
    pub workflow: Option<WorkflowId>,
    pub run: Option<RunId>,
}

impl EventScope {
    pub fn of_goal(goal: GoalId) -> Self {
        Self {
            goal: Some(goal),
            workflow: None,
            run: None,
        }
    }

    /// A run's: its goal when it is a goal's, its workflow, and itself.
    pub fn of_run(run: &WorkflowRun) -> Self {
        Self {
            goal: run.scope.goal(),
            workflow: Some(run.workflow.id),
            run: Some(run.id),
        }
    }

    pub fn event(self, work_item: Option<WorkItemId>, payload: EnginePayload) -> EngineEvent {
        EngineEvent {
            goal: self.goal,
            workflow: self.workflow,
            run: self.run,
            work_item,
            payload,
        }
    }
}

impl EnginePayload {
    /// The topic a `platform` start, wait or boundary names: `run.finished`,
    /// `step.changed`, `goal.closed`, … One per variant, snake-cased with a
    /// dot, so a filter reads plainly and a new variant can never answer to
    /// an old name. Every topic is in [`TOPICS`].
    pub fn topic(&self) -> &'static str {
        match self {
            EnginePayload::Session { .. } => "session",
            EnginePayload::Scheduled { .. } => "work_item.scheduled",
            EnginePayload::ExecutionEnded { .. } => "work_item.execution_ended",
            EnginePayload::SessionState { .. } => "session.state",
            EnginePayload::SessionGone { .. } => "session.gone",
            EnginePayload::GateOpened { .. } => "gate.opened",
            EnginePayload::QuestionAsked { .. } => "question.asked",
            EnginePayload::Guided { .. } => "guided.status",
            EnginePayload::AgentThinking { .. } => "agent.thinking",
            EnginePayload::AgentStreamed { .. } => "agent.streamed",
            EnginePayload::AgentReplied { .. } => "agent.replied",
            EnginePayload::GateDecided { .. } => "gate.decided",
            EnginePayload::ResultAccepted => "result.accepted",
            EnginePayload::RunStarted { .. } => "run.started",
            EnginePayload::RunQueued { .. } => "run.queued",
            EnginePayload::RunFinished { .. } => "run.finished",
            EnginePayload::RunCancelled { .. } => "run.cancelled",
            EnginePayload::StepChanged { .. } => "step.changed",
            EnginePayload::GoalClosed { .. } => "goal.closed",
            EnginePayload::GoalCreated { .. } => "goal.created",
            EnginePayload::WorkflowProposed { .. } => "workflow.proposed",
            EnginePayload::WorkflowChanged { .. } => "workflow.changed",
            EnginePayload::WorkflowDeleted { .. } => "workflow.deleted",
            EnginePayload::WorkflowArchived { .. } => "workflow.archived",
            EnginePayload::AttachmentChanged { .. } => "attachment.changed",
            EnginePayload::GoalArchived { .. } => "goal.archived",
            EnginePayload::GoalDeleted { .. } => "goal.deleted",
            EnginePayload::ProjectArchived { .. } => "project.archived",
            EnginePayload::ProjectDeleted { .. } => "project.deleted",
            EnginePayload::DocumentAdded { .. } => "document.added",
            EnginePayload::ProjectCreated { .. } => "project.created",
            EnginePayload::CommitterNeeded { .. } => "committer.needed",
            EnginePayload::CommitterSet { .. } => "committer.set",
            EnginePayload::ProjectChanged { .. } => "project.changed",
            EnginePayload::WorkstreamEdited { .. } => "workstream.edited",
            EnginePayload::NoteChanged { .. } => "note.changed",
            EnginePayload::DrawingChanged { .. } => "drawing.changed",
            EnginePayload::DrawingRequest { .. } => "drawing.request",
            EnginePayload::WorkstreamOpened { .. } => "workstream.opened",
            EnginePayload::WorkstreamChanged { .. } => "workstream.changed",
            EnginePayload::WorkstreamCommitted { .. } => "workstream.committed",
            EnginePayload::WorkstreamScriptRan { .. } => "workstream.script_ran",
            EnginePayload::WorkstreamPublishFailed { .. } => "workstream.publish_failed",
            EnginePayload::ServerChanged { .. } => "workstream.server_changed",
            EnginePayload::BrowserRequest { .. } => "browser.request",
            EnginePayload::MobileDevelopmentChanged { .. } => "mobile_development.changed",
            EnginePayload::McpProbed { .. } => "mcp.probed",
            EnginePayload::PeopleChanged { .. } => "people.changed",
            EnginePayload::InviteChanged { .. } => "invite.changed",
            EnginePayload::MessageHeld { .. } => "message.held",
            EnginePayload::MessageReleased { .. } => "message.released",
            EnginePayload::ContentScreened { .. } => "content.screened",
            EnginePayload::HostedChanged { .. } => "hosted.changed",
            EnginePayload::RelaysChanged => "relays.changed",
            EnginePayload::GuardDecided { .. } => "guard.decided",
            EnginePayload::Judged { .. } => "decision.judged",
            EnginePayload::Redacted { .. } => "security.redacted",
            EnginePayload::ChangesMoved { .. } => "changes.moved",
            EnginePayload::ChangesSettled { .. } => "changes.settled",
            EnginePayload::AskOpened { .. } => "conversation.ask_opened",
            EnginePayload::AskSettled { .. } => "conversation.ask_settled",
            EnginePayload::ModelSwitched { .. } => "model.switched",
            EnginePayload::SignalReceived { .. } => "signal.received",
            EnginePayload::ListenerFired { .. } => "listener.fired",
            EnginePayload::ListenerFailed { .. } => "listener.failed",
            EnginePayload::ListeningChanged { .. } => "listening.changed",
            EnginePayload::BoundaryFired { .. } => "boundary.fired",
            EnginePayload::FileChanged { .. } => "file.changed",
            EnginePayload::Lsp { .. } => "lsp.notified",
            EnginePayload::SettingsChanged { .. } => "settings.changed",
            EnginePayload::GitSetupChanged { .. } => "git.setup_changed",
            EnginePayload::ConnectorsChanged { .. } => "connectors.changed",
            EnginePayload::AddonsChanged { .. } => "addons.changed",
            EnginePayload::ConversationCreated { .. } => "conversation.created",
            EnginePayload::ConversationChanged { .. } => "conversation.changed",
            EnginePayload::Paused => "engine.paused",
            EnginePayload::Resumed => "engine.resumed",
        }
    }

    /// The fields a `platform` filter may match on. Flat, stringly-typed and
    /// exact-match only — the same closed grammar as a `decide` step's
    /// conditions, for the same reason. The words an agent streams are never
    /// fields: too many frames, and a filter reads what an agent did, not
    /// each token it wrote.
    pub fn fields(&self) -> BTreeMap<String, String> {
        let mut f = BTreeMap::new();
        let mut put = |k: &str, v: String| {
            f.insert(k.to_string(), v);
        };
        match self {
            EnginePayload::Scheduled { harness } => put("harness", harness.clone()),
            EnginePayload::ExecutionEnded { outcome } => {
                put("outcome", outcome.as_str().to_string())
            }
            EnginePayload::SessionState { presence, .. } => {
                put("state", presence.state.as_str().to_string());
                put("kind", format!("{:?}", presence.kind).to_lowercase());
                put("harness", presence.harness.clone());
                if let Some(agent) = &presence.agent {
                    put("agent", agent.to_string());
                }
            }
            EnginePayload::SessionGone { live_run } => put("live_run", live_run.to_string()),
            EnginePayload::GuardDecided {
                tool,
                verdict,
                by,
                rule,
                ..
            } => {
                put("tool", tool.clone());
                put("verdict", verdict.as_str().to_string());
                put("by", by.as_str().to_string());
                if let Some(rule) = rule {
                    put("rule", rule.clone());
                }
            }
            EnginePayload::Redacted { count, at, .. } => {
                put("count", count.to_string());
                put("at", at.clone());
            }
            EnginePayload::Judged { judgement, .. } => {
                put("point", judgement.point.as_str().to_string());
                put("outcome", judgement.outcome.as_str().to_string());
                put("provider", judgement.provider.as_str().to_string());
            }
            EnginePayload::GateOpened { gate, .. } => {
                put("gate", format!("{gate:?}").to_lowercase())
            }
            EnginePayload::GateDecided { gate, approve, .. } => {
                put("gate", format!("{gate:?}").to_lowercase());
                put("approve", approve.to_string());
            }
            EnginePayload::Guided {
                phase,
                status,
                detail,
                ..
            } => {
                put("phase", phase.as_str().to_string());
                put("status", status.as_str().to_string());
                if let Some(d) = detail {
                    put("detail", d.clone());
                }
            }
            EnginePayload::AgentThinking { scope, agent }
            | EnginePayload::AgentStreamed { scope, agent, .. }
            | EnginePayload::AgentReplied { scope, agent, .. } => {
                put("scope", scope.clone());
                put("agent", agent.clone());
            }
            EnginePayload::RunStarted { run, workflow } => {
                put("run", run.to_string());
                put("workflow", workflow.to_string());
            }
            EnginePayload::RunQueued {
                run,
                workflow,
                position,
            } => {
                put("run", run.to_string());
                put("workflow", workflow.to_string());
                put("position", position.to_string());
            }
            EnginePayload::RunFinished {
                run,
                workflow,
                outcome,
            } => {
                put("run", run.to_string());
                put("workflow", workflow.to_string());
                put("outcome", outcome.as_str().to_string());
            }
            EnginePayload::RunCancelled {
                run,
                workflow,
                cause,
            } => {
                put("run", run.to_string());
                put("workflow", workflow.to_string());
                put("cause", cause.as_str().to_string());
            }
            EnginePayload::StepChanged {
                run,
                workflow,
                step,
                state,
                kind,
            } => {
                put("run", run.to_string());
                put("workflow", workflow.to_string());
                put("step", step.to_string());
                put("state", state.clone());
                put("kind", kind.clone());
            }
            EnginePayload::GoalClosed { reason } => put("reason", reason.as_str().to_string()),
            EnginePayload::WorkflowProposed {
                workflow, revision, ..
            } => {
                put("workflow", workflow.to_string());
                put("revision", revision.to_string());
            }
            EnginePayload::WorkflowChanged {
                workflow,
                revision,
                designed,
            } => {
                put("workflow", workflow.to_string());
                put("revision", revision.to_string());
                put("designed", designed.to_string());
            }
            EnginePayload::AttachmentChanged { project, attached } => {
                put("project", project.to_string());
                put("attached", attached.to_string());
            }
            EnginePayload::GoalArchived { goal, archived } => {
                put("goal", goal.to_string());
                put("archived", archived.to_string());
            }
            EnginePayload::GoalDeleted { goal } => put("goal", goal.to_string()),
            EnginePayload::WorkflowDeleted { workflow } => put("workflow", workflow.to_string()),
            EnginePayload::WorkflowArchived { workflow, archived } => {
                put("workflow", workflow.to_string());
                put("archived", archived.to_string());
            }
            EnginePayload::ProjectArchived { project, archived } => {
                put("project", project.to_string());
                put("archived", archived.to_string());
            }
            EnginePayload::ProjectDeleted { project } => put("project", project.to_string()),
            EnginePayload::GoalCreated { goal, origin } => {
                put("goal", goal.to_string());
                put("origin", origin.as_str().to_string());
            }
            EnginePayload::DocumentAdded { goal, file } => {
                put("goal", goal.to_string());
                put("name", file.name.clone());
                put("mime", file.mime.clone());
                put("sha256", file.sha256.clone());
            }
            EnginePayload::ProjectCreated {
                project,
                slug,
                origin,
            } => {
                put("project", project.to_string());
                put("slug", slug.clone());
                put("origin", origin.as_str().to_string());
                if let Some(goal) = origin.goal() {
                    put("origin_goal", goal.to_string());
                }
                if let Some(wf) = origin.workflow() {
                    put("origin_workflow", wf.to_string());
                }
            }
            EnginePayload::CommitterNeeded {
                project,
                slug,
                reason,
                origin,
                ..
            } => {
                put("project", project.to_string());
                put("slug", slug.clone());
                put("reason", reason.as_str().to_string());
                put("origin", origin.as_str().to_string());
            }
            EnginePayload::CommitterSet { project, .. } => put("project", project.to_string()),
            EnginePayload::GitSetupChanged { what } => put("what", what.as_str().to_string()),
            EnginePayload::ConnectorsChanged { what } => put("what", what.as_str().to_string()),
            EnginePayload::AddonsChanged { what } => {
                put("what", what.as_str().to_string());
                put("id", what.id().to_string());
            }
            EnginePayload::ProjectChanged { project } => put("project", project.to_string()),
            EnginePayload::WorkstreamEdited {
                workstream,
                project,
            } => {
                put("workstream", workstream.to_string());
                put("project", project.to_string());
            }
            EnginePayload::WorkstreamOpened {
                workstream,
                project,
                ..
            } => {
                put("workstream", workstream.to_string());
                put("project", project.to_string());
            }
            EnginePayload::WorkstreamChanged { workstream, state } => {
                put("workstream", workstream.to_string());
                put("state", state.as_str().to_string());
            }
            EnginePayload::WorkstreamPublishFailed {
                workstream,
                project,
                what,
                ..
            } => {
                put("workstream", workstream.to_string());
                put("project", project.to_string());
                put("what", what.clone());
            }
            EnginePayload::WorkstreamCommitted {
                workstream,
                branch,
                commit,
            } => {
                put("workstream", workstream.to_string());
                put("branch", branch.clone());
                put("commit", commit.clone());
            }
            EnginePayload::ModelSwitched { from, to, .. } => {
                put("from", from.clone());
                put("to", to.clone());
            }
            EnginePayload::SignalReceived {
                signal,
                listener,
                source,
            } => {
                put("signal", signal.clone());
                put("source", source.as_str().to_string());
                if let Some(listener) = listener {
                    put("listener", listener.to_string());
                }
            }
            EnginePayload::ListenerFired {
                listener,
                signal,
                outcome,
            } => {
                put("listener", listener.to_string());
                put("signal", signal.clone());
                match outcome {
                    FiredOutcome::Started { run, goal } => {
                        put("outcome", "started".to_string());
                        put("run", run.to_string());
                        if let Some(goal) = goal {
                            put("goal", goal.to_string());
                        }
                    }
                    FiredOutcome::Skipped { .. } => put("outcome", "skipped".to_string()),
                }
            }
            EnginePayload::ListenerFailed { listener, .. } => put("listener", listener.to_string()),
            EnginePayload::ListeningChanged { host, on } => {
                put("host", host.to_string());
                put("on", on.to_string());
            }
            EnginePayload::BoundaryFired {
                run,
                workflow,
                step,
                boundary,
                diverts,
            } => {
                put("run", run.to_string());
                put("workflow", workflow.to_string());
                put("step", step.to_string());
                put("boundary", boundary.to_string());
                put("diverts", diverts.to_string());
            }
            _ => {}
        }
        f
    }
}

/// Every topic [`EnginePayload::topic`] answers, in its order — what
/// validation holds a `platform` start, wait or boundary to (the engine hands
/// it to the store as [`bisa_store::KnownRuntime::topics`]).
pub const TOPICS: &[&str] = &[
    "session",
    "work_item.scheduled",
    "work_item.execution_ended",
    "session.state",
    "session.gone",
    "gate.opened",
    "question.asked",
    "guided.status",
    "agent.thinking",
    "agent.streamed",
    "agent.replied",
    "gate.decided",
    "result.accepted",
    "run.started",
    "run.queued",
    "run.finished",
    "run.cancelled",
    "step.changed",
    "goal.closed",
    "goal.created",
    "workflow.proposed",
    "workflow.changed",
    "workflow.deleted",
    "workflow.archived",
    "attachment.changed",
    "goal.archived",
    "goal.deleted",
    "project.archived",
    "project.deleted",
    "document.added",
    "project.created",
    "committer.needed",
    "committer.set",
    "project.changed",
    "workstream.edited",
    "note.changed",
    "drawing.changed",
    "drawing.request",
    "workstream.opened",
    "workstream.changed",
    "workstream.committed",
    "workstream.script_ran",
    "workstream.publish_failed",
    "workstream.server_changed",
    "browser.request",
    "mobile_development.changed",
    "mcp.probed",
    "people.changed",
    "invite.changed",
    "message.held",
    "message.released",
    "content.screened",
    "hosted.changed",
    "relays.changed",
    "guard.decided",
    "decision.judged",
    "security.redacted",
    "changes.moved",
    "changes.settled",
    "conversation.ask_opened",
    "conversation.ask_settled",
    "model.switched",
    "signal.received",
    "listener.fired",
    "listener.failed",
    "listening.changed",
    "boundary.fired",
    "file.changed",
    "lsp.notified",
    "settings.changed",
    "git.setup_changed",
    "connectors.changed",
    "addons.changed",
    "conversation.created",
    "conversation.changed",
    "engine.paused",
    "engine.resumed",
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The dotted names inside `fn topic`, read from this file: the list and
    /// the match say the same thing, so a variant added to one and not the
    /// other fails here rather than in a workflow that names it.
    #[test]
    fn every_topic_is_listed_once_and_the_list_names_only_topics() {
        let src = include_str!("events.rs");
        let start = src.find("pub fn topic(&self)").expect("topic exists");
        let body = &src[start..];
        let end = body.find("\n    }\n").expect("the match ends");
        let mut matched: Vec<&str> = body[..end].split('"').skip(1).step_by(2).collect();
        matched.sort_unstable();
        let mut listed: Vec<&str> = TOPICS.to_vec();
        listed.sort_unstable();
        let mut deduped = listed.clone();
        deduped.dedup();
        assert_eq!(deduped, listed, "a topic listed twice");
        assert_eq!(matched, listed);
        assert!(!TOPICS.iter().any(|t| t.starts_with("trigger")));
    }

    #[test]
    fn a_listener_fact_carries_its_listener_and_outcome_as_fields() {
        let listener: ListenerKey = format!(
            "workspace:{}/nightly",
            WorkflowId::from_ulid(ulid::Ulid::from_parts(1, 1))
        )
        .parse()
        .unwrap();
        let run = RunId::from_ulid(ulid::Ulid::from_parts(2, 2));
        let fired = EnginePayload::ListenerFired {
            listener: listener.clone(),
            signal: "01S".into(),
            outcome: FiredOutcome::Started { run, goal: None },
        };
        assert_eq!(fired.topic(), "listener.fired");
        let fields = fired.fields();
        assert_eq!(fields["listener"], listener.to_string());
        assert_eq!(fields["outcome"], "started");
        assert_eq!(fields["run"], run.to_string());
        let json = serde_json::to_value(&fired).unwrap();
        assert_eq!(json["type"], "listener_fired");
        assert_eq!(json["outcome"]["outcome"], "started");
        assert!(json["outcome"].get("goal").is_none());
    }
}
