//! Result intake: the unix-socket JSONL server harness-side tools (the MCP
//! layer) call back into.
//!
//! Wire protocol (one JSON object per line, LF-terminated; one reply line per
//! request, in order). Requests are scoped by EITHER `work_item` (worker
//! sessions) or `goal` (designing sessions); ops that shape the goal — revise
//! its statement, propose or amend its workflow — accept goal scope only.
//!
//! ```json
//! {"op":"result_submit","work_item":"<ulid>","output":{...}}
//!   -> {"ok":true,"result_event":"<hex>"}
//!   -> {"ok":false,"errors":["..."],"attempts_left":2}
//! {"op":"progress","work_item":"<ulid>","verb":"...","object":"...","outcome":"..."}
//!   -> {"ok":true}
//! {"op":"ask_human","work_item":"<ulid>"|,"goal":"<ulid>","question":"...","expects":"decision"|"answer",
//!  "options":[{"id":"...","label":"...","detail":"...","recommended":false}],"multi":false}
//!   -> {"ok":true,"gate":"<gate-id>"}
//! {"op":"await_decision","gate":"<gate-id>"}
//!   -> {"ok":true,"approve":true|false,
//!       "answer":{"selected":["..."],"text":"..."|null,"unsure":false}|null,
//!       "clarify_rounds_left":<n>}   (long-polls; the last field only on "unsure")
//! {"op":"get_goal","goal":"<ulid>"|,"work_item":"<ulid>"}
//!   -> {"ok":true,"goal":{...},"run":{...}|null,"work_items":[...],"projects":[...],
//!       "notes":[...],"journal_tail":[...]}
//!   (refused, by name, for a work item of a run of the workspace: it has no goal)
//! {"op":"get_run","run":"<ulid>"|,"work_item":"<ulid>"}
//!   -> {"ok":true,"run":{...},"home":"goal:<ulid>"|"run:<ulid>","goal":"<ulid>"|null,
//!       "budget":{...},"spent":{...},"work_items":[...],"journal_tail":[...]}
//! {"op":"revise_statement","goal":"<ulid>","statement":"...","why":"..."}
//! {"op":"propose_workflow","goal":"<ulid>","agent":"...","workflow":{"name":"...",
//!  "description":"...","inputs":[...],"steps":[...],"tags":[...]}}
//!   -> {"ok":true,"workflow":"<ulid>","revision":n,"gate":"<gate-id>"}
//!   -> {"ok":false,"errors":[...],"problems":[{"step":"...","kind":"...","message":"..."}]}
//! {"op":"amend_workflow","goal":"<ulid>","agent":"...","workflow":{...}}
//!   -> {"ok":true,"gate":"<gate-id>"}
//! {"op":"add_note","goal"|"work_item":"<ulid>","text":"..."}
//! {"op":"spawn_sub_goal","goal"|"work_item":"<ulid>","statement":"...","title":"..."}
//!   -> {"ok":true,"child":"<ulid>"}
//! {"op":"browser","request":{"action":"open"|"tabs"|"read"|"find"|"snapshot"|"click"|"fill"|"type"|"press"
//!  |"select"|"hover"|"scroll"|"wait"|"back"|"forward"|"reload"|"console"|"eval"|"close"|"screenshot",
//!  "tab":"b1","url":"...","target":"<selector or ref>","query":"...","text":"...","key":"Enter",
//!  "until":"load"|"selector"|"text"|"gone"|"idle","timeout_ms":10000,"expression":"..."},
//!  "scope":"...","agent":"..."}
//!   -> {"ok":true,"result":{"ok":true,"tab":"b1","url":"...","title":"...","text":"...","tabs":[...],
//!       "navigated":true,"dialogs":[...],"console":[...],"scroll":{...},"value":...}}
//!   (waits for the desktop; `result.ok:false` with `error` when nobody answers — ide/18)
//! {"op":"mobile_development","request":{"action":"status"|"devices"|"boot"|"screenshot","device":"<id>","fresh":true},
//!  "scope":"...","agent":"..."}
//!   -> {"ok":true,"result":{...}}   (the node asks this machine's simulators and adb itself — ide/19;
//!   a refusal — off, nobody, a platform that is off, no tools — is an error the agent reads)
//! ```
//!
//! Plus message/recall ops delegating to the store's conversation surface.
//! Nothing in intake can record a gate Decision — humans decide, governance
//! is the backstop.
//!
//! ## The core agents' ops
//!
//! Some ops belong to the platform's own agents alone. The General Agent and the Workflow Agent may
//! read the shape of the workspace and browse the catalog; the **General
//! Agent** alone installs, assigns and captures what recurs as a standing
//! goal; the **Workflow
//! Agent** alone reads the template library and validates a definition. Each
//! carries `agent` and is refused for anyone else — see [`core_agent_only`]
//! for why the check is here and not only in the MCP tool list.
//!
//! `create_project` is documented with them because it shares their wire
//! shape, and is deliberately **not** one of them: making the folder the work
//! happens in is the work, not the platform's business, and a session that
//! cannot do it writes into its scratch folder instead.
//!
//! ```json
//! {"op":"workspace_overview","agent":"general-agent"|"workflow-agent"}
//!   -> {"ok":true,"agents":{...},"teams":[...],"channels":[...],"skills":n,
//!       "mcp_servers":{...},"listening":{...},"projects":[...],"goals":{...},
//!       "workflows":{...},"running":[...],"catalog":{...}}
//! {"op":"list_staff","agent":"general-agent"|"workflow-agent","goal":"<ulid>"|null}
//!   -> {"ok":true,"agents":[...],"teams":[...],"scoped":bool,"text":"STAFF — …"}
//!   (`goal`: that goal's roster — the agents and teams it names to carry it,
//!   when it names any; absent, the whole enabled staff)
//! {"op":"list_catalog","agent":"...","kind":"agent"|"skill"|"team"|"channel"|"workflow"|null}
//!   (`null`: every kind of the catalog, connectors and addons among them)
//!   -> {"ok":true,"entries":[{"kind","slug","name","description","tags",
//!                             "requires","installed"}]}
//! {"op":"install_catalog_entry","agent":"general-agent","kind":"...","slug":"...",
//!  "goal":"<ulid>"|null}
//!   -> {"ok":true,"installed":{"agents":[],"skills":[],"teams":[],"channels":[],"workflows":[]}}
//! {"op":"assign","agent":"general-agent","goal":"<ulid>","project":"<ulid>"|null,
//!  "assignees":["agent:developer","team:engineering","human:<64hex>"],"replace":bool}
//!   -> {"ok":true,"assignees":["agent:developer", ...]}
//! {"op":"capture_goal","agent":"general-agent","statement":"Every Monday, post the digest…",
//!  "title":"..."|null}
//!   -> {"ok":true,"goal":"<ulid>"}   (the Workflow Agent designs it — the start event its
//!   statement says — and it listens once its design is adopted)
//! {"op":"list_workflow_templates","agent":"workflow-agent"}
//!   -> {"ok":true,"templates":[{"slug","name","description","tags","requires","installed"}],
//!       "workflows":[{"id","name","description","steps","origin"}]}
//! {"op":"get_workflow","agent":"workflow-agent","workflow":"<ulid>"|"<slug>"}
//!   -> {"ok":true,"workflow":{...},"problems":[...]}
//! {"op":"validate_workflow","agent":"workflow-agent","workflow":{...},"goal":"<ulid>"|null}
//!   -> {"ok":true,"problems":[...]}   (staffing judged against `goal`'s roster when given)
//! {"op":"list_connectors","agent":"<id>"}
//!   -> {"ok":true,"connectors":[{"id","name","description","auth","accounts","operations"}],
//!       "text":"CONNECTORS — ..."}
//! {"op":"call_connector","connector":"<slug>","operation":"<id>","account":"<ulid>"|absent,
//!  "params":{"<name>":"<text>"},"agent":"...","work_item":"<ulid>"|absent,"goal":...|absent,"scope":...|absent}
//!   -> {"ok":true,"screen":{"verdict","note"},"output":<selected>,"truncated":bool}
//!    | {"ok":true,"withheld":true,"text":"..."}   (a write is refused: a step behind a gate)
//! {"op":"create_project","goal":"<ulid>","slug":"...","name":"..."|null,
//!  "assignees":["agent:developer"],"agent":"..."|absent,"work_item":"<ulid>"|absent}
//!   -> {"ok":true,"project":"<ulid>","slug":"...","path":"..."}
//! ```
//!
//! `assign`, `capture_goal`, `propose_workflow` and `create_project` change
//! the workspace, so each journals a Note. An autonomous change is only
//! acceptable while it stays visible after the fact — and the Note is signed
//! by the agent that made the change, not by the platform, because
//! `create_project` is reachable by any session and a folder a Developer made
//! must not read as a core agent's work.
//!
//! **No op accepts `spec.agent`.** Work items exist only because an `agent`
//! step asked for one; the engine writes the runner back once it has picked.
//! Provenance is recorded, never accepted.

use crate::events::{EngineEvent, EnginePayload};
use crate::{effects, executor, ops, warn_on_err, Inner};
use bisa_core::event::JournalPayload;
use bisa_core::workitem::{WorkItemSpec, WorkItemState};
use bisa_core::{
    AgentId, Assignee, GoalId, GoalOrigin, Home, MessageBody, ProjectId, ProjectRoot,
    PublishPolicy, RunId, Slug, StepState, Vcs, WorkItemId, WorkItemTransition, Workflow,
    WorkflowId, WorkflowOrigin,
};
use bisa_core::{AskKind, AskOption, Gate};
use bisa_store::{CatalogKind, NewProject, NewWorkflow, PostOrigin, StoreError};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

/// One offered answer, in the shape a tool call sends it.
#[derive(Deserialize)]
struct AskOptionInput {
    id: String,
    label: String,
    #[serde(default)]
    detail: Option<String>,
    #[serde(default)]
    recommended: bool,
}

impl From<AskOptionInput> for AskOption {
    fn from(o: AskOptionInput) -> Self {
        AskOption {
            id: o.id,
            label: o.label,
            detail: o.detail,
            recommended: o.recommended,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case", tag = "op")]
enum Op {
    ResultSubmit {
        work_item: WorkItemId,
        output: serde_json::Value,
    },
    Progress {
        work_item: WorkItemId,
        verb: String,
        object: String,
        #[serde(default)]
        outcome: Option<String>,
    },
    AskHuman {
        #[serde(default)]
        work_item: Option<WorkItemId>,
        #[serde(default)]
        goal: Option<String>,
        question: String,
        #[serde(default)]
        expects: Option<String>,
        /// The answers the asker is offering. Empty is fine and common: a
        /// free-text question is the right shape when the answer is not
        /// enumerable.
        #[serde(default)]
        options: Vec<AskOptionInput>,
        /// Whether more than one option may be chosen.
        #[serde(default)]
        multi: bool,
    },
    AwaitDecision {
        gate: String,
    },
    /// Ask the embedded browser to do something (ide/18). Every scope may
    /// ask; whether this agent may is the workspace's word (`browser.*`),
    /// checked here. The engine decides where the tab is at home
    /// (`browser::home_of`): the checkout a conversation about one runs
    /// in, the goal, the channel or the direct message, the work item's
    /// checkout, the conversation — else the workspace's Browser pane —
    /// and keeps it out of sight when the goal runs unattended
    /// (`browser.agents.headless`), unless the agent asked.
    /// `browser_serve` (ide/18 §Serving a folder): the platform serves a
    /// folder of the session's checkout — the checkout itself when none is
    /// named — on a loopback port through the node's server, and answers
    /// the URL to `browser_open`. The same word on who may use the browser
    /// as every browser tool; a session in no checkout, or an engine no
    /// node lent a server to, is told so in a sentence.
    BrowserServe {
        #[serde(default)]
        folder: Option<String>,
        #[serde(default)]
        scope: Option<String>,
        #[serde(default)]
        agent: Option<String>,
        #[serde(default)]
        work_item: Option<WorkItemId>,
    },
    /// `decide`: a session puts typed questions about a state to the
    /// Decision-Making Agent and reads the answers. **This is the boundary**:
    /// the tool is on every session's menu, and the engine answers only a
    /// session whose agent has the Decision-Making Agent on — its own switch,
    /// or the workspace's.
    Decide {
        request: bisa_core::DecisionRequest,
        #[serde(default)]
        scope: Option<String>,
        #[serde(default)]
        agent: Option<String>,
        #[serde(default)]
        work_item: Option<WorkItemId>,
        #[serde(default)]
        goal: Option<GoalCandidate>,
    },
    Browser {
        request: crate::browser::BrowserRequest,
        #[serde(default)]
        scope: Option<String>,
        #[serde(default)]
        agent: Option<String>,
        #[serde(default)]
        work_item: Option<WorkItemId>,
        /// The goal a goal-scoped session works on — the one a conversation
        /// scope names is read from the scope itself.
        #[serde(default)]
        goal: Option<GoalCandidate>,
    },
    /// A drawing tool (19 — Drawings): a list, a reading, a new drawing or
    /// an erasure the engine answers from the store; a skeleton, a Mermaid
    /// text or a snapshot parked for the desktop's canvas. Whether this
    /// agent may is the workspace's word (`draw.*`), checked here; a call
    /// naming no drawing means the conversation's.
    Draw {
        request: crate::drawings::DrawRequest,
        #[serde(default)]
        scope: Option<String>,
        #[serde(default)]
        agent: Option<String>,
        #[serde(default)]
        work_item: Option<WorkItemId>,
        #[serde(default)]
        goal: Option<GoalCandidate>,
    },
    /// Ask this machine's mobile tools (ide/19): the toolchain, the
    /// devices, a boot, a device's screen. Every scope may ask; whether this
    /// agent may is the workspace's word (`mobile_development.*`), checked here.
    MobileDevelopment {
        request: MobileDevelopmentRequest,
        #[serde(default)]
        scope: Option<String>,
        #[serde(default)]
        agent: Option<String>,
        #[serde(default)]
        work_item: Option<WorkItemId>,
    },
    GetGoal {
        #[serde(default)]
        work_item: Option<WorkItemId>,
        #[serde(default)]
        goal: Option<String>,
    },
    /// The run a session works in — a goal's or the workspace's — as
    /// `get_goal` reads a goal (`get_run`).
    GetRun {
        #[serde(default)]
        work_item: Option<WorkItemId>,
        #[serde(default)]
        run: Option<String>,
    },
    ReviseStatement {
        goal: String,
        statement: String,
        #[serde(default)]
        why: Option<String>,
    },
    /// Propose the workflow a goal will run. Validated, recorded, and gated
    /// for the person to adopt; nothing is installed and nothing starts.
    ProposeWorkflow {
        goal: String,
        agent: String,
        workflow: NewWorkflow,
    },
    /// Propose an amendment to a goal's running workflow. Gated.
    AmendWorkflow {
        goal: String,
        agent: String,
        workflow: NewWorkflow,
    },
    /// Read one note: the named one, else the note the session's
    /// conversation is about (`ConversationOrigin::Note`).
    NoteRead {
        #[serde(default)]
        note: Option<String>,
        #[serde(default)]
        scope: Option<String>,
    },
    /// Add a block to the end of a note — the named one, else the
    /// conversation's. Adds under the agent's name and never changes what
    /// is there: the write for "add this".
    NoteAppend {
        #[serde(default)]
        note: Option<String>,
        text: String,
        #[serde(default)]
        agent: Option<String>,
        #[serde(default)]
        scope: Option<String>,
    },
    /// Rewrite a note's body — the named one, else the conversation's — at
    /// the hash the agent read (`note_read` answers it): the write for
    /// "change this". A note that moved since is refused with its current
    /// hash; the person's unsaved text is the desktop's to keep.
    NoteWrite {
        #[serde(default)]
        note: Option<String>,
        text: String,
        base_hash: String,
        #[serde(default)]
        agent: Option<String>,
        #[serde(default)]
        scope: Option<String>,
    },
    AddNote {
        #[serde(default)]
        work_item: Option<WorkItemId>,
        #[serde(default)]
        goal: Option<String>,
        text: String,
    },
    /// Review notes on the projects this session can see: the
    /// named `project`, else the work item's project, else every project
    /// attached to the goal (a goal thread's `scope` names the goal).
    ReviewNotesList {
        #[serde(default)]
        project: Option<String>,
        #[serde(default)]
        work_item: Option<WorkItemId>,
        #[serde(default)]
        goal: Option<String>,
        #[serde(default)]
        scope: Option<String>,
        #[serde(default)]
        include_resolved: bool,
    },
    /// Mark one review note dealt with. `note` is the review note's id — not
    /// a scratchpad note — and is searched across the same projects
    /// `review_notes_list` would show.
    ReviewNoteResolve {
        note: String,
        #[serde(default)]
        project: Option<String>,
        #[serde(default)]
        work_item: Option<WorkItemId>,
        #[serde(default)]
        goal: Option<String>,
        #[serde(default)]
        scope: Option<String>,
    },
    /// The GitHub reviews and resolvable threads on the pull request of the
    /// workstream this session runs in. `scope` names the workstream
    /// (a workstream chat's scope id).
    PrReviewsList {
        #[serde(default)]
        scope: Option<String>,
        /// The session's agent, for the content screen's ask and record.
        #[serde(default)]
        agent: Option<String>,
        #[serde(default)]
        work_item: Option<WorkItemId>,
        #[serde(default)]
        goal: Option<GoalCandidate>,
    },
    /// Submit a GitHub review on that workstream's pull request. `agent` is
    /// the session's agent (the MCP scope puts it on the wire); the engine
    /// signs the review with it so the IDE can tell an agent's review from
    /// the person's, which the one credential cannot.
    PrReviewSubmit {
        #[serde(default)]
        scope: Option<String>,
        #[serde(default)]
        agent: Option<String>,
        event: crate::codehost::ReviewEvent,
        #[serde(default)]
        body: Option<String>,
        #[serde(default)]
        comments: Vec<crate::codehost::ReviewComment>,
    },
    /// Reply on one review thread of that workstream's pull request — and
    /// resolve it in the same act when `resolve` — signed with the session's
    /// agent like a review is.
    PrThreadReply {
        #[serde(default)]
        scope: Option<String>,
        #[serde(default)]
        agent: Option<String>,
        thread: String,
        body: String,
        #[serde(default)]
        resolve: bool,
    },
    /// Resolve (or reopen) one review thread of that workstream's pull request.
    PrThreadResolve {
        #[serde(default)]
        scope: Option<String>,
        thread: String,
        #[serde(default = "default_true")]
        resolved: bool,
    },
    SpawnSubGoal {
        #[serde(default)]
        work_item: Option<WorkItemId>,
        #[serde(default)]
        goal: Option<String>,
        statement: String,
        #[serde(default)]
        title: Option<String>,
    },
    // --- conversation/recall delegation (store surface) ---
    PostMessage {
        scope: String,
        content: String,
        #[serde(default)]
        reply_to: Option<String>,
        /// Who this message addresses, as tokens the scope resolves: a pubkey
        /// hex, an agent definition id, or the scope's own id (the channel
        /// handle). The same tokens the node route takes, resolved by the
        /// same `Workspace::resolve_mentions` — an agent that could only post
        /// unaddressed messages could not hand a question on to anybody.
        #[serde(default)]
        mentions: Vec<String>,
        /// Files to attach, as paths the agent has already written.
        ///
        /// Paths rather than bytes, because an agent produces a file by
        /// writing one — and paths **inside the session's own roots**
        /// ([`session_roots`]), because posting a file is publishing it: an
        /// agent may hand over what it made where it works and not something
        /// it found elsewhere on the disk.
        #[serde(default)]
        attachments: Vec<String>,
        /// What the agent made for the person to look at — a page, a chart, a
        /// report, a sheet, a deck — each a path under the same roots with an
        /// optional title. Rendered live where the person reads.
        #[serde(default)]
        artifacts: Vec<ArtifactSpec>,
        /// Speak as this agent (conversation scope sets it); when absent it
        /// is derived from the work item's agent, so a worker's messages are
        /// signed by the agent that is doing the work rather than the owner.
        #[serde(default)]
        agent: Option<String>,
        #[serde(default)]
        work_item: Option<WorkItemId>,
        /// The goal a goal session serves, when it posts into another scope:
        /// its scratch is one of the roots a file may come from.
        #[serde(default)]
        goal: Option<GoalCandidate>,
    },
    RecallStore {
        #[serde(default)]
        agent: Option<String>,
        #[serde(default)]
        work_item: Option<WorkItemId>,
        slug: String,
        value: String,
        #[serde(default)]
        base_hash: Option<String>,
    },
    RecallGet {
        #[serde(default)]
        agent: Option<String>,
        #[serde(default)]
        work_item: Option<WorkItemId>,
        slug: String,
    },
    RecallList {
        #[serde(default)]
        agent: Option<String>,
        #[serde(default)]
        work_item: Option<WorkItemId>,
    },
    /// Raise a named signal from inside a session — through the one emit
    /// door, as an `emit` step does: heard by every `signal` start, wait and
    /// boundary that names it; nobody hearing it is a quiet success.
    EmitSignal {
        name: String,
        #[serde(default)]
        payload: serde_json::Value,
        /// `workspace` | `goal:<ulid>`. Absent means "wherever this belongs"
        /// — the session's own scope, when it has one.
        #[serde(default)]
        signal_scope: Option<String>,
        #[serde(default)]
        goal: Option<GoalCandidate>,
        #[serde(default)]
        work_item: Option<WorkItemId>,
    },
    // --- the core agent's ops, plus create_project (see the module doc) ---
    /// The shape of the whole workspace in one answer.
    WorkspaceOverview {
        agent: String,
    },
    /// Who may be named on a step: every enabled agent and team, with what
    /// each does and who is on each team — or, for a `goal` that names
    /// agents or teams to carry it, those alone.
    ListStaff {
        agent: String,
        #[serde(default)]
        goal: Option<String>,
    },
    /// The connectors installed here, their operations and which have an
    /// account — what a `connector` step or a `call_connector` may name. Any
    /// agent's: it carries no secret, only definitions and which accounts
    /// exist.
    ListConnectors {
        /// Who asks, for the log: the session's agent, or the work item one
        /// is resolved from. Neither is required — a cycle a person drives
        /// by hand names nobody and reads the roster all the same.
        #[serde(default)]
        agent: Option<String>,
        #[serde(default)]
        work_item: Option<WorkItemId>,
    },
    /// Call one **read** operation of an installed connector now, as the
    /// account named or the connector's default, with the parameters as
    /// text. A writing operation is refused: a write is a workflow step
    /// behind an approval, never a tool call. The answer is redacted, then
    /// screened as content from outside (`crate::content`) before the agent
    /// reads a word of it, and bounded like a page read.
    CallConnector {
        connector: String,
        operation: String,
        #[serde(default)]
        account: Option<String>,
        #[serde(default)]
        params: BTreeMap<String, String>,
        /// The session's agent, for the screen's ask and record.
        #[serde(default)]
        agent: Option<String>,
        #[serde(default)]
        work_item: Option<WorkItemId>,
        #[serde(default)]
        goal: Option<GoalCandidate>,
        #[serde(default)]
        scope: Option<String>,
    },
    ListCatalog {
        agent: String,
        /// One kind, or every kind when absent.
        #[serde(default)]
        kind: Option<String>,
    },
    InstallCatalogEntry {
        agent: String,
        kind: String,
        slug: String,
        /// Journal what was created on this goal. Absent when the install
        /// is not on anyone's behalf.
        #[serde(default)]
        goal: Option<String>,
    },
    Assign {
        agent: String,
        goal: String,
        /// Assign the project rather than the goal. The note still lands on
        /// the goal, because that is the record a human reads.
        #[serde(default)]
        project: Option<String>,
        /// `agent:<id>`, `team:<id>` or `human:<64 hex>` — the one assignee
        /// grammar, shared with the CLI and governance's `Listed` policy.
        assignees: Vec<String>,
        /// `false` adds to what is there; `true` swaps the list.
        #[serde(default)]
        replace: bool,
    },
    /// Capture what recurs as a standing goal: a statement that says when —
    /// *every Monday…*, *whenever someone posts in #support…* — for the
    /// Workflow Agent to design with the start event it names. The goal moves
    /// in the workspace's default mode; it listens once its design is
    /// adopted.
    CaptureGoal {
        agent: String,
        statement: String,
        #[serde(default)]
        title: Option<String>,
    },
    /// The catalog's workflow templates and this workspace's own workflows.
    ListWorkflowTemplates {
        agent: String,
    },
    /// One workflow, by id or catalog slug, with its problems.
    GetWorkflow {
        agent: String,
        workflow: String,
    },
    /// Every problem a definition has, without recording it — its staffing
    /// judged against `goal`'s roster when one is given.
    ValidateWorkflow {
        agent: String,
        workflow: NewWorkflow,
        #[serde(default)]
        goal: Option<String>,
    },
    /// Write a library workflow — the one the session's conversation is
    /// about (`ConversationOrigin::Workflow`), and no other — as its next
    /// revision, at the revision the caller read. A named `workflow` must be
    /// that one. Refused with its problems; refused when the workflow moved.
    SaveWorkflow {
        agent: String,
        #[serde(default)]
        scope: Option<String>,
        #[serde(default)]
        workflow: Option<String>,
        revision: u64,
        definition: NewWorkflow,
    },
    /// Make the folder the work happens in: a managed project under the
    /// goal, initialised as a git repository.
    ///
    /// `agent` and `work_item` are both optional and are only used to sign the
    /// journal note. A work-item session has no agent on its scope — the
    /// runner is a fact the engine wrote, not one the session carries — so it
    /// sends `work_item` instead and the engine looks the runner up. Neither
    /// present means the owner signs, which is `signer_for`'s standing
    /// fallback.
    CreateProject {
        #[serde(default)]
        agent: Option<String>,
        #[serde(default)]
        work_item: Option<WorkItemId>,
        /// The goal to attach the new project to, when the session has one.
        /// A project belongs to the workspace either way.
        #[serde(default)]
        goal: Option<String>,
        slug: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        assignees: Vec<String>,
    },
}

/// Parse the wire form of a signal's scope. Kept next to [`Op::EmitSignal`]
/// because it IS that op's wire contract.
fn parse_signal_scope(s: &str) -> Result<bisa_core::SignalScope, String> {
    use std::str::FromStr;
    let s = s.trim();
    if s.eq_ignore_ascii_case("workspace") {
        return Ok(bisa_core::SignalScope::Workspace);
    }
    if let Some(id) = s.strip_prefix("goal:") {
        return GoalId::from_str(id)
            .map(|goal| bisa_core::SignalScope::Goal { goal })
            .map_err(|_| format!("{id:?} is not a goal id"));
    }
    Err(format!("scope {s:?} must be `workspace` or `goal:<ulid>`"))
}

/// Bind the intake listener, falling back to a short temp path when the
/// preferred path exceeds the platform's `sun_path` limit (~104 bytes on
/// macOS).
pub fn bind(preferred: &std::path::Path) -> std::io::Result<(UnixListener, std::path::PathBuf)> {
    if let Some(dir) = preferred.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // A socket file left by a previous process of ours; absent is fine.
    if let Err(e) = std::fs::remove_file(preferred) {
        if e.kind() != std::io::ErrorKind::NotFound {
            return Err(e);
        }
    }
    match UnixListener::bind(preferred) {
        Ok(l) => Ok((l, preferred.to_path_buf())),
        Err(_) if preferred.as_os_str().len() > 90 => {
            // A fresh ULID name: nothing to remove first.
            let short = std::env::temp_dir().join(format!(
                "itf-{}.sock",
                ulid::Ulid::from_datetime(std::time::SystemTime::now())
            ));
            let l = UnixListener::bind(&short)?;
            Ok((l, short))
        }
        Err(e) => Err(e),
    }
}

/// The longest request line the intake reads whole — the harness reader's
/// own limit (16 MiB): a tool argument carries a message, a note, a patch;
/// past this the rest of the line is let go and the caller told the limit.
pub const MAX_REQUEST_LINE_BYTES: usize = bisa_harness::proc::MAX_LINE_BYTES;

/// Serve until the engine shuts down (listener dropped via abort).
pub async fn serve(inner: Arc<Inner>, listener: UnixListener) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let inner = Arc::clone(&inner);
                tokio::spawn(async move { handle_conn(inner, stream).await });
            }
            Err(e) => {
                tracing::warn!("intake accept failed: {e}");
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        }
    }
}

async fn handle_conn(inner: Arc<Inner>, stream: UnixStream) {
    let (read, mut write) = stream.into_split();
    let mut reader = BufReader::new(read);
    while let Ok(Some(read)) =
        bisa_harness::proc::next_line(&mut reader, MAX_REQUEST_LINE_BYTES).await
    {
        if read.cut {
            // One answer naming the limit, then the door closes: the rest of
            // the line was let go, so nothing after it can be trusted to
            // start where a request starts.
            tracing::warn!(
                limit = MAX_REQUEST_LINE_BYTES,
                "intake envelope rejected: a request line over the limit"
            );
            let mut buf = line_too_long().to_string();
            buf.push('\n');
            crate::warn_on_err(write.write_all(buf.as_bytes()).await, "intake refusal");
            break;
        }
        let line = read.text;
        if line.trim().is_empty() {
            continue;
        }
        // The request half is what the agent hands back — a message, a note,
        // a result, a proposal, a review. Redacted here, once, for every tool
        // at once, before any op reads it: a secret the agent read with its
        // own tools is stored, journaled and synced as a placeholder. It is
        // never restored — what an agent hands back keeps its placeholders
        // wherever it goes.
        let request = serde_json::from_str::<serde_json::Value>(&line).map(|mut v| {
            inner.security.redact_inbound_value(&mut v, "mcp_request");
            v
        });
        // Two steps — the op's name off the value, then the typed op — so an
        // envelope this engine cannot read is refused naming the op, and
        // logged as such: the op and serde's words, never the body, which
        // may carry a message, a note or a URL.
        let mut reply = match request {
            Err(e) => {
                tracing::warn!(error = %e, "intake envelope rejected: not JSON");
                envelope_fault(None, &e.to_string())
            }
            Ok(value) => {
                let op = value
                    .get("op")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                match serde_json::from_value::<Op>(value) {
                    Err(e) => {
                        tracing::warn!(
                            op = op.as_deref().unwrap_or("<none>"),
                            error = %e,
                            "intake envelope rejected"
                        );
                        envelope_fault(op.as_deref(), &e.to_string())
                    }
                    Ok(op) => handle_op(&inner, op).await,
                }
            }
        };
        // The reply half of the socket is the other way text reaches an
        // agent: a goal's instructions, a note's body, a review's hunk, the
        // person's answer. Redacted here, once, for every tool at once.
        let redaction = inner.security.redact_value(&mut reply);
        if redaction.count > 0 {
            inner.emit(EngineEvent::global(EnginePayload::Redacted {
                count: redaction.count,
                kinds: redaction.kinds,
                at: "mcp_reply".into(),
            }));
        }
        let mut buf = reply.to_string();
        buf.push('\n');
        if write.write_all(buf.as_bytes()).await.is_err() {
            break;
        }
    }
}

fn err(msg: impl Into<String>) -> serde_json::Value {
    json!({"ok": false, "errors": [msg.into()]})
}

/// The reply for a line over [`MAX_REQUEST_LINE_BYTES`]: the caller's to
/// fix — a refusal, not a platform fault — and the limit in the words.
fn line_too_long() -> serde_json::Value {
    err(format!(
        "this request is longer than the {MAX_REQUEST_LINE_BYTES} bytes the engine reads as one \
         line; send less — a file belongs in an attachment, not in a tool argument"
    ))
}

/// The reply for an envelope this engine could not read at all — a fault of
/// the platform, never a refusal: the caller did nothing a different call
/// would fix, and the sentence says so, so an agent reports it and carries
/// on rather than treating it as "not done" and stopping. `op` is the name
/// the envelope gave, when it gave one.
fn envelope_fault(op: Option<&str>, detail: &str) -> serde_json::Value {
    let what = op.map_or_else(
        || "this request".to_string(),
        |o| format!("this `{o}` request"),
    );
    err(format!(
        "{}the engine could not read {what} ({detail}) — not a policy refusal; report it to the \
         person and continue the rest of the task without this tool",
        bisa_core::browser::PLATFORM_FAULT
    ))
}

/// `resolved` on a `pr_thread_resolve` left unsaid means resolve: reopening
/// is the rarer act, asked for by name.
fn default_true() -> bool {
    true
}

/// The workstream a session's `scope` names — a conversation about a
/// workstream runs in it, one about a project in its primary. A channel, DM
/// or goal scope names none, and the PR tools say so rather than acting on
/// the wrong thing.
fn workstream_from_scope(
    inner: &Arc<Inner>,
    scope: Option<&str>,
) -> Result<bisa_core::WorkstreamId, serde_json::Value> {
    let refused =
        || err("this tool needs a session of a conversation about a workstream or a project");
    let scope = scope.ok_or_else(refused)?;
    let origin = inner
        .ws
        .conversation_of_scope(scope)
        .map(|c| c.origin)
        .ok_or_else(refused)?;
    let wid = match origin {
        bisa_core::ConversationOrigin::Workstream { id, .. } => id,
        bisa_core::ConversationOrigin::Project { id } => bisa_core::WorkstreamId::primary_of(id),
        _ => return Err(refused()),
    };
    inner.ws.get_workstream(wid).map_err(|_| refused())?;
    Ok(wid)
}

/// Resolve the home this request writes to: the work item's (worker scope)
/// — its goal, or its run of the workspace — or the explicit `goal` field
/// (guided scope).
fn resolve_scope(
    inner: &Arc<Inner>,
    work_item: Option<WorkItemId>,
    goal: Option<&str>,
) -> Result<(Home, Option<WorkItemSpec>), serde_json::Value> {
    match (work_item, goal) {
        (Some(wi), _) => match executor::find_work_item(inner, wi) {
            Some(spec) => Ok((spec.home, Some(spec))),
            None => Err(err("unknown work item")),
        },
        (None, Some(goal)) => match GoalId::from_str(goal) {
            Ok(id) => match goal_of_scope_id(inner, id) {
                Some(id) => Ok((Home::Goal { goal: id }, None)),
                None => Err(err("unknown goal")),
            },
            Err(_) => Err(err("goal is not a ULID")),
        },
        (None, None) => Err(err("request needs `work_item` or `goal`")),
    }
}

/// The `goal` a request carries as the MCP client sent it: a goal's id, or a
/// scope id the client could not tell from one — a conversation session
/// hands its scope id out on every request (`Scope::apply`, bisa-mcp
/// `client.rs`). Deserialises from any string, so parsing it never fails an
/// envelope: a channel's slug is a candidate that names no goal, not a
/// malformed request. The engine, the one side that can say, resolves it.
#[derive(Clone, Debug, Deserialize)]
#[serde(transparent)]
pub(crate) struct GoalCandidate(String);

impl GoalCandidate {
    /// The goal it names — itself, or the goal a conversation scope is about
    /// (`goal_of_scope_id`); a channel's or a direct message's candidate is
    /// `None`, not an error.
    pub(crate) fn resolve(&self, inner: &Inner) -> Option<GoalId> {
        GoalId::from_str(&self.0)
            .ok()
            .and_then(|id| goal_of_scope_id(inner, id))
    }
}

/// The goal a session's scope id names: the goal itself for a goal thread,
/// the origin's goal for a conversation about one, nothing otherwise. A
/// conversation session hands its scope id out as its `goal` candidate (the
/// MCP client cannot tell), and the engine is the one that can say.
pub(crate) fn goal_of_scope_id(inner: &Inner, id: GoalId) -> Option<GoalId> {
    if inner.ws.get_goal(id).is_ok() {
        return Some(id);
    }
    inner
        .ws
        .conversation_of_scope(&id.to_string())
        .and_then(|c| c.origin.goal())
}

/// The projects a review-note op may look at, in order of specificity:
/// a named project; the work item's project (or, for an item with none,
/// every project attached to its goal); every project attached to a named
/// goal; and finally a conversation `scope` that is itself a goal thread.
fn review_projects(
    inner: &Arc<Inner>,
    project: Option<&str>,
    work_item: Option<WorkItemId>,
    goal: Option<&str>,
    scope: Option<&str>,
) -> Result<Vec<bisa_core::ProjectId>, serde_json::Value> {
    use bisa_core::ProjectId;
    let attached = |goal: GoalId| -> Result<Vec<ProjectId>, serde_json::Value> {
        inner
            .ws
            .projects_for(goal)
            .map(|ps| ps.into_iter().map(|p| p.id).collect())
            .map_err(|e| err(e.to_string()))
    };
    if let Some(p) = project {
        let id = ProjectId::from_str(p).map_err(|_| err("project is not a ULID"))?;
        inner
            .ws
            .get_project(id)
            .map_err(|_| err("unknown project"))?;
        return Ok(vec![id]);
    }
    if let Some(wi) = work_item {
        let spec = executor::find_work_item(inner, wi).ok_or_else(|| err("unknown work item"))?;
        // A run of the workspace's item attaches nothing: its own project,
        // or none.
        return match (spec.project, spec.home.goal()) {
            (Some(p), _) => Ok(vec![p]),
            (None, Some(goal)) => attached(goal),
            (None, None) => Ok(vec![]),
        };
    }
    if let Some(g) = goal {
        let id = GoalId::from_str(g).map_err(|_| err("goal is not a ULID"))?;
        return attached(id);
    }
    if let Some(s) = scope {
        if let Some(id) = GoalId::from_str(s)
            .ok()
            .and_then(|id| goal_of_scope_id(inner, id))
        {
            return attached(id);
        }
        if let Some(project) = inner
            .ws
            .conversation_of_scope(s)
            .and_then(|c| c.origin.project())
        {
            return Ok(vec![project]);
        }
    }
    Err(err(
        "pass `project`, or call from a goal, work item, goal-thread or project conversation session",
    ))
}

/// Resolve an op that MUST be goal-scoped (guided mutations).
fn resolve_goal_scope(inner: &Arc<Inner>, goal: &str) -> Result<GoalId, serde_json::Value> {
    match GoalId::from_str(goal) {
        Ok(id) => match inner.ws.get_goal(id) {
            Ok(_) => Ok(id),
            Err(_) => Err(err("unknown goal")),
        },
        Err(_) => Err(err("goal is not a ULID")),
    }
}

async fn handle_op(inner: &Arc<Inner>, op: Op) -> serde_json::Value {
    match op {
        Op::ResultSubmit { work_item, output } => result_submit(inner, work_item, output).await,
        Op::Progress {
            work_item,
            verb,
            object,
            outcome,
        } => {
            let Some(spec) = executor::find_work_item(inner, work_item) else {
                return err("unknown work item");
            };
            let (signer, attestation) = ops::signer_for(&inner.ws, spec.agent.as_deref());
            match inner.ws.append_journal(
                &spec.home,
                JournalPayload::Progress {
                    work_item,
                    verb,
                    object,
                    outcome,
                },
                &signer,
                attestation,
            ) {
                Ok(_) => json!({"ok": true}),
                Err(e) => err(e.to_string()),
            }
        }
        Op::AskHuman {
            work_item,
            goal,
            question,
            expects,
            options,
            multi,
        } => {
            ask_human(
                inner,
                work_item,
                goal.as_deref(),
                question,
                expects,
                options,
                multi,
            )
            .await
        }
        Op::AwaitDecision { gate } => await_decision(inner, gate).await,
        Op::BrowserServe {
            folder,
            scope,
            agent,
            work_item,
        } => browser_serve(inner, folder, scope.as_deref(), agent, work_item).await,
        Op::Browser {
            request,
            scope,
            agent,
            work_item,
            goal,
        } => {
            let goal = goal.as_ref().and_then(|g| g.resolve(inner));
            browser(inner, request, scope.as_deref(), agent, work_item, goal).await
        }
        Op::Draw {
            request,
            scope,
            agent,
            work_item,
            goal,
        } => {
            let goal = goal.as_ref().and_then(|g| g.resolve(inner));
            draw(inner, request, scope.as_deref(), agent, work_item, goal).await
        }
        Op::Decide {
            request,
            scope,
            agent,
            work_item,
            goal,
        } => {
            let goal = goal.as_ref().and_then(|g| g.resolve(inner));
            decide(inner, request, scope.as_deref(), agent, work_item, goal).await
        }
        Op::MobileDevelopment {
            request,
            scope,
            agent,
            work_item,
        } => mobile_development(inner, request, scope.as_deref(), agent, work_item).await,
        Op::GetGoal { work_item, goal } => get_goal(inner, work_item, goal.as_deref()),
        Op::GetRun { work_item, run } => get_run(inner, work_item, run.as_deref()),
        Op::ReviseStatement {
            goal,
            statement,
            why,
        } => revise_statement(inner, &goal, statement, why),
        Op::ProposeWorkflow {
            goal,
            agent,
            workflow,
        } => {
            if let Err(e) = core_agent_only(&agent, &[AgentId::WORKFLOW]) {
                return e;
            }
            propose_workflow(inner, &goal, &agent, workflow)
        }
        Op::AmendWorkflow {
            goal,
            agent,
            workflow,
        } => {
            if let Err(e) = core_agent_only(&agent, &[AgentId::WORKFLOW]) {
                return e;
            }
            amend_workflow(inner, &goal, &agent, workflow)
        }
        Op::NoteRead { note, scope } => match note_of(inner, note.as_deref(), scope.as_deref()) {
            Err(why) => err(why),
            Ok(id) => match inner.ws.get_note(id) {
                // The hash beside the body: what a `note_write` states as the
                // text it rewrites.
                Ok(n) => json!({
                    "ok": true,
                    "title": n.title,
                    "body": n.body,
                    "hash": bisa_store::body_hash(&n.body),
                }),
                Err(e) => err(e.to_string()),
            },
        },
        Op::NoteAppend {
            note,
            text,
            agent,
            scope,
        } => {
            let id = match note_of(inner, note.as_deref(), scope.as_deref()) {
                Ok(id) => id,
                Err(why) => return err(why),
            };
            // Attributed the same way an answer is, so a block an agent added
            // unprompted reads exactly like one it was asked for —
            // there is no second format for a person to learn.
            let author = agent.clone().unwrap_or_else(|| "agent".to_string());
            let by = resolve_recall_agent(inner, agent, None);
            let block =
                crate::notes::attribution_block(&author, crate::notes::now_secs(), text.trim());
            match inner.ws.append_note(id, &block) {
                Ok(n) => {
                    inner.notes_git.touched(inner);
                    crate::notes::emit_written(inner, &n, by);
                    json!({"ok": true, "title": n.title})
                }
                Err(e) => err(e.to_string()),
            }
        }
        Op::NoteWrite {
            note,
            text,
            base_hash,
            agent,
            scope,
        } => {
            let id = match note_of(inner, note.as_deref(), scope.as_deref()) {
                Ok(id) => id,
                Err(why) => return err(why),
            };
            if text.trim().is_empty() {
                return err(crate::notes::WRITE_NOTHING);
            }
            let current = match inner.ws.get_note(id) {
                Ok(n) => n,
                Err(e) => return err(e.to_string()),
            };
            // Said here in words before the store refuses it under its lock,
            // so the agent reads what to do; the store's check is the one
            // that holds.
            let current_hash = bisa_store::body_hash(&current.body);
            if current_hash != base_hash {
                return err(crate::notes::stale_write_words(&current_hash));
            }
            if bisa_store::body_hash(&text) == base_hash {
                return json!({"ok": true, "title": current.title, "hash": base_hash, "changed": false});
            }
            // The agent read a redacted copy (every reply is redacted): writing
            // it back would store a placeholder over the person's secret.
            if inner.security.redact(&current.body).count > 0 {
                return err(crate::notes::WRITE_HOLDS_SECRET);
            }
            let by = resolve_recall_agent(inner, agent, None);
            match crate::notes::write_by_agent(inner, id, &text, &base_hash, by) {
                Ok(n) => json!({
                    "ok": true,
                    "title": n.title,
                    "hash": bisa_store::body_hash(&n.body),
                    "changed": true,
                }),
                Err(crate::EngineError::Store(bisa_store::StoreError::EditConflict {
                    current_hash,
                    ..
                })) => err(crate::notes::stale_write_words(&current_hash)),
                Err(e) => err(e.to_string()),
            }
        }
        Op::ReviewNotesList {
            project,
            work_item,
            goal,
            scope,
            include_resolved,
        } => {
            let projects = match review_projects(
                inner,
                project.as_deref(),
                work_item,
                goal.as_deref(),
                scope.as_deref(),
            ) {
                Ok(p) => p,
                Err(e) => return e,
            };
            let mut notes = Vec::new();
            for pid in &projects {
                match crate::ide::review::list(inner, *pid, None, include_resolved) {
                    Ok(n) => notes.extend(n),
                    Err(e) => return err(e.to_string()),
                }
            }
            json!({
                "ok": true,
                "projects": projects.iter().map(ToString::to_string).collect::<Vec<_>>(),
                "notes": notes,
            })
        }
        Op::ReviewNoteResolve {
            note,
            project,
            work_item,
            goal,
            scope,
        } => {
            let Ok(id) = note.parse::<bisa_core::NoteId>() else {
                return err(format!("{note:?} is not a review note id"));
            };
            let projects = match review_projects(
                inner,
                project.as_deref(),
                work_item,
                goal.as_deref(),
                scope.as_deref(),
            ) {
                Ok(p) => p,
                Err(e) => return e,
            };
            for pid in projects {
                if crate::ide::review::get(inner, pid, id).is_ok() {
                    return match crate::ide::review::resolve(inner, pid, id) {
                        Ok(n) => json!({"ok": true, "note": n}),
                        Err(e) => err(e.to_string()),
                    };
                }
            }
            err("no such review note in the projects this session can see")
        }
        Op::PrReviewsList {
            scope,
            agent,
            work_item,
            goal,
        } => {
            let wid = match workstream_from_scope(inner, scope.as_deref()) {
                Ok(w) => w,
                Err(e) => return e,
            };
            let reviews = match crate::codehost::pr_reviews(inner, wid).await {
                Ok(r) => r,
                Err(e) => return err(e.to_string()),
            };
            // What the code host said is content from outside: screened once,
            // whole, before the agent reads a word of it (`crate::content`).
            let facts = scope
                .as_deref()
                .map(|s| crate::conversation::scope_facts(inner, s))
                .unwrap_or_default();
            let screening = crate::content::Screening::gather(
                inner,
                &facts,
                scope.as_deref(),
                agent,
                work_item,
                goal.as_ref().and_then(|g| g.resolve(inner)),
            );
            let source = match crate::codehost::review_source(inner, wid).await {
                Ok(s) => s,
                Err(e) => return err(e.to_string()),
            };
            let text = crate::codehost::review_words(&reviews);
            if text.trim().is_empty() {
                return json!({"ok": true, "reviews": reviews.reviews, "threads": reviews.threads});
            }
            match crate::content::screen(
                inner,
                &screening,
                crate::content::ContentSubject { source, text },
            )
            .await
            {
                crate::content::Screened::Read { framing, verdict } => json!({
                    "ok": true,
                    "screen": {"verdict": verdict, "note": framing},
                    "reviews": reviews.reviews,
                    "threads": reviews.threads,
                }),
                crate::content::Screened::Withheld { sentence } => json!({
                    "ok": true,
                    "withheld": true,
                    "text": sentence,
                    "reviews": [],
                    "threads": [],
                }),
            }
        }
        Op::PrReviewSubmit {
            scope,
            agent,
            event,
            body,
            comments,
        } => {
            let wid = match workstream_from_scope(inner, scope.as_deref()) {
                Ok(w) => w,
                Err(e) => return e,
            };
            let review = crate::codehost::Review {
                event,
                body,
                comments,
            };
            // An intake request is never the person's: a session without an
            // agent id is still an agent, signed as one.
            let reviewer =
                crate::codehost::Reviewer::Agent(agent.unwrap_or_else(|| "agent".to_string()));
            match crate::codehost::review(inner, wid, review, reviewer).await {
                Ok(()) => json!({"ok": true}),
                Err(e) => err(e.to_string()),
            }
        }
        Op::PrThreadReply {
            scope,
            agent,
            thread,
            body,
            resolve,
        } => {
            let wid = match workstream_from_scope(inner, scope.as_deref()) {
                Ok(w) => w,
                Err(e) => return e,
            };
            let reviewer =
                crate::codehost::Reviewer::Agent(agent.unwrap_or_else(|| "agent".to_string()));
            match crate::codehost::reply_review_thread(
                inner, wid, &thread, &body, resolve, reviewer,
            )
            .await
            {
                Ok(()) => json!({"ok": true, "thread": thread, "resolved": resolve}),
                Err(e) => err(e.to_string()),
            }
        }
        Op::PrThreadResolve {
            scope,
            thread,
            resolved,
        } => {
            let wid = match workstream_from_scope(inner, scope.as_deref()) {
                Ok(w) => w,
                Err(e) => return e,
            };
            match crate::codehost::resolve_review_thread(inner, wid, &thread, resolved).await {
                Ok(()) => json!({"ok": true, "thread": thread, "resolved": resolved}),
                Err(e) => err(e.to_string()),
            }
        }
        Op::AddNote {
            work_item,
            goal,
            text,
        } => {
            let (home, spec) = match resolve_scope(inner, work_item, goal.as_deref()) {
                Ok(x) => x,
                Err(e) => return e,
            };
            let (signer, attestation) =
                ops::signer_for(&inner.ws, spec.and_then(|s| s.agent).as_deref());
            match inner.ws.append_journal(
                &home,
                JournalPayload::Note { text },
                &signer,
                attestation,
            ) {
                Ok(_) => json!({"ok": true}),
                Err(e) => err(e.to_string()),
            }
        }
        Op::SpawnSubGoal {
            work_item,
            goal,
            statement,
            title,
        } => spawn_sub_goal(inner, work_item, goal.as_deref(), statement, title),
        Op::EmitSignal {
            name,
            payload,
            signal_scope,
            goal,
            work_item,
        } => {
            // An explicit scope wins; otherwise the session's own scope is
            // what "wherever this belongs" means, so a work item's signal
            // belongs to its goal — and a run of the workspace's item's
            // signal is the workspace's.
            let scope = match signal_scope.as_deref() {
                Some(raw) => match parse_signal_scope(raw) {
                    Ok(s) => s,
                    Err(e) => return err(e),
                },
                // `goal` here is never author-supplied — it is filled from
                // the session's scope, a candidate the engine resolves
                // (`GoalCandidate`): a goal thread's, or a conversation's
                // about a goal; a channel's or a DM's names none, which
                // means this session has no goal, which is the workspace.
                None => match goal.as_ref().and_then(|g| g.resolve(inner)) {
                    Some(goal) => bisa_core::SignalScope::Goal { goal },
                    None => match work_item
                        .and_then(|w| inner.active_items.get(&w).map(|e| *e.value()))
                    {
                        Some(Home::Goal { goal }) => bisa_core::SignalScope::Goal { goal },
                        Some(Home::Run { .. }) | None => bisa_core::SignalScope::Workspace,
                    },
                },
            };
            // A run's work raising a signal carries the run's causal chain:
            // a listener that led to this run never hears it back.
            let chain = work_item
                .and_then(|w| executor::find_work_item(inner, w))
                .and_then(|spec| spec.run)
                .map(|run| crate::listen::chain_of_run(inner, run))
                .unwrap_or_default();
            match crate::listen::emit::emit(inner, &name, payload, scope, chain, None) {
                Ok(emitted) => json!({
                    "ok": true,
                    "signal": emitted.signal,
                    "listeners": emitted.listeners,
                }),
                Err(e) => err(e.to_string()),
            }
        }
        // --- conversation/recall delegation (store surface) ---
        Op::PostMessage {
            scope,
            content,
            reply_to,
            mentions,
            attachments,
            artifacts,
            agent,
            work_item,
            goal,
        } => {
            // The roots are the session's own: a session with no agent has
            // no agent scratch to publish from. The signer is never nobody.
            let session = resolve_recall_agent(inner, agent.clone(), work_item);
            // A goal that exists, or none: the roots never take a candidate's word for it.
            let goal = goal.as_ref().and_then(|g| g.resolve(inner));
            let roots = session_roots(inner, &scope, goal, session.as_ref(), work_item);
            let files =
                match ingest_agent_attachments(inner, session.as_ref(), &roots, &attachments) {
                    Ok(files) => files,
                    Err(reason) => return err(reason),
                };
            let artifacts =
                match ingest_agent_artifacts(inner, session.as_ref(), &roots, &artifacts) {
                    Ok(artifacts) => artifacts,
                    Err(reason) => return err(reason),
                };
            let signer = posting_agent(inner, agent, work_item);
            // Resolved by the store, never here: a token may be a pubkey, an
            // agent id or the channel handle, and a second resolver is how two
            // surfaces end up disagreeing about who `@something` meant. An
            // unknown token is refused rather than dropped — a mention that
            // quietly resolves to nobody is a hand-off you believe you made.
            let mentions = match inner.ws.resolve_mentions(&scope, &mentions) {
                Ok(m) => m,
                Err(e) => return err(e.to_string()),
            };
            // A run's work speaking: what hears the message carries the
            // run's causal chain, so a listener that led to this run never
            // hears its own work back.
            let run = work_item
                .and_then(|w| executor::find_work_item(inner, w))
                .and_then(|spec| spec.run);
            let posted = crate::listen::post_for_run(inner, run, || {
                inner.ws.post_message(
                    &scope,
                    MessageBody::Post {
                        text: content,
                        context: vec![],
                        artifacts,
                        thinking: None,
                        said: None,
                    },
                    reply_to,
                    &mentions,
                    &files,
                    Some(&signer),
                    // An agent speaking through its tool is somebody speaking
                    // — and that somebody is an agent, the General Agent when
                    // the session has no name of its own.
                    PostOrigin::Asked,
                )
            });
            match posted {
                Ok(id) => json!({"ok": true, "message": id}),
                Err(e) => err(e.to_string()),
            }
        }
        Op::RecallStore {
            agent,
            work_item,
            slug,
            value,
            base_hash,
        } => {
            let Some(agent) = resolve_recall_agent(inner, agent, work_item) else {
                return err(
                    "recall requires an agent identity (pass `agent` or a work item run by one)",
                );
            };
            match inner
                .ws
                .recall_store(&agent, &slug, &value, base_hash.as_deref())
            {
                Ok(hash) => json!({"ok": true, "hash": hash}),
                Err(bisa_store::StoreError::RecallConflict {
                    current_value,
                    current_hash,
                }) => json!({
                    "ok": false,
                    "errors": ["the memory changed since you read it — merge and retry with base_hash"],
                    "current_value": current_value,
                    "current_hash": current_hash,
                }),
                Err(e) => err(e.to_string()),
            }
        }
        Op::RecallGet {
            agent,
            work_item,
            slug,
        } => {
            let Some(agent) = resolve_recall_agent(inner, agent, work_item) else {
                return err("recall requires an agent identity");
            };
            match inner.ws.recall_get(&agent, &slug) {
                Ok(Some(r)) => json!({"ok": true, "slug": r.slug, "value": r.value,
                                       "hash": r.hash, "links": r.links}),
                Ok(None) => json!({"ok": true, "value": null}),
                Err(e) => err(e.to_string()),
            }
        }
        Op::RecallList { agent, work_item } => {
            let Some(agent) = resolve_recall_agent(inner, agent, work_item) else {
                return err("recall requires an agent identity");
            };
            match inner.ws.recall_list(&agent) {
                Ok(list) => json!({"ok": true, "records": list.iter().map(|r| json!({
                    "slug": r.slug, "hash": r.hash, "updated_at": r.updated_at,
                    "links": r.links,
                })).collect::<Vec<_>>()}),
                Err(e) => err(e.to_string()),
            }
        }
        // --- the core agent's ops ---
        Op::WorkspaceOverview { agent } => {
            if let Err(e) = core_agent_only(&agent, &AgentId::CORE) {
                return e;
            }
            workspace_overview(inner)
        }
        Op::ListStaff { agent, goal } => {
            if let Err(e) = core_agent_only(&agent, &AgentId::CORE) {
                return e;
            }
            let goal = match goal
                .as_deref()
                .map(|g| resolve_goal_scope(inner, g))
                .transpose()
            {
                Ok(g) => g,
                Err(e) => return e,
            };
            match roster_for(inner, goal) {
                Ok(roster) => json!({
                    "ok": true,
                    "agents": roster.agents,
                    "teams": roster.teams,
                    "scoped": roster.scoped,
                    "text": roster.render(),
                }),
                Err(e) => err(e.to_string()),
            }
        }
        Op::ListConnectors { agent, work_item } => {
            // Every session's to read; who asked is said in the log, since
            // the roster names which accounts exist.
            let who = resolve_recall_agent(inner, agent, work_item)
                .map(|agent| agent.to_string())
                .unwrap_or_else(|| "a session with no agent".to_string());
            tracing::debug!(target: "bisa_engine::intake", agent = %who, "connectors listed");
            match crate::connectors::ConnectorRoster::of(&inner.ws) {
                Ok(roster) => json!({
                    "ok": true,
                    "connectors": roster.connectors,
                    "text": roster.render(),
                }),
                Err(e) => err(e.to_string()),
            }
        }
        Op::CallConnector {
            connector,
            operation,
            account,
            params,
            agent,
            work_item,
            goal,
            scope,
        } => {
            call_connector(
                inner, connector, operation, account, params, agent, work_item, goal, scope,
            )
            .await
        }
        Op::ListCatalog { agent, kind } => {
            if let Err(e) = core_agent_only(&agent, &AgentId::CORE) {
                return e;
            }
            let kind = match kind.as_deref().map(CatalogKind::from_str).transpose() {
                Ok(k) => k,
                Err(e) => return err(e.to_string()),
            };
            match inner.ws.catalog_entries(kind) {
                Ok(entries) => json!({"ok": true, "entries": entries}),
                Err(e) => err(e.to_string()),
            }
        }
        Op::InstallCatalogEntry {
            agent,
            kind,
            slug,
            goal,
        } => {
            if let Err(e) = core_agent_only(&agent, &[AgentId::GENERAL]) {
                return e;
            }
            install_catalog_entry(inner, &kind, &slug, goal.as_deref())
        }
        Op::Assign {
            agent,
            goal,
            project,
            assignees,
            replace,
        } => {
            if let Err(e) = core_agent_only(&agent, &[AgentId::GENERAL]) {
                return e;
            }
            assign(inner, &goal, project.as_deref(), assignees, replace)
        }
        Op::CaptureGoal {
            agent,
            statement,
            title,
        } => {
            if let Err(e) = core_agent_only(&agent, &[AgentId::GENERAL]) {
                return e;
            }
            capture_goal(inner, statement, title)
        }
        Op::ListWorkflowTemplates { agent } => {
            if let Err(e) = core_agent_only(&agent, &[AgentId::WORKFLOW]) {
                return e;
            }
            list_workflow_templates(inner)
        }
        Op::GetWorkflow { agent, workflow } => {
            if let Err(e) = core_agent_only(&agent, &[AgentId::WORKFLOW]) {
                return e;
            }
            get_workflow(inner, &workflow)
        }
        Op::ValidateWorkflow {
            agent,
            workflow,
            goal,
        } => {
            if let Err(e) = core_agent_only(&agent, &[AgentId::WORKFLOW]) {
                return e;
            }
            let goal = match goal
                .as_deref()
                .map(|g| resolve_goal_scope(inner, g))
                .transpose()
            {
                Ok(g) => g,
                Err(e) => return e,
            };
            validate_workflow(inner, goal, workflow)
        }
        Op::SaveWorkflow {
            agent,
            scope,
            workflow,
            revision,
            definition,
        } => {
            if let Err(e) = core_agent_only(&agent, &[AgentId::WORKFLOW]) {
                return e;
            }
            save_workflow(
                inner,
                scope.as_deref(),
                workflow.as_deref(),
                revision,
                definition,
            )
        }
        // No `core_agent_only` gate: making somewhere for files to live is
        // the work itself, not the platform's business, and an agent without
        // it writes into its scratch folder instead. What kept this op safe
        // was never the caller's name — it is `ProjectRoot::Managed` below,
        // which means no path on disk can be adopted, and the store's slug
        // allowlist.
        Op::CreateProject {
            agent,
            work_item,
            goal,
            slug,
            name,
            assignees,
        } => {
            // The work item — when the session runs one — carries everything
            // provenance needs: the goal, the run, the step, and the runner
            // to name in the journal.
            let spec = work_item.and_then(|w| executor::find_work_item(inner, w));
            let by = agent.or_else(|| spec.as_ref().and_then(|s| s.agent.clone()));
            create_project(
                inner,
                by.as_deref(),
                goal.as_deref(),
                spec.as_ref(),
                slug,
                name,
                assignees,
            )
            .await
        }
    }
}

// ---------------------------------------------------------------------------
// The core agent's ops
// ---------------------------------------------------------------------------

/// Refuse an op that belongs to the platform's own agents alone.
///
/// The MCP server offers these tools only to a core agent's session, and
/// that is the ergonomics. **This is the boundary.** The intake socket is a
/// path on disk, and any session that knows it can write to it, so a tool list
/// is a menu and not a permission. One helper rather than a copy per op: a
/// check written out ten times is a check that will one day be written out
/// nine. `allowed` names which of the General Agent and the Workflow Agent may call the op.
/// `call_connector`: an agent reads an outside platform through a connector,
/// under everything a step is under and one thing more — a write is refused
/// by name, because a write is a workflow step behind a gate or the person's
/// `unattended`, never something a session does on its own word. The answer
/// is redacted by the door and then screened as content from outside, the
/// way a page or a review is, bounded to what one read may hand an agent.
#[allow(clippy::too_many_arguments)]
async fn call_connector(
    inner: &Arc<Inner>,
    connector: String,
    operation: String,
    account: Option<String>,
    params: BTreeMap<String, String>,
    agent: Option<String>,
    work_item: Option<WorkItemId>,
    goal: Option<GoalCandidate>,
    scope: Option<String>,
) -> serde_json::Value {
    let Ok(connector) = bisa_core::ConnectorId::new(&connector) else {
        return err(format!("{connector:?} is not a connector slug"));
    };
    let Ok(operation) = bisa_core::OperationId::new(&operation) else {
        return err(format!("{operation:?} is not an operation id"));
    };
    let def = match inner.ws.get_connector(&connector) {
        Ok(d) => d,
        Err(e) => return err(e.to_string()),
    };
    let Some(op) = def.operation(&operation) else {
        return err(format!(
            "connector {connector} has no operation `{operation}` (it has {})",
            def.operations
                .iter()
                .map(|o| o.id.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    };
    if op.writes {
        return err(format!(
            "{connector}.{operation} writes to the platform; a write is a `connector` step of a workflow, behind an approval — not a call a session makes on its own"
        ));
    }
    let wanted = match account.as_deref().map(str::parse::<bisa_core::AccountId>) {
        None => None,
        Some(Ok(id)) => Some(id),
        Some(Err(_)) => return err("`account` is not an account id"),
    };
    let acct = match crate::connectors::resolve_account(&inner.ws, &def, wanted) {
        Ok(a) => a,
        Err(e) => return err(crate::connectors::redact_reason(inner, &e)),
    };
    let facts = scope
        .as_deref()
        .map(|s| crate::conversation::scope_facts(inner, s))
        .unwrap_or_default();
    let goal_named = goal.as_ref().and_then(|g| g.resolve(inner));
    let screening = crate::content::Screening::gather(
        inner,
        &facts,
        scope.as_deref(),
        agent,
        work_item,
        goal_named,
    );
    let judge = crate::connectors::PolicyHostJudge {
        inner,
        home: screening.home,
        subject: format!("{} {}{}", op.method.as_str(), def.hosts.join("|"), op.path),
        declared: &def.hosts,
    };
    let outcome = match crate::connectors::invoke(
        inner,
        crate::connectors::Invocation {
            def: &def,
            op,
            account: acct.as_ref(),
            params: &params,
            files: &bisa_connectors::NoFiles,
            judge: &judge,
            key: None,
            caller: crate::connectors::Caller::Agent,
        },
    )
    .await
    {
        Ok(o) => o,
        Err(e) => return err(crate::connectors::redact_reason(inner, &e)),
    };
    // What the platform said is content from outside: screened once, whole,
    // before the agent reads a word of it — and bounded like a page read.
    let full = serde_json::to_string_pretty(&outcome.selected).unwrap_or_default();
    let truncated = full.chars().count() > crate::content::MAX_SCREENED_CHARS;
    let text: String = full
        .chars()
        .take(crate::content::MAX_SCREENED_CHARS)
        .collect();
    if text.trim().is_empty() {
        return json!({"ok": true, "output": outcome.selected, "truncated": false});
    }
    match crate::content::screen(
        inner,
        &screening,
        crate::content::ContentSubject {
            source: crate::content::ContentSource::Connector {
                connector: connector.to_string(),
                operation: operation.to_string(),
            },
            text: text.clone(),
        },
    )
    .await
    {
        crate::content::Screened::Read { framing, verdict } => json!({
            "ok": true,
            "screen": {"verdict": verdict, "note": framing},
            "output": if truncated { Value::String(text) } else { outcome.selected },
            "truncated": truncated,
            "pages": outcome.pages,
        }),
        crate::content::Screened::Withheld { sentence } => json!({
            "ok": true,
            "withheld": true,
            "text": sentence,
        }),
    }
}

fn core_agent_only(agent: &str, allowed: &[&str]) -> Result<(), serde_json::Value> {
    if allowed.contains(&agent) {
        return Ok(());
    }
    Err(err(format!(
        "only {} may call this op; this session is {agent:?}",
        allowed.join(" or ")
    )))
}

/// Journal a note on `goal`, signed by the General Agent's own key.
///
/// Every autonomous change goes through here. The agent changed somebody's
/// workspace without being asked twice; that is only acceptable while it stays
/// visible after the fact, and a note in the goal's journal is where a human
/// looks. It signs as itself for the same reason a work item does: a note the
/// platform wrote must not read as one the owner wrote.
fn journal_core_note(inner: &Arc<Inner>, goal: GoalId, text: String) {
    ops::journal_note_as(inner, Some(AgentId::GENERAL), Home::Goal { goal }, text);
}

/// The shape of the whole workspace in one answer.
///
/// A core agent's first call on every wake, so it is one round trip and it is
/// **counts wherever a count is what the next decision needs**: how much
/// staff exists, how much of the catalog is installed, what listens, how many
/// workflows there are. What it lists instead of counting is
/// what a following call has to name — the agent ids to delegate to, the
/// teams and channels to address, the projects to work in, and the goals that
/// are stuck on a person.
fn workspace_overview(inner: &Arc<Inner>) -> serde_json::Value {
    let ws = &inner.ws;

    let agents = ws.list_agents().unwrap_or_default();
    // Every agent that is not a core one: the staff this workspace actually
    // has to delegate to, whether it came from the catalog or was made here.
    let installed: Vec<&str> = agents
        .iter()
        .filter(|a| !a.origin.is_core())
        .map(|a| a.id.as_str())
        .collect();

    // Member counts are the *stored* lists. The core agents are in every team
    // and every channel implicitly, and a number that is two larger everywhere
    // says nothing about any of them.
    let teams: Vec<serde_json::Value> = ws
        .list_teams()
        .unwrap_or_default()
        .iter()
        .map(|t| {
            json!({
                "id": t.id,
                "name": t.name,
                "agents": t.members.iter().filter(|m| m.as_agent().is_some()).count(),
                "humans": t.members.iter().filter(|m| m.as_human().is_some()).count(),
            })
        })
        .collect();
    // Standing channels only. An agent reading the workspace shape is being
    // told what rooms exist; a DM is somebody's private thread, and counting
    // one as a channel both overstates the shared surface and leaks a
    // conversation the agent has no business enumerating.
    let channels: Vec<serde_json::Value> = ws
        .list_channels_of_kind(bisa_core::ChannelKind::Standing)
        .unwrap_or_default()
        .iter()
        .map(|c| {
            json!({
                "id": c.id,
                "name": c.name,
                "roster": c.roster.as_str(),
                "members": ws.channel_members(&c.id).map(|m| m.len()).unwrap_or(0),
            })
        })
        .collect();

    let mcps = ws.list_mcps().unwrap_or_default();
    let listening = ws.list_listening().unwrap_or_default();
    let armed = crate::listen::armed(inner);
    // The earliest thing that will happen by itself. `None` means nothing is
    // scheduled, which is a different answer from "nothing listens".
    let next_due = armed
        .armed
        .iter()
        .filter_map(|a| ws.listener_runtime(&a.key).next_due)
        .min();

    let projects: Vec<serde_json::Value> = ws
        .list_projects()
        .unwrap_or_default()
        .iter()
        .map(|p| {
            json!({
                "id": p.id.to_string(),
                "slug": p.slug,
                "goals": ws
                    .goals_of_project(p.id)
                    .unwrap_or_default()
                    .iter()
                    .map(|g| g.to_string())
                    .collect::<Vec<_>>(),
                "vcs": p.vcs,
            })
        })
        .collect();

    let goals = ws.list_goals(None).unwrap_or_default();
    let mut by_status: BTreeMap<String, usize> = Default::default();
    let mut waiting_on_human: Vec<String> = Vec::new();
    for g in &goals {
        let run = ws.get_current_run(g.id).ok().flatten();
        *by_status
            .entry(g.status(run.as_ref()).as_str().to_string())
            .or_default() += 1;
        // A pending gate and a waiting `human` or `approval` step are the same
        // fact to whoever is deciding what to do next: this goal moves when a
        // person acts.
        let waiting_step = run.as_ref().is_some_and(|r| {
            r.steps.iter().any(|(id, rec)| {
                rec.state == StepState::Waiting
                    && r.workflow.step(id).is_some_and(|s| {
                        matches!(
                            s.kind,
                            bisa_core::StepKind::Human { .. }
                                | bisa_core::StepKind::Approval { .. }
                        )
                    })
            })
        });
        if waiting_step || !inner.gates.pending_for_goal(g.id).is_empty() {
            waiting_on_human.push(g.id.to_string());
        }
    }
    let workflows = ws.list_workflows().unwrap_or_default();

    let running: Vec<serde_json::Value> = inner
        .registry
        .list()
        .iter()
        .filter(|a| a.status == crate::registry::AgentStatus::Running)
        .map(|a| {
            json!({
                "agent": a.id.to_string(),
                "kind": a.kind,
                "work_item": a.work_item.map(|w| w.to_string()),
            })
        })
        .collect();

    let mut catalog = serde_json::Map::new();
    for kind in CatalogKind::ALL.iter().copied() {
        let entries = ws.catalog_entries(Some(kind)).unwrap_or_default();
        catalog.insert(
            kind.as_str().to_string(),
            json!({
                "total": entries.len(),
                "installed": entries.iter().filter(|e| e.installed).count(),
            }),
        );
    }

    json!({
        "ok": true,
        "agents": {
            "total": agents.len(),
            "enabled": agents.iter().filter(|a| a.enabled).count(),
            "core": AgentId::CORE,
            "installed": installed,
        },
        "teams": teams,
        "channels": channels,
        "skills": ws.list_skills().unwrap_or_default().len(),
        "mcp_servers": {
            "total": mcps.len(),
            "enabled": mcps.iter().filter(|m| m.enabled).count(),
        },
        "listening": {
            "workflows": listening
                .iter()
                .filter(|(h, _)| matches!(h, bisa_core::ListenerHost::Workspace { .. }))
                .count(),
            "goals": listening
                .iter()
                .filter(|(h, _)| matches!(h, bisa_core::ListenerHost::Goal { .. }))
                .count(),
            "paused": listening.iter().filter(|(_, l)| l.is_paused()).count(),
            "listeners": armed.armed.len(),
            "next_due": next_due,
        },
        "projects": projects,
        "goals": {
            "by_status": by_status,
            "waiting_on_human": waiting_on_human,
        },
        "workflows": {
            "total": workflows.len(),
            "catalog": workflows.iter().filter(|w| w.origin.catalog_slug().is_some()).count(),
        },
        "running": running,
        "catalog": catalog,
    })
}

/// Install a catalog entry and everything it needs.
///
/// The store owns transitivity, idempotence and collision refusal; this adds
/// the record. `Installed` lists what was **created**, so an entry that was
/// already here journals "nothing created" rather than claiming a staff it did
/// not hire.
fn install_catalog_entry(
    inner: &Arc<Inner>,
    kind: &str,
    slug: &str,
    goal: Option<&str>,
) -> serde_json::Value {
    let kind = match CatalogKind::from_str(kind) {
        Ok(k) => k,
        Err(e) => return err(e.to_string()),
    };
    // Resolve the goal before installing: journalling onto a goal that
    // does not exist would leave the install with no record at all.
    let goal_id = match goal {
        Some(raw) => match resolve_goal_scope(inner, raw) {
            Ok(id) => Some(id),
            Err(e) => return e,
        },
        None => None,
    };
    let installed = match crate::admin::install_catalog_entry(inner, kind, slug) {
        Ok(i) => i,
        Err(e) => return err(e.to_string()),
    };
    if let Some(goal_id) = goal_id {
        let mut parts = Vec::new();
        for (label, ids) in [
            ("agents", &installed.agents),
            ("skills", &installed.skills),
            ("teams", &installed.teams),
            ("channels", &installed.channels),
        ] {
            if !ids.is_empty() {
                parts.push(format!("{label}: {}", ids.join(", ")));
            }
        }
        if !installed.workflows.is_empty() {
            parts.push(format!(
                "workflows: {}",
                installed
                    .workflows
                    .iter()
                    .map(|(slug, id)| format!("{slug} ({id})"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        let text = if parts.is_empty() {
            format!("catalog {kind} {slug:?} was already installed; nothing created")
        } else {
            format!("installed catalog {kind} {slug:?} — {}", parts.join("; "))
        };
        journal_core_note(inner, goal_id, text);
    }
    json!({"ok": true, "installed": installed})
}

/// The wire form of an assignee list — `agent:<id>` / `team:<id>` /
/// `human:<64 hex>` — turned into domain values, or this surface's refusal.
///
/// Existence is `ops::check_assignees`' question, not this one's: parsing and
/// resolving are two failures a caller fixes differently, and one function
/// answering both would report a typo as a missing agent.
fn parse_assignees(raw: &[String]) -> Result<Vec<Assignee>, serde_json::Value> {
    raw.iter()
        .map(|s| Assignee::from_str(s).map_err(|e| err(e.to_string())))
        .collect()
}

/// Set who carries a goal, or a project inside it.
///
/// `replace: false` adds, which is the common case — the core agent staffs a
/// piece of work without knowing what else is already on the goal. `true`
/// swaps, which is how a wrong assignment gets corrected rather than
/// accumulated. Either way the resulting list is returned, so the caller never
/// has to guess which happened.
fn assign(
    inner: &Arc<Inner>,
    goal: &str,
    project: Option<&str>,
    assignees: Vec<String>,
    replace: bool,
) -> serde_json::Value {
    let goal_id = match resolve_goal_scope(inner, goal) {
        Ok(id) => id,
        Err(e) => return e,
    };
    let parsed = match parse_assignees(&assignees) {
        Ok(a) => a,
        Err(e) => return e,
    };
    if let Err(e) = ops::check_assignees(inner, &parsed) {
        return err(e.to_string());
    }

    let (resulting, target) = match project {
        Some(raw) => {
            let Ok(pid) = ProjectId::from_str(raw) else {
                return err("project is not a ULID");
            };
            let Ok(mut p) = inner.ws.get_project(pid) else {
                return err("unknown project");
            };
            p.assignees = merged_assignees(std::mem::take(&mut p.assignees), parsed, replace);
            let slug = p.slug.to_string();
            match inner.ws.update_project(p) {
                Ok(p) => (p.assignees, format!("project {slug}")),
                Err(e) => return err(e.to_string()),
            }
        }
        None => {
            let Ok(i) = inner.ws.get_goal(goal_id) else {
                return err("unknown goal");
            };
            let next = merged_assignees(i.assignees, parsed, replace);
            match inner.ws.set_goal_assignees(goal_id, next) {
                Ok(i) => (i.assignees, "this goal".to_string()),
                Err(e) => return err(e.to_string()),
            }
        }
    };

    let list: Vec<String> = resulting.iter().map(|a| a.to_string()).collect();
    let verb = if replace { "reassigned" } else { "assigned" };
    journal_core_note(
        inner,
        goal_id,
        format!("{verb} {target} to {}", list.join(", ")),
    );
    json!({"ok": true, "assignees": list})
}

/// Merge in list order, dropping duplicates at their first position — the same
/// rule `assign::workers` reads the union by, so what is stored and what is
/// resolved agree on precedence.
fn merged_assignees(current: Vec<Assignee>, added: Vec<Assignee>, replace: bool) -> Vec<Assignee> {
    let mut out = if replace { Vec::new() } else { current };
    for a in added {
        if !out.contains(&a) {
            out.push(a);
        }
    }
    out
}

/// Capture what recurs as a standing goal, in the workspace's default mode:
/// the Workflow Agent designs its workflow — the start event its statement
/// names — and, once the design is adopted, the goal listens. A note on the
/// goal says who captured it.
fn capture_goal(inner: &Arc<Inner>, statement: String, title: Option<String>) -> serde_json::Value {
    let captured = ops::submit(
        inner,
        ops::SubmitRequest {
            title: title.filter(|t| !t.trim().is_empty()),
            mode: ops::default_goal_mode(inner),
            ..ops::SubmitRequest::captured(statement)
        },
    );
    match captured {
        Ok(goal) => {
            journal_core_note(
                inner,
                goal.id,
                "captured by the General Agent: work that recurs, to be designed with the event it starts on".to_string(),
            );
            json!({"ok": true, "goal": goal.id.to_string()})
        }
        Err(e) => err(e.to_string()),
    }
}

/// A workflow named by id or by catalog slug. `install` says whether a slug
/// that is not installed yet may be installed on the way or must be answered
/// from the catalog alone (the Workflow Agent's `get_workflow`, which installs
/// nothing).
fn resolve_workflow_ref(
    inner: &Arc<Inner>,
    raw: &str,
    install: bool,
) -> Result<Option<Workflow>, String> {
    if let Ok(id) = WorkflowId::from_str(raw) {
        return match inner.ws.get_workflow(id) {
            Ok(wf) => Ok(Some(wf)),
            Err(StoreError::WorkflowNotFound(_)) => Ok(None),
            Err(e) => Err(e.to_string()),
        };
    }
    match inner.ws.workflow_for_slug(raw) {
        Ok(Some(wf)) => return Ok(Some(wf)),
        Ok(None) => {}
        Err(e) => return Err(e.to_string()),
    }
    if !install {
        return Ok(None);
    }
    let installed = crate::admin::install_catalog_entry(inner, CatalogKind::Workflow, raw)
        .map_err(|e| e.to_string())?;
    match installed.workflows.iter().find(|(slug, _)| slug == raw) {
        Some((_, id)) => inner
            .ws
            .get_workflow(*id)
            .map(Some)
            .map_err(|e| e.to_string()),
        None => inner.ws.workflow_for_slug(raw).map_err(|e| e.to_string()),
    }
}

/// The catalog's workflow templates and this workspace's own workflows.
/// The catalog's workflow templates and this workspace's own workflows —
/// what the op below renders as JSON and the Workflow Agent's wake renders
/// as its `TEMPLATES` block.
pub(crate) fn workflow_templates(
    inner: &Inner,
) -> Result<(Vec<bisa_store::CatalogEntry>, Vec<bisa_core::Workflow>), bisa_store::StoreError> {
    let templates = inner.ws.catalog_entries(Some(CatalogKind::Workflow))?;
    let workflows = inner.ws.list_workflows()?;
    Ok((templates, workflows))
}

fn list_workflow_templates(inner: &Arc<Inner>) -> serde_json::Value {
    let (templates, workflows) = match workflow_templates(inner) {
        Ok(x) => x,
        Err(e) => return err(e.to_string()),
    };
    let workflows: Vec<serde_json::Value> = workflows
        .iter()
        .map(|w| {
            json!({
                "id": w.id.to_string(),
                "name": w.name,
                "description": w.description,
                "steps": w.steps.len(),
                "origin": match &w.origin {
                    WorkflowOrigin::Catalog { slug } => json!({"catalog": slug}),
                    WorkflowOrigin::Workspace => json!("workspace"),
                    WorkflowOrigin::Goal { goal } => json!({"goal": goal.to_string()}),
                },
                "tags": w.tags,
            })
        })
        .collect();
    json!({"ok": true, "templates": templates, "workflows": workflows})
}

/// A workflow's problems, or the refusal to say when they could not be read:
/// an empty list would tell the agent the workflow is sound, and it would
/// propose one nobody checked.
fn problems_of(
    inner: &Arc<Inner>,
    wf: &bisa_core::Workflow,
) -> Result<Vec<bisa_core::Problem>, serde_json::Value> {
    inner.ws.validate_workflow(wf).map_err(|e| {
        tracing::warn!(workflow = %wf.id, "a workflow's problems could not be read for the Workflow Agent: {e}");
        err(format!(
            "the problems of workflow {} could not be read: {e}; ask again",
            wf.id
        ))
    })
}

/// One workflow by id or catalog slug, with its current problems. An
/// uninstalled template answers as the catalog reads it, under a fresh id
/// that nothing holds.
fn get_workflow(inner: &Arc<Inner>, raw: &str) -> serde_json::Value {
    match resolve_workflow_ref(inner, raw, false) {
        Ok(Some(wf)) => {
            let problems = match problems_of(inner, &wf) {
                Ok(problems) => problems,
                Err(refusal) => return refusal,
            };
            json!({"ok": true, "workflow": wf, "installed": true, "problems": problems})
        }
        Ok(None) => match inner.ws.catalog_workflow(raw) {
            Ok(Some(draft)) => {
                let wf = draft_as_workflow(inner, draft);
                let problems = match problems_of(inner, &wf) {
                    Ok(problems) => problems,
                    Err(refusal) => return refusal,
                };
                json!({"ok": true, "workflow": wf, "installed": false, "problems": problems,
                       "note": "a catalog template that is not installed; its agent steps \
                                name catalog agents the person installs with Install"})
            }
            Ok(None) => err(format!("no workflow or catalog template named {raw:?}")),
            Err(e) => err(e.to_string()),
        },
        Err(e) => err(e),
    }
}

/// A definition that exists only to be validated or read: a fresh id, the
/// owner as author, local, revision zero.
fn draft_as_workflow(inner: &Arc<Inner>, draft: NewWorkflow) -> Workflow {
    Workflow {
        id: WorkflowId::from_ulid(ulid::Ulid::from_datetime(std::time::SystemTime::now())),
        name: draft.name,
        description: draft.description,
        inputs: draft.inputs,
        steps: draft.steps,
        origin: WorkflowOrigin::Workspace,
        author: inner.ws.owner_principal(),
        tags: draft.tags,
        revision: 0,
        archived: None,
        decision_making: draft.decision_making,
        created_at: 0,
    }
}

/// Every problem a definition has, without recording it — the staffing
/// judged against `goal`'s roster when the design is a goal's.
fn validate_workflow(
    inner: &Arc<Inner>,
    goal: Option<GoalId>,
    draft: NewWorkflow,
) -> serde_json::Value {
    let staffing = staffing_problems(inner, goal, &draft.steps);
    let wf = draft_as_workflow(inner, draft);
    match inner.ws.validate_workflow(&wf) {
        Ok(problems) => {
            let mut all: Vec<serde_json::Value> = problems
                .iter()
                .filter_map(|p| serde_json::to_value(p).ok())
                .collect();
            all.extend(staffing);
            json!({"ok": true, "problems": all})
        }
        Err(e) => err(e.to_string()),
    }
}

/// The workflow `save_workflow` writes: the one the session's conversation
/// is about, and no other. A call that names one must name that one; a
/// session in no such conversation — a goal's cycle, a goal's thread, a
/// channel — is told what writes a goal's design instead. The rule is what
/// keeps the agent's one write the person's ask: it edits the workflow they
/// opened a conversation about, where the canvas is beside them.
fn workflow_of(
    inner: &Arc<Inner>,
    named: Option<&str>,
    scope: Option<&str>,
) -> Result<WorkflowId, String> {
    let about = scope
        .map(|s| crate::conversation::scope_facts(inner, s))
        .and_then(|facts| facts.conversation)
        .and_then(|c| c.origin.workflow())
        .ok_or_else(|| {
            "save_workflow writes the library workflow a conversation is about, from that \
             conversation; this session is in none. A goal's design is proposed with \
             propose_workflow."
                .to_string()
        })?;
    match named {
        None => Ok(about),
        Some(raw) if raw.trim().is_empty() => Ok(about),
        Some(raw) => match WorkflowId::from_str(raw.trim()) {
            Ok(id) if id == about => Ok(about),
            Ok(id) => Err(format!(
                "this conversation is about workflow {about}; {id} is another — ask in a \
                 conversation about the one you mean, or leave `workflow` out"
            )),
            Err(e) => Err(format!("{raw:?} is not a workflow id: {e}")),
        },
    }
}

/// The Workflow Agent's write to the conversation's workflow
/// ([`ops::revise_workflow`]): staffed, validated, at the revision it read.
fn save_workflow(
    inner: &Arc<Inner>,
    scope: Option<&str>,
    named: Option<&str>,
    revision: u64,
    draft: NewWorkflow,
) -> serde_json::Value {
    let id = match workflow_of(inner, named, scope) {
        Ok(id) => id,
        Err(why) => return err(why),
    };
    let stored = match inner.ws.get_workflow(id) {
        Ok(wf) => wf,
        Err(e) => return err(e.to_string()),
    };
    if stored.archived.is_some() {
        return err(format!(
            "workflow {id} is archived — it is out of the library and not edited; the person \
             takes it back out first"
        ));
    }
    // A library workflow is no goal's: the whole enabled staff.
    if let Some(refusal) = unstaffed_refusal(inner, None, &draft) {
        return refusal;
    }
    match ops::revise_workflow(inner, id, draft, revision) {
        Ok(wf) => json!({"ok": true, "workflow": wf.id.to_string(), "revision": wf.revision}),
        Err(crate::EngineError::Store(StoreError::RevisionConflict {
            actual, expected, ..
        })) => err(format!(
            "workflow {id} moved to revision {actual} since you read revision {expected} — \
                 read it again with get_workflow and save at that revision"
        )),
        Err(e) => problems_or_err(e),
    }
}

/// The roster a step may name from: `goal`'s — the agents and teams it names
/// to carry it, when it names any — else the whole enabled staff. The one
/// door `list_staff` and the staffing rule read, so the two never disagree.
fn roster_for(
    inner: &Arc<Inner>,
    goal: Option<GoalId>,
) -> Result<crate::staff::StaffRoster, StoreError> {
    match goal {
        Some(goal) => crate::staff::StaffRoster::for_goal(&inner.ws, goal),
        None => crate::staff::StaffRoster::of(&inner.ws),
    }
}

/// The staffing rule at the agent's door: while staff is installed and
/// enabled, an agent step names one of them — an agent, a team, or an input of
/// kind `assignee` — and never someone who is not here; for a goal that names
/// agents or teams to carry it, never anyone it does not name (`goal`'s
/// roster, [`roster_for`]). Reported in the same `{step, kind, message}` shape
/// as the validator's problems, so the Workflow Agent fixes them the same way.
/// Hand-drawn designs are not held to this: an unassigned step there falls
/// back to the goal's own assignees.
fn staffing_problems(
    inner: &Arc<Inner>,
    goal: Option<GoalId>,
    steps: &[bisa_core::Step],
) -> Vec<serde_json::Value> {
    let roster = match roster_for(inner, goal) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("cannot read the staff roster for a proposal: {e}");
            return Vec::new();
        }
    };
    let mut out = Vec::new();
    let count = roster.standing();
    for step in crate::staff::unstaffed_steps(steps, &roster) {
        out.push(json!({
            "step": step,
            "kind": "unstaffed_step",
            "message": format!("step `{step}` names nobody; {count} — name one with {{\"agent\": id}} or {{\"team\": id}}, or an input of kind `assignee`"),
        }));
    }
    for (step, who) in crate::staff::misstaffed_steps(steps, &roster) {
        let outsider = roster.outsider();
        out.push(json!({
            "step": step,
            "kind": "unknown_assignee",
            "message": format!("step `{step}` names `{who}`, who is {outsider}; {count}"),
        }));
    }
    out
}

/// A proposal or an amendment whose steps are not staffed — against `goal`'s
/// roster when the design is a goal's: refused in the shape `problems_or_err`
/// answers, before anything is written — and with every other problem the
/// definition has beside the staffing ones, so the session fixes the whole
/// draft in one round rather than one refusal at a time.
fn unstaffed_refusal(
    inner: &Arc<Inner>,
    goal: Option<GoalId>,
    draft: &NewWorkflow,
) -> Option<serde_json::Value> {
    let mut problems = staffing_problems(inner, goal, &draft.steps);
    if problems.is_empty() {
        return None;
    }
    // The validator names an unknown assignee too; the staffing line above
    // says it with the roster in hand, so only the rest is added.
    let wf = draft_as_workflow(inner, draft.clone());
    if let Ok(found) = inner.ws.validate_workflow(&wf) {
        problems.extend(
            found
                .iter()
                .filter(|p| p.kind != bisa_core::ProblemKind::UnknownAssignee)
                .filter_map(|p| serde_json::to_value(p).ok()),
        );
    }
    Some(json!({
        "ok": false,
        "errors": problems.iter().map(|p| p["message"].clone()).collect::<Vec<_>>(),
        "problems": problems,
    }))
}

/// Make the folder the work happens in.
///
/// The root is always `Managed` — this op has no way to name a folder on
/// disk, and that is deliberate: an agent that could adopt a path could adopt
/// the workspace. Because the folder is one Bisa just made, it is
/// initialised as a git repository; the old `git` flag was a choice an agent
/// had no basis to make and mostly did not pass, which left the work it went
/// on to do in a throwaway copy rather than on a branch.
///
/// A failed materialization takes the record with it — a project row claiming
/// a folder that is not there is worse than no project.
/// Where a project an op creates is born: a work-item session's project is
/// born of its step — the goal's when the step's workflow is the goal's own
/// design, the workflow's when it is the library's or the run is the
/// workspace's (`ProjectOrigin::born_of_step`); a goal's session (or an
/// explicit goal) makes it the goal's; anything else is the workspace's. Pure, so the rule has a unit test per
/// row rather than a socket each. `run` is the frozen run the spec names,
/// when the run machine still knows it.
fn project_origin_for(
    spec: Option<&bisa_core::WorkItemSpec>,
    goal: Option<GoalId>,
    run: Option<&bisa_core::WorkflowRun>,
) -> bisa_core::ProjectOrigin {
    use bisa_core::{ProjectOrigin, StepRef};
    match (spec, goal) {
        (Some(spec), _) => match (spec.run, spec.step.clone(), run) {
            (Some(run_id), Some(step), Some(run)) if run.id == run_id => {
                ProjectOrigin::born_of_step(
                    spec.home.goal(),
                    StepRef {
                        run: run_id,
                        step,
                        workflow: run.workflow.id,
                    },
                    &run.workflow.origin,
                )
            }
            _ => match spec.home.goal() {
                Some(goal) => ProjectOrigin::from_goal(goal),
                None => ProjectOrigin::Workspace,
            },
        },
        (None, Some(goal)) => ProjectOrigin::from_goal(goal),
        (None, None) => ProjectOrigin::Workspace,
    }
}

/// Make a managed, git-initialised folder under a goal.
///
/// `agent` is the session that asked, and it is used only to sign the journal
/// entry: a project a Developer made should not read as the core agent's
/// work. `None` — a session with no agent identity at all — signs as the
/// owner, which is what every other unattributed write here does. Any agent
/// may call this; see the dispatch arm for why.
async fn create_project(
    inner: &Arc<Inner>,
    agent: Option<&str>,
    goal: Option<&str>,
    spec: Option<&bisa_core::WorkItemSpec>,
    slug: String,
    name: Option<String>,
    assignees: Vec<String>,
) -> serde_json::Value {
    // Resolved before anything is written: attaching to a goal that does not
    // exist would leave a project nobody asked for.
    let goal_id = match goal {
        Some(raw) => match resolve_goal_scope(inner, raw) {
            Ok(id) => Some(id),
            Err(e) => return e,
        },
        // A run of the workspace's item has no goal to attach to.
        None => spec.and_then(|s| s.home.goal()),
    };
    // Where the project is being born: a step of a run, a goal's session, or
    // the workspace — derived here from what the op already holds, never sent
    // by a caller (I28a).
    let run = spec
        .and_then(|s| s.run)
        .and_then(|run| inner.ws.get_run(run).ok());
    let origin = project_origin_for(spec, goal_id, run.as_ref());
    let assignees = match parse_assignees(&assignees) {
        Ok(a) => a,
        Err(e) => return e,
    };
    if let Err(e) = ops::check_assignees(inner, &assignees) {
        return err(e.to_string());
    }
    // The core owns the slug allowlist: a slug becomes a directory name under
    // the workspace root, so it is a path-traversal boundary, not a label.
    let slug = match Slug::new(slug) {
        Ok(s) => s,
        Err(e) => return err(e.to_string()),
    };
    // The one path a project is made by: the folder, `git init`, a root
    // commit, and — with a goal — the attachment and the journal line that
    // tells a person later. The bus was told by `create`.
    let created = match crate::projects::create_for_agent(
        inner,
        NewProject {
            origin,
            slug,
            name,
            root: ProjectRoot::Managed,
            vcs: Vcs::None,
            assignees,
            publish: PublishPolicy::default(),
            tags: Default::default(),
        },
        goal_id,
        agent,
    )
    .await
    {
        Ok(created) => created,
        Err(e) => return err(e.to_string()),
    };
    let record = created.project;
    let materialized = inner.ws.project_root_path(&record);
    json!({"ok": true, "project": record.id.to_string(), "slug": record.slug,
           "origin": record.origin.as_str(),
           "attached_to": goal_id.map(|g| g.to_string()),
           "path": materialized.display().to_string()})
}

/// Take an agent's own files into the blob store, refusing anything it did not
/// make.
///
/// **The containment check is the point.** An agent runs with its harness's
/// full tools and can read most of this disk, so "attach this path" without a
/// boundary would be a way to publish anything it could open — a key file, a
/// journal, somebody's document — into a conversation a collaborator syncs.
/// Confining it to the agent's own `work/` says: you may hand over what you
/// made. `resolve_within` is the same primitive the file routes use, and it
/// refuses `..`, an absolute path and a symlink pointing out of the tree.
///
/// An owner-signed post has no work folder to be confined to, and no reason to
/// want one: the desktop uploads its files through the HTTP route.
fn ingest_agent_attachments(
    inner: &Inner,
    agent: Option<&AgentId>,
    roots: &[SessionRoot],
    paths: &[String],
) -> Result<Vec<bisa_core::AttachmentRef>, String> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    if agent.is_none() {
        return Err("only an agent may attach a file to a message it posts".into());
    }
    let mut out = Vec::new();
    for path in paths {
        let (full, _) =
            resolve_in_roots(roots, path).map_err(|e| format!("cannot attach {path:?}: {e}"))?;
        out.push(store_file(inner, &full, path)?);
    }
    Ok(out)
}

/// One artifact as the tool names it: a path under the session's roots, and
/// the title it renders under — the file's stem when none is given.
#[derive(Clone, Debug, Deserialize)]
pub struct ArtifactSpec {
    pub path: String,
    #[serde(default)]
    pub title: Option<String>,
}

/// Where a session may take a file from: its own scratch, the checkout it
/// works in, its home's scratch — the goal's, or the run of the workspace's.
/// Only the last two carry a `source` — a file made in a checkout or a
/// home's scratch can be opened in the IDE; an agent's scratch folder is
/// nobody's root but the agent's.
#[derive(Clone, Debug)]
pub struct SessionRoot {
    pub path: std::path::PathBuf,
    pub source: Option<(bisa_core::FileScope, String)>,
}

/// The roots a session's `post_message` resolves a path against, in the
/// order they are tried: the agent's scratch (always), the work item's open
/// workstream checkout and its home's scratch — its goal's, or its run of the
/// workspace's — the checkout and the goal a conversation's origin names,
/// the goal a goal scope or the session's `goal` names.
///
/// Widening the door from the agent's scratch alone to these is what lets a
/// session working in a checkout publish what it wrote there — the whole
/// point of an artifact — while still refusing anything it merely found on
/// the disk.
pub fn session_roots(
    inner: &Inner,
    scope: &str,
    goal: Option<GoalId>,
    agent: Option<&AgentId>,
    work_item: Option<WorkItemId>,
) -> Vec<SessionRoot> {
    use bisa_core::FileScope;
    fn push(roots: &mut Vec<SessionRoot>, root: SessionRoot) {
        if !roots.iter().any(|r| r.path == root.path) {
            roots.push(root);
        }
    }
    fn home_scratch(inner: &Inner, roots: &mut Vec<SessionRoot>, home: Home) {
        let scope = match home {
            Home::Goal { .. } => FileScope::Goal,
            Home::Run { .. } => FileScope::Run,
        };
        push(
            roots,
            SessionRoot {
                path: inner.ws.paths().home(&home).scratch(),
                source: Some((scope, home.id())),
            },
        );
    }
    fn goal_scratch(inner: &Inner, roots: &mut Vec<SessionRoot>, g: GoalId) {
        home_scratch(inner, roots, Home::Goal { goal: g });
    }
    let mut roots: Vec<SessionRoot> = Vec::new();
    if let Some(agent) = agent {
        push(
            &mut roots,
            SessionRoot {
                path: inner.ws.paths().agent(agent).scratch(),
                source: None,
            },
        );
    }
    if let Some(item) = work_item {
        if let Some(spec) = executor::find_work_item(inner, item) {
            if let Ok(workstreams) = inner
                .ws
                .list_workstreams(bisa_store::WorkstreamFilter::WorkItem(item))
            {
                for w in workstreams {
                    if matches!(w.state, bisa_core::WorkstreamState::Closed) {
                        continue;
                    }
                    if let Ok(project) = inner.ws.get_project(w.project) {
                        let path = inner.ws.checkout_in(&project, &w);
                        if path.is_dir() {
                            push(
                                &mut roots,
                                SessionRoot {
                                    path,
                                    source: Some((FileScope::Workstream, w.id.to_string())),
                                },
                            );
                        }
                    }
                }
            }
            home_scratch(inner, &mut roots, spec.home);
        }
    }
    // A conversation's roots are its origin's: the checkout it runs in, the
    // goal it is about.
    let origin = inner.ws.conversation_of_scope(scope).map(|c| c.origin);
    let conversation_workstream = origin.as_ref().and_then(|o| match o {
        bisa_core::ConversationOrigin::Workstream { id, .. } => Some(*id),
        bisa_core::ConversationOrigin::Project { id } => {
            Some(bisa_core::WorkstreamId::primary_of(*id))
        }
        _ => None,
    });
    if let Some(g) = origin.as_ref().and_then(|o| o.goal()) {
        goal_scratch(inner, &mut roots, g);
    }
    if let Some(wid) = conversation_workstream {
        if let Ok((w, _, path)) = crate::projects::checkout_of(inner, wid) {
            if path.is_dir() {
                push(
                    &mut roots,
                    SessionRoot {
                        path,
                        source: Some((FileScope::Workstream, w.id.to_string())),
                    },
                );
            }
        }
    }
    if let Ok(g) = scope.parse::<GoalId>() {
        if inner.ws.get_goal(g).is_ok() {
            goal_scratch(inner, &mut roots, g);
        }
    }
    if let Some(g) = goal {
        goal_scratch(inner, &mut roots, g);
    }
    roots
}

/// A path as the tool gave it, resolved under one of the roots: an absolute
/// path is matched to the root it lies under and the rest resolved beneath
/// it; a relative path is tried against each root in order. A refusal names
/// every boundary, so an agent can see where it may take a file from.
pub fn resolve_in_roots<'a>(
    roots: &'a [SessionRoot],
    path: &str,
) -> Result<(std::path::PathBuf, &'a SessionRoot), String> {
    if roots.is_empty() {
        return Err("this session has no folder to take a file from".into());
    }
    let requested = std::path::Path::new(path);
    if requested.is_absolute() {
        for root in roots {
            let Ok(base) = root.path.canonicalize() else {
                continue;
            };
            let candidate = requested
                .canonicalize()
                .unwrap_or_else(|_| requested.to_path_buf());
            if let Ok(rest) = candidate.strip_prefix(&base) {
                let rel = rest.to_string_lossy().to_string();
                if rel.is_empty() {
                    return Err(format!("{path:?} is a folder, not a file"));
                }
                let full =
                    bisa_store::resolve_within(&root.path, &rel).map_err(|e| e.to_string())?;
                return Ok((full, root));
            }
        }
    } else {
        for root in roots {
            if let Ok(full) = bisa_store::resolve_within(&root.path, path) {
                if full.is_file() {
                    return Ok((full, root));
                }
            }
        }
    }
    let boundaries = roots
        .iter()
        .map(|r| r.path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!(
        "{path:?} is not a file under any folder this session may publish from ({boundaries})"
    ))
}

/// Read a file the roots vouched for into the content-addressed store.
fn store_file(
    inner: &Inner,
    full: &std::path::Path,
    path: &str,
) -> Result<bisa_core::AttachmentRef, String> {
    let bytes = std::fs::read(full).map_err(|e| format!("cannot read {path:?}: {e}"))?;
    let name = full
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string());
    // Labelled from the extension here and nowhere else. Whether a viewer
    // renders something inline is decided from the bytes it fetched.
    let mime = bisa_core::mime_of_name(&name);
    inner
        .ws
        .put_attachment(&bytes, &name, mime)
        .map_err(|e| format!("cannot attach {path:?}: {e}"))
}

/// The artifacts a tool call names, stored and described. More than the cap
/// is refused before anything is read; a file under a checkout or a goal's
/// scratch records where it came from.
fn ingest_agent_artifacts(
    inner: &Inner,
    agent: Option<&AgentId>,
    roots: &[SessionRoot],
    specs: &[ArtifactSpec],
) -> Result<Vec<bisa_core::ArtifactRef>, String> {
    if specs.is_empty() {
        return Ok(Vec::new());
    }
    if agent.is_none() {
        return Err("only an agent may post an artifact through its tool".into());
    }
    if specs.len() > bisa_core::MAX_ARTIFACTS_PER_MESSAGE {
        return Err(format!(
            "a message carries at most {} artifacts, not {}",
            bisa_core::MAX_ARTIFACTS_PER_MESSAGE,
            specs.len()
        ));
    }
    let mut out = Vec::new();
    for spec in specs {
        let (full, root) = resolve_in_roots(roots, &spec.path)
            .map_err(|e| format!("cannot post {:?} as an artifact: {e}", spec.path))?;
        let file = store_file(inner, &full, &spec.path)?;
        let source = root.source.as_ref().and_then(|(scope, id)| {
            // `resolve_within` answers the real path; the root may be spelt
            // through a symlink (a tempdir under `/var` on macOS), so the
            // prefix is stripped as the file system knows it.
            let base = root
                .path
                .canonicalize()
                .unwrap_or_else(|_| root.path.clone());
            let rel = full
                .strip_prefix(&base)
                .or_else(|_| full.strip_prefix(&root.path))
                .ok()?;
            let rel = bisa_core::RelPath::new(rel.to_string_lossy().to_string()).ok()?;
            Some(bisa_core::ArtifactSource {
                scope: *scope,
                id: id.clone(),
                path: rel,
            })
        });
        let artifact = bisa_core::ArtifactRef::from_attachment(file, spec.title.clone(), source);
        artifact.validate().map_err(|e| e.to_string())?;
        out.push(artifact);
    }
    Ok(out)
}

/// The agent a recall op belongs to: explicit, else the work item's agent.
/// Whose words a post through intake is: the session's agent, else the work
/// item's, else the General Agent's — never the person's. An agent speaking
/// through its tool is somebody speaking, and that somebody is an agent.
/// Recall keeps its own rule (`resolve_recall_agent`): a memory needs an
/// identity to belong to, and the General Agent's is not a nameless
/// session's to write.
fn posting_agent(
    inner: &Arc<Inner>,
    agent: Option<String>,
    work_item: Option<WorkItemId>,
) -> AgentId {
    resolve_recall_agent(inner, agent, work_item).unwrap_or_else(AgentId::general)
}

pub(crate) fn resolve_recall_agent(
    inner: &Arc<Inner>,
    agent: Option<String>,
    work_item: Option<WorkItemId>,
) -> Option<AgentId> {
    agent
        .or_else(|| {
            work_item
                .and_then(|wi| executor::find_work_item(inner, wi))
                .and_then(|spec| spec.agent)
        })
        .and_then(|a| AgentId::new(a).ok())
}

async fn ask_human(
    inner: &Arc<Inner>,
    work_item: Option<WorkItemId>,
    goal: Option<&str>,
    question: String,
    expects: Option<String>,
    options: Vec<AskOptionInput>,
    multi: bool,
) -> serde_json::Value {
    let (home, _spec) = match resolve_scope(inner, work_item, goal) {
        Ok(x) => x,
        Err(e) => return e,
    };
    let options: Vec<AskOption> = options.into_iter().map(Into::into).collect();
    // A question is an answer unless the asker says "decision": a person met
    // with Approve/Decline where they were meant to type something cannot
    // answer at all.
    let expects = match expects.as_deref() {
        Some("decision") => {
            // A decision gate is genuinely binary; options beside it would be
            // a list nothing can ever select from.
            if !options.is_empty() {
                return err(
                    "a decision gate is approve/decline; use expects=\"answer\" to offer options",
                );
            }
            AskKind::Decision
        }
        None | Some("answer") => AskKind::Answer { options, multi },
        Some(other) => return err(format!("expects must be decision|answer, got {other:?}")),
    };
    // The question's own shape is checked before it reaches a human: two
    // recommendations is the asker failing to have an opinion, and a duplicate
    // option id makes a recorded answer ambiguous forever.
    if let Err(e) = expects.validate() {
        return err(e.to_string());
    }
    let subject = match work_item {
        Some(wi) => format!("ask_human:{wi}"),
        None => format!("ask_human:{}", home.id()),
    };
    let (gate_id, _rx) = inner.gates.open(
        home,
        work_item,
        Gate::Escalation,
        subject.clone(),
        question.clone(),
        expects.clone(),
    );
    // Journal the question under its *subject*, not the gate's ULID: the
    // subject is what survives a restart, so the inbox can rebuild the
    // question and a durable decision can name it.
    let owner = inner.ws.owner_keys().clone();
    warn_on_err(
        inner.ws.append_journal(
            &home,
            JournalPayload::Question {
                work_item,
                gate: subject,
                text: question.clone(),
                expects: expects.clone(),
            },
            &owner,
            None,
        ),
        "journaling a question",
    );
    if let (None, Some(goal)) = (work_item, home.goal()) {
        // A goal-scoped question with no work item is the Workflow Agent's
        // own, when a guided wake owns the goal: its standing is now `asking`.
        crate::guided::note_asked(inner, goal, &question);
    }
    // The session that asked is waiting on the person now.
    let asker = work_item
        .and_then(|wi| inner.presence.by_work_item(wi))
        .or_else(|| {
            home.goal()
                .and_then(|goal| inner.presence.by_goal(goal).into_iter().next())
        });
    if let Some(run) = asker {
        inner.presence.waiting(
            inner,
            run,
            &gate_id,
            crate::presence::WaitingOn::Question {
                text: question.clone(),
                gate_id: Some(gate_id.clone()),
            },
        );
    }
    let scope = inner.home_scope(&home);
    inner.emit(scope.event(
        work_item,
        EnginePayload::QuestionAsked {
            gate_id: gate_id.clone(),
            text: question.clone(),
            expects,
        },
    ));
    inner.emit(scope.event(
        work_item,
        EnginePayload::GateOpened {
            gate_id: gate_id.clone(),
            gate: Gate::Escalation,
            question,
        },
    ));
    json!({"ok": true, "gate": gate_id})
}

async fn await_decision(inner: &Arc<Inner>, gate: String) -> serde_json::Value {
    let Some(entry) = inner.gates.get(&gate) else {
        return err("unknown gate");
    };
    if let Some(r) = entry.resolution {
        return resolved(r);
    }
    // Poll at a coarse interval; decisions are rare human events.
    let inner2 = Arc::clone(inner);
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        match inner2.gates.get(&gate) {
            Some(e) => {
                if let Some(r) = e.resolution {
                    return resolved(r);
                }
            }
            None => return err("gate vanished"),
        }
    }
}

/// What an agent is told when the Decision-Making Agent is not on for it.
pub const DECIDE_OFF: &str = "the Decision-Making Agent is off for this agent: decide it yourself. It is switched on for the workspace in Settings › Decision Settings › Decision Making, or for one agent on the agent";

/// `decide`: the session's questions to the Decision-Making Agent, for a
/// session whose agent has it on. The answer is the decision contract's
/// response, with `sure` — whether it cleared `decisions.confidence.act` —
/// beside it: an agent reads an unsure answer as what it is, and nothing acts
/// on it but the agent.
async fn decide(
    inner: &Arc<Inner>,
    request: bisa_core::DecisionRequest,
    scope: Option<&str>,
    agent: Option<String>,
    work_item: Option<WorkItemId>,
    goal_named: Option<GoalId>,
) -> serde_json::Value {
    if let Err(e) = request.validate() {
        return err(e.to_string());
    }
    let agent_id = resolve_recall_agent(inner, agent, work_item).unwrap_or_else(AgentId::general);
    let facts = scope
        .map(|s| crate::conversation::scope_facts(inner, s))
        .unwrap_or_default();
    let home = facts
        .goal
        .or(goal_named)
        .map(Home::from)
        .or_else(|| work_item.and_then(|wi| inner.ws.home_of_work_item(wi).ok()));
    let standing = crate::decider::Standing {
        home,
        project: crate::browser::project_of(inner, &facts, work_item),
        switched_on: inner
            .ws
            .get_agent(&agent_id)
            .is_ok_and(|a| a.decision_making),
        agent: Some(agent_id.to_string()),
        ..Default::default()
    };
    let point = bisa_core::DecisionPoint::AgentDecide;
    match crate::decider::judge(inner, point, &standing, request).await {
        crate::decider::Judged::Off => err(DECIDE_OFF),
        crate::decider::Judged::Failed(why) => err(format!("no judgement: {why}")),
        crate::decider::Judged::Answered(response) => {
            json!({"ok": true, "result": {"sure": true, "response": response}})
        }
        crate::decider::Judged::Unsure(response) => {
            json!({"ok": true, "result": {"sure": false, "response": response}})
        }
    }
}

/// Park a browser request for the desktop and wait for its answer — after
/// the workspace's word on who may ask (`browser::Access`): the switch, the
/// policy against this session's agent, the reach against an `open`'s URL,
/// the scripts policy against an `eval`. **This is the boundary**, as
/// `core_agent_only` is for the platform's ops: the tool list is a menu. The
/// agent is the session's, else the work item's, else the General Agent's
/// (`resolve_recall_agent`); the project whose settings bind is the
/// checkout's or the work item's (`browser::project_of`); the tab is at home
/// where the engine says (`browser::home_of`); and a screenshot's upload
/// becomes a named copy the agent reads by path.
async fn browser(
    inner: &Arc<Inner>,
    request: crate::browser::BrowserRequest,
    scope: Option<&str>,
    agent: Option<String>,
    work_item: Option<WorkItemId>,
    goal_named: Option<GoalId>,
) -> serde_json::Value {
    let facts = scope
        .map(|s| crate::conversation::scope_facts(inner, s))
        .unwrap_or_default();
    // The agent, and the goal whose mode decides out of sight — the scope's,
    // else the one the session named (resolved already, so it exists), else
    // the work item's — gathered once for the policy, the tab and the screen.
    let screening =
        crate::content::Screening::gather(inner, &facts, scope, agent, work_item, goal_named);
    let agent_id = screening.agent.clone();
    let home = screening.home;
    let access =
        crate::browser::access(inner, crate::browser::project_of(inner, &facts, work_item));
    let has_skill = || {
        inner
            .ws
            .get_agent(&agent_id)
            .map(|a| {
                a.skills
                    .iter()
                    .any(|s| s.as_str() == crate::browser::BROWSER_SKILL)
            })
            .unwrap_or(false)
    };
    if let Some(why) = access.refusal(&agent_id, has_skill, &request) {
        tracing::info!(agent = %agent_id, action = ?request.action, why, "browser request refused by policy");
        return err(why);
    }
    // Nobody home is said at once, never after a wait: a desktop that is
    // open reads the list within the presence window.
    if !inner.browser.desktop_present() {
        tracing::debug!(agent = %agent_id, action = ?request.action, "browser request refused: nobody home");
        return json!({"ok": true, "result": crate::browser::BrowserResult::refused(crate::browser::NOBODY_HOME)});
    }
    // Out of sight in a goal that runs unattended, unless the agent said. A
    // run of the workspace is attended: its tabs are shown.
    let mode = home
        .and_then(|h| h.goal())
        .and_then(|g| inner.ws.get_goal(g).ok())
        .map(|g| g.mode);
    let headless = crate::browser::headless_for(access.headless, mode, request.headless);
    // Where the rule had a choice to make, the Decision-Making Agent — when
    // it is on here — may say the page needs a person, and the tab is shown.
    let standing = crate::decider::Standing {
        home,
        project: crate::browser::project_of(inner, &facts, work_item),
        switched_on: inner
            .ws
            .get_agent(&agent_id)
            .is_ok_and(|a| a.decision_making),
        agent: Some(agent_id.to_string()),
        ..Default::default()
    };
    let headless =
        match crate::browser::shown_after_all(inner, &access, &request, headless, standing).await {
            Some(true) => false,
            _ => headless,
        };
    let (id, rx) = inner.browser.ask(
        inner,
        request,
        crate::browser::BrowserScope {
            home: crate::browser::home_of(inner, &facts, scope, work_item, goal_named),
            agent: Some(agent_id.to_string()),
        },
        headless,
    );
    let result = inner.browser.wait(&id, rx).await.with_named_copy(inner);
    // What the page said is content from outside: screened before the agent
    // reads it, framed as data whatever the verdict (`crate::content`).
    let readable = result
        .text
        .clone()
        .filter(|t| !t.trim().is_empty())
        .or_else(|| {
            result
                .value
                .as_ref()
                .filter(|v| !v.is_null())
                .map(|v| v.to_string())
        });
    let Some(text) = readable else {
        return json!({"ok": true, "result": result});
    };
    let source = crate::content::ContentSource::Page {
        url: result.url.clone().unwrap_or_default(),
        title: result.title.clone(),
    };
    match crate::content::screen(
        inner,
        &screening,
        crate::content::ContentSubject { source, text },
    )
    .await
    {
        crate::content::Screened::Read { framing, verdict } => {
            let mut reply = serde_json::to_value(&result).unwrap_or_else(|_| json!({"ok": true}));
            reply["screen"] = json!({"verdict": verdict, "note": framing});
            json!({"ok": true, "result": reply})
        }
        crate::content::Screened::Withheld { sentence } => json!({
            "ok": true,
            "result": {"ok": true, "tab": result.tab, "url": result.url, "withheld": true, "text": sentence},
        }),
    }
}

/// A drawing tool (19 — Drawings): the workspace's word on who may draw is
/// checked first — the switch, then the policy against the asking agent —
/// and then the request is answered from the store or parked for the
/// desktop's canvas (`crate::drawings::perform`). The drawing a call names
/// none is the conversation's; the scope a new drawing is filed under, when
/// the call says nothing, is what the conversation is about or the goal the
/// session serves.
async fn draw(
    inner: &Arc<Inner>,
    request: crate::drawings::DrawRequest,
    scope: Option<&str>,
    agent: Option<String>,
    work_item: Option<WorkItemId>,
    goal_named: Option<GoalId>,
) -> serde_json::Value {
    let facts = scope
        .map(|s| crate::conversation::scope_facts(inner, s))
        .unwrap_or_default();
    let screening =
        crate::content::Screening::gather(inner, &facts, scope, agent, work_item, goal_named);
    let agent_id = screening.agent.clone();
    let access =
        crate::drawings::access(inner, crate::browser::project_of(inner, &facts, work_item));
    let has_skill = || {
        inner
            .ws
            .get_agent(&agent_id)
            .map(|a| {
                a.skills
                    .iter()
                    .any(|s| s.as_str() == crate::drawings::DRAW_SKILL)
            })
            .unwrap_or(false)
    };
    if let Some(why) = access.refusal(&agent_id, has_skill) {
        tracing::info!(agent = %agent_id, action = ?request.action, why, "drawing request refused by policy");
        return err(why);
    }
    let standing = crate::drawings::Standing {
        conversation: facts.conversation.clone(),
        goal: screening.home.and_then(|h| h.goal()),
        // A run of the workspace's session files what it draws under the
        // run's workflow.
        workflow: screening
            .home
            .and_then(|h| h.run())
            .and_then(|run| inner.ws.get_run(run).ok())
            .map(|run| run.workflow.id),
    };
    let draw_scope = crate::drawings::DrawScope {
        agent: Some(agent_id.to_string()),
        conversation: facts.conversation.as_ref().map(|c| c.id.to_string()),
    };
    let result = crate::drawings::perform(inner, request, &standing, draw_scope).await;
    json!({"ok": true, "result": result})
}

/// The note a note tool means: the one named, else the one the session's
/// conversation is about — the way a drawing tool takes the conversation's
/// drawing. A call that names none outside such a conversation is refused
/// in a sentence that says what to pass.
fn note_of(
    inner: &Arc<Inner>,
    note: Option<&str>,
    scope: Option<&str>,
) -> Result<bisa_core::NoteId, String> {
    if let Some(named) = note {
        return named
            .parse()
            .map_err(|e| format!("{named:?} is not a note id: {e}"));
    }
    scope
        .map(|s| crate::conversation::scope_facts(inner, s))
        .and_then(|facts| facts.conversation)
        .and_then(|c| c.origin.note())
        .ok_or_else(|| {
            "No note given and this conversation is not about one — pass `note`.".to_string()
        })
}

/// `browser_serve`: the folder of the session's checkout, served by the
/// node's server and answered as a URL. The checkout is the scope's — a
/// conversation about a workstream or a project — else the work item's
/// (its workstream on disk); a session in neither has nothing to serve.
async fn browser_serve(
    inner: &Arc<Inner>,
    folder: Option<String>,
    scope: Option<&str>,
    agent: Option<String>,
    work_item: Option<WorkItemId>,
) -> serde_json::Value {
    let agent_id = resolve_recall_agent(inner, agent, work_item).unwrap_or_else(AgentId::general);
    let facts = scope
        .map(|s| crate::conversation::scope_facts(inner, s))
        .unwrap_or_default();
    let access =
        crate::browser::access(inner, crate::browser::project_of(inner, &facts, work_item));
    let has_skill = || {
        inner
            .ws
            .get_agent(&agent_id)
            .map(|a| {
                a.skills
                    .iter()
                    .any(|s| s.as_str() == crate::browser::BROWSER_SKILL)
            })
            .unwrap_or(false)
    };
    if let Some(why) = access.may_use(&agent_id, has_skill) {
        return err(why);
    }
    let server = inner
        .folder_server
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let Some(server) = server else {
        return err(crate::browser::NO_SERVER);
    };
    let checkout = facts
        .checkout
        .as_ref()
        .map(|(w, p, _)| (w.id, inner.ws.checkout_in(p, w)))
        .or_else(|| {
            let item = work_item?;
            let workstreams = inner
                .ws
                .list_workstreams(bisa_store::WorkstreamFilter::WorkItem(item))
                .ok()?;
            workstreams.into_iter().find_map(|w| {
                let path = inner.ws.workstream_checkout(&w).ok()?;
                path.is_dir().then_some((w.id, path))
            })
        });
    let Some((workstream, path)) = checkout else {
        return err(crate::browser::NO_CHECKOUT);
    };
    match server
        .serve(workstream, &path, folder.as_deref().unwrap_or(""))
        .await
    {
        Ok(page) => {
            // The Browser menu, the footer and the browser bar read the list
            // again, as they do when a person serves a folder.
            inner.emit(EngineEvent::global(EnginePayload::ServerChanged {
                workstream: Some(workstream),
            }));
            json!({"ok": true, "result": page})
        }
        Err(why) => err(&why),
    }
}

/// What an agent may ask of the mobile tools (ide/19).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MobileDevelopmentAction {
    /// What is installed here, and Flutter's doctor's word.
    Status,
    /// The simulators, emulators and phones, with their state.
    Devices,
    /// Boot a simulator, or start an emulator's image.
    Boot,
    /// A device's screen as a PNG, answered by path.
    Screenshot,
}

/// One mobile request, as the tool sent it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct MobileDevelopmentRequest {
    pub action: MobileDevelopmentAction,
    /// The device's id, for `boot` and `screenshot`.
    #[serde(default)]
    pub device: Option<String>,
    /// For `status`: examine the machine again rather than answer the last look.
    #[serde(default)]
    pub fresh: Option<bool>,
}

async fn mobile_development(
    inner: &Arc<Inner>,
    request: MobileDevelopmentRequest,
    scope: Option<&str>,
    agent: Option<String>,
    work_item: Option<WorkItemId>,
) -> serde_json::Value {
    let agent_id = resolve_recall_agent(inner, agent, work_item).unwrap_or_else(AgentId::general);
    let facts = scope
        .map(|s| crate::conversation::scope_facts(inner, s))
        .unwrap_or_default();
    let project = facts.project();
    let access = crate::mobile_development::access(inner, project);
    let has_skill = || {
        inner
            .ws
            .get_agent(&agent_id)
            .map(|a| {
                a.skills
                    .iter()
                    .any(|s| s.as_str() == crate::mobile_development::MOBILE_DEVELOPMENT_SKILL)
            })
            .unwrap_or(false)
    };
    // The platform of the device named is checked by the op itself, which
    // reads the list: the policy here is the switch and the agent.
    if let Some(why) = access.refusal(&agent_id, has_skill, None) {
        return err(why);
    }
    let device = || {
        request
            .device
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .ok_or_else(|| "name a device — mobile_development_devices lists them".to_string())
    };
    let result: Result<serde_json::Value, String> = match request.action {
        MobileDevelopmentAction::Status => {
            crate::mobile_development::status(inner, request.fresh.unwrap_or(false))
                .await
                .map(|s| json!(s))
                .map_err(|e| e.to_string())
        }
        MobileDevelopmentAction::Devices => crate::mobile_development::devices(inner, project)
            .await
            .map(|d| json!({"devices": d}))
            .map_err(|e| e.to_string()),
        MobileDevelopmentAction::Boot => match device() {
            Ok(id) => crate::mobile_development::boot(inner, project, id)
                .await
                .map(|d| json!({"device": d}))
                .map_err(|e| e.to_string()),
            Err(why) => Err(why),
        },
        MobileDevelopmentAction::Screenshot => match device() {
            Ok(id) => crate::mobile_development::screenshot(inner, project, id)
                .await
                .map(|s| json!(s))
                .map_err(|e| e.to_string()),
            Err(why) => Err(why),
        },
    };
    match result {
        Ok(result) => json!({"ok": true, "result": result}),
        Err(why) => err(why),
    }
}

/// A resolved gate, as the waiting session reads it.
///
/// `clarify_rounds_left` rides along rather than being looked up separately,
/// because the caller that needs it is on the other side of a socket: an agent
/// that learns "not sure" without learning how many attempts remain has no way
/// to know when to stop asking.
fn resolved(r: crate::gates::GateResolution) -> serde_json::Value {
    let mut v = json!({"ok": true, "approve": r.approve, "answer": r.answer});
    if let Some(left) = r.clarify_rounds_left {
        v["clarify_rounds_left"] = json!(left);
    }
    v
}

/// A run as a session reads it: the steps in workflow order, each with its
/// state and what it produced, so a Repair wake sees the failure and a worker
/// sees what upstream steps decided.
fn run_view(r: &bisa_core::WorkflowRun) -> serde_json::Value {
    let steps: Vec<serde_json::Value> = r
        .workflow
        .steps
        .iter()
        .map(|s| {
            let rec = r.steps.get(&s.id).cloned().unwrap_or_default();
            json!({
                "id": s.id,
                "name": s.name,
                "kind": s.kind.as_str(),
                "state": rec.state.as_str(),
                "output": rec.output,
                "answer": rec.answer,
                "error": rec.error,
                "work_item": rec.work_item,
            })
        })
        .collect();
    json!({
        "id": r.id.to_string(),
        "scope": r.scope.as_str(),
        "status": r.status().as_str(),
        "workflow": {"id": r.workflow.id, "name": r.workflow.name, "revision": r.workflow.revision},
        "inputs": r.inputs,
        "start": r.start,
        "event": r.event,
        "steps": steps,
    })
}

/// The work items filed at a home — of one run only when `run` names it (a
/// goal files every run's items together).
fn work_items_view(inner: &Inner, home: &Home, run: Option<RunId>) -> Vec<serde_json::Value> {
    inner
        .ws
        .list_work_items(home)
        .unwrap_or_default()
        .iter()
        .filter(|w| run.is_none() || w.run == run)
        .map(|w| {
            json!({
                "id": w.id.to_string(),
                "step": w.step,
                "instructions": w.instructions,
                "state": w.state,
                "harness_candidates": w.harness_candidates,
                "agent": w.agent,
            })
        })
        .collect()
}

/// The last fifty entries of a home's journal, oldest first.
fn journal_tail(inner: &Inner, home: &Home) -> Vec<serde_json::Value> {
    inner
        .ws
        .journal(home)
        .unwrap_or_default()
        .iter()
        .rev()
        .take(50)
        .rev()
        .map(|e| json!({"at": e.at, "author": e.author.to_string(), "payload": e.payload}))
        .collect()
}

/// What a home has spent, for an orientation — a read that fails leaves the
/// field null rather than the call.
fn spent_view(inner: &Inner, home: &Home) -> Option<bisa_core::BudgetSpent> {
    inner
        .ws
        .spent(home)
        .inspect_err(|e| tracing::debug!(target: "bisa_engine", %home, "spend not read for the orientation: {e}"))
        .ok()
}

fn get_goal(
    inner: &Arc<Inner>,
    work_item: Option<WorkItemId>,
    goal: Option<&str>,
) -> serde_json::Value {
    let (home, _) = match resolve_scope(inner, work_item, goal) {
        Ok(x) => x,
        Err(e) => return e,
    };
    // A run of the workspace has no goal to read: the refusal names the op
    // that orients its sessions, so the agent's next call is the right one.
    let Some(goal_id) = home.goal() else {
        return err(
            "this work item belongs to a run of the workspace, which has no goal — orient with \
             get_run",
        );
    };
    let Ok(g) = inner.ws.get_goal(goal_id) else {
        return err("unknown goal");
    };
    let run = inner.ws.get_current_run(goal_id).ok().flatten();
    let items = work_items_view(inner, &home, None);
    let run_view = run.as_ref().map(run_view);
    let journal = journal_tail(inner, &home);
    // Every project this goal can see — own, linked, and its ancestors' —
    // with the absolute path of each. This is how a session finds out that
    // somewhere to put files already exists; without it an agent holding
    // `create_project` makes a second folder for the same job every time it
    // is asked. `owner_goal` rather than the desktop's Own/Linked/Inherited
    // label (`bisa-node`'s `visible_to`): the word is for a reader
    // scanning a panel, and what a session needs is the path and whose it is.
    let projects: Vec<serde_json::Value> = inner
        .ws
        .projects_for(goal_id)
        .unwrap_or_default()
        .iter()
        .map(|p| {
            json!({
                "id": p.id.to_string(),
                "slug": p.slug,
                "name": p.name,
                "path": inner.ws.project_root_path(p).display().to_string(),
            })
        })
        .collect();
    // This goal's notes, so a session can find the scratchpad somebody kept
    // while thinking about the work. Titles and ids only — a body belongs in
    // `note_read`, and putting several of them in an orientation call would
    // spend the context window before the session had decided it wanted one.
    let notes: Vec<serde_json::Value> = inner
        .ws
        .list_notes(bisa_store::OwnerFilter::Scope(
            bisa_core::OwnerScope::Goal { id: goal_id },
        ))
        .unwrap_or_default()
        .iter()
        .map(|n| json!({"id": n.id.to_string(), "title": n.title, "updated_at": n.updated_at}))
        .collect();
    let spent = spent_view(inner, &home);
    // The documents the person gave the goal as context, each by its
    // absolute path — a session reads them the way it reads any file.
    let documents = crate::documents::orientation(inner, goal_id);
    json!({
        "ok": true,
        "goal": {
            "id": g.id.to_string(),
            "statement": g.statement,
            "title": g.title,
            "status": g.status(run.as_ref()).as_str(),
            "workflow": g.workflow,
            "origin": g.origin,
            "mode": g.mode,
            "budget": g.budget,
            "spent": spent,
        },
        "run": run_view,
        "work_items": items,
        "projects": projects,
        "documents": documents,
        "notes": notes,
        "journal_tail": journal,
    })
}

/// `get_run`: the run a session works in — its work item's, else the one
/// `run` names — read the way `get_goal` reads a goal: the steps and what
/// each produced, the run's own work items, its home's journal tail, the
/// budget it spends against and what it has spent. Any run answers: a
/// goal's run names its goal (`goal`, for `get_goal`); a run of the
/// workspace carries its own ceiling and has none.
fn get_run(
    inner: &Arc<Inner>,
    work_item: Option<WorkItemId>,
    run: Option<&str>,
) -> serde_json::Value {
    let run_id = match (work_item, run) {
        (Some(wi), _) => match executor::find_work_item(inner, wi) {
            Some(spec) => match spec.run {
                Some(run) => run,
                None => return err("this work item belongs to no run"),
            },
            None => return err("unknown work item"),
        },
        (None, Some(raw)) => match RunId::from_str(raw) {
            Ok(id) => id,
            Err(_) => return err("run is not a ULID"),
        },
        (None, None) => return err("request needs `work_item` or `run`"),
    };
    let Ok(r) = inner.ws.get_run(run_id) else {
        return err("unknown run");
    };
    let home = r.home();
    // A goal files every run's items together; only this run's are its own.
    let items = work_items_view(inner, &home, Some(r.id));
    let budget = match &r.scope {
        bisa_core::RunScope::Workspace { budget } => Some(budget.clone()),
        bisa_core::RunScope::Goal { goal } => inner.ws.get_goal(*goal).ok().map(|g| g.budget),
    };
    json!({
        "ok": true,
        "run": run_view(&r),
        "home": home.to_string(),
        "goal": r.scope.goal().map(|g| g.to_string()),
        "budget": budget,
        "spent": spent_view(inner, &home),
        "work_items": items,
        "journal_tail": journal_tail(inner, &home),
    })
}

fn revise_statement(
    inner: &Arc<Inner>,
    goal: &str,
    statement: String,
    why: Option<String>,
) -> serde_json::Value {
    let goal_id = match resolve_goal_scope(inner, goal) {
        Ok(x) => x,
        Err(e) => return e,
    };
    let Ok(mut g) = inner.ws.get_goal(goal_id) else {
        return err("unknown goal");
    };
    if g.is_closed() {
        return err("the goal is closed; its statement no longer changes");
    }
    let live = inner
        .ws
        .get_current_run(goal_id)
        .ok()
        .flatten()
        .is_some_and(|r| !r.is_finished());
    if live {
        return err(
            "a run is live — the statement can no longer change; use ask_human to escalate",
        );
    }
    let old = g.statement.clone();
    g.statement = statement.clone();
    if inner.ws.update_goal(g).is_err() {
        return err("failed to update the goal");
    }
    let owner = inner.ws.owner_keys().clone();
    let note = match why {
        Some(why) => format!("statement revised: {why}\nwas: {old}"),
        None => format!("statement revised\nwas: {old}"),
    };
    warn_on_err(
        inner.ws.append_journal(
            &Home::Goal { goal: goal_id },
            JournalPayload::Note { text: note },
            &owner,
            None,
        ),
        "journaling a statement revision",
    );
    json!({"ok": true})
}

/// The Workflow Agent's proposal: validated, recorded, then gated, adopted
/// or drafted by the goal's mode (`gate` is null when none opened). Problems
/// come back by step and kind; nothing is written when there are any.
fn propose_workflow(
    inner: &Arc<Inner>,
    goal: &str,
    agent: &str,
    draft: NewWorkflow,
) -> serde_json::Value {
    let goal_id = match resolve_goal_scope(inner, goal) {
        Ok(x) => x,
        Err(e) => return e,
    };
    if let Some(refusal) = unstaffed_refusal(inner, Some(goal_id), &draft) {
        return refusal;
    }
    match ops::propose_workflow(inner, goal_id, draft, Some(agent)) {
        Ok((wf, gate)) => json!({
            "ok": true,
            "workflow": wf.id.to_string(),
            "revision": wf.revision,
            "gate": gate,
        }),
        Err(e) => problems_or_err(e),
    }
}

/// The Workflow Agent's amendment to a live run: checked, then held and
/// gated — or applied at once on an auto goal (`gate` null).
fn amend_workflow(
    inner: &Arc<Inner>,
    goal: &str,
    agent: &str,
    draft: NewWorkflow,
) -> serde_json::Value {
    let goal_id = match resolve_goal_scope(inner, goal) {
        Ok(x) => x,
        Err(e) => return e,
    };
    if let Some(refusal) = unstaffed_refusal(inner, Some(goal_id), &draft) {
        return refusal;
    }
    match ops::propose_amend(inner, goal_id, draft, Some(agent)) {
        Ok(gate) => json!({"ok": true, "gate": gate}),
        Err(e) => problems_or_err(e),
    }
}

/// A refusal, with the validation problems spelled out when that is what it
/// was: a session fixes `{step, kind, message}` faster than it parses prose.
fn problems_or_err(e: crate::EngineError) -> serde_json::Value {
    match e {
        crate::EngineError::Store(StoreError::WorkflowInvalid(problems)) => json!({
            "ok": false,
            "errors": problems.iter().map(|p| p.text.to_string()).collect::<Vec<_>>(),
            "problems": problems,
        }),
        other => err(other.to_string()),
    }
}

/// `spawn_sub_goal`: a session hands part of the work on as a goal of its
/// own. A goal's session (or a worker of a goal's run) makes a sub-goal that
/// refines its goal and moves as it does; a worker of a run of the workspace
/// has no goal to refine, so its child is a goal of its own, born of the run
/// and its step (`GoalOrigin::Run`), in the workspace's default mode — the
/// same rule a `spawn` step follows (`effects::spawn_goal`).
fn spawn_sub_goal(
    inner: &Arc<Inner>,
    work_item: Option<WorkItemId>,
    goal: Option<&str>,
    statement: String,
    title: Option<String>,
) -> serde_json::Value {
    let (home, parent_spec) = match resolve_scope(inner, work_item, goal) {
        Ok(x) => x,
        Err(e) => return e,
    };
    // Spawn policy (wired preflight): a worker's spec bounds it; the guided driver
    // (goal scope) is trusted with bounded depth by construction.
    if let Some(spec) = &parent_spec {
        if spec.depth_budget == 0 {
            return err("spawn depth exhausted for this work item");
        }
        if !spec.spawn_allowlist.iter().any(|a| a == "*") {
            return err("this work item's spawn allowlist does not permit sub-goals");
        }
    }
    let (mode, origin) = match home {
        Home::Goal { goal: parent } => (
            inner
                .ws
                .get_goal(parent)
                .map(|g| g.mode)
                .unwrap_or_default(),
            GoalOrigin::Spawned { parent },
        ),
        Home::Run { run } => {
            let Some(step) = parent_spec.as_ref().and_then(|s| s.step.clone()) else {
                return err("this work item names no step of its run to spawn from");
            };
            (ops::default_goal_mode(inner), GoalOrigin::Run { run, step })
        }
    };
    let child = match ops::submit(
        inner,
        ops::SubmitRequest {
            statement,
            title,
            mode,
            origin,
            ..ops::SubmitRequest::captured("")
        },
    ) {
        Ok(c) => c,
        Err(e) => return err(e.to_string()),
    };
    let owner = inner.ws.owner_keys().clone();
    warn_on_err(
        inner.ws.append_journal(
            &home,
            JournalPayload::Note {
                text: format!("sub-goal spawned: {}", child.id),
            },
            &owner,
            None,
        ),
        "journaling a spawned sub-goal",
    );
    json!({"ok": true, "child": child.id.to_string()})
}

async fn result_submit(
    inner: &Arc<Inner>,
    work_item: WorkItemId,
    output: serde_json::Value,
) -> serde_json::Value {
    let Some(spec) = executor::find_work_item(inner, work_item) else {
        return err("unknown work item");
    };
    if matches!(spec.state, WorkItemState::Accepted) {
        return err("work item already accepted");
    }
    if !matches!(spec.state, WorkItemState::InProgress { .. }) {
        return err(format!(
            "work item is {}; only an item in progress can yield a result",
            spec.state.as_str()
        ));
    }

    // The bound lives on the item: a restart neither refunds nor charges it.
    let attempts = spec.result_attempts;
    if attempts >= inner.config.max_result_attempts {
        return json!({"ok": false, "errors": ["result attempts exhausted"], "attempts_left": 0});
    }

    if let Some(schema) = &spec.output_schema {
        let errors = effects::schema_errors(schema, &output);
        if !errors.is_empty() {
            let used = attempts + 1;
            let left = inner.config.max_result_attempts.saturating_sub(used);
            if left == 0 {
                // The last refusal fails the step now, with the schema's own
                // words — the executor's settle path, called early. The
                // reservation goes with it, so the live session aborts.
                let reason = format!(
                    "the result did not conform to the step's output schema after {used} attempts: {}",
                    errors.join("; ")
                );
                let mut spec = spec;
                spec.state = executor::block_item(inner, &spec, reason.clone());
                effects::item_settled(inner, &spec, Err(reason));
                // The mark is signalled, not only dropped: the driver selects
                // on it and aborts the process now, not on the harness's next
                // word (a quiet one has none).
                executor::stop_item(inner, work_item);
                inner.inflight.remove(&work_item);
                return json!({"ok": false, "errors": errors, "attempts_left": 0, "failed": true});
            }
            let mut spec = spec;
            spec.result_attempts = used;
            warn_on_err(inner.ws.put_work_item(&spec), "recording a result attempt");
            return json!({"ok": false, "errors": errors, "attempts_left": left});
        }
    }

    let (signer, attestation) = ops::signer_for(&inner.ws, spec.agent.as_deref());
    let event_id = match inner.ws.append_journal(
        &spec.home,
        JournalPayload::Result {
            work_item,
            output: output.clone(),
            artifacts: vec![],
        },
        &signer,
        attestation,
    ) {
        Ok(id) => id,
        Err(e) => return err(e.to_string()),
    };

    let spec =
        match inner
            .ws
            .transition_work_item(&spec.home, work_item, &WorkItemTransition::Submit)
        {
            Ok(s) => s,
            Err(e) => return err(e.to_string()),
        };

    // The step hears of it now, synchronously: `Review` is written before the
    // ack, and the step is done with the item before the session's turn ends.
    effects::item_settled(inner, &spec, Ok(output));

    json!({"ok": true, "result_event": event_id})
}

#[cfg(test)]
mod provenance_tests {
    use super::project_origin_for;
    use bisa_core::{
        GoalId, Home, ProjectOrigin, RunId, RunScope, StepId, WorkItemSpec, WorkflowId,
    };

    fn goal() -> GoalId {
        GoalId::from_ulid(ulid::Ulid::from_parts(8, 1))
    }

    /// A work item of the goal's run.
    fn spec(run: Option<RunId>, step: Option<StepId>) -> WorkItemSpec {
        homed(Home::Goal { goal: goal() }, run, step)
    }

    fn homed(home: Home, run: Option<RunId>, step: Option<StepId>) -> WorkItemSpec {
        WorkItemSpec {
            id: bisa_core::WorkItemId::from_ulid(ulid::Ulid::from_parts(8, 9)),
            home,
            run,
            step,
            instructions: "do the thing".into(),
            state: bisa_core::workitem::WorkItemState::Open,
            project: None,
            harness_candidates: vec![],
            model: None,
            effort: None,
            output_schema: None,
            budget: Default::default(),
            assignees: vec![],
            tier_ceiling: bisa_core::ToolTier::Exec,
            agent: None,
            spawn_allowlist: vec![],
            depth_budget: 1,
            result_attempts: 0,
            interruptions: 0,
        }
    }

    /// A frozen run of a workflow with the given origin, for the table below.
    fn frozen_run(
        run: RunId,
        scope: RunScope,
        wf: WorkflowId,
        origin: bisa_core::WorkflowOrigin,
    ) -> bisa_core::WorkflowRun {
        let workflow = bisa_core::Workflow {
            id: wf,
            name: "one step".into(),
            description: String::new(),
            inputs: vec![],
            steps: vec![],
            origin,
            author: bisa_core::PrincipalId::new("ab".repeat(32)).unwrap(),
            tags: Default::default(),
            revision: 1,
            archived: None,
            decision_making: false,
            created_at: 0,
        };
        bisa_core::WorkflowRun::new(
            run,
            scope,
            workflow,
            Default::default(),
            bisa_core::RunEntry::by_hand(),
            0,
        )
    }

    /// One row per way a session can hold the op — the table the plan states,
    /// with split: a step of the goal's own design is the goal's,
    /// a step of a library workflow is the workflow's.
    #[test]
    fn a_projects_origin_follows_the_session() {
        use bisa_core::{StepRef, WorkflowOrigin};
        let goal = goal();
        let of_goal = RunScope::Goal { goal };
        let run = RunId::from_ulid(ulid::Ulid::from_parts(8, 2));
        let step = StepId::new("build").unwrap();
        let wf = WorkflowId::from_ulid(ulid::Ulid::from_parts(8, 3));
        let by_step = StepRef {
            run,
            step: step.clone(),
            workflow: wf,
        };

        // An agent step's session on a library workflow: born of the step.
        let library = frozen_run(run, of_goal.clone(), wf, WorkflowOrigin::Workspace);
        assert_eq!(
            project_origin_for(
                Some(&spec(Some(run), Some(step.clone()))),
                None,
                Some(&library)
            ),
            ProjectOrigin::Step {
                goal: Some(goal),
                step: by_step.clone()
            }
        );
        // The same step on the goal's own design: the goal's, step named.
        let design = frozen_run(run, of_goal.clone(), wf, WorkflowOrigin::Goal { goal });
        assert_eq!(
            project_origin_for(
                Some(&spec(Some(run), Some(step.clone()))),
                None,
                Some(&design)
            ),
            ProjectOrigin::Goal {
                goal,
                step: Some(by_step.clone())
            }
        );
        // A run that is not the spec's: the goal's, nothing invented.
        let other = frozen_run(
            RunId::from_ulid(ulid::Ulid::from_parts(8, 9)),
            of_goal,
            wf,
            WorkflowOrigin::Workspace,
        );
        assert_eq!(
            project_origin_for(
                Some(&spec(Some(run), Some(step.clone()))),
                None,
                Some(&other)
            ),
            ProjectOrigin::from_goal(goal)
        );
        // A work item the run machine no longer knows: the goal's.
        assert_eq!(
            project_origin_for(Some(&spec(Some(run), Some(step.clone()))), None, None),
            ProjectOrigin::from_goal(goal)
        );
        // A step of a run of the workspace: born of the step, with no goal.
        let workspace = frozen_run(
            run,
            RunScope::Workspace {
                budget: Default::default(),
            },
            wf,
            WorkflowOrigin::Workspace,
        );
        let of_run = homed(Home::Run { run }, Some(run), Some(step.clone()));
        assert_eq!(
            project_origin_for(Some(&of_run), None, Some(&workspace)),
            ProjectOrigin::Step {
                goal: None,
                step: by_step
            }
        );
        // The same item once the run machine forgets it: the workspace's —
        // there is no goal to fall back on.
        assert_eq!(
            project_origin_for(Some(&of_run), None, None),
            ProjectOrigin::Workspace
        );
        // A goal-scoped session with no work item: the goal's.
        assert_eq!(
            project_origin_for(None, Some(goal), None),
            ProjectOrigin::from_goal(goal)
        );
        // A channel or a DM: the workspace's.
        assert_eq!(
            project_origin_for(None, None, None),
            ProjectOrigin::Workspace
        );
    }
}
