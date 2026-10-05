//! Request/response DTOs — the wire contract, and the input to the
//! `api-schema` bin that generates `desktop/src/types.gen.ts`. Every struct
//! here derives `JsonSchema`; keep responses typed (no ad-hoc `json!`) for
//! anything the desktop renders structurally.

use bisa_core::event::JournalPayload;
use bisa_core::tags::TagEntity;
use bisa_core::{
    ActivityConcept, ActivitySourceKind, AddonManifest, AddonPermission, AddonProblem, Answer,
    AskKind, AttachmentRef, ConversationOrigin, Gate, GoalStatus, InputDef, Problem, RunId,
    RunOutcome, RunStatus, Step, StepId, Workflow, WorkflowId,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Goals
// ---------------------------------------------------------------------------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewGoalBody {
    pub statement: String,
    #[serde(default)]
    pub title: Option<String>,
    /// The workflow the goal will run: a workflow id, or the catalog slug of
    /// an installed template. Absent on an auto or guided goal means the
    /// Workflow Agent designs one; on a manual goal, that the person will.
    #[serde(default)]
    pub workflow: Option<String>,
    /// Inputs for the run. **Present** starts the run at once (with
    /// `workflow`); absent records the goal and waits for `POST
    /// /goals/{id}/run`.
    #[serde(default)]
    pub inputs: Option<BTreeMap<String, serde_json::Value>>,
    /// How the goal moves — `auto` (the Workflow Agent designs and the
    /// platform adopts, starts and repairs alone), `guided` (it proposes,
    /// you adopt) or `manual` (you design on the Workflow tab). Absent takes
    /// the workspace's `goals.default_mode`.
    #[serde(default)]
    pub mode: Option<bisa_core::GoalMode>,
    /// Optional assignees, in `Assignee`'s wire form (`agent:<id>` /
    /// `human:<hex>` / `team:<id>`), to carry the goal from the start.
    #[serde(default)]
    pub assignees: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Files given as the goal's initial context: the descriptors `POST
    /// /attachments` answered, materialised under the goal's `documents/`
    /// before anything runs. A hash this node does not hold refuses the
    /// capture.
    #[serde(default)]
    pub documents: Vec<AttachmentRef>,
}

/// `POST /goals/{id}/documents`: more documents for a goal that exists.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GoalDocumentsBody {
    pub documents: Vec<AttachmentRef>,
}

/// One of a goal's documents as the goal page lists it: the descriptor as
/// given, the name it is kept under, its absolute path, and whether the
/// bytes — and so the file — are on this node.
#[derive(Serialize, JsonSchema)]
pub struct GoalDocumentRow {
    pub file: AttachmentRef,
    pub name: String,
    pub path: String,
    pub present: bool,
}

impl From<bisa_store::GoalDocument> for GoalDocumentRow {
    fn from(d: bisa_store::GoalDocument) -> Self {
        Self {
            file: d.file,
            name: d.name,
            path: d.path.display().to_string(),
            present: d.present,
        }
    }
}

/// `POST /workflows`, the body of `POST /goals/{id}/amend`, and
/// `POST /workflows/validate`: a definition without its record fields. A key
/// the definition does not have is a 400, never dropped. Also what a catalog
/// row carries for a workflow template (`CatalogEntryDto::workflow`), so the
/// same shape is read back as it is written.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewWorkflowBody {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub inputs: Vec<InputDef>,
    pub steps: Vec<Step>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Whether the Decision-Making Agent stands in at the decision points a
    /// run of this workflow reaches, whatever the workspace's switch says.
    #[serde(default)]
    pub decision_making: bool,
}

/// `PUT /goals/{id}/workflow` — the workflow the next run uses. `null`
/// clears it.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetWorkflowBody {
    /// A workflow id, or the catalog slug of an installed template.
    #[serde(default)]
    pub workflow: Option<String>,
    /// A whole definition instead: recorded as **this goal's design**
    /// (`WorkflowOrigin::Goal`), kept as a draft with its problems, and
    /// pointed at. Exclusive with `workflow`.
    #[serde(default)]
    pub definition: Option<NewWorkflowBody>,
    /// With `definition`, the revision of the goal's design the caller
    /// edited. Required once the goal has a design; a design that moved
    /// since is a 409.
    #[serde(default)]
    pub revision: Option<u64>,
}

/// `PUT /workflows/{wfid}` — the edited definition and the revision it was
/// edited from. The stored copy must still be at `revision`; otherwise 409,
/// and nothing is written. The definition's fields are spelled here rather
/// than flattened in, because a flattened struct cannot refuse a key it does
/// not know — and a save must refuse one exactly as a create does.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PutWorkflowBody {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub inputs: Vec<InputDef>,
    pub steps: Vec<Step>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub decision_making: bool,
    pub revision: u64,
}

impl PutWorkflowBody {
    /// The definition and the revision it was edited from.
    pub fn into_parts(self) -> (NewWorkflowBody, u64) {
        (
            NewWorkflowBody {
                name: self.name,
                description: self.description,
                inputs: self.inputs,
                steps: self.steps,
                tags: self.tags,
                decision_making: self.decision_making,
            },
            self.revision,
        )
    }
}

/// Every error body: the message; for a definition that cannot start, its
/// problems by step and kind (omitted when there are none); and, for the
/// refusals a client renders differently from one another, a `code` naming
/// which one it is (omitted otherwise). A status alone cannot tell them
/// apart — every refusal below is a 409 — and a client that guessed from the
/// status told people their project publishes manually when their branch had
/// simply never been pushed.
#[derive(Serialize, JsonSchema, Clone)]
pub struct ErrorBody {
    /// The sentence, in the request's language (`Accept-Language`; English when none).
    pub error: String,
    /// The sentence as data — the catalog message and its arguments — for a
    /// client that renders in its own language or switches on the id.
    pub text: bisa_core::Text,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<Problem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<ErrorCode>,
    /// The typed facts behind a `code`, when it has any: a conflict's
    /// `paths` and `in_progress`, a refused fast-forward's `ahead`/`behind`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<serde_json::Value>,
}

/// `GET /notes/git` and `GET /drawings/git` — where a folder repository
/// stands (ide/04 §The notes repository; 19 — Drawings): the desktop's strip
/// reads *3 changes · ↑1 · committed 2 h ago* from it.
#[derive(Serialize, JsonSchema)]
pub struct FolderRepo {
    /// Files changed since the last commit — a commit takes them all.
    pub changed: u32,
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// `origin`'s URL, when set.
    pub remote: Option<String>,
    pub identity: GitIdentityView,
    pub last_commit: Option<RepoCommitRow>,
    /// A merge or rebase half-done.
    pub in_progress: Option<String>,
}

/// A folder repository's last commit.
#[derive(Serialize, JsonSchema)]
pub struct RepoCommitRow {
    pub short: String,
    pub subject: String,
    /// Author time, unix seconds.
    pub at: u64,
}

/// `POST /notes/git/commit`, `POST /drawings/git/commit`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RepoCommitBody {
    pub message: String,
}

/// `PUT /notes/git/remote`, `PUT /drawings/git/remote`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RepoRemoteBody {
    pub url: String,
}

/// `PUT /notes/git/identity`, `PUT /drawings/git/identity`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RepoIdentityBody {
    pub name: String,
    pub email: String,
}

/// `POST /notes/git/pull` — what a consented fast-forward answered: the
/// safety ref written first, the pull's own outcome, and where the
/// repository stands after it.
#[derive(Serialize, JsonSchema)]
pub struct PullOutcomeBody {
    /// The safety ref written first, as the IDE's pull answers it (`GitRecoveryRef`).
    pub recovery: serde_json::Value,
    /// The pull's own outcome — fast-forwarded, already up to date, or refused with why.
    pub pull: serde_json::Value,
    pub status: FolderRepo,
}

/// `GET /harnesses/{id}/usage` — what a harness's account has left, as the
/// harness's own source reports it, or why there is no report. Never a
/// credential: percentages, labels, reset times, a plan's word.
#[derive(Serialize, JsonSchema)]
pub struct HarnessUsage {
    pub harness: String,
    pub usage: bisa_harness::UsageState,
}

/// `GET /logs` — the diagnostic log on this machine: the folder, every
/// process family with its files newest first, the crash reports, the bytes
/// they hold together and the newest report in a line. What the node, the
/// CLI, the MCP servers and the desktop wrote about themselves; nothing here
/// is a line of it, and nothing is sent anywhere.
#[derive(Serialize, JsonSchema)]
pub struct LogsView {
    /// The root — the workspace's `logs/`.
    pub dir: String,
    /// One per process family, in the crate's order; a family with no folder
    /// yet has no files.
    pub families: Vec<LogFamilyView>,
    /// The crash reports' folder — `crashes/` under the root.
    pub crashes_dir: String,
    /// The crash reports in it, newest first.
    pub crashes: Vec<LogFileView>,
    /// The size of every file and every report together.
    pub bytes: u64,
    /// The newest crash report, in a line — what a panel shows first.
    pub latest_crash: Option<CrashSummaryView>,
}

/// One process family: whose it is, its folder and its files.
#[derive(Serialize, JsonSchema)]
pub struct LogFamilyView {
    /// `node` · `cli` · `mcp` · `desktop`.
    pub process: String,
    pub dir: String,
    pub files: Vec<LogFileView>,
}

/// One file: `<process>.<period>.jsonl` or `<process>.<stamp>.<pid>.json`,
/// its size and when it was last written (unix seconds).
#[derive(Serialize, JsonSchema)]
pub struct LogFileView {
    pub name: String,
    pub bytes: u64,
    pub modified_at: u64,
}

/// A crash report in a line: which file, whose, what kind, when and the
/// words.
#[derive(Serialize, JsonSchema)]
pub struct CrashSummaryView {
    pub name: String,
    pub process: String,
    /// `panic` · `abrupt_end` · `child_exit`.
    pub kind: String,
    /// RFC 3339, UTC.
    pub at: String,
    pub message: String,
}

/// `GET /logs/crashes/{name}` — one crash report whole: the process, the
/// build, the pid, the moment, what died and how, the location and the
/// thread of a panic and its backtrace, the child's exit and last stderr
/// lines when a supervised process died, and the flight recorder's entries
/// at the moment — the log's own lines, oldest first.
#[derive(Serialize, JsonSchema)]
pub struct CrashReportView {
    pub process: String,
    pub version: String,
    pub pid: u32,
    pub at: String,
    pub kind: String,
    pub message: String,
    pub location: Option<String>,
    pub thread: Option<String>,
    pub backtrace: Option<String>,
    pub child: Option<ChildExitView>,
    pub recent: Vec<RecordedView>,
}

/// How a supervised child ended and what it last said.
#[derive(Serialize, JsonSchema)]
pub struct ChildExitView {
    pub process: String,
    pub pid: u32,
    pub code: Option<i32>,
    pub signal: Option<i32>,
    /// Its last stderr lines, oldest first.
    pub stderr: Vec<String>,
}

/// One recorded line: when, how loud, from where, the words and the fields
/// as their words.
#[derive(Serialize, JsonSchema)]
pub struct RecordedView {
    pub at: String,
    pub level: String,
    pub target: String,
    pub message: String,
    pub fields: Vec<(String, String)>,
}

impl From<bisa_engine::logging::CrashReport> for CrashReportView {
    fn from(r: bisa_engine::logging::CrashReport) -> Self {
        Self {
            process: r.process.prefix().to_string(),
            version: r.version,
            pid: r.pid,
            at: r.at,
            kind: r.kind.as_str().to_string(),
            message: r.message,
            location: r.location,
            thread: r.thread,
            backtrace: r.backtrace,
            child: r.child.map(|c| ChildExitView {
                process: c.process.prefix().to_string(),
                pid: c.pid,
                code: c.code,
                signal: c.signal,
                stderr: c.stderr,
            }),
            recent: r
                .recent
                .into_iter()
                .map(|e| RecordedView {
                    at: e.at,
                    level: e.level,
                    target: e.target,
                    message: e.message,
                    fields: e.fields,
                })
                .collect(),
        }
    }
}

/// The refusals a client tells apart. Each names one producer that exists;
/// a message is still the words, the code is what a client switches on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The project's publishing policy is `manual`: nothing publishes from here.
    PublishManual,
    /// The project is gated and the workstream belongs to no goal, so there
    /// is nobody to ask.
    PublishNoGoal,
    /// The Publish gate was declined; nothing left the machine.
    PublishDeclined,
    /// The branch has no commits beyond its base; there is nothing to push or
    /// open a pull request for.
    NothingToPublish,
    /// A commit was asked of a clean tree.
    NothingToCommit,
    /// The workstream's state does not admit the transition (`cannot … a
    /// workstream that is …`), the primary was asked to close or finish, or
    /// the workstream is already closed.
    WorkstreamState,
    /// A workstream was asked for from a pull request that is not open;
    /// `detail.number` and `detail.state` say which and what it is.
    PullRequestState,
    /// A merge, rebase, cherry-pick, revert or pull stopped on conflicting
    /// content; `detail.paths` names the files and `detail.in_progress` the
    /// operation to resolve or abort.
    Conflict,
    /// A fast-forward-only pull met local commits; `detail.ahead` and
    /// `detail.behind` say how far apart the branch and its upstream are.
    NotFastForward,
    /// An operation is already half-done in this checkout (`detail.in_progress`);
    /// resolve or abort it before starting another.
    InProgress,
    /// Another git process — an editor, an agent's own `git` in a terminal —
    /// holds the repository's lock; nothing was written. Try again once it
    /// has finished. (The platform's own writers queue per checkout and never
    /// meet each other here.)
    RepositoryBusy,
    /// A workstream script stood in the way (ide/07 §Workstream scripts): the
    /// pre-create script refused the checkout, or the clean script kept it —
    /// non-zero, timed out, or not approved on this machine. `detail.phase`
    /// names which and `detail.output` is the tail of what it printed.
    ScriptFailed,
    /// An approval or a change request on a pull request the connected account
    /// itself opened; the code host takes only a comment from its author.
    /// `detail.author` names the account.
    OwnPullRequest,
    /// A stash was asked of a tree with nothing it would save — clean, only
    /// untracked files without *include untracked*, paths with no change, or
    /// no commit yet. Nothing was written, no recovery ref included.
    NothingToStash,
    /// `stash@{index}` no longer holds the commit the caller saw there: the
    /// list moved (a push or a drop here or in another worktree). `detail`
    /// carries `index`, `commit` and `now` — what sits there today. Reload.
    StashMoved,
}

/// One goal on the `GET /goals` list — and the same row `GET /teams/{id}`
/// and `GET /goals/{id}` carry, so every screen reads one shape.
#[derive(Serialize, JsonSchema)]
pub struct GoalRow {
    pub id: String,
    pub title: Option<String>,
    /// A card needs something to say for an untitled goal.
    pub statement: String,
    pub status: bisa_core::GoalStatus,
    pub workflow: Option<bisa_core::WorkflowId>,
    pub run: Option<bisa_core::RunId>,
    /// The goal names a run this node cannot read — the file is gone, or
    /// another shape of the code wrote it. `status`, `strip` and `holder`
    /// are then drawn as for a goal with no run; this says why.
    pub run_unreadable: bool,
    pub origin: bisa_core::GoalOrigin,
    pub mode: bisa_core::GoalMode,
    pub assignees: Vec<bisa_core::Assignee>,
    pub tags: bisa_core::Tags,
    /// The run drawn as a strip: every step in definition order with its
    /// state. A draft with a chosen or proposed workflow ghosts the
    /// definition with every step pending.
    pub strip: RunStrip,
    /// Who the goal waits on — the word the Goals screen, the CLI and the
    /// inbox share (`Goal::holder`).
    pub holder: bisa_core::Holder,
    /// The current run's status, when the goal has one — what a list draws
    /// a card's run verbs from (stop, restart, a new run) without a fetch
    /// per goal.
    pub run_status: Option<bisa_core::RunStatus>,
    /// How many runs wait behind the live one.
    pub queued: usize,
    /// The newest step movement, else the run's start, else the goal's birth.
    pub last_activity_at: u64,
    /// Put away — a mark on a closed goal, never a status; a list shows it only when asked.
    pub archived: Option<bisa_core::Archived>,
    /// The goal's standing while its workflow begins on events: armed, or
    /// paused after a failed run or a spent budget; `null` for a goal whose
    /// work begins by hand.
    pub listening: Option<bisa_core::Listening>,
}

/// `GET /goals/{id}/retirement` and `GET /workflows/{wfid}/retirement`: what
/// retiring would touch, for the person to weigh — the goal's run that is
/// going, a workflow's runs of the workspace that are going (retired with
/// it) and how many it has in all (kept under an archived workflow, gone
/// with a deleted one), the agent sessions aborted and the harnesses
/// terminated, the refusal a deletion would meet, the designs that go or
/// stay, what still uses a workflow, the projects born of the thing with
/// what each holds (their workstreams by id, so the desktop can count its
/// own terminals there), the projects merely attached.
#[derive(Serialize, JsonSchema)]
pub struct RetirementDto {
    pub agents: usize,
    pub harnesses: usize,
    pub run: Option<bisa_engine::retire::RunFacts>,
    pub runs: Vec<bisa_engine::retire::RunFacts>,
    pub history: usize,
    pub refusal: Option<String>,
    pub designs: usize,
    pub used_by: Vec<ReferenceDto>,
    pub projects_born: Vec<bisa_engine::retire::ProjectFacts>,
    pub projects_attached: Vec<bisa_engine::retire::ProjectFacts>,
}

impl From<bisa_engine::retire::Retirement> for RetirementDto {
    fn from(r: bisa_engine::retire::Retirement) -> Self {
        Self {
            agents: r.agents,
            harnesses: r.harnesses,
            run: r.run,
            runs: r.runs,
            history: r.history,
            refusal: r.refusal,
            designs: r.designs,
            used_by: r.used_by.into_iter().map(ReferenceDto::from).collect(),
            projects_born: r.projects_born,
            projects_attached: r.projects_attached,
        }
    }
}

/// `POST /projects/{pid}/git/init`: the same `[key, value]` local git config a
/// creation request can name, applied to the new repository; empty for none.
#[derive(Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InitRepositoryBody {
    #[serde(default)]
    pub git_config: Vec<(String, String)>,
}

/// What `POST /projects/{pid}/git/init` answers: the project as it now reads
/// (a row) and the committer policy's one sentence, when it had something to
/// say — `null` when the repository already had an identity of its own.
#[derive(Serialize, JsonSchema)]
pub struct InitRepositoryReply {
    pub project: ProjectRow,
    pub committer: Option<String>,
}

/// `POST /goals/{id}/archive`, `/workflows/{wfid}/archive`, `/projects/{pid}/archive`: put away, or taken back out.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArchiveBody {
    pub archived: bool,
}

/// A run flattened for a list row: the steps in definition order, the live
/// ones, and how far it got.
#[derive(Serialize, JsonSchema)]
pub struct RunStrip {
    pub run: Option<bisa_core::RunId>,
    pub workflow_name: Option<String>,
    pub steps: Vec<StripStep>,
    /// The live (`running` | `waiting`) step ids.
    pub current: Vec<bisa_core::StepId>,
    /// Steps that reached an end state, over the total. The reader divides.
    pub reached: u32,
    pub total: u32,
    pub outcome: Option<bisa_core::RunOutcome>,
    pub cancelled: bool,
    /// Where a failed run failed: the newest step in `failed`, with its
    /// error — so a row and a banner say why without opening the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<RunFailure>,
}

/// The step a run failed at, by id and name, and what it recorded.
#[derive(Serialize, JsonSchema)]
pub struct RunFailure {
    pub step: bisa_core::StepId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl RunFailure {
    /// The newest failed step of `run` — the one the run's outcome is about.
    pub fn of(run: &bisa_core::WorkflowRun) -> Option<Self> {
        let (id, record) = run
            .steps
            .iter()
            .filter(|(_, r)| r.state == bisa_core::StepState::Failed)
            .max_by_key(|(_, r)| r.seq)?;
        Some(RunFailure {
            step: id.clone(),
            name: run
                .workflow
                .step(id)
                .map(|s| s.name.clone())
                .unwrap_or_else(|| id.to_string()),
            error: record.error.clone(),
        })
    }
}

#[derive(Serialize, JsonSchema)]
pub struct StripStep {
    pub id: bisa_core::StepId,
    pub name: String,
    /// The step kind's wire word (`agent`, `human`, ...).
    pub kind: String,
    pub state: bisa_core::StepState,
}

impl RunStrip {
    /// Flatten the current run — or, for a draft, ghost the chosen
    /// definition with every step pending.
    pub fn from_parts(
        run: Option<&bisa_core::WorkflowRun>,
        definition: Option<&bisa_core::Workflow>,
    ) -> Self {
        use bisa_core::StepState;
        let workflow = run.map(|r| &r.workflow).or(definition);
        let Some(workflow) = workflow else {
            return RunStrip {
                run: None,
                workflow_name: None,
                steps: vec![],
                current: vec![],
                reached: 0,
                total: 0,
                outcome: None,
                cancelled: false,
                failure: None,
            };
        };
        let state_of = |id: &bisa_core::StepId| -> StepState {
            run.and_then(|r| r.steps.get(id))
                .map(|rec| rec.state.clone())
                .unwrap_or(StepState::Pending)
        };
        let steps: Vec<StripStep> = workflow
            .steps
            .iter()
            .map(|s| StripStep {
                id: s.id.clone(),
                name: s.name.clone(),
                kind: s.kind.as_str().to_string(),
                state: state_of(&s.id),
            })
            .collect();
        let current = steps
            .iter()
            .filter(|s| s.state.is_live())
            .map(|s| s.id.clone())
            .collect();
        // The core says which states are an end — a diverted step among them.
        let reached = steps.iter().filter(|s| s.state.is_terminal()).count() as u32;
        RunStrip {
            run: run.map(|r| r.id),
            workflow_name: Some(workflow.name.clone()),
            total: steps.len() as u32,
            steps,
            current,
            reached,
            outcome: run.and_then(|r| r.outcome),
            cancelled: run.is_some_and(|r| r.cancelled.is_some()),
            failure: run.and_then(RunFailure::of),
        }
    }
}

/// The newest movement on a goal: a step starting or settling, else the
/// run's start, else the goal's birth.
pub fn last_activity_at(goal: &bisa_core::Goal, run: Option<&bisa_core::WorkflowRun>) -> u64 {
    run.map(|r| {
        r.steps
            .values()
            .flat_map(|rec| [rec.started_at, rec.finished_at])
            .flatten()
            .max()
            .unwrap_or(r.started_at.unwrap_or(r.queued_at))
    })
    .unwrap_or(goal.created_at)
}

/// `POST /goals/{id}/run` — make a run with these inputs: started at once,
/// or queued behind the goal's live run — and `POST /workflows/{wfid}/runs`,
/// a run of the workspace, started at once beside any other.
#[derive(Deserialize, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StartRunBody {
    #[serde(default)]
    pub inputs: BTreeMap<String, serde_json::Value>,
    /// The start step the run begins at — an event start, for a test run.
    /// Absent, a person's run: at the manual entry.
    #[serde(default)]
    pub start: Option<StepId>,
    /// A test run's sample payload: the event its start's mapping reads, as
    /// if it had happened.
    #[serde(default)]
    pub event: Option<serde_json::Value>,
}

/// What `POST /goals/{id}/run` answers when the start armed the goal rather
/// than running it: no run — the goal listens — where the goal stands, and
/// the public hooks' secrets the start minted, shown this once.
#[derive(Serialize, JsonSchema)]
pub struct GoalArmed {
    /// The goal's status once armed.
    pub status: GoalStatus,
    /// Absent when the start minted none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<bisa_engine::HookSecret>,
}

/// `POST /runs/{rid}/steps/{step}/release` — let a held `wait` step go; a
/// payload, when given, is the step's output.
#[derive(Deserialize, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReleaseStepBody {
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
}

/// `POST /goals/{id}/stop` and `POST /runs/{rid}/stop` — stop the goal or
/// the run, with the person's word on why.
#[derive(Deserialize, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StopBody {
    #[serde(default)]
    pub rationale: Option<String>,
}

/// What a stop did: the live run it cancelled and the queued runs it
/// withdrew — `POST /goals/{id}/stop`'s answer.
#[derive(Serialize, JsonSchema)]
pub struct StopOutcome {
    pub stopped: Option<RunId>,
    pub withdrawn: Vec<RunId>,
}

impl From<bisa_engine::Stopped> for StopOutcome {
    fn from(s: bisa_engine::Stopped) -> Self {
        Self {
            stopped: s.run,
            withdrawn: s.withdrawn,
        }
    }
}

/// `POST /goals/{id}/amend` — replace the steps of the current run that have
/// not started.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AmendBody {
    pub workflow: NewWorkflowBody,
}

/// `POST /runs/{rid}/steps/{step}/answer` — a `human` step's answer.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StepAnswerBody {
    pub answer: Answer,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DecideBody {
    pub approve: bool,
    #[serde(default)]
    pub rationale: Option<String>,
    /// What the human said, for question gates. Carries the options they
    /// picked, what they typed, and whether they said "I'm not sure" — a
    /// third outcome that resolves the question without declining it.
    #[serde(default)]
    pub answer: Option<Answer>,
    /// Optional specific gate id (defaults to the home's pending gate — the
    /// goal's, or the run's).
    #[serde(default)]
    pub gate: Option<String>,
    /// The run's inputs, when the gate is an adoption (`adopt:<workflow>@…`)
    /// and approving it starts the run.
    #[serde(default)]
    pub inputs: Option<BTreeMap<String, serde_json::Value>>,
    /// Which waiting step is meant, when no `gate` is named and the run has
    /// several `approval` or `human` steps waiting. Refused, not guessed,
    /// when it is needed and absent. A held `wait` step is released only by
    /// name while an adoption or an amendment is still owed a decision: an
    /// unnamed approval decides that instead.
    #[serde(default)]
    pub step: Option<StepId>,
}

/// A row of `GET /workflows`, and the flat body of `GET /workflows/{wfid}`.
#[derive(Serialize, JsonSchema)]
pub struct WorkflowRow {
    pub workflow: Workflow,
    /// Why this definition cannot start as written. Empty when it can.
    pub problems: Vec<bisa_core::Problem>,
    /// Why it cannot run in the workspace, beyond `problems`: a step that
    /// reads `{goal.…}` has no goal to read there (`needs_goal`) — it runs
    /// on a goal only. Empty when *Run…* can start it.
    pub workspace_problems: Vec<bisa_core::Problem>,
    /// Its runs of the workspace, counted — what a card says and its verbs
    /// are drawn from without a fetch per workflow.
    pub runs: WorkflowRuns,
    /// The goals and workflows pointing at it — what a delete refuses over.
    pub used_by: Vec<ReferenceDto>,
    /// Its standing while it is On — the inputs its event runs bind, the
    /// per-run budget, since when, why it paused; `null` while it is Off.
    pub listening: Option<bisa_core::Listening>,
    /// One per `start` step, the manual one included: which event, in words.
    pub starts: Vec<StartSummary>,
    /// Only events begin it: it names starts and none is by hand — *Run…*
    /// is a test run.
    pub event_only: bool,
    /// What turning it On asks: the inputs its event starts read and do not
    /// map, with no default to fill them.
    pub listening_needs: Vec<String>,
}

/// One way a workflow begins, as its row says it: the start step, its
/// event's word (`StartOn::as_str`) and the step's summary.
#[derive(Serialize, JsonSchema)]
pub struct StartSummary {
    pub step: StepId,
    pub event: String,
    pub summary: bisa_core::Text,
}

impl StartSummary {
    /// Every `start` step of `wf`, in definition order. `listening` is what
    /// its host listens with, when it listens: a start whose event reads an
    /// input is then summarised as it is armed — the schedule, never the
    /// input's name.
    pub fn list(wf: &Workflow, listening: Option<&bisa_core::Listening>) -> Vec<Self> {
        wf.steps
            .iter()
            .filter_map(|step| {
                let declared = step.start_on()?;
                let armed = listening.and_then(|l| declared.resolve(&l.inputs).ok());
                Some(Self::of(step, armed.as_ref().unwrap_or(declared)))
            })
            .collect()
    }

    /// One start step, read as `on`: the event it declares, or that event
    /// resolved against what its host listens with.
    pub fn of(step: &Step, on: &bisa_core::StartOn) -> Self {
        Self {
            step: step.id.clone(),
            event: on.as_str().to_string(),
            summary: on.summary(),
        }
    }
}

/// A workflow's runs of the workspace on its row: how many are going, how
/// many it has had, and the newest.
#[derive(Serialize, JsonSchema)]
pub struct WorkflowRuns {
    pub live: usize,
    pub total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last: Option<RunSummary>,
}

impl WorkflowRuns {
    /// From the workflow's runs of the workspace, oldest first. `detail`
    /// says what there is to say about the event that began the newest
    /// (`RunSummary::of`) — the one run summarised here.
    pub fn of(
        runs: &[bisa_core::WorkflowRun],
        detail: impl Fn(&bisa_core::WorkflowRun) -> Option<String>,
    ) -> Self {
        Self {
            live: runs.iter().filter(|r| r.is_live()).count(),
            total: runs.len(),
            // A run of the workspace never queues, so the newest has no
            // place in a queue to say.
            last: runs
                .last()
                .map(|r| RunSummary::of(r, runs.len(), detail(r))),
        }
    }
}

/// `GET /sessions`: every session's presence, the engine's own shape.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SessionsResponse {
    pub sessions: Vec<bisa_engine::SessionPresence>,
}

/// `POST /sessions/terminal`: a harness a person is opening in a desktop
/// terminal — where (a file scope and its id) and which one.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenInteractiveBody {
    pub scope: String,
    pub id: String,
    pub harness: String,
}

/// What the desktop shell applies to the command it is about to run. `session`
/// is `None` when the harness cannot report — it opens as a plain terminal.
/// The session's secret is in `env` (`BISA_SESSION_SECRET`), once.
#[derive(Debug, Clone, Default, Serialize, JsonSchema)]
pub struct InteractiveOpened {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    pub env: std::collections::BTreeMap<String, String>,
    /// Names the shell removes from the command's environment — the proxy
    /// variables the `network.proxy.mode` setting says the harness must not
    /// inherit.
    pub env_remove: Vec<String>,
    pub args: Vec<String>,
    pub intercept_approval_notifications: bool,
}

/// `POST /sessions/{id}/report`: what a hook inside the session reported,
/// already in the engine's vocabulary.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportBody {
    pub events: Vec<bisa_harness::SessionEvent>,
}

/// `POST /sessions/{id}/exit`: the process behind the session ended — with
/// this status, or by this signal, or (both absent) without saying.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExitBody {
    #[serde(default)]
    pub code: Option<i32>,
    #[serde(default)]
    pub signal: Option<String>,
}

/// `POST /goals/{id}/close`: its person's word on why, or the goal that
/// takes its place.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CloseBody {
    #[serde(default)]
    pub rationale: Option<String>,
    /// The goal that replaces this one, when there is one.
    #[serde(default)]
    pub superseded_by: Option<String>,
}

/// `PUT /governance`: who may decide each gate. A gate left out keeps the
/// policy it has.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GovernanceBody {
    #[serde(default)]
    pub approval: Option<bisa_store::GatePolicy>,
    #[serde(default)]
    pub escalation: Option<bisa_store::GatePolicy>,
    #[serde(default)]
    pub publish: Option<bisa_store::GatePolicy>,
}

/// `PATCH /teams/{id}`: what is absent is kept.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TeamPatch {
    #[serde(default)]
    pub name: Option<String>,
    /// What the team is for. `null` removes it.
    #[serde(default, deserialize_with = "clearable")]
    #[schemars(with = "Option<String>")]
    pub purpose: Option<Option<String>>,
    /// The team's picture. `null` removes it.
    #[serde(default, deserialize_with = "clearable")]
    pub photo: Option<Option<bisa_core::AttachmentRef>>,
    /// The whole roster, replaced: `{"agent": id}` or `{"human": pubkey}`.
    #[serde(default)]
    pub members: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// `POST /ide/replace/{scope}/{id}`.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "IdeReplace")]
#[serde(deny_unknown_fields)]
pub struct ReplaceBody {
    pub q: String,
    #[serde(default)]
    pub regex: bool,
    #[serde(default)]
    pub case: Option<String>,
    #[serde(default)]
    pub word: bool,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    pub replacement: String,
    /// `false` previews; `true` applies to `files`, each with the hash its
    /// preview carried.
    #[serde(default)]
    pub apply: bool,
    #[serde(default)]
    pub files: Vec<ReplaceFile>,
}

/// One file a replace is applied to, as its preview read it.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "IdeReplaceFile")]
#[serde(deny_unknown_fields)]
pub struct ReplaceFile {
    pub path: String,
    pub base_hash: String,
}

/// One run — a goal's or the workspace's — in a list: enough to draw a row
/// without the frozen workflow it carries.
#[derive(Serialize, JsonSchema)]
pub struct RunSummary {
    pub id: RunId,
    /// `goal` or `workspace` (`RunScope::as_str`).
    pub scope: String,
    /// The goal whose run it is; absent for a run of the workspace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<bisa_core::GoalId>,
    /// Its place among the runs of its goal — or of its workflow in the
    /// workspace — oldest first, 1 the first: *run #3*.
    pub number: usize,
    pub status: RunStatus,
    pub workflow: WorkflowId,
    /// The workflow's name as the run froze it.
    pub workflow_name: String,
    pub revision: u64,
    /// Who started it: a person, an event one of its start events heard, or a
    /// test run — read from the run's event.
    pub started_by: StartedBy,
    /// When the run was made; the queue's order.
    pub queued_at: u64,
    /// Absent while the run is queued.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<RunOutcome>,
    /// Why a cancelled run was cancelled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<bisa_core::CancelCause>,
    /// A queued run's place in its goal's queue, 1 next.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<usize>,
}

impl RunSummary {
    /// One run, as the `number`-th of its goal's or its workflow's. `detail`
    /// is what the caller found to say about the event that began it — who
    /// sent the message, which signal, which run — when an event did.
    pub fn of(r: &bisa_core::WorkflowRun, number: usize, detail: Option<String>) -> Self {
        Self {
            id: r.id,
            scope: r.scope.as_str().to_string(),
            goal: r.scope.goal(),
            number,
            status: r.status(),
            workflow: r.workflow.id,
            workflow_name: r.workflow.name.clone(),
            revision: r.workflow.revision,
            started_by: StartedBy::of(r, detail),
            queued_at: r.queued_at,
            started_at: r.started_at,
            finished_at: r.finished_at,
            outcome: r.outcome,
            cause: r.cancelled.clone(),
            position: None,
        }
    }

    /// A goal's runs — or a workflow's runs of the workspace — given oldest
    /// first, answered newest first: each numbered by its place, each queued
    /// one by its place in the queue, each with the `detail` of the event
    /// that began it.
    pub fn list(
        runs: &[bisa_core::WorkflowRun],
        detail: impl Fn(&bisa_core::WorkflowRun) -> Option<String>,
    ) -> Vec<RunSummary> {
        let mut position = 0;
        let mut out: Vec<RunSummary> = runs
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let mut summary = RunSummary::of(r, i + 1, detail(r));
                if r.is_queued() {
                    position += 1;
                    summary.position = Some(position);
                }
                summary
            })
            .collect();
        out.reverse();
        out
    }
}

/// Who started a run, as a list row says it: a person's *Run…*, an event one
/// of the workflow's start events heard — its kind, and a detail when there
/// is one (who sent the message, which signal, which run) — or a test run.
#[derive(Serialize, JsonSchema, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "by")]
pub enum StartedBy {
    /// By hand: no event began it.
    You,
    /// An occurrence one of its start events heard.
    Event {
        event: EventKind,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    /// A test run: begun at a start as if its event had happened. `event`
    /// is that start's event word (`StartOn::as_str`).
    Test { event: String },
}

impl StartedBy {
    /// Who started `run`, read off its event and the start it began at. A
    /// run with no event began by hand; one whose event is a test's sample
    /// (`SignalSource::Test`, which only a test run carries) is a test run;
    /// any other event is the occurrence that began it, whether a listener
    /// dispatched the run or a restart copied the event.
    pub fn of(run: &bisa_core::WorkflowRun, detail: Option<String>) -> Self {
        use bisa_core::StartOn;
        let Some(event) = &run.event else {
            return StartedBy::You;
        };
        match EventKind::of(event.source) {
            Some(kind) => StartedBy::Event {
                event: kind,
                detail,
            },
            None => StartedBy::Test {
                event: run
                    .entry_step()
                    .and_then(Step::start_on)
                    .map_or(StartOn::Manual.as_str(), StartOn::as_str)
                    .to_string(),
            },
        }
    }
}

/// The kind of event that began a run: the start events' words, a person's
/// start and a test run apart.
#[derive(Serialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Schedule,
    Hook,
    Message,
    Signal,
    Project,
    Run,
    Platform,
    Connector,
    Check,
}

impl EventKind {
    /// The kind a signal's source names; none for a test run's sample.
    pub fn of(source: bisa_core::SignalSource) -> Option<Self> {
        use bisa_core::SignalSource as S;
        match source {
            S::Schedule => Some(Self::Schedule),
            S::Hook => Some(Self::Hook),
            S::Message => Some(Self::Message),
            S::Signal => Some(Self::Signal),
            S::Project => Some(Self::Project),
            S::Run => Some(Self::Run),
            S::Platform => Some(Self::Platform),
            S::Connector => Some(Self::Connector),
            S::Check => Some(Self::Check),
            S::Test => None,
        }
    }
}

/// `GET /runs/{rid}`: one run, whole — a goal's or the workspace's — with
/// its place among its siblings, who it waits on and what it owes a person.
#[derive(Serialize, JsonSchema)]
pub struct RunView {
    pub run: bisa_core::WorkflowRun,
    pub summary: RunSummary,
    /// Who the run waits on (`WorkflowRun::holder`) — the word the goal
    /// row and the Runs tab share.
    pub holder: bisa_core::Holder,
    /// What the run owes a person: its live gates, else the asks rebuilt
    /// from the run and its home's journal — decided through `home`.
    pub needs_actions: Vec<NeedsAction>,
}

/// `PUT /goals/{id}/assignees` — an empty list clears the assignment.
/// Entries use `Assignee`'s wire form: `agent:<id>`, `human:<hex>`, `team:<id>`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssigneesBody {
    #[serde(default)]
    pub assignees: Vec<String>,
}

/// The Workflow Agent's standing over a goal — drives the goal page's
/// designing card and the "the Workflow Agent is asking…" strip.
#[derive(Serialize, JsonSchema)]
pub struct GuidanceInfo {
    /// The goal's mode; the agent designs for `auto` and `guided` goals.
    pub mode: bisa_core::GoalMode,
    /// Whether this node designs at all (`EngineConfig::design_enabled`).
    pub design_enabled: bool,
    /// `design` while the Workflow Agent owes a proposal, `repair` after a
    /// failed run, otherwise the goal's status.
    pub phase: String,
    /// Where the Workflow Agent stands on that job — working, asking,
    /// proposed, stalled, failed, off — from the goal's last guidance fact,
    /// with `live` saying whether a wake is behind it right now. Absent when
    /// no fact applies (a goal that was never guided, or one already running).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design: Option<bisa_engine::guided::DesignStatus>,
    pub open_questions: Vec<NeedsAction>,
}

// ---------------------------------------------------------------------------
// Inbox / Pulse
// ---------------------------------------------------------------------------

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
    pub fn from_gate(g: bisa_engine::gates::GateEntry) -> Self {
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
    pub on: bisa_engine::WaitingOn,
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

/// The kinds of thing an Inbox row is — the engine's word, so a frame and a
/// row agree on it.
pub use bisa_engine::notices::{InboxKind, NoticeKind};

/// One notice on a row: which kind of thing happened, and the fact itself
/// as the Pulse carries it — so the desktop words it with the same function
/// a Pulse row gets, and a door opens the same record.
#[derive(Serialize, JsonSchema)]
pub struct NoticeDto {
    pub notice: NoticeKind,
    #[serde(flatten)]
    pub row: PulseRow,
}

/// Which message stream a surface reads and writes — a goal's thread, a
/// channel, a direct channel or a conversation (13 — Conversations). The
/// desktop's `Conversation` component takes one; a workstream or a project
/// has none of its own.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationKind {
    Goal,
    Channel,
    Dm,
    Conversation,
}

/// One page of the activity feed: the rows, newest first, and where the next
/// page starts — `null` on the last one.
#[derive(Serialize, JsonSchema)]
pub struct PulsePage {
    pub rows: Vec<PulseRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<PulseCursor>,
}

/// Where a page stopped: the last row's `(at, seq)`, sent back as
/// `?before=&before_seq=` for the page after.
#[derive(Serialize, JsonSchema, Clone, Copy, PartialEq, Eq)]
pub struct PulseCursor {
    pub at: u64,
    pub seq: i64,
}

/// One thing that happened, carried as facts rather than as a finished
/// sentence.
///
/// The prose used to be composed here and shipped as `text`, which threw the
/// event's type away before the client ever saw it. A stored row could then
/// only guess its glyph from where it pointed, so a criterion failure and a
/// note drew the same symbol; a verification could say it failed but not what
/// the evidence was; and `TurnMetrics` — which both the CLI and the live activity timeline
/// render — produced no row at all. Live rows meanwhile *were* rendered from
/// their payload, so one event looked one way as it arrived and another way
/// after a reload. Shipping the payload leaves exactly one renderer, on the
/// client, and removes that divergence rather than papering over it.
#[derive(Serialize, JsonSchema)]
pub struct PulseRow {
    /// The row's place in the feed — the keyset's tiebreak within a second.
    pub seq: i64,
    pub at: u64,
    /// The concept the row belongs to — the filter it answers.
    pub concept: ActivityConcept,
    /// The payload's tag, so a client can key a glyph or a filter without opening `event`.
    pub kind: String,
    /// What the row is about: the record a click opens.
    pub source: PulseSource,
    /// What that record is called — a goal's title, a channel's name, a
    /// workstream's label. Its own field rather than interpolated into the
    /// line, because a title buried in a sentence cannot be styled, cannot be
    /// linked, and cannot be used to group consecutive rows under one heading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The signing principal, raw. Naming it is the client's job: the desktop
    /// already resolves a pubkey to a member or agent name, and a name
    /// resolved here would be a second, staler copy of that map.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub event: PulseEvent,
}

/// The record a row is about.
#[derive(Serialize, JsonSchema)]
pub struct PulseSource {
    pub kind: ActivitySourceKind,
    /// The record's id; empty for this node or the workspace itself.
    pub id: String,
}

/// What happened.
///
/// Untagged because every arm is already internally tagged on `type`, so the
/// union stays one flat discriminated shape on the wire. The journal arm
/// carries the payload **verbatim**: nothing here restates a variant's
/// fields, which is what makes it structurally impossible for a payload added
/// to the journal to reach the Pulse as anything less than itself. The engine
/// arm is the `EnginePayload` the frame carried, as recorded — the desktop's
/// hand-written mirror of that enum (`types.hand.ts`) is its type.
#[derive(Serialize, JsonSchema)]
#[serde(untagged)]
pub enum PulseEvent {
    Journal(JournalPayload),
    Message(PulseMessage),
    Engine(EngineActivity),
}

/// An engine fact as it was recorded — the frame's payload verbatim, tagged
/// on `type` like every other arm. Its schema says only that much: the
/// engine's enum has no schema of its own, and the desktop's hand-written
/// mirror (`types.hand.ts`, `EnginePayload`) is its type there.
#[derive(Serialize)]
#[serde(transparent)]
pub struct EngineActivity(pub serde_json::Value);

impl JsonSchema for EngineActivity {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("EngineActivity")
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "object",
            "description": "An engine fact as recorded — the frame's payload verbatim; the desktop's hand-written `EnginePayload` is its type.",
            "properties": { "type": { "type": "string" } },
            "required": ["type"],
            "additionalProperties": true
        })
    }
}

/// A message in a conversation — a post or a membership change. A snippet,
/// not the body: the row is a pointer into the conversation, not a copy of
/// it. It carries `type: "message"` as an explicit field, so the untagged
/// union stays one flat shape discriminated on `type` in the schema as well
/// as on the wire.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PulseMessage {
    #[serde(rename = "type")]
    pub kind: PulseMessageTag,
    pub message_id: String,
    /// `post` or `membership`.
    pub body_kind: String,
    pub snippet: String,
}

/// The one value `PulseMessage.type` takes.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum PulseMessageTag {
    Message,
}

// ---------------------------------------------------------------------------
// Conversation
// ---------------------------------------------------------------------------

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewChannelBody {
    pub name: String,
    #[serde(default)]
    pub topic: Option<String>,
    /// The roster: agent ids and team ids that belong here. The
    /// members are derived from it and enablement; nobody is subscribed by
    /// being rostered — a rostered agent still speaks only when addressed.
    /// Only `general` is everyone, and it is seeded, never created here.
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub teams: Vec<String>,
    /// The people (pubkeys of hosted members) on the roster — what a guest
    /// reaches (14-collaboration).
    #[serde(default)]
    pub humans: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// `PATCH /channels/{id}` — omitted fields keep their current value.
///
/// The audience and the kind are absent by design: an audience is who past
/// messages were encrypted to, which an edit cannot revisit. A DM has neither
/// a roster nor a topic, and the store refuses this on one.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PatchChannelBody {
    #[serde(default)]
    pub topic: Option<String>,
    /// Replaces the agent half of the roster when present.
    #[serde(default)]
    pub agents: Option<Vec<String>>,
    /// Replaces the team half of the roster when present.
    #[serde(default)]
    pub teams: Option<Vec<String>>,
    /// Replaces the people on the roster when present.
    #[serde(default)]
    pub humans: Option<Vec<String>>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewMessageBody {
    pub content: String,
    #[serde(default)]
    pub reply_to: Option<String>,
    /// The chips this message carries (ide/09): what the person attached, in
    /// order. Bounded by the core at 64 KiB serialised; over it is a 400.
    #[serde(default)]
    pub context: Vec<bisa_core::ContextRef>,
    /// Who this message addresses, as tokens the scope resolves: a pubkey
    /// hex, an agent definition id, or the conversation's own id — the channel
    /// handle, which expands to its roster. An unknown token is refused
    /// rather than dropped.
    #[serde(default)]
    pub mentions: Vec<String>,
    /// Files this message carries, already uploaded to `POST /attachments`.
    ///
    /// Descriptors rather than bytes: the upload is a separate call so a failed
    /// post does not re-send the file, and so the client can show progress
    /// before anything is said. A descriptor whose bytes are not in this
    /// workspace is refused — a message must not name a file nobody can fetch.
    #[serde(default)]
    pub attachments: Vec<bisa_core::AttachmentRef>,
    /// What the person shares to be looked at rather than handed over as a
    /// file — a page, a sheet, a deck — each uploaded first and described
    /// here with its title and kind. Rendered live where it is read.
    #[serde(default)]
    pub artifacts: Vec<bisa_core::ArtifactRef>,
}

/// `POST /attachments/{sha256}/file`: the name the copy is made under.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NamedFileBody {
    pub name: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenDmBody {
    /// Participant pubkeys (the caller is always included).
    pub members: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReactBody {
    pub emoji: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScopeBody {
    pub scope: String,
}

// ---------------------------------------------------------------------------
// Conversations
// ---------------------------------------------------------------------------

/// `POST /conversations` — what it is about, and a title if there is one.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewConversationBody {
    pub origin: bisa_core::ConversationOrigin,
    #[serde(default)]
    pub title: Option<String>,
    /// The mode it starts in; absent, `agents.conversation.mode` where the
    /// conversation stands. Read only for a project's or a workstream's.
    #[serde(default)]
    pub mode: Option<bisa_core::ConversationMode>,
}

/// `PATCH /conversations/{id}`. `title` is tri-state: absent keeps it,
/// `null` takes it away, a value sets it.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PatchConversationBody {
    #[serde(default, deserialize_with = "clearable")]
    #[schemars(with = "Option<String>")]
    pub title: Option<Option<String>>,
    #[serde(default)]
    pub archived: Option<bool>,
    /// `manual` · `auto` · `plan` — a project's or a workstream's
    /// conversation only (ide/20).
    #[serde(default)]
    pub mode: Option<bisa_core::ConversationMode>,
}

/// `POST /conversations/{id}/changes/settle`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SettleChangesBody {
    pub verdict: bisa_engine::changes::settle::Verdict,
    pub target: bisa_engine::changes::settle::Target,
    /// Undo an `overlapped` file too: the person was asked and said yes.
    #[serde(default)]
    pub force: bool,
}

/// `POST /conversations/{id}/changes/restore`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RestoreChangesBody {
    pub turn: bisa_core::TurnId,
}

/// A conversation as a list reads it: the record with the facts the index
/// keeps beside it. `agents` are the ids of those who spoke or were
/// addressed.
#[derive(Serialize, JsonSchema)]
pub struct ConversationView {
    pub id: String,
    pub origin: bisa_core::ConversationOrigin,
    /// The project a conversation stands in — a project's own or one of its
    /// checkouts' — so a list by project needs no reading of the origin.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<bisa_core::ProjectId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The first line of the first post, for a row with no title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_line: Option<String>,
    pub author: String,
    pub created_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message_at: Option<u64>,
    pub message_count: u64,
    pub archived: bool,
    /// How far an agent goes on its own here (ide/20).
    pub mode: bisa_core::ConversationMode,
    pub agents: Vec<String>,
}

/// One turn in flight (`GET /conversations/{id}/live`): who speaks, the
/// words and the thinking so far, when the turn began, and the tool it
/// runs right now — `null` between tools.
#[derive(Serialize, JsonSchema)]
pub struct LiveTurnView {
    pub agent: String,
    pub text: String,
    pub thinking: String,
    pub since: u64,
    pub working: Option<String>,
}

impl From<bisa_engine::conversations::LiveTurnRow> for LiveTurnView {
    fn from(row: bisa_engine::conversations::LiveTurnRow) -> Self {
        Self {
            agent: row.agent,
            text: row.text,
            thinking: row.thinking,
            since: row.since,
            working: row.working,
        }
    }
}

/// `GET /conversations/{id}/live` — empty when nothing runs.
#[derive(Serialize, JsonSchema)]
pub struct LiveTurnsResponse {
    pub turns: Vec<LiveTurnView>,
}

/// What an indexed conversation is about, from the parts its row carries —
/// the one reading of them, for the conversation's own view and for the
/// Inbox row that sits under what the conversation is about.
///
/// A row the index holds was written from a record, so its parts always make
/// an origin; a row that does not is the index's bug, and reads as the
/// workspace's rather than as nothing at all.
pub(crate) fn origin_of(row: &bisa_store::ConversationRow) -> bisa_core::ConversationOrigin {
    bisa_core::ConversationOrigin::from_parts(
        &row.origin_kind,
        row.origin_id.as_deref(),
        row.project.as_deref(),
    )
    .unwrap_or_else(|| {
        tracing::warn!(
            target: "bisa_node",
            conversation = %row.id,
            kind = %row.origin_kind,
            "an indexed conversation's parts make no origin; read as the workspace's"
        );
        bisa_core::ConversationOrigin::Workspace
    })
}

impl From<bisa_store::ConversationRow> for ConversationView {
    fn from(row: bisa_store::ConversationRow) -> Self {
        let origin = origin_of(&row);
        Self {
            id: row.id,
            project: origin.project(),
            origin,
            title: row.title,
            first_line: row.first_line,
            author: row.author,
            created_at: row.created_at,
            last_message_at: row.last_message_at,
            message_count: row.message_count,
            archived: row.archived,
            mode: bisa_core::ConversationMode::parse(&row.mode).unwrap_or_default(),
            agents: row.agents,
        }
    }
}

/// `GET /conversations`.
#[derive(Serialize, JsonSchema)]
pub struct ConversationsResponse {
    pub conversations: Vec<ConversationView>,
}

// ---------------------------------------------------------------------------
// Agents, skills, MCP servers, teams
// ---------------------------------------------------------------------------

/// `POST /agents`. A key it does not know is refused, never dropped: a switch
/// spelled wrong would otherwise read as a switch left off.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewAgentBody {
    pub name: String,
    /// The agent's picture — an attachment this machine holds (ide/14 §Photos).
    #[serde(default)]
    pub photo: Option<bisa_core::AttachmentRef>,
    #[serde(default)]
    pub description: Option<String>,
    pub system_prompt: String,
    pub harness: String,
    /// Ordered model plan. Omitted = the harness's own default model.
    #[serde(default)]
    pub models: Option<bisa_core::ModelPlan>,
    /// Skill **library** ids. An agent carries references, never markdown:
    /// twenty agents sharing one checklist is the whole point of the library,
    /// and `POST /skills` is where a procedure is written.
    #[serde(default)]
    pub skills: Vec<String>,
    /// MCP **registry** ids, resolved to transports at launch.
    #[serde(default)]
    pub mcps: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// "owner_only" (default) | "members"
    #[serde(default)]
    pub respond: Option<String>,
    /// Whether the Decision-Making Agent stands in at the decision points
    /// this agent reaches, whatever the workspace's switch says.
    #[serde(default)]
    pub decision_making: bool,
}

/// `PATCH /agents/{id}`. Every field is optional, which is why a key it does
/// not know is refused: an edit that changed nothing would otherwise answer
/// 200.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PatchAgentBody {
    #[serde(default)]
    pub name: Option<String>,
    /// The agent's picture, as an attachment. `null` removes it.
    #[serde(default, deserialize_with = "clearable")]
    #[schemars(with = "Option<bisa_core::AttachmentRef>")]
    pub photo: Option<Option<bisa_core::AttachmentRef>>,
    /// The line under the agent's name. `null` removes it.
    #[serde(default, deserialize_with = "clearable")]
    #[schemars(with = "Option<String>")]
    pub description: Option<Option<String>>,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub harness: Option<String>,
    /// Replaces the whole plan when present — the list is ordered, so a
    /// partial patch would silently reorder it.
    #[serde(default)]
    pub models: Option<bisa_core::ModelPlan>,
    /// Replaces the whole list when present, for the same reason as `models`:
    /// a harness reads skills in order. Attaching or detaching one without
    /// resending the rest is `POST`/`DELETE /agents/{id}/skills/{skill_id}`.
    #[serde(default)]
    pub skills: Option<Vec<String>>,
    #[serde(default)]
    pub mcps: Option<Vec<String>>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub respond: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    /// One of the three things a core agent may change.
    #[serde(default)]
    pub decision_making: Option<bool>,
}

/// `POST /skills` — a procedure written once and followed by every agent that
/// references it.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewSkillBody {
    /// Slug, and what agents reference. Immutable afterwards.
    pub id: String,
    pub name: String,
    /// The one line a model reads before deciding whether to open the skill,
    /// so it says *when* to reach for it. Required, not decorative.
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub markdown: String,
}

/// `PATCH /skills/{id}` — omitted fields keep their current value. The id is
/// not patchable: agents reference it, and a rename that silently detached
/// every reference would be worse than refusing.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PatchSkillBody {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub markdown: Option<String>,
}

// --- notes ------------------------------------------------------------------

/// What a note is attached to, on the wire.
///
/// A flat pair rather than `bisa_store::OwnerScope`'s tagged enum: this
/// arrives as a query string (`?scope=goal&id=…`) as well as in a body, and
/// a query string cannot carry a tagged union. The store's type is the one
/// that decides validity; this is the two strings it is built from.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OwnerScopeBody {
    /// `workspace`, `project`, `goal`, `workflow`, `channel` or `node`.
    pub scope: String,
    /// The record's id — a goal's, a project's or a workflow's ULID, a
    /// channel's slug. Absent for `workspace` and `node`, and refused for them.
    #[serde(default)]
    pub id: Option<String>,
}

/// `POST /notes` — a markdown scratchpad on a project, a goal, a workflow, a
/// channel, the workspace, or this node.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewNoteBody {
    pub scope: String,
    #[serde(default)]
    pub id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub body: String,
}

/// `PATCH /notes/{id}` — omitted fields keep their value.
///
/// `base_hash` is the SHA-256 of the body you last read, and it is required
/// whenever `body` is present: a note has two writers, you and any agent you
/// asked, so an unguarded write is a way to lose the other one's paragraph
/// without either of you seeing it happen. A mismatch answers 409 carrying
/// the current body.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PatchNoteBody {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub pinned: Option<bool>,
    #[serde(default)]
    pub base_hash: Option<String>,
}

/// One note in a list. Carries the body: a scope holds a handful of notes, and
/// a list you had to follow with a read per row would be the slower shape.
#[derive(Serialize, JsonSchema)]
pub struct NoteRow {
    pub id: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<String>,
    pub title: String,
    pub body: String,
    /// SHA-256 of `body`. Pass it back as `base_hash` to edit safely.
    pub hash: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub pinned: bool,
}

/// `POST /drawings` — a drawing on the canvas (19 — Drawings), filed under
/// a scope like a note; `scene` is a template's elements, else an empty canvas.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewDrawingBody {
    pub scope: String,
    #[serde(default)]
    pub id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub scene: Option<bisa_core::Scene>,
}

/// `PATCH /drawings/{id}` — omitted fields keep their value.
///
/// `base_hash` is the scene's hash you last read, required whenever `scene`
/// is present: a drawing has two writers, you on the canvas and any agent
/// drawing into it, so an unguarded write is a way to lose the other one's
/// strokes without either of you seeing it happen. A mismatch answers 409
/// carrying the current scene.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PatchDrawingBody {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub pinned: Option<bool>,
    #[serde(default)]
    pub scene: Option<bisa_core::Scene>,
    #[serde(default)]
    pub base_hash: Option<String>,
}

/// One drawing in a list — every column but the scene, which a list never
/// carries: a scene can be hundreds of kibibytes.
#[derive(Serialize, JsonSchema)]
pub struct DrawingRow {
    pub id: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<String>,
    pub title: String,
    pub pinned: bool,
    /// The scene's hash. Pass it back as `base_hash` to edit safely.
    pub hash: String,
    pub element_count: usize,
    pub created_at: u64,
    pub updated_at: u64,
}

/// `GET /drawings/{id}` — the row and the scene.
#[derive(Serialize, JsonSchema)]
pub struct DrawingDetail {
    #[serde(flatten)]
    pub row: DrawingRow,
    pub scene: bisa_core::Scene,
}

/// `POST /pets` — install a pet package from a folder on this machine.
///
/// A path rather than an upload: a package is two files with a required
/// layout, and the desktop's folder picker already returns exactly this. The
/// path is validated as an adopted project's is — absolute, canonicalized,
/// existing, and outside the workspace.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewPetBody {
    pub path: String,
}

/// `POST /mcp` — register a server once, reference it from many agents.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewMcpBody {
    /// Slug, and what agents reference. Immutable afterwards.
    pub id: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// The server's name comes from here, not from a separate field: two
    /// registry entries may legitimately front the same server name on
    /// different transports.
    pub transport: bisa_core::McpServerConfig,
}

/// `PATCH /mcp/{id}` — omitted fields keep their current value.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PatchMcpBody {
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub transport: Option<bisa_core::McpServerConfig>,
    /// Disabled servers stay registered and are skipped at launch — being
    /// switched off is the owner's decision, not a broken reference.
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// `POST /mcp/probe`: dial a transport that is nobody's entry yet — the
/// editor's *Test connection* before *Save*.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProbeMcpBody {
    pub transport: bisa_core::McpServerConfig,
    /// How long to wait for the whole conversation, 1–30 s; absent, ten.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

/// `POST /mcp/{id}/probe`: the budget alone; the transport is the entry's.
#[derive(Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProbeMcpByIdBody {
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

/// What a probe answered — `bisa-mcp-probe`'s report, on the wire.
pub type McpProbeReportDto = bisa_mcp_probe::McpProbeReport;
/// A server's health as the registry answers it beside every entry.
pub type McpHealthDto = bisa_engine::mcp_health::McpHealthView;

/// An MCP server as the wire carries it: the record — its secrets masked —
/// with its health beside it (`GET /mcp`, `POST /mcp`, `GET|PATCH /mcp/{id}`).
/// The record's own fields are flattened, so a reader that wants the record
/// takes `server`; one type for the desktop, the command line and the node.
///
/// Read by hand: a derived `flatten` hands the record every key `health`
/// did not take and lets it drop what it does not know, whatever the
/// record's own rule says — so the health is taken out first and the rest is
/// read as the record, which refuses a key nobody knows by name.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct McpServerView {
    #[serde(flatten)]
    pub server: bisa_core::McpServer,
    pub health: McpHealthDto,
}

impl<'de> Deserialize<'de> for McpServerView {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;
        let mut raw = serde_json::Map::<String, serde_json::Value>::deserialize(d)?;
        let health = raw
            .remove("health")
            .ok_or_else(|| D::Error::missing_field("health"))?;
        let health: McpHealthDto = serde_json::from_value(health).map_err(D::Error::custom)?;
        let server: bisa_core::McpServer =
            serde_json::from_value(serde_json::Value::Object(raw)).map_err(D::Error::custom)?;
        Ok(Self { server, health })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TeamBody {
    pub name: String,
    #[serde(default)]
    pub purpose: Option<String>,
    /// The team's picture — an attachment this machine holds (ide/14 §Photos).
    #[serde(default)]
    pub photo: Option<bisa_core::AttachmentRef>,
    /// Members: `{"human": "<pubkey>"}` or `{"agent": "<agent-id>"}`. A team
    /// inside a team is refused — list that team's members directly.
    pub members: Vec<serde_json::Value>,
    #[serde(default)]
    pub tags: Vec<String>,
}

// ---------------------------------------------------------------------------
// The catalog
// ---------------------------------------------------------------------------
//
// These three mirror `bisa_store::{CatalogKind, CatalogEntry, Installed}`
// field for field. The store does not derive `JsonSchema` — see the
// `api-schema` bin's note — and a picker is the most structural surface the
// desktop has, so the shapes are declared here rather than emitted as ad-hoc
// JSON the desktop would then hand-mirror. `From` is the only conversion, so
// a field added to the store fails to compile here instead of going missing
// on the wire.
//
// The `Dto` suffix keeps them apart from the store's types in Rust and is
// renamed away for schemars, which keys nested `$defs` by the Rust name: an
// entry referencing the kind enum would otherwise put that shape in the
// bundle twice, once under each spelling, and the desktop would import
// whichever it happened to find.

/// What kind of definition a catalog entry is: the five values `?kind=` and
/// `POST /catalog/install` accept.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "CatalogKind")]
pub enum CatalogKindDto {
    Agent,
    Skill,
    Team,
    Channel,
    Connector,
    Workflow,
    Addon,
}

impl From<bisa_store::CatalogKind> for CatalogKindDto {
    fn from(k: bisa_store::CatalogKind) -> Self {
        use bisa_store::CatalogKind as K;
        match k {
            K::Agent => Self::Agent,
            K::Skill => Self::Skill,
            K::Team => Self::Team,
            K::Channel => Self::Channel,
            K::Connector => Self::Connector,
            K::Workflow => Self::Workflow,
            K::Addon => Self::Addon,
        }
    }
}

/// A row of `GET /catalog`: one installable definition, and whether its id is
/// already taken here.
#[derive(Serialize, JsonSchema)]
#[schemars(rename = "CatalogEntry")]
pub struct CatalogEntryDto {
    pub kind: CatalogKindDto,
    /// The slug, which is also the id the entry installs under.
    pub slug: String,
    pub name: String,
    pub description: String,
    pub tags: bisa_core::Tags,
    /// What installing this pulls in with it — agent slugs for a team, a
    /// channel or a workflow's steps, skill slugs for an agent, and
    /// `workflow:<slug>` for the templates a workflow's `spawn` steps start. A
    /// picker can say what a choice costs before it is made.
    pub requires: Vec<String>,
    /// Whether an object with this id already exists here. It says the id is
    /// taken, not that this entry took it: an id held by something you created
    /// reads as installed and the install refuses it rather than overwriting.
    pub installed: bool,
    /// A workflow template's whole definition — inputs, steps, flows — so a
    /// gallery draws its graph before it is installed; absent for every other
    /// kind. A `spawn` step whose target is not installed names a placeholder
    /// id: the picture is drawn, the install resolves the reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow: Option<NewWorkflowBody>,
}

impl From<bisa_store::NewWorkflow> for NewWorkflowBody {
    fn from(w: bisa_store::NewWorkflow) -> Self {
        Self {
            name: w.name,
            description: w.description,
            inputs: w.inputs,
            steps: w.steps,
            tags: w.tags.as_slice().to_vec(),
            decision_making: w.decision_making,
        }
    }
}

impl From<bisa_store::CatalogEntry> for CatalogEntryDto {
    fn from(e: bisa_store::CatalogEntry) -> Self {
        Self {
            kind: e.kind.into(),
            slug: e.slug,
            name: e.name,
            description: e.description,
            tags: e.tags,
            requires: e.requires,
            installed: e.installed,
            workflow: e.workflow.map(NewWorkflowBody::from),
        }
    }
}

/// What `POST /catalog/install` created, by kind.
///
/// Only creations. An entry already present is left exactly as it is and
/// appears nowhere here, so installing the same thing twice answers with every
/// list empty — that is idempotence reported honestly, not a failure.
#[derive(Serialize, Default, JsonSchema)]
#[schemars(rename = "Installed")]
pub struct InstalledDto {
    pub agents: Vec<String>,
    pub skills: Vec<String>,
    pub teams: Vec<String>,
    pub channels: Vec<String>,
    /// A workflow's id is a ULID, so the caller that wants to open what it
    /// just installed needs the slug and the id both.
    pub workflows: Vec<InstalledWorkflowDto>,
    pub connectors: Vec<String>,
    pub addons: Vec<String>,
}

#[derive(Serialize, JsonSchema)]
#[schemars(rename = "InstalledWorkflow")]
pub struct InstalledWorkflowDto {
    pub slug: String,
    pub id: WorkflowId,
}

impl From<bisa_store::Installed> for InstalledDto {
    fn from(i: bisa_store::Installed) -> Self {
        Self {
            agents: i.agents,
            skills: i.skills,
            teams: i.teams,
            channels: i.channels,
            workflows: i
                .workflows
                .into_iter()
                .map(|(slug, id)| InstalledWorkflowDto { slug, id })
                .collect(),
            connectors: i.connectors,
            addons: i.addons,
        }
    }
}

// ---------------------------------------------------------------------------
// Addons (18 — Addons)
// ---------------------------------------------------------------------------

/// `POST /addons` — import a folder on this machine, with what the person
/// chose in the review: the grants (a subset of what the manifest declares;
/// nothing by default) and whether it runs at once.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewAddonBody {
    pub path: String,
    #[serde(default)]
    pub granted: Vec<AddonPermission>,
    #[serde(default)]
    pub enabled: bool,
}

/// `POST /addons/validate` — a folder to judge, writing nothing.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ValidateAddonBody {
    pub path: String,
}

/// `PATCH /addons/{id}` — the switch, the grants, or both.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddonPatchBody {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub granted: Option<Vec<AddonPermission>>,
}

/// `POST /addons/{id}/fetch` — one URL the addon asks for.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddonFetchBody {
    pub url: String,
    #[serde(default)]
    pub accept: Option<String>,
}

/// One installed addon: the record, and two facts the store derives — whether
/// its files are on this machine, and whether it is active (enabled with its
/// files here).
#[derive(Serialize, JsonSchema)]
#[schemars(rename = "Addon")]
pub struct AddonDto {
    pub id: String,
    pub manifest: AddonManifest,
    pub origin: bisa_core::Origin,
    pub enabled: bool,
    pub granted: Vec<AddonPermission>,
    pub installed_at: u64,
    pub files_present: bool,
    pub active: bool,
}

impl From<bisa_store::AddonEntry> for AddonDto {
    fn from(a: bisa_store::AddonEntry) -> Self {
        let active = a.is_active();
        Self {
            id: a.record.manifest.id.to_string(),
            manifest: a.record.manifest,
            origin: a.record.origin,
            enabled: a.record.enabled,
            granted: a.record.granted,
            installed_at: a.record.installed_at,
            files_present: a.files_present,
            active,
        }
    }
}

/// `GET /addons`.
#[derive(Serialize, JsonSchema)]
#[schemars(rename = "Addons")]
pub struct AddonsDto {
    pub addons: Vec<AddonDto>,
    /// The machine's `addons.enabled` switch: off, nothing draws or is served.
    pub addons_enabled: bool,
}

/// `POST /addons/validate`: the manifest as the node read it, and every
/// problem the folder has — none, and the folder installs.
#[derive(Serialize, JsonSchema)]
#[schemars(rename = "AddonProblems")]
pub struct AddonProblemsDto {
    pub manifest: AddonManifest,
    pub problems: Vec<AddonProblem>,
}

/// One built-in the catalog offers, with its manifest — what the review
/// shows before an install.
#[derive(Serialize, JsonSchema)]
#[schemars(rename = "AddonOffer")]
pub struct AddonOfferDto {
    pub slug: String,
    pub manifest: AddonManifest,
    pub installed: bool,
}

impl From<bisa_store::AddonOffer> for AddonOfferDto {
    fn from(o: bisa_store::AddonOffer) -> Self {
        Self {
            slug: o.slug,
            manifest: o.manifest,
            installed: o.installed,
        }
    }
}

/// `GET /addons/offer`.
#[derive(Serialize, JsonSchema)]
#[schemars(rename = "AddonOffers")]
pub struct AddonOffersDto {
    pub offers: Vec<AddonOfferDto>,
}

/// `POST /catalog/install` — one entry, named by kind and slug.
///
/// The kind is a string rather than the enum so a typo is refused by the same
/// message `?kind=` gives, naming all five spellings, instead of by serde's
/// report of an unmatched variant.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogInstallBody {
    pub kind: String,
    pub slug: String,
}

/// One entity kind's share of a tag — a row of [`TagFacet::entities`].
#[derive(Serialize, JsonSchema)]
pub struct TagEntityCount {
    pub entity: TagEntity,
    pub count: u64,
}

/// A row of `GET /tags`: one tag, how many objects carry it, and which kinds
/// they are — enough to draw a filter chip and know what it narrows.
#[derive(Serialize, JsonSchema)]
pub struct TagFacet {
    pub tag: String,
    /// Summed across every kind counted. With `?entity=`, that is the one
    /// kind asked for.
    pub total: u64,
    pub entities: Vec<TagEntityCount>,
}

// ---------------------------------------------------------------------------
// Admin
// ---------------------------------------------------------------------------

/// `PUT /workspace/me` — the owner's own profile (14-collaboration): what
/// this node's people list and every host this node is a guest of call and
/// draw them. Each part is kept when absent and cleared with `null`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MeBody {
    /// What to be called — here and on every host; cleaned to 64 printable characters.
    #[serde(default, deserialize_with = "clearable")]
    #[schemars(with = "Option<String>")]
    pub label: Option<Option<String>>,
    /// The face — a small picture this machine holds (`MAX_FACE_BYTES`). `null` removes it.
    #[serde(default, deserialize_with = "clearable")]
    #[schemars(with = "Option<bisa_core::AttachmentRef>")]
    pub photo: Option<Option<bisa_core::AttachmentRef>>,
}

/// `POST /workspace/people` — a person admitted by hand, without an invite.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PersonBody {
    pub pubkey: String,
    /// admin | member | guest (default). Never owner.
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
}

/// `PUT /workspace/people/{pubkey}/role`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoleBody {
    /// admin | member | guest.
    pub role: String,
}

/// One person hosted here, as the People panel reads them.
#[derive(Serialize, Clone, JsonSchema)]
pub struct PersonRow {
    #[serde(flatten)]
    pub member: bisa_core::WorkspaceMember,
    /// What the role holds.
    pub permissions: Vec<bisa_core::Permission>,
    /// The standing channels this person is rostered on — what a guest
    /// reaches. Empty for a role that reaches every channel.
    pub channels: Vec<String>,
}

/// One role with its words and permissions — `GET /workspace/roles`.
#[derive(Serialize, Clone, JsonSchema)]
pub struct RoleRow {
    pub role: bisa_core::MemberRole,
    pub words: String,
    pub permissions: Vec<bisa_core::Permission>,
}

/// One permission with its words.
#[derive(Serialize, Clone, JsonSchema)]
pub struct PermissionRow {
    pub permission: bisa_core::Permission,
    pub words: String,
}

/// `POST /workspace/invites`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewInviteBody {
    /// admin | member | guest (default).
    #[serde(default)]
    pub role: Option<String>,
    /// The standing channels a guest is put on.
    #[serde(default)]
    pub channels: Vec<String>,
    /// What to call the person, until they say.
    #[serde(default)]
    pub label: Option<String>,
}

/// `POST /hosts/join`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JoinHostBody {
    /// The invite link (`bisa://join/…`) or the text code (`<nprofile>:<secret>`).
    pub code: String,
    /// What to be called there.
    #[serde(default)]
    pub label: Option<String>,
}

/// `POST /hosts/{host}/channels/{id}/messages` — a guest's post: text,
/// pubkeys and a parent. No files, no artifacts: the bytes would have
/// nowhere to go.
#[derive(Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HostedPostBody {
    pub content: String,
    /// Pubkeys the host's directory names, or the host's agents' keys.
    #[serde(default)]
    pub mentions: Vec<String>,
    #[serde(default)]
    pub reply_to: Option<String>,
}

/// What a guest's write left: the fact's id, its scope, and how many
/// secrets the redactor replaced before it was signed.
#[derive(Serialize, Clone, JsonSchema)]
pub struct HostedPosted {
    pub id: String,
    pub scope: String,
    pub redacted: usize,
}

/// A hosted channel with what this person has not read of it.
#[derive(Serialize, Clone, JsonSchema)]
pub struct HostedChannelRow {
    pub channel: bisa_core::Channel,
    pub unread_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_at: Option<u64>,
}

/// `POST /sync/relays/check`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelayUrlBody {
    pub url: String,
}

/// `GET /sync` — the wire as it stands.
#[derive(Serialize, Clone, Default, JsonSchema)]
pub struct SyncReport {
    /// Whether a collaboration pump runs in the node's process at all.
    pub running: bool,
    /// The `sync.enabled` switch: off, every relay is listed as `off` and
    /// nothing is contacted.
    pub enabled: bool,
    /// Every configured relay, in order, with its state — `off` rows while
    /// the switch is off.
    pub relays: Vec<bisa_collab::RelayHealth>,
    pub connected_relays: usize,
    pub published: u64,
    pub ingested: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_catchup: Option<u64>,
    /// People hosted here.
    pub people: usize,
    /// Workspaces this node is a guest of.
    pub hosts: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iroh_node_id: Option<String>,
    pub iroh_peers_connected: usize,
}

impl SyncReport {
    /// A node with no pump in its process: nothing runs, and the report
    /// still says what the settings hold — the switch, and every relay as
    /// `off` — so a screen lists them.
    pub fn not_running(settings: &bisa_core::SyncSettings) -> Self {
        Self {
            enabled: settings.enabled,
            relays: settings
                .relays
                .iter()
                .map(bisa_collab::RelayHealth::off)
                .collect(),
            ..Self::default()
        }
    }
}

// ---------------------------------------------------------------------------
// Projects and workstreams
// ---------------------------------------------------------------------------

/// `POST /goals/{id}/projects` — **provenance is explicit**.
///
/// There are exactly five ways a project comes into existence, and the caller
/// says which one it is rather than the node guessing from which fields are
/// present. A **managed** root — `new`, `clone`, `import` — is a folder this
/// node makes, so `new` initialises it as a repository and carries no flag for
/// it; `adopt` never writes anything into the folder it points at, which is
/// the half of the rule that stayed absolute.
///
/// `import` and `adopt` are the two answers to the same folder. `import`
/// copies the tree in and the files become the goal's; `adopt` points at it
/// where it lies and Bisa writes nothing into it, ever. Neither one
/// modifies the folder it was given — an import reads the source and writes
/// only inside the workspace.
#[derive(Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NewProjectBody {
    /// Create `goals/<owner>/projects/<slug>`, initialised as a git
    /// repository.
    ///
    /// There is no `git` flag. The folder is one Bisa makes under the
    /// owning goal, so `git init` writes into nothing that was already
    /// somebody's — and a project without a repository gets copy workstreams
    /// and `.patch` artifacts instead of branches and commits, which is
    /// almost never what the person clicking "create" meant. `adopt` is where
    /// the rule still bites: it never writes into the folder it is given.
    New {
        slug: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        assignees: Vec<String>,
        #[serde(default)]
        publish: Option<bisa_core::PublishPolicy>,
        #[serde(default)]
        tags: Vec<String>,
        /// Git config for the new repository, written to its local layer key
        /// by key (schema keys only). Omitted, the repository
        /// inherits the global config and asks only when no identity resolves.
        #[serde(default)]
        git_config: std::collections::BTreeMap<String, String>,
    },
    /// `git clone <url>` into `goals/<owner>/projects/<slug>`.
    Clone {
        slug: String,
        #[serde(default)]
        name: Option<String>,
        url: String,
        #[serde(default)]
        depth: Option<u32>,
        #[serde(default)]
        assignees: Vec<String>,
        #[serde(default)]
        publish: Option<bisa_core::PublishPolicy>,
        #[serde(default)]
        tags: Vec<String>,
        /// Git config for the new repository, written to its local layer key
        /// by key (schema keys only). Omitted, the repository
        /// inherits the global config and asks only when no identity resolves.
        #[serde(default)]
        git_config: std::collections::BTreeMap<String, String>,
    },
    /// Adopt an existing folder anywhere on disk by reference. `path` must be
    /// **absolute** and an existing directory; it is canonicalized before it
    /// is stored, and a folder inside the Bisa workspace is refused.
    Adopt {
        slug: String,
        #[serde(default)]
        name: Option<String>,
        path: String,
        #[serde(default)]
        assignees: Vec<String>,
        #[serde(default)]
        publish: Option<bisa_core::PublishPolicy>,
        #[serde(default)]
        tags: Vec<String>,
        /// Git config for the new repository, written to its local layer key
        /// by key (schema keys only). Omitted, the repository
        /// inherits the global config and asks only when no identity resolves.
        #[serde(default)]
        git_config: std::collections::BTreeMap<String, String>,
    },
    /// Copy an existing folder into `goals/<owner>/projects/<slug>`, as a
    /// **managed** root: the files become this goal's, and the source keeps
    /// its own copy untouched.
    ///
    /// `path` is validated exactly as `adopt` validates it — absolute,
    /// canonicalized, an existing directory, and outside the Bisa
    /// workspace. `.git` is copied, so an imported repository keeps its
    /// history and its `vcs` is detected from the copy. Symlinks are skipped
    /// rather than followed: a link pointing out of the source would turn an
    /// import into a copy of whatever it targets. The copy is refused above a
    /// size and entry bound, and refused outright if the destination folder
    /// already exists.
    Import {
        slug: String,
        #[serde(default)]
        name: Option<String>,
        path: String,
        #[serde(default)]
        assignees: Vec<String>,
        #[serde(default)]
        publish: Option<bisa_core::PublishPolicy>,
        #[serde(default)]
        tags: Vec<String>,
        /// Git config for the new repository, written to its local layer key
        /// by key (schema keys only). Omitted, the repository
        /// inherits the global config and asks only when no identity resolves.
        #[serde(default)]
        git_config: std::collections::BTreeMap<String, String>,
    },
    /// Make another goal's project visible here. Identical to
    /// `POST /projects/{pid}/attach`.
    ///
    /// The odd one out, and deliberately so: nothing is created and nothing is
    /// written, because the project already exists somewhere else. It is on
    /// this route so one endpoint answers "add a project to this goal"
    /// whatever the source.
    /// Attach an existing project to the goal in the path.
    Attach { project: String },
}

/// `PATCH /projects/{pid}`. Omitted fields keep their current value.
///
/// The slug is not patchable: it is a directory name, and moving a folder
/// (possibly with live worktrees in it) is not something a record edit should
/// imply.
///
/// `group` and `photo` are **tri-state**: absent keeps the value, `null`
/// clears it, a value sets it — see [`clearable`].
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PatchProjectBody {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub publish: Option<bisa_core::PublishPolicy>,
    /// `Assignee` wire form: `agent:<id>` / `human:<hex>` / `team:<id>`.
    #[serde(default)]
    pub assignees: Option<Vec<String>>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// The rail group. `null` removes the project from its group.
    #[serde(default, deserialize_with = "clearable")]
    #[schemars(with = "Option<String>")]
    pub group: Option<Option<String>>,
    /// The project's picture, as an attachment. `null` removes it.
    #[serde(default, deserialize_with = "clearable")]
    #[schemars(with = "Option<bisa_core::AttachmentRef>")]
    pub photo: Option<Option<bisa_core::AttachmentRef>>,
}

/// `PATCH /workstreams/{wid}` — what a person may change about a workstream.
/// `name` and `note` are tri-state like a project's `group`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PatchWorkstreamBody {
    /// A label; `null` goes back to calling it by its branch.
    #[serde(default, deserialize_with = "clearable")]
    #[schemars(with = "Option<String>")]
    pub name: Option<Option<String>>,
    #[serde(default, deserialize_with = "clearable")]
    #[schemars(with = "Option<String>")]
    pub note: Option<Option<String>>,
    #[serde(default)]
    pub pinned: Option<bool>,
    /// The Board's due date, `YYYY-MM-DD`; `null` clears it. A day, not an instant.
    #[serde(default, deserialize_with = "clearable")]
    #[schemars(with = "Option<String>")]
    pub due: Option<Option<bisa_core::DueDate>>,
}

/// `PUT /workstreams/{wid}/board/place` — put a card at an index in a Board
/// column. The engine turns the index into a rank; the reply is every record
/// that changed (one, or the whole column when it was renumbered).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlaceWorkstreamBody {
    pub column: bisa_core::BoardColumn,
    /// Where among the column's cards, from the top; past the end is last.
    pub index: u32,
}

/// A field a PATCH may leave alone, clear, or set — the three answers
/// `Option<Option<T>>` gives once "absent" and "null" are told apart. Serde
/// folds both into `None` by default; this keeps `null` as `Some(None)`.
/// Absent still arrives as `None` through `#[serde(default)]`.
pub(crate) fn clearable<'de, D, T>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

/// `POST`/`DELETE /projects/{pid}/attach` — the goal the project becomes
/// visible in (or stops being visible in).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectAttachBody {
    pub goal: String,
}

/// `POST /projects/{pid}/workstreams` — open a checkout by hand.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewWorkstreamBody {
    /// Whose folder holds the checkout. Defaults to the project's owner; any
    /// other goal must be able to *see* the project (own ∪ linked ∪
    /// ancestors').
    #[serde(default)]
    pub goal: Option<String>,
    /// Human-readable half of the branch name (`work/<label>-<tail>`).
    #[serde(default)]
    pub label: Option<String>,
    /// Agent this workstream belongs to, for attribution in the journal.
    #[serde(default)]
    pub agent: Option<String>,
    /// Where the checkout starts (ide/07 §Where a workstream starts): a new
    /// branch (typed or derived, at a ref or the base), an existing local
    /// branch, a remote's branch (fetched and tracked), a tag (a new branch
    /// at it, the tag made first when `create_at` says where), or an open
    /// pull request (its head tracked, its base as the base, the record born
    /// linked). Absent: a derived new branch at the base.
    #[serde(default)]
    pub source: bisa_core::WorkstreamSource,
    /// The branch the work goes back to; the project's default branch when
    /// absent. A pull request source brings its own and ignores this.
    #[serde(default)]
    pub base: Option<String>,
}

/// `POST /workstreams/{wid}/commit`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommitBody {
    pub message: String,
}

/// `POST /workstreams/{wid}/git/stage` and `/workstreams/{wid}/git/unstage` — the
/// paths a person ticked, exactly as `GET /workstreams/{wid}/git/files` reported
/// them (relative to the repository root).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StageBody {
    pub paths: Vec<String>,
}

/// `GET /workstreams/{wid}/git/identity` — who authors commits in this
/// checkout's repository, and where that answer comes from. A worktree shares
/// its repository's config, so every workstream of a project answers alike.
#[derive(Serialize, JsonSchema)]
pub struct GitIdentityView {
    /// The effective `user.name`, whatever layer it comes from.
    pub name: Option<String>,
    pub email: Option<String>,
    pub source: IdentitySource,
    /// The global pair, when there is one — offered so a person can pin it to
    /// this repository.
    pub global: Option<GitIdent>,
    /// The profile the effective identity came from (ide/04 §Profiles by
    /// organization), when it came from one; the `source` reads `global` then,
    /// since the value resolves outside the repository.
    pub profile: Option<String>,
    /// What this repository could commit as, when nobody is set: the identity
    /// the checkout's connected code host account suggests — offered for one
    /// click, never written by itself. `null` with an identity, without an
    /// account, or when the host gives nothing to build an email from.
    pub suggested: Option<CommitterSuggestionView>,
}

/// A committer a code host account suggests, and whose account it is.
#[derive(Serialize, JsonSchema, Clone, PartialEq, Eq)]
pub struct CommitterSuggestionView {
    pub name: String,
    pub email: String,
    pub login: String,
}

/// Where a repository's commit identity comes from (`bisa_vcs::IdentitySource`).
#[derive(Serialize, JsonSchema, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IdentitySource {
    /// Both keys are set in the repository itself.
    Local,
    /// Both resolve from outside the repository — the global config, in practice.
    Global,
    /// A commit here would fail.
    None,
}

/// A committer as the wire spells it: `{name, email}` — what an identity's
/// view answers. Who commits is *written* as two keys of the repository's
/// local config (`PUT /workstreams/{wid}/git/config`, `user.name` and
/// `user.email`, never global); a creation body carries the same two keys
/// inside its `git_config` map.
#[derive(Serialize, JsonSchema, Clone, PartialEq, Eq)]
pub struct GitIdent {
    pub name: String,
    pub email: String,
}

impl From<bisa_vcs::Ident> for GitIdent {
    fn from(i: bisa_vcs::Ident) -> Self {
        Self {
            name: i.name,
            email: i.email,
        }
    }
}

impl From<GitIdent> for bisa_vcs::Ident {
    fn from(i: GitIdent) -> Self {
        Self {
            name: i.name,
            email: i.email,
        }
    }
}

impl From<bisa_vcs::GitIdentity> for GitIdentityView {
    fn from(id: bisa_vcs::GitIdentity) -> Self {
        Self {
            name: id.name,
            email: id.email,
            source: match id.source {
                bisa_vcs::IdentitySource::Local => IdentitySource::Local,
                bisa_vcs::IdentitySource::Global => IdentitySource::Global,
                bisa_vcs::IdentitySource::None => IdentitySource::None,
            },
            global: id.global.map(GitIdent::from),
            profile: None,
            suggested: None,
        }
    }
}

/// `GET /git/committer` — what the ask dialog seeds from: the
/// person's global git pair and the projects still asking who commits.
#[derive(Serialize, JsonSchema)]
pub struct CommitterView {
    /// The person's global git pair, when both keys are set. Read, never written here.
    pub global: Option<GitIdent>,
    /// Projects nobody has said who commits in yet.
    pub pending: Vec<PendingCommitterView>,
}

/// One project still asking who commits — the facts the `committer_needed`
/// frame carries, so a dialog seeded here shows what one that heard the
/// frame shows.
#[derive(Serialize, JsonSchema)]
pub struct PendingCommitterView {
    pub project: String,
    pub slug: String,
    /// The primary workstream — where `PUT …/git/config` writes the answer.
    pub workstream: String,
    pub reason: CommitterReason,
    /// Where the project was born — the dialog's first sentence says it.
    pub origin: bisa_core::ProjectOrigin,
    /// The person's global git pair, when both keys are set.
    pub global: Option<GitIdent>,
}

/// Why a project is asking (`bisa_engine::CommitterReason`).
#[derive(Serialize, JsonSchema, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommitterReason {
    /// Just created; nothing resolved.
    Created,
    /// A person's commit was refused.
    CommitRefused,
    /// An agent's settlement commit was refused; it is made once an identity is set.
    SettlementRefused,
    /// Found at a start with no identity resolving in it.
    Unresolved,
}

impl From<bisa_engine::CommitterReason> for CommitterReason {
    fn from(r: bisa_engine::CommitterReason) -> Self {
        use bisa_engine::CommitterReason as R;
        match r {
            R::Created => Self::Created,
            R::CommitRefused => Self::CommitRefused,
            R::SettlementRefused => Self::SettlementRefused,
            R::Unresolved => Self::Unresolved,
        }
    }
}

impl From<bisa_engine::CommitterOverview> for CommitterView {
    fn from(o: bisa_engine::CommitterOverview) -> Self {
        Self {
            global: o.global.map(GitIdent::from),
            pending: o
                .pending
                .into_iter()
                .map(|p| PendingCommitterView {
                    project: p.project.to_string(),
                    slug: p.slug,
                    workstream: p.workstream.to_string(),
                    reason: p.reason.into(),
                    origin: p.origin,
                    global: p.global.map(GitIdent::from),
                })
                .collect(),
        }
    }
}

/// One key of the platform's git config schema (`bisa_vcs::config_schema`),
/// as a form renders it; `label` and `hint` are the catalog's message
/// `git-config-<key>` in the request's language.
#[derive(Serialize, JsonSchema)]
pub struct GitConfigKeyDto {
    pub key: String,
    pub kind: GitConfigKindDto,
    pub label: String,
    pub hint: String,
    /// The layers this key may be written at: `global`, `local`.
    pub scopes: Vec<GitConfigScopeDto>,
}

#[derive(Serialize, JsonSchema, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GitConfigScopeDto {
    Global,
    Local,
}

#[derive(Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GitConfigKindDto {
    Text,
    Bool,
    Choice { options: Vec<String> },
}

/// One schema key as the two layers hold it. `effective` is what git resolves
/// for the repository: its own value, else the global one.
#[derive(Serialize, JsonSchema)]
pub struct GitConfigEntryDto {
    pub key: String,
    pub local: Option<String>,
    pub global: Option<String>,
    pub effective: Option<String>,
}

/// `GET /git/config` and `GET /workstreams/{wid}/git/config`: the schema and
/// every key's layers. Read from no repository, `local` is `None` throughout.
#[derive(Serialize, JsonSchema)]
pub struct GitConfigView {
    pub schema: Vec<GitConfigKeyDto>,
    pub entries: Vec<GitConfigEntryDto>,
}

/// `PUT /git/config` and `PUT /workstreams/{wid}/git/config`: keys to set at
/// that layer, then keys to unset (an unset key falls through to the layer
/// beneath). Schema keys only; 400 names the first refusal.
#[derive(Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct GitConfigWrite {
    #[serde(default)]
    pub set: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub unset: Vec<String>,
}

impl GitConfigWrite {
    pub fn into_parts(self) -> (Vec<(String, String)>, Vec<String>) {
        (self.set.into_iter().collect(), self.unset)
    }
}

impl GitConfigView {
    /// The view with the schema's words rendered in `locale`.
    pub fn localized(v: bisa_vcs::GitConfigView, locale: &bisa_i18n::Locale) -> Self {
        Self {
            schema: bisa_vcs::GIT_CONFIG_KEYS
                .iter()
                .map(|d| GitConfigKeyDto {
                    key: d.key.to_string(),
                    kind: match d.kind {
                        bisa_vcs::ConfigKind::Text => GitConfigKindDto::Text,
                        bisa_vcs::ConfigKind::Bool => GitConfigKindDto::Bool,
                        bisa_vcs::ConfigKind::Choice(options) => GitConfigKindDto::Choice {
                            options: options.iter().map(|o| o.to_string()).collect(),
                        },
                    },
                    label: bisa_i18n::render(locale, &bisa_core::Text::new(d.message_id())),
                    hint: bisa_i18n::attribute(
                        locale,
                        &d.message_id(),
                        "hint",
                        &bisa_core::Text::new(d.message_id()),
                    )
                    .unwrap_or_default(),
                    scopes: d
                        .scopes
                        .iter()
                        .map(|s| match s {
                            bisa_vcs::ConfigScope::Global => GitConfigScopeDto::Global,
                            bisa_vcs::ConfigScope::Local => GitConfigScopeDto::Local,
                        })
                        .collect(),
                })
                .collect(),
            entries: v
                .entries
                .into_iter()
                .map(|e| GitConfigEntryDto {
                    effective: e.effective().map(str::to_string),
                    key: e.key,
                    local: e.local,
                    global: e.global,
                })
                .collect(),
        }
    }
}

/// `POST /workstreams/{wid}/git/commit`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkstreamCommitBody {
    pub message: String,
    /// Staged first, then committed. **Empty means "commit what is already
    /// staged"**, never "commit everything": a project's folder is the user's
    /// own working tree and may hold edits that have nothing to do with this
    /// commit.
    #[serde(default)]
    pub paths: Vec<String>,
}

/// One row of `GET /workstreams/{wid}/git/files`: a path and the two letters git
/// uses for it.
///
/// The letters are kept apart rather than folded into one verdict because a
/// file can be staged *and* modified again since, and a person staging by hand
/// is entitled to see that. `staged`/`unstaged` are the derived answers most
/// of the UI actually wants.
#[derive(Serialize, JsonSchema)]
pub struct GitFileRow {
    /// Relative to the repository root — hand it straight back to
    /// stage/unstage/diff.
    pub path: String,
    /// Where a rename or copy came from.
    pub old_path: Option<String>,
    /// Index against HEAD: `.` unmodified, `?` untracked, else git's letter
    /// (`M`, `A`, `D`, `R`, `C`, `T`, `U`).
    pub index: String,
    /// Worktree against the index, same alphabet.
    pub worktree: String,
    pub staged: bool,
    pub unstaged: bool,
    pub untracked: bool,
    /// An unmerged path. Neither staged nor unstaged: resolve it first.
    pub conflicted: bool,
    /// What kind of conflict, for an unmerged path: which side changed,
    /// added or deleted it (git's words — `us` is the current branch, under
    /// a rebase the branch rebased onto).
    pub conflict: Option<GitConflictKind>,
}

/// What kind of conflict an unmerged path is (`bisa_vcs::ConflictKind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GitConflictKind {
    BothModified,
    BothAdded,
    BothDeleted,
    DeletedByUs,
    DeletedByThem,
    AddedByUs,
    AddedByThem,
}

impl From<bisa_vcs::ConflictKind> for GitConflictKind {
    fn from(k: bisa_vcs::ConflictKind) -> Self {
        use bisa_vcs::ConflictKind as K;
        match k {
            K::BothModified => Self::BothModified,
            K::BothAdded => Self::BothAdded,
            K::BothDeleted => Self::BothDeleted,
            K::DeletedByUs => Self::DeletedByUs,
            K::DeletedByThem => Self::DeletedByThem,
            K::AddedByUs => Self::AddedByUs,
            K::AddedByThem => Self::AddedByThem,
        }
    }
}

/// `GET /workstreams/{wid}/git/conflict?path=`: a conflicted path whole —
/// its kind, its three sides from the index's stages (git's words: `ours`
/// is the current branch, under a rebase the branch rebased onto), and the
/// file as git wrote it on disk with its markers, for the block-by-block
/// resolution; `hash` guards the save that follows. A side that is not
/// there — deleted by one side, no base for a file added on both — is
/// `null`; `binary` when any side or the file is not text.
#[derive(Serialize, JsonSchema)]
pub struct GitConflict {
    pub workstream: String,
    pub path: String,
    pub kind: Option<GitConflictKind>,
    pub base: Option<String>,
    pub ours: Option<String>,
    pub theirs: Option<String>,
    pub text: Option<String>,
    pub hash: Option<String>,
    pub binary: bool,
    /// A side was cut at the cap.
    pub truncated: bool,
}

/// What a side of a half-done operation is (`bisa_vcs::SideRole`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GitSideRole {
    Branch,
    Upstream,
    Commit,
}

/// One side of a half-done operation, named as far as git can name it.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct GitSideRef {
    pub role: GitSideRole,
    /// The branch or remote-tracking branch, when the side is one.
    pub name: Option<String>,
    /// The commit's full sha, when known.
    pub commit: Option<String>,
    /// The commit's subject line, when known.
    pub subject: Option<String>,
}

/// Where a rebase, pick or revert of several commits stands: the step git
/// stopped on, one-based, of the total.
#[derive(Clone, Copy, Debug, Serialize, JsonSchema)]
pub struct GitStep {
    pub done: u32,
    pub total: u32,
}

/// `GET /workstreams/{wid}/git/operation`: the operation git has left
/// half-done, as facts read from its directory — so one started in a
/// terminal is described as well as one started here. `ours` and `theirs`
/// are git's: under a rebase `ours` is the branch rebased onto, `theirs`
/// the commit replayed. The desktop turns them into *mine* and *theirs*
/// once (`conflictSidesModel`).
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct GitOperationFacts {
    pub kind: GitInProgress,
    /// The branch the operation is on — under a rebase, the branch replayed.
    pub branch: Option<String>,
    pub ours: GitSideRef,
    pub theirs: GitSideRef,
    pub step: Option<GitStep>,
}

impl From<bisa_vcs::OperationFacts> for GitOperationFacts {
    fn from(f: bisa_vcs::OperationFacts) -> Self {
        let side = |s: bisa_vcs::SideRef| GitSideRef {
            role: match s.role {
                bisa_vcs::SideRole::Branch => GitSideRole::Branch,
                bisa_vcs::SideRole::Upstream => GitSideRole::Upstream,
                bisa_vcs::SideRole::Commit => GitSideRole::Commit,
            },
            name: s.name,
            commit: s.commit,
            subject: s.subject,
        };
        Self {
            kind: f.kind.into(),
            branch: f.branch,
            ours: side(f.ours),
            theirs: side(f.theirs),
            step: f.step.map(|s| GitStep {
                done: s.done,
                total: s.total,
            }),
        }
    }
}

/// `GET /workstreams/{wid}/git/merge-preview?source=`: what merging
/// `source` into HEAD would do, before anything moves — `supported` is
/// false on a git without `merge-tree --write-tree`, and then nothing is
/// foreseen.
#[derive(Clone, Debug, Default, Serialize, JsonSchema)]
pub struct GitMergePreview {
    pub workstream: String,
    pub source: String,
    pub supported: bool,
    pub clean: bool,
    /// The paths that would conflict.
    pub paths: Vec<String>,
}

/// `POST /workstreams/{wid}/pr`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrBody {
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    /// Only when the code host has draft pull requests.
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub reviewers: Vec<String>,
    #[serde(default)]
    pub labels: Vec<String>,
}

/// `POST /workstreams/{wid}/pr/review`. `body` is the summary — required, in
/// words, for a `comment` or a `request_changes`; optional beside an `approve`.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "PrReview")]
#[serde(deny_unknown_fields)]
pub struct PrReviewBody {
    pub event: bisa_engine::codehost::ReviewEvent,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub comments: Vec<bisa_engine::codehost::ReviewComment>,
}

/// `POST /workstreams/{wid}/pr/threads/{thread_id}/resolve`.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "PrThreadResolve")]
#[serde(deny_unknown_fields)]
pub struct PrThreadResolveBody {
    pub resolved: bool,
}

/// `POST /workstreams/{wid}/pr/threads/{thread_id}/reply`: the person's
/// words on the thread, and whether the thread is resolved in the same act.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "PrThreadReply")]
#[serde(deny_unknown_fields)]
pub struct PrThreadReplyBody {
    pub body: String,
    #[serde(default)]
    pub resolve: bool,
}

/// `POST /workstreams/{wid}/pr/merge`.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "PrMerge")]
#[serde(deny_unknown_fields)]
pub struct PrMergeBody {
    pub strategy: bisa_engine::codehost::MergeStrategy,
    /// Delete the head branch on the code host once the merge lands (the merge
    /// dialog's toggle, defaulting from `git.delete_branch_after_merge`).
    #[serde(default)]
    pub delete_branch: bool,
}

/// `PUT /codehost/{kind}/accounts` — a token, verified with the host first
/// and stored under the login the host answers; written once, never read
/// back. `login` is the account a Bitbucket token belongs to (a Basic
/// credential); GitHub and GitLab answer it themselves.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "AddAccount")]
#[serde(deny_unknown_fields)]
pub struct AddAccountBody {
    pub token: String,
    #[serde(default)]
    pub login: Option<String>,
}

/// `PUT /codehost/{kind}/default` — the kind's default account (the global
/// `codehost.<kind>.account`), or `null` to clear it.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "DefaultAccount")]
#[serde(deny_unknown_fields)]
pub struct DefaultAccountBody {
    #[serde(default)]
    pub login: Option<String>,
}

/// `POST /codehost/inspect` — what a remote is before a project exists: a
/// URL about to be cloned, or a folder the person picked. One of the two.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "Inspect")]
#[serde(deny_unknown_fields)]
pub struct InspectBody {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
}

/// `PUT /workstreams/{wid}/git/account` — pin one repository to an account
/// (its local `codehost.account`), or `null` to unpin it.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "AccountPin")]
#[serde(deny_unknown_fields)]
pub struct AccountPinBody {
    #[serde(default)]
    pub login: Option<String>,
}

/// `POST /git/ssh/test` — one handshake with a git host: `user@host`, with
/// `key` when a profile's key should be the one offered.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "SshTest")]
#[serde(deny_unknown_fields)]
pub struct SshTestBody {
    pub host: String,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
}

/// `PUT /decisions/key/{provider}` — a remote decision provider's API key. It
/// goes to this machine's keystore and is never read back.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "DecisionKeyRequest")]
#[serde(deny_unknown_fields)]
pub struct DecisionKeyBody {
    pub key: String,
}

/// `GET /decisions` — how many of the newest judgements to read.
#[derive(Deserialize, JsonSchema)]
pub struct RecentJudgementsQuery {
    #[serde(default)]
    pub limit: Option<usize>,
}

/// `POST /security/redact-preview` — text to try the redaction rules on. It
/// stays on the node and teaches the real vault nothing.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "RedactPreviewRequest")]
#[serde(deny_unknown_fields)]
pub struct RedactPreviewBody {
    pub text: String,
}

/// `POST /security/guard-preview` — a tool call to try the guard rules on:
/// the tool's name and its input as the harness would carry it
/// (`{"command": "…"}` for a shell, `{"file_path": "…"}` for a file tool).
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "GuardPreviewRequest")]
#[serde(deny_unknown_fields)]
pub struct GuardPreviewBody {
    pub tool: String,
    #[serde(default)]
    pub input: serde_json::Value,
}

/// `POST /sessions/{id}/guard` — one `PreToolUse` hook payload from a
/// terminal-hosted harness, as the harness wrote it.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "GuardHookRequest")]
#[serde(deny_unknown_fields)]
pub struct GuardHookBody {
    pub payload: serde_json::Value,
}

/// `GET /projects/{pid}/workstream-scripts` — the three scripts and whether
/// this machine will run each (ide/07 §Workstream scripts).
#[derive(Serialize, JsonSchema)]
pub struct WorkstreamScriptsView {
    pub project: String,
    /// `workstreams.script.timeout_secs`, resolved for the project.
    pub timeout_secs: u64,
    pub scripts: Vec<bisa_engine::scripts::ScriptStatus>,
}

/// `POST /workstreams/{wid}/git/resolve` — a conflicted path settled: with a
/// `side`, that side is taken whole (consented — the tree moves); without,
/// the merged text the person saved is staged, which is *Mark resolved*.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "GitResolve")]
#[serde(deny_unknown_fields)]
pub struct ResolveBody {
    pub path: String,
    /// How the path is settled whole, by git; absent, the merged text
    /// already saved is staged (index only).
    #[serde(default)]
    pub take: Option<ResolutionBody>,
}

/// How a conflicted path is settled whole, in git's words: `ours` is the
/// branch the operation runs on — during a rebase, the branch rebased onto
/// — `theirs` the other side; `delete` removes the path, the answer to a
/// side that deleted it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "GitResolution")]
pub enum ResolutionBody {
    Ours,
    Theirs,
    Delete,
}

impl From<ResolutionBody> for bisa_vcs::Resolution {
    fn from(r: ResolutionBody) -> Self {
        match r {
            ResolutionBody::Ours => bisa_vcs::Resolution::Ours,
            ResolutionBody::Theirs => bisa_vcs::Resolution::Theirs,
            ResolutionBody::Delete => bisa_vcs::Resolution::Delete,
        }
    }
}

/// A working tree's git state — counts and pointers, never file contents.
///
/// Neutral (`git: false`, everything zero, `clean: true`) for a project with
/// no repository, which is a legitimate kind of project rather than an error.
#[derive(Serialize, Default, Clone, JsonSchema)]
pub struct GitStatusInfo {
    /// The path is a git repository *on disk* — the disk is the authority,
    /// not the record.
    pub git: bool,
    /// The folder is there at all.
    pub exists: bool,
    /// Short branch name; absent when HEAD is detached or unborn.
    pub branch: Option<String>,
    pub detached: bool,
    /// HEAD's commit; absent in a repository with no commits yet.
    pub head: Option<String>,
    pub upstream: Option<String>,
    /// `origin`'s URL, when there is one.
    pub remote: Option<String>,
    /// Commits HEAD has that the upstream lacks (zero without an upstream).
    pub ahead: u32,
    /// Commits the upstream has that HEAD lacks (zero without an upstream).
    pub behind: u32,
    pub staged: u32,
    pub unstaged: u32,
    pub untracked: u32,
    pub conflicted: u32,
    pub clean: bool,
    /// A merge, rebase, cherry-pick or revert git has left half-done here.
    pub in_progress: Option<GitInProgress>,
    /// Why git could not be asked, when it could not be.
    pub error: Option<String>,
}

/// One changed file in a workstream, against HEAD.
#[derive(Serialize, Clone, JsonSchema)]
pub struct ChangedFile {
    pub path: String,
    pub old_path: Option<String>,
    /// added | modified | deleted | renamed | copied | type-changed | unmerged
    /// | unknown
    pub kind: String,
    pub insertions: u64,
    pub deletions: u64,
    pub binary: bool,
}

/// A row of `GET /goals/{id}/projects` — and of `GET /projects`, which
/// answers the workspace-wide question with the same shape.
///
/// A project belongs to the workspace, not to a goal; `goals` is
/// the whole of its relationship to any of them — the attachments — and a
/// project with none is a standalone project, which is an ordinary thing to
/// be.
#[derive(Serialize, JsonSchema)]
pub struct ProjectRow {
    pub project: bisa_core::Project,
    /// Where the files are — resolved, so an `External` root shows its real
    /// path rather than a managed one that does not exist.
    pub path: String,
    pub exists: bool,
    /// Workstreams recorded against this project, in any state.
    pub workstreams: usize,
    /// Every goal this project is attached to.
    pub goals: Vec<String>,
}

// ---------------------------------------------------------------------------
// Listening, listeners and signals
// ---------------------------------------------------------------------------

/// `PUT /workflows/{wfid}/listening` — turn a library workflow On: what its
/// event runs bind, and what each may spend.
#[derive(Deserialize, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListeningBody {
    /// The inputs its start events read and do not map
    /// (`WorkflowRow.listening_needs`), and any other it declares.
    #[serde(default)]
    pub inputs: BTreeMap<String, serde_json::Value>,
    /// Each run's own ceiling; absent, the workspace default
    /// (`budget.default.*`); `{}`, no ceiling whatever the default.
    #[serde(default)]
    pub budget: Option<bisa_core::Budget>,
}

/// `PUT /goals/{id}/listening` — the goal listens, or listens again after a
/// failed run or a spent budget paused it. Its runs spend against the goal's
/// budget, so it names none.
#[derive(Deserialize, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GoalListeningBody {
    /// What it listens with; absent, what it listened with before.
    #[serde(default)]
    pub inputs: Option<BTreeMap<String, serde_json::Value>>,
}

/// `POST /signals` — raise a named signal: heard by the starts, waits and
/// boundary events that name it.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EmitSignalBody {
    /// Dot-separated parts of lowercase letters, digits, `-` and `_`
    /// (`report.ready`).
    pub name: String,
    #[serde(default)]
    pub payload: serde_json::Value,
    /// The goal it is raised on; absent, the workspace.
    #[serde(default)]
    pub goal: Option<bisa_core::GoalId>,
}

/// One listener — a start event armed for a host — as `GET /listeners` and
/// the listening routes list it: what it listens for, when it next comes
/// due, what waits behind it, and how it is called when it is a hook.
#[derive(Serialize, JsonSchema)]
pub struct ListenerView {
    pub listener: bisa_core::ListenerKey,
    /// `workspace:<WorkflowId>` or `goal:<GoalId>`.
    pub host: bisa_core::ListenerHost,
    pub step: StepId,
    /// The start's event word (`StartOn::as_str`).
    pub event: String,
    pub summary: bisa_core::Text,
    /// When a timed or polled start next comes due, unix seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_due: Option<u64>,
    /// When it last heard its event, unix seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_fired_at: Option<u64>,
    /// Occurrences waiting in the durable backlog.
    pub backlog: usize,
    /// Runs it started that are still going.
    pub live_runs: u32,
    /// A hook start's local route, called under the control-plane token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_hook: Option<String>,
    /// A public hook start's route and whether its secret is minted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_hook: Option<PublicHook>,
    /// Why it could not be armed, when it could not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed: Option<String>,
}

/// A public hook start's route — a path, never a URL: the host is whatever
/// tunnel the operator put in front of the node — and whether the secret
/// that guards it is minted. Never the secret.
#[derive(Serialize, JsonSchema)]
pub struct PublicHook {
    pub path: String,
    pub has_secret: bool,
}

/// What a listener's view is built from beyond its start step: what the
/// registry armed of it, and what the store remembers.
pub struct ListenerFacts {
    /// The start's event as the registry armed it — every template rendered
    /// and every input read; `None` for a start that is not armed.
    pub armed: Option<bisa_core::StartOn>,
    /// Its runtime memory: when it next comes due, why it could not be armed.
    pub runtime: bisa_store::ListenerRuntime,
    /// When it last heard its event.
    pub last_signal_at: Option<u64>,
    pub backlog: usize,
    pub live_runs: u32,
    /// Whether its hook secret is minted.
    pub has_secret: bool,
}

impl ListenerView {
    /// The listener `key` names — a start of its host's workflow, beginning
    /// on `declared` — from what is known of it. It reads as it is armed
    /// when it is, and as its step declares it otherwise; only an armed
    /// listener comes due.
    pub fn of(
        key: bisa_core::ListenerKey,
        declared: &bisa_core::StartOn,
        facts: ListenerFacts,
    ) -> Self {
        use bisa_core::StartOn;
        let on = facts.armed.as_ref().unwrap_or(declared);
        let (local_hook, public_hook) = match on {
            StartOn::Hook { public } => (
                Some(local_hook_path(&key)),
                (*public).then(|| PublicHook {
                    path: bisa_engine::listen::turn::public_path(&key.host, &key.step),
                    has_secret: facts.has_secret,
                }),
            ),
            StartOn::Manual
            | StartOn::Schedule { .. }
            | StartOn::Message { .. }
            | StartOn::Signal { .. }
            | StartOn::Project { .. }
            | StartOn::Run { .. }
            | StartOn::Platform { .. }
            | StartOn::Connector { .. }
            | StartOn::Check { .. } => (None, None),
        };
        Self {
            host: key.host,
            step: key.step.clone(),
            event: on.as_str().to_string(),
            summary: on.summary(),
            next_due: facts.armed.as_ref().and(facts.runtime.next_due),
            last_fired_at: facts.last_signal_at,
            backlog: facts.backlog,
            live_runs: facts.live_runs,
            local_hook,
            public_hook,
            failed: facts.runtime.failed,
            listener: key,
        }
    }
}

/// The route a hook start is called on from this machine, under the
/// control-plane token.
pub fn local_hook_path(key: &bisa_core::ListenerKey) -> String {
    match key.host {
        bisa_core::ListenerHost::Workspace { workflow } => {
            format!("/workflows/{workflow}/hooks/{}", key.step)
        }
        bisa_core::ListenerHost::Goal { goal } => format!("/goals/{goal}/hooks/{}", key.step),
    }
}

/// Where a signal stands in the queue, as `GET /signals` reports it. Mirrors
/// the store's `SignalState` one variant at a time so the wire contract is a
/// decision here rather than a leak of an internal enum.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "SignalState")]
pub enum SignalStateDto {
    Queued,
    Running,
    /// Kept behind a run of its listener that is still going, or over its
    /// listener's rate.
    Waiting,
    /// Waiting on a person: an outside payload the content screen would not
    /// pass. `POST /signals/{id}/release` lets it through.
    Held,
    /// It began its run — or, a named signal with no listener, it is kept
    /// for the waits that replay it.
    Done,
    Skipped,
    Failed,
}

impl From<bisa_store::SignalState> for SignalStateDto {
    fn from(s: bisa_store::SignalState) -> Self {
        use bisa_store::SignalState as S;
        match s {
            S::Queued => Self::Queued,
            S::Running => Self::Running,
            S::Waiting => Self::Waiting,
            S::Held => Self::Held,
            S::Done => Self::Done,
            S::Skipped => Self::Skipped,
            S::Failed => Self::Failed,
        }
    }
}

/// One queued or settled occurrence, as `GET /signals` lists it: the fact
/// and where it stands — never its payload, which a run's event carries.
#[derive(Serialize, JsonSchema)]
pub struct SignalView {
    pub id: String,
    /// The listener it was raised for; absent for a named signal kept for
    /// the waits that replay it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listener: Option<bisa_core::ListenerKey>,
    pub source: bisa_core::SignalSource,
    /// A named signal's name, or a platform topic.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub at: u64,
    pub state: SignalStateDto,
    pub scope: bisa_core::SignalScope,
    /// Why it was skipped, failed, or is waiting or held, when it says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl From<bisa_store::QueuedSignal> for SignalView {
    fn from(q: bisa_store::QueuedSignal) -> Self {
        Self {
            id: q.signal.id,
            listener: q.signal.listener,
            source: q.signal.source,
            name: q.signal.name,
            at: q.signal.at,
            state: q.state.into(),
            scope: q.signal.scope,
            note: q.note,
        }
    }
}

// ---------------------------------------------------------------------------
// Usage: what points at an object
// ---------------------------------------------------------------------------

/// What kind of thing holds a reference, as `GET /usage/{kind}/{id}` reports
/// it. Mirrors the store's `ReferenceKind` one variant at a time so the wire
/// contract is a decision here rather than a leak of an internal enum.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "ReferenceKind")]
pub enum ReferenceKindDto {
    Agent,
    Team,
    Channel,
    Goal,
    Project,
    WorkItem,
    /// A workflow's step names this object — a start event's, too — or a
    /// goal runs it.
    Workflow,
    /// A `Listed` gate policy names this object; the `id` is the gate.
    Governance,
    /// A connector account on this machine still uses the connector.
    Account,
}

impl From<bisa_store::ReferenceKind> for ReferenceKindDto {
    fn from(k: bisa_store::ReferenceKind) -> Self {
        use bisa_store::ReferenceKind as K;
        match k {
            K::Agent => Self::Agent,
            K::Team => Self::Team,
            K::Channel => Self::Channel,
            K::Goal => Self::Goal,
            K::Project => Self::Project,
            K::WorkItem => Self::WorkItem,
            K::Workflow => Self::Workflow,
            K::Governance => Self::Governance,
            K::Account => Self::Account,
        }
    }
}

/// One place an object is referenced. A row of `GET /usage/{kind}/{id}`, and
/// one of the things the matching `DELETE` refuses over.
#[derive(Serialize, JsonSchema)]
#[schemars(rename = "Reference")]
pub struct ReferenceDto {
    pub kind: ReferenceKindDto,
    /// The holder's own id — where a client navigates to detach.
    pub id: String,
    /// What a person reads: the team's name, the goal's title, the
    /// workflow's name. A row that renders as a bare ULID tells the reader
    /// nothing.
    pub label: String,
    /// In motion: a goal whose unfinished run is of this workflow. The
    /// designer is read-only while any holder is live.
    pub live: bool,
}

impl From<bisa_store::Reference> for ReferenceDto {
    fn from(r: bisa_store::Reference) -> Self {
        Self {
            kind: r.kind.into(),
            id: r.id,
            label: r.label,
            live: r.live,
        }
    }
}

// ---------------------------------------------------------------------------
// Files: the folders a workspace owns
// ---------------------------------------------------------------------------
//
// These five mirror `bisa_store::tree`'s types one field at a time, for
// the reason the catalog DTOs give above: the store does not derive
// `JsonSchema`, and a file browser is about as structural as a surface gets,
// so the shapes are declared here rather than emitted as ad-hoc JSON the
// desktop would then hand-mirror. `From` is the only conversion, so a field
// added to the store fails to compile here instead of going missing on the
// wire.

// Which rooted thing a path is read under — the three values `/tree`, `/file`
// and `/placement` accept in their first path segment — is the core's
// `FileScope`, which derives its own schema: an artifact's source names one
// on the wire, so there is one spelling of it.

/// What an entry *is*, not merely what it is called.
///
/// This is the point of the tree route. A goal's folder annotated this way
/// reads as the life of that goal — what it plans against, where its work
/// ran, what it produced — rather than as a directory dump. `dir` and `file`
/// are the honest fallback for everything inside somebody's own source tree.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "FileEntryKind")]
pub enum FileEntryKindDto {
    /// Where a goal's sessions run when the work names no project.
    Work,
    /// A note's truth file.
    Note,
    /// A work item's captured result: the patch a copy workstream left.
    Result,
    /// The signed event log.
    Journal,
    /// An addressable snapshot.
    State,
    /// A file a person gave the goal as context (`documents/`).
    Document,
    Dir,
    File,
}

impl From<bisa_store::EntryKind> for FileEntryKindDto {
    fn from(k: bisa_store::EntryKind) -> Self {
        use bisa_store::EntryKind as K;
        match k {
            K::Work => Self::Work,
            K::Note => Self::Note,
            K::Result => Self::Result,
            K::Journal => Self::Journal,
            K::State => Self::State,
            K::Document => Self::Document,
            K::Dir => Self::Dir,
            K::File => Self::File,
        }
    }
}

/// One row of `GET /tree/{scope}/{id}`.
#[derive(Serialize, JsonSchema)]
#[schemars(rename = "FileEntry")]
pub struct FileEntryDto {
    /// Relative to the scope's root and `/`-separated — always to the root,
    /// never to whatever sub-path was listed, so a client hands it straight
    /// back as `?path=`.
    pub path: String,
    pub name: String,
    pub kind: FileEntryKindDto,
    pub dir: bool,
    /// Listed, never descended into. Omitting it would make the listing lie
    /// about what is in the directory; following it would leave the root.
    pub symlink: bool,
    /// Files only.
    pub size: Option<u64>,
    /// Seconds since the epoch, when the filesystem will say.
    pub modified: Option<u64>,
    /// Matched by the root's ignore rules: dimmed by the explorer, never hidden.
    pub ignored: bool,
}

impl From<bisa_store::FileEntry> for FileEntryDto {
    fn from(e: bisa_store::FileEntry) -> Self {
        Self {
            path: e.path,
            name: e.name,
            kind: e.kind.into(),
            dir: e.dir,
            symlink: e.symlink,
            size: e.size,
            modified: e.modified,
            ignored: e.ignored,
        }
    }
}

/// `GET /tree/{scope}/{id}` — a bounded listing under one root.
#[derive(Serialize, JsonSchema)]
#[schemars(rename = "FileTree")]
pub struct FileTreeDto {
    pub scope: bisa_core::FileScope,
    pub id: String,
    /// The absolute root the listing is relative to. The same path
    /// `/placement/{scope}/{id}` reports, so the two routes can be checked
    /// against each other.
    pub root: String,
    /// What was listed, relative to `root`. Empty means the root itself.
    pub path: String,
    /// How deep this listing actually went, after clamping.
    pub depth: usize,
    /// The entry budget cut this listing: what is here is not all of it.
    pub truncated: bool,
    /// There are entries below the depth asked for. A one-level listing of a
    /// folder with subfolders says so; a client lists them when opened.
    pub deeper: bool,
    pub entries: Vec<FileEntryDto>,
}

impl FileTreeDto {
    pub(crate) fn new(scope: bisa_store::FileScope, id: &str, t: bisa_store::FileTree) -> Self {
        Self {
            scope,
            id: id.to_string(),
            root: t.root.display().to_string(),
            path: t.path,
            depth: t.depth,
            truncated: t.truncated,
            deeper: t.deeper,
            entries: t.entries.into_iter().map(FileEntryDto::from).collect(),
        }
    }
}

/// `GET /file/{scope}/{id}?path=` — one file, bounded.
#[derive(Serialize, JsonSchema)]
#[schemars(rename = "FileContent")]
pub struct FileContentDto {
    /// As asked for, relative to the root — not the absolute path, which would
    /// put the workspace's location into every response.
    pub path: String,
    /// The file's real length, which is larger than `text` when `truncated`.
    pub size: u64,
    /// No bytes are served for a binary file: there is nothing a caller could
    /// do with a JSON string of a PNG except render mojibake. Detected from the
    /// bytes, never from the extension.
    pub binary: bool,
    pub truncated: bool,
    pub text: Option<String>,
}

impl From<bisa_store::FileContent> for FileContentDto {
    fn from(c: bisa_store::FileContent) -> Self {
        Self {
            path: c.path,
            size: c.size,
            binary: c.binary,
            truncated: c.truncated,
            text: c.text,
        }
    }
}

/// `GET /placement/{scope}/{id}` — where a scope's files live.
///
/// `exists: false` is an answer rather than an error. A project whose folder
/// has not been created and a project id nothing knows about are different
/// problems, and this is the route that keeps them apart.
#[derive(Serialize, JsonSchema)]
#[schemars(rename = "Placement")]
pub struct PlacementDto {
    pub path: String,
    pub exists: bool,
}

impl From<bisa_store::Placement> for PlacementDto {
    fn from(p: bisa_store::Placement) -> Self {
        Self {
            path: p.path.display().to_string(),
            exists: p.exists,
        }
    }
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

/// One value a `Choice` setting may take: the word stored, and the word shown.
#[derive(Serialize, JsonSchema)]
#[schemars(rename = "SettingChoice")]
pub struct SettingChoiceDto {
    /// The value, as the registry and a write spell it.
    pub value: String,
    /// The value's word in the request's language (`setting-<key>.choice-<value>`).
    pub label: String,
}

/// What a setting may hold — the registry's `Kind`, on the wire.
#[derive(Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
#[schemars(rename = "SettingKind")]
pub enum SettingKindDto {
    Bool,
    Integer { min: i64, max: i64 },
    Number { min: f64, max: f64 },
    Text,
    Choice { choices: Vec<SettingChoiceDto> },
    Structured,
}

/// One registry entry, for a panel generated from the registry rather than
/// written by hand (`GET /settings/registry`). Its `label` and `help` are the
/// catalog's message `setting-<key>` in the request's language
/// (17 — Internationalisation); the registry itself carries no words.
#[derive(Serialize, JsonSchema)]
#[schemars(rename = "SettingDef")]
pub struct SettingDefDto {
    pub key: String,
    /// The key's first segment: `editor`, `terminal`, `git`, …
    pub group: String,
    pub kind: SettingKindDto,
    pub default: serde_json::Value,
    /// The scopes that may hold a value: `machine`, `workspace`, `project`.
    pub scopes: Vec<bisa_core::SettingScope>,
    /// The label, in the request's language.
    pub label: String,
    /// The sentence under the label, in the request's language; empty when the key has none.
    pub help: String,
}

impl SettingDefDto {
    /// One entry with its words rendered in `locale`.
    pub fn localized(d: &bisa_core::SettingDef, locale: &bisa_i18n::Locale) -> Self {
        use bisa_core::settings::Kind;
        let text = d.text();
        let id = d.message_id();
        Self {
            key: d.key.to_string(),
            group: d.group().to_string(),
            kind: match &d.kind {
                Kind::Bool => SettingKindDto::Bool,
                Kind::Integer { min, max } => SettingKindDto::Integer {
                    min: *min,
                    max: *max,
                },
                Kind::Number { min, max } => SettingKindDto::Number {
                    min: *min,
                    max: *max,
                },
                Kind::Text => SettingKindDto::Text,
                Kind::Choice(c) => SettingKindDto::Choice {
                    choices: c
                        .iter()
                        .map(|value| SettingChoiceDto {
                            value: value.to_string(),
                            label: bisa_i18n::attribute(
                                locale,
                                &id,
                                &format!("choice-{value}"),
                                &text,
                            )
                            .unwrap_or_else(|| value.to_string()),
                        })
                        .collect(),
                },
                Kind::Structured => SettingKindDto::Structured,
            },
            default: d.default.clone(),
            scopes: d.scopes.list(),
            label: bisa_i18n::render(locale, &text),
            help: bisa_i18n::attribute(locale, &id, "help", &text).unwrap_or_default(),
        }
    }
}

/// `POST /mobile-development/simulators`: the simulator to make (ide/19) — a device type
/// and a runtime the status lists.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "CreateSimulator")]
#[serde(deny_unknown_fields)]
pub struct CreateSimulatorBody {
    pub name: String,
    pub devicetype: String,
    pub runtime: String,
}

/// `PUT /settings/{scope}`: the keys to write at that scope.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "SettingsWrite")]
#[serde(deny_unknown_fields)]
pub struct SettingsWriteBody {
    pub values: std::collections::BTreeMap<String, serde_json::Value>,
}

/// `POST /network/check`: the URL to reach once through the outbound client.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "NetworkCheckRequest")]
#[serde(deny_unknown_fields)]
pub struct NetworkCheckBody {
    pub url: String,
}

// ---------------------------------------------------------------------------
// The IDE's file routes (ide/03)
// ---------------------------------------------------------------------------

/// `GET /ide/file/{scope}/{id}?path=` — a file for the editor.
#[derive(Serialize, JsonSchema)]
#[schemars(rename = "IdeFile")]
pub struct IdeFileDto {
    pub scope: String,
    pub id: String,
    pub path: String,
    pub size: u64,
    pub binary: bool,
    pub truncated: bool,
    /// The text, when the file is text and within the cap.
    pub text: Option<String>,
    /// sha256 of the served bytes — the `base_hash` a save must carry.
    pub hash: Option<String>,
    /// Within the editable size; above it the client opens read-only.
    pub editable: bool,
}

/// `GET /workstreams/{wid}/git/sides?path=&staged=` — the two whole texts
/// of one file's change, for a comparison (ide/04 §The Changes view): the
/// index against the working tree, or HEAD against the index. A side git
/// does not hold — a new file's left, a deleted file's right — is `null`;
/// a binary side says so and carries no text; a side over the cap is cut
/// on a line and says `truncated`.
#[derive(Serialize, JsonSchema)]
pub struct FileSides {
    pub workstream: String,
    pub path: String,
    pub staged: bool,
    pub original: Option<String>,
    pub modified: Option<String>,
    /// Either side is not text.
    pub binary: bool,
    /// Either side was cut at the cap.
    pub truncated: bool,
}

/// `GET /workstreams/{wid}/git/commit/{sha}/diff?path=` — one file's patch
/// in one commit, against the first parent (ide/05): the *Hunks* view of a
/// commit's file. Cut at 2 MiB on a line, `truncated` says so.
#[derive(Serialize, JsonSchema)]
pub struct CommitFileDiff {
    pub workstream: String,
    pub sha: String,
    pub path: String,
    pub diff: String,
    pub truncated: bool,
}

/// `GET /workstreams/{wid}/git/commit/{sha}/sides?path=` — the two whole
/// texts of one file's change in one commit, for a comparison (ide/05): the
/// file as the first parent held it — at its old path for a rename — against
/// the file as the commit holds it. A side that is not there — a root
/// commit's or a new file's left, a deleted file's right — is `null`; a
/// binary side says so and carries no text; a side over the cap is cut on a
/// line and says `truncated`. The same shape as `FileSides` past the keys,
/// so one reader draws both.
#[derive(Serialize, JsonSchema)]
pub struct CommitFileSides {
    pub workstream: String,
    pub sha: String,
    pub path: String,
    pub original: Option<String>,
    pub modified: Option<String>,
    /// Either side is not text.
    pub binary: bool,
    /// Either side was cut at the cap.
    pub truncated: bool,
}

/// `PUT /ide/file/{scope}/{id}?path=`.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "IdeWriteFile")]
#[serde(deny_unknown_fields)]
pub struct IdeWriteBody {
    pub text: String,
    /// Absent means create — refused if the path exists.
    #[serde(default)]
    pub base_hash: Option<String>,
}

/// `POST /workstreams/{wid}/git/hunk` — one hunk, or the lines of one a person
/// picked, as a unified diff. `reverse` takes it back out of the index.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "GitHunkApply")]
#[serde(deny_unknown_fields)]
pub struct HunkBody {
    pub patch: String,
    #[serde(default)]
    pub reverse: bool,
}

/// `POST /projects/{pid}/review` — annotate a hunk.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "ReviewNoteCreate")]
#[serde(deny_unknown_fields)]
pub struct ReviewNoteBody {
    /// Project-relative path of the file the hunk is in.
    pub path: String,
    /// 1-based, inclusive.
    pub start: u32,
    pub end: u32,
    pub scope: bisa_core::DiffScope,
    /// The hunk text as displayed; hashed into the note's identity and
    /// carried as the chip an agent sees.
    #[serde(default)]
    pub hunk: String,
    pub body: String,
    #[serde(default)]
    pub workstream: Option<String>,
}

/// `PATCH /projects/{pid}/review/{id}`.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "ReviewNoteEdit")]
#[serde(deny_unknown_fields)]
pub struct ReviewNoteEditBody {
    pub body: String,
}

/// `POST /projects/{pid}/review/send` — `ids` empty means every unsent note.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "ReviewNotesSend")]
#[serde(deny_unknown_fields)]
pub struct ReviewSendBody {
    #[serde(default)]
    pub ids: Vec<String>,
}

/// `POST /workstreams/{wid}/git/checkout`.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "GitCheckout")]
#[serde(deny_unknown_fields)]
pub struct CheckoutBody {
    /// A branch (attached) or a commit (detached).
    pub target: String,
}

/// `POST /workstreams/{wid}/git/branches`.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "GitBranchCreate")]
#[serde(deny_unknown_fields)]
pub struct BranchCreateBody {
    pub name: String,
    /// Start point; HEAD when absent.
    #[serde(default)]
    pub start: Option<String>,
    /// Follow `start` — a remote branch — as the upstream (`--track`).
    #[serde(default)]
    pub track: bool,
    /// Check the new branch out too. That half is consented.
    #[serde(default)]
    pub switch: bool,
}

/// `PUT /workstreams/{wid}/git/branches/{name}/upstream`.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "GitBranchUpstream")]
#[serde(deny_unknown_fields)]
pub struct UpstreamBody {
    /// The remote branch to follow — `origin/main` — or null for none.
    #[serde(default)]
    pub upstream: Option<String>,
}

/// `POST /workstreams/{wid}/git/branches/{name}/rename`.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "GitBranchRename")]
#[serde(deny_unknown_fields)]
pub struct BranchRenameBody {
    pub to: String,
}

/// `POST /workstreams/{wid}/git/tags`.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "GitTagCreate")]
#[serde(deny_unknown_fields)]
pub struct TagCreateBody {
    pub name: String,
    #[serde(default)]
    pub target: Option<String>,
    /// Present makes an annotated tag.
    #[serde(default)]
    pub message: Option<String>,
}

/// `POST /workstreams/{wid}/git/remotes`.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "GitRemoteAdd")]
#[serde(deny_unknown_fields)]
pub struct RemoteAddBody {
    pub name: String,
    pub url: String,
}

/// `POST /workstreams/{wid}/git/rebase`: the current branch replayed onto
/// `upstream` — or, with `onto`, only its commits since `upstream` replayed
/// onto `onto` — the dirty tree carried across with `autostash`.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "GitRebase")]
#[serde(deny_unknown_fields)]
pub struct RebaseBody {
    pub upstream: String,
    #[serde(default)]
    pub onto: Option<String>,
    #[serde(default)]
    pub autostash: bool,
}

impl From<RebaseBody> for bisa_vcs::RebaseRequest {
    fn from(b: RebaseBody) -> Self {
        bisa_vcs::RebaseRequest {
            upstream: b.upstream,
            onto: b.onto,
            autostash: b.autostash,
        }
    }
}

/// What happens to one commit of an interactive rebase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "GitRebaseAction")]
pub enum RebaseActionBody {
    Pick,
    Reword,
    Squash,
    Fixup,
    Drop,
}

impl From<RebaseActionBody> for bisa_vcs::RebaseAction {
    fn from(a: RebaseActionBody) -> Self {
        use bisa_vcs::RebaseAction as A;
        match a {
            RebaseActionBody::Pick => A::Pick,
            RebaseActionBody::Reword => A::Reword,
            RebaseActionBody::Squash => A::Squash,
            RebaseActionBody::Fixup => A::Fixup,
            RebaseActionBody::Drop => A::Drop,
        }
    }
}

/// One step of an interactive rebase: the commit, its action, and — for a
/// reword or a squash the person wrote words for — the message.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "GitRebaseStep")]
#[serde(deny_unknown_fields)]
pub struct RebaseStepBody {
    pub action: RebaseActionBody,
    pub commit: String,
    #[serde(default)]
    pub message: Option<String>,
}

/// `POST /workstreams/{wid}/git/rebase/plan`: an interactive rebase planned
/// in full — every commit since `upstream`, oldest first, each with its action.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "GitRebasePlan")]
#[serde(deny_unknown_fields)]
pub struct RebasePlanBody {
    pub upstream: String,
    #[serde(default)]
    pub onto: Option<String>,
    pub steps: Vec<RebaseStepBody>,
}

impl From<RebasePlanBody> for bisa_vcs::RebasePlan {
    fn from(b: RebasePlanBody) -> Self {
        bisa_vcs::RebasePlan {
            upstream: b.upstream,
            onto: b.onto,
            steps: b
                .steps
                .into_iter()
                .map(|s| bisa_vcs::RebaseStep {
                    action: s.action.into(),
                    commit: s.commit,
                    message: s.message,
                })
                .collect(),
        }
    }
}

/// How a merge lands: git's default (a fast-forward when it can), always a
/// merge commit, a fast-forward or a refusal, or every change squashed into
/// the index for one commit of the person's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "GitMergeMode")]
pub enum MergeModeBody {
    #[default]
    Ff,
    NoFf,
    FfOnly,
    Squash,
}

impl From<MergeModeBody> for bisa_vcs::MergeMode {
    fn from(m: MergeModeBody) -> Self {
        use bisa_vcs::MergeMode as M;
        match m {
            MergeModeBody::Ff => M::Ff,
            MergeModeBody::NoFf => M::NoFf,
            MergeModeBody::FfOnly => M::FfOnly,
            MergeModeBody::Squash => M::Squash,
        }
    }
}

/// `POST /workstreams/{wid}/git/merge`.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "GitMerge")]
#[serde(deny_unknown_fields)]
pub struct MergeBody {
    pub source: String,
    #[serde(default)]
    pub mode: MergeModeBody,
    /// The merge commit's message, when there is one to make.
    #[serde(default)]
    pub message: Option<String>,
}

/// `POST /workstreams/{wid}/git/cherry-pick`: the commits, oldest first.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "GitCherryPick")]
#[serde(deny_unknown_fields)]
pub struct CherryPickBody {
    pub commits: Vec<String>,
    /// Add the *(cherry picked from …)* line (`-x`).
    #[serde(default)]
    pub record_origin: bool,
    /// Leave the change staged for one commit of the person's.
    #[serde(default)]
    pub no_commit: bool,
    /// The parent a merge commit is picked against (`-m`, 1-based).
    #[serde(default)]
    pub mainline: Option<u32>,
}

impl From<CherryPickBody> for bisa_vcs::PickRequest {
    fn from(b: CherryPickBody) -> Self {
        bisa_vcs::PickRequest {
            commits: b.commits,
            record_origin: b.record_origin,
            no_commit: b.no_commit,
            mainline: b.mainline,
        }
    }
}

/// `POST /workstreams/{wid}/git/abort`, `…/continue`, `…/skip`: the
/// operation half-done, by the same word a status reports it with.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "GitOperation")]
#[serde(deny_unknown_fields)]
pub struct OperationBody {
    pub what: GitInProgress,
}

/// The operation git has left half-done in a checkout, on the wire — what a
/// status reports, and what a client sends back to abort, continue or skip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GitInProgress {
    Rebase,
    Merge,
    CherryPick,
    Revert,
}

impl From<bisa_engine::ide::interactive::InProgress> for GitInProgress {
    fn from(op: bisa_engine::ide::interactive::InProgress) -> Self {
        use bisa_engine::ide::interactive::InProgress as I;
        match op {
            I::Rebase => GitInProgress::Rebase,
            I::Merge => GitInProgress::Merge,
            I::CherryPick => GitInProgress::CherryPick,
            I::Revert => GitInProgress::Revert,
        }
    }
}

impl From<GitInProgress> for bisa_engine::ide::interactive::InProgress {
    fn from(w: GitInProgress) -> Self {
        use bisa_engine::ide::interactive::InProgress as I;
        match w {
            GitInProgress::Rebase => I::Rebase,
            GitInProgress::Merge => I::Merge,
            GitInProgress::CherryPick => I::CherryPick,
            GitInProgress::Revert => I::Revert,
        }
    }
}

/// `POST /workstreams/{wid}/git/fetch`.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "GitFetch")]
#[serde(deny_unknown_fields)]
pub struct FetchBody {
    /// The remote to fetch; `origin` when omitted.
    #[serde(default)]
    pub remote: Option<String>,
}

/// How a pull moves the branch once the fetch has landed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PullModeBody {
    FfOnly,
    Rebase,
    Merge,
}

impl From<PullModeBody> for bisa_vcs::PullMode {
    fn from(m: PullModeBody) -> Self {
        match m {
            PullModeBody::FfOnly => bisa_vcs::PullMode::FfOnly,
            PullModeBody::Rebase => bisa_vcs::PullMode::Rebase,
            PullModeBody::Merge => bisa_vcs::PullMode::Merge,
        }
    }
}

/// `POST /workstreams/{wid}/git/pull`.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "GitPull")]
#[serde(deny_unknown_fields)]
pub struct PullBody {
    pub mode: PullModeBody,
    /// The remote to fetch first; `origin` when omitted.
    #[serde(default)]
    pub remote: Option<String>,
}

/// `POST /workstreams/{wid}/servers` (ide/18): serve a folder of the checkout
/// on a loopback port — `folder` relative to the checkout, absent for the
/// checkout itself.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "ServeFolder")]
#[serde(deny_unknown_fields)]
pub struct ServeBody {
    #[serde(default)]
    pub folder: Option<String>,
}

/// `POST /workstreams/{wid}/git/amend`: the last commit rewritten with what
/// is staged and this message.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "GitAmend")]
#[serde(deny_unknown_fields)]
pub struct GitAmendBody {
    /// The commit's message, replacing its own.
    pub message: String,
    /// Staged first, then folded in. **Empty means "what is already
    /// staged"**, never "everything" — the same rule as a commit's.
    #[serde(default)]
    pub paths: Vec<String>,
}

/// `POST /workstreams/{wid}/git/revert`: the commits to undo, oldest first.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "GitRevert")]
#[serde(deny_unknown_fields)]
pub struct RevertBody {
    pub commits: Vec<String>,
    #[serde(default)]
    pub no_commit: bool,
    /// The parent a merge commit is reverted against (`-m`, 1-based).
    #[serde(default)]
    pub mainline: Option<u32>,
}

impl From<RevertBody> for bisa_vcs::RevertRequest {
    fn from(b: RevertBody) -> Self {
        bisa_vcs::RevertRequest {
            commits: b.commits,
            no_commit: b.no_commit,
            mainline: b.mainline,
        }
    }
}

/// `PUT /workstreams/{wid}/git/remotes/{name}`.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "GitRemoteSet")]
#[serde(deny_unknown_fields)]
pub struct RemoteSetBody {
    pub url: String,
}

/// `POST /workstreams/{wid}/git/discard` — exactly one of the two.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "GitDiscard")]
#[serde(deny_unknown_fields)]
pub struct DiscardBody {
    /// One hunk of the *unstaged* patch, reversed onto the tree.
    #[serde(default)]
    pub patch: Option<String>,
    /// Whole paths, restored from the index.
    #[serde(default)]
    pub paths: Option<Vec<String>>,
}

/// `POST /workstreams/{wid}/git/recovery/restore`.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "GitRestore")]
#[serde(deny_unknown_fields)]
pub struct RestoreBody {
    /// The full recovery ref name, as `GET …/git/recovery` listed it.
    pub r#ref: String,
}

/// What a recovery ref points at (`bisa_vcs::RecoveryKind`, mirrored):
/// a tip to check out, the saved index and tree to apply, or a dropped stash
/// entry to put back on the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryKindDto {
    Commit,
    Tree,
    Stash,
}

impl From<bisa_vcs::RecoveryKind> for RecoveryKindDto {
    fn from(k: bisa_vcs::RecoveryKind) -> Self {
        match k {
            bisa_vcs::RecoveryKind::Commit => RecoveryKindDto::Commit,
            bisa_vcs::RecoveryKind::Tree => RecoveryKindDto::Tree,
            bisa_vcs::RecoveryKind::Stash => RecoveryKindDto::Stash,
        }
    }
}

/// One row of `GET /workstreams/{wid}/git/recovery` (ide/04): what a
/// consented operation saved before it ran, as `restore` wants it named.
#[derive(Clone, Debug, Serialize, JsonSchema)]
#[schemars(rename = "GitRecoveryRef")]
pub struct RecoveryRefView {
    /// The full ref name under `refs/bisa/safety/`.
    pub ref_name: String,
    pub commit: String,
    /// The operation that wrote it: `checkout`, `stash_drop`, …
    pub op: String,
    /// The branch HEAD was on, when it was on one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub kind: RecoveryKindDto,
    /// Seconds since the epoch.
    pub at: u64,
}

impl From<bisa_vcs::RecoveryRef> for RecoveryRefView {
    fn from(r: bisa_vcs::RecoveryRef) -> Self {
        Self {
            ref_name: r.ref_name,
            commit: r.commit.as_str().to_string(),
            op: r.op,
            branch: r.branch,
            kind: r.kind.into(),
            at: r.at,
        }
    }
}

/// One entry of `GET /workstreams/{wid}/git/stashes` (ide/04 §Stash): the
/// list is the repository's, shared by every worktree of the project.
#[derive(Clone, Debug, Serialize, JsonSchema)]
#[schemars(rename = "GitStash")]
pub struct StashEntryView {
    /// Its position, newest first — what `stash@{n}` means right now. It
    /// shifts under every push and drop, so a verb names `commit` too.
    pub index: u32,
    pub commit: String,
    /// The branch it was made on; `null` when HEAD was detached.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// The message the person gave; `null` for git's own `WIP on …`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// The reflog subject verbatim.
    pub subject: String,
    /// Seconds since the epoch.
    pub at: u64,
    /// The stash carries untracked files.
    pub untracked: bool,
}

impl From<bisa_vcs::StashEntry> for StashEntryView {
    fn from(e: bisa_vcs::StashEntry) -> Self {
        Self {
            index: e.index,
            commit: e.commit.as_str().to_string(),
            branch: e.branch,
            message: e.message,
            subject: e.subject,
            at: e.at,
            untracked: e.untracked,
        }
    }
}

/// `POST /workstreams/{wid}/git/stash`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "GitStashPush")]
pub struct StashPushBody {
    /// `On <branch>: <message>`; none is git's own `WIP on <branch>: …`.
    #[serde(default)]
    pub message: Option<String>,
    /// Untracked files go too (`-u`; never ignored ones).
    #[serde(default)]
    pub include_untracked: bool,
    /// The staged half stays staged in the tree as well (`--keep-index`).
    #[serde(default)]
    pub keep_index: bool,
    /// Only these paths; absent or empty is the whole tree.
    #[serde(default)]
    pub paths: Option<Vec<String>>,
}

impl From<StashPushBody> for bisa_vcs::StashPush {
    fn from(b: StashPushBody) -> Self {
        Self {
            message: b.message,
            include_untracked: b.include_untracked,
            keep_index: b.keep_index,
            paths: b.paths.unwrap_or_default(),
        }
    }
}

/// `POST /workstreams/{wid}/git/stashes/{sha}/{apply|pop|drop}`: the index
/// the entry was listed at. The sha travels in the path; the two together
/// are the target, and a moved list is refused (`stash_moved`).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "GitStashTarget")]
pub struct StashTargetBody {
    pub index: u32,
}

/// `POST /ide/lsp/{scope}/{id}/open` and `/change`.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "LspDocument")]
#[serde(deny_unknown_fields)]
pub struct LspDocumentBody {
    pub path: String,
    pub text: String,
}

/// `POST /ide/lsp/{scope}/{id}/close`.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "LspPath")]
#[serde(deny_unknown_fields)]
pub struct LspPathBody {
    pub path: String,
}

/// `POST /ide/lsp/{scope}/{id}/request`.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "LspRequest")]
#[serde(deny_unknown_fields)]
pub struct LspRequestBody {
    /// The document the request is about (decides the language, hence the server).
    pub path: String,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// `POST /ide/lsp/{scope}/{id}/restart`.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "LspRestart")]
#[serde(deny_unknown_fields)]
pub struct LspRestartBody {
    pub language: String,
}

/// `POST /ide/files/{scope}/{id}`.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "IdeCreateEntry")]
#[serde(deny_unknown_fields)]
pub struct IdeCreateBody {
    pub path: String,
    /// `file` or `dir`.
    pub kind: String,
}

/// `POST /ide/files/{scope}/{id}/move`.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "IdeMoveEntry")]
#[serde(deny_unknown_fields)]
pub struct IdeMoveBody {
    pub from: String,
    pub to: String,
}

/// `POST /ide/files/{scope}/{id}/copy`.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "IdeCopyEntry")]
#[serde(deny_unknown_fields)]
pub struct IdeCopyBody {
    pub from: String,
    pub to: String,
}

/// `GET /node`: what this node is — the process, where it listens, where it
/// keeps the workspace, whether the engine is paused and how many sessions
/// are live. The footer's node overlay reads it on open.
#[derive(Serialize, JsonSchema)]
pub struct NodeInfo {
    pub version: String,
    pub pid: u32,
    /// Unix seconds when the node started serving.
    pub started_at: u64,
    /// The unix socket bound — the fallback path when the data dir is long.
    pub socket: String,
    /// The loopback address, as a URL, when the node listens on TCP too.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listen: Option<String>,
    pub data_dir: String,
    pub logs_dir: String,
    /// Nothing new starts while paused; running sessions finish.
    pub paused: bool,
    /// Sessions going right now — workers, turns, terminals.
    pub live_sessions: usize,
}

/// One cache's live counters, for `GET /cache/stats`.
#[derive(Serialize, JsonSchema)]
pub struct CacheStatsDto {
    /// The cache's stable name, e.g. `harness.listing`.
    pub name: String,
    pub hits: u64,
    pub misses: u64,
    /// Live entries held right now.
    pub entries: u64,
    /// Hits ÷ (hits + misses), 0 when never read — the panel's headline number.
    pub hit_rate: f64,
}

impl From<bisa_cache::CacheStats> for CacheStatsDto {
    fn from(s: bisa_cache::CacheStats) -> Self {
        let total = s.hits + s.misses;
        let hit_rate = if total == 0 {
            0.0
        } else {
            s.hits as f64 / total as f64
        };
        Self {
            name: s.name.to_string(),
            hits: s.hits,
            misses: s.misses,
            entries: s.entries as u64,
            hit_rate,
        }
    }
}

// ---------------------------------------------------------------------------
// Connectors: definitions, accounts (never a value), the OAuth flow
// ---------------------------------------------------------------------------

/// One operation of a connector as a listing shows it — enough for a picker
/// and the Workflow Agent's roster, without the request shape.
#[derive(Serialize, JsonSchema)]
#[schemars(rename = "ConnectorOperation")]
pub struct ConnectorOperationRow {
    pub id: bisa_core::OperationId,
    pub name: String,
    pub description: String,
    pub method: bisa_core::HttpMethod,
    /// Changes something on the platform: a workflow puts an approval before it.
    pub writes: bool,
    pub params: Vec<bisa_core::ParamDef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub select: Option<String>,
}

impl From<&bisa_core::Operation> for ConnectorOperationRow {
    fn from(op: &bisa_core::Operation) -> Self {
        Self {
            id: op.id.clone(),
            name: op.name.clone(),
            description: op.description.clone(),
            method: op.method,
            writes: op.writes,
            params: op.params.clone(),
            select: op.output.select.clone(),
        }
    }
}

/// A row of `GET /connectors`: the definition's facts and how many accounts
/// this machine holds for it.
#[derive(Serialize, JsonSchema)]
#[schemars(rename = "ConnectorRow")]
pub struct ConnectorRow {
    pub id: bisa_core::ConnectorId,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub origin: bisa_core::Origin,
    /// The auth scheme's word: `none`, `api_key`, `bearer`, `basic`, `oauth2`.
    pub auth: String,
    pub hosts: Vec<String>,
    pub operations: Vec<ConnectorOperationRow>,
    pub accounts: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_account: Option<bisa_core::AccountId>,
}

impl ConnectorRow {
    pub fn from_parts(c: &bisa_core::Connector, accounts: &[bisa_core::ConnectorAccount]) -> Self {
        Self {
            id: c.id.clone(),
            name: c.name.clone(),
            description: c.description.clone(),
            tags: c.tags.as_slice().to_vec(),
            origin: c.origin.clone(),
            auth: c.auth.word().to_string(),
            hosts: c.hosts.clone(),
            operations: c
                .operations
                .iter()
                .map(ConnectorOperationRow::from)
                .collect(),
            accounts: accounts.len(),
            default_account: accounts.iter().find(|a| a.default).map(|a| a.id),
        }
    }
}

/// Where a stored secret lives — a `0600` file under `identity/`, or the OS
/// keyring when the node runs with `BISA_KEYSTORE=keyring`.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "SecretSource")]
pub enum SecretSourceDto {
    File,
    Keyring,
}

impl From<bisa_store::SecretSource> for SecretSourceDto {
    fn from(s: bisa_store::SecretSource) -> Self {
        match s {
            bisa_store::SecretSource::File => Self::File,
            bisa_store::SecretSource::Keyring => Self::Keyring,
        }
    }
}

/// What an OAuth account knows about its tokens — never the tokens.
#[derive(Serialize, JsonSchema)]
#[schemars(rename = "ConnectorOauthFacts")]
pub struct ConnectorOauthFacts {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// The access token's expiry has passed; a refresh happens on the next call.
    pub expired: bool,
}

/// One account of a connector on this machine: its label and parameters,
/// which secret fields are set and where they live. **Never a value** — the
/// route test asserts it.
#[derive(Serialize, JsonSchema)]
#[schemars(rename = "ConnectorAccountRow")]
pub struct ConnectorAccountRow {
    pub id: bisa_core::AccountId,
    pub connector: bisa_core::ConnectorId,
    pub label: String,
    pub params: BTreeMap<String, serde_json::Value>,
    pub default: bool,
    pub secrets_set: Vec<bisa_core::SecretField>,
    pub token_source: SecretSourceDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth: Option<ConnectorOauthFacts>,
}

impl ConnectorAccountRow {
    pub fn from_parts(
        a: &bisa_core::ConnectorAccount,
        auth: &bisa_core::AuthScheme,
        source: bisa_store::SecretSource,
        now: u64,
    ) -> Self {
        let oauth =
            matches!(auth, bisa_core::AuthScheme::OAuth2 { .. }).then(|| ConnectorOauthFacts {
                expires_at: a.auth.expires_at,
                scope: a.auth.scope.clone(),
                expired: a.auth.expires_at.is_some_and(|at| at <= now),
            });
        Self {
            id: a.id,
            connector: a.connector.clone(),
            label: a.label.clone(),
            params: a.params.clone(),
            default: a.default,
            secrets_set: a.auth.fields_set.clone(),
            token_source: source.into(),
            oauth,
        }
    }
}

/// `GET /connectors/{cid}`: the whole definition and this machine's accounts.
#[derive(Serialize, JsonSchema)]
#[schemars(rename = "ConnectorDetail")]
pub struct ConnectorDetailDto {
    pub connector: bisa_core::Connector,
    pub accounts: Vec<ConnectorAccountRow>,
}

/// `PUT /connectors/{cid}/accounts`: a new account when `id` is absent, an
/// edit of that account otherwise. `secrets` are written once — each field
/// replaced — and never read back.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "NewConnectorAccount")]
#[serde(deny_unknown_fields)]
pub struct NewConnectorAccountBody {
    #[serde(default)]
    pub id: Option<bisa_core::AccountId>,
    pub label: String,
    #[serde(default)]
    pub params: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub secrets: BTreeMap<bisa_core::SecretField, String>,
    #[serde(default)]
    pub default: Option<bool>,
}

/// `POST /connectors/{cid}/accounts/{aid}/oauth/complete`: the code a person
/// pasted, for a platform that cannot send the browser back to loopback.
#[derive(Deserialize, JsonSchema)]
#[schemars(rename = "OauthComplete")]
#[serde(deny_unknown_fields)]
pub struct OauthCompleteBody {
    pub code: String,
}

/// A connector definition as `POST /connectors`, `PUT /connectors/{cid}` and
/// `POST /connectors/validate` take it: every field of the record but the two
/// the store stamps (origin, time). The catalog's TOML is the same shape
/// under `[connector]`, with the slug as the id.
#[derive(Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "ConnectorDefinition")]
#[serde(deny_unknown_fields)]
pub struct ConnectorDefinitionBody {
    pub id: bisa_core::ConnectorId,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub base_url: String,
    pub hosts: Vec<String>,
    #[serde(default)]
    pub insecure_tls: bool,
    pub auth: bisa_core::AuthScheme,
    #[serde(default)]
    pub params: Vec<bisa_core::ParamDef>,
    pub operations: Vec<bisa_core::Operation>,
    #[serde(default)]
    pub check: Option<bisa_core::OperationId>,
}

impl ConnectorDefinitionBody {
    /// The definition as validation reads it — provenance and time are
    /// placeholders the rules never read.
    pub fn as_connector(&self) -> Result<bisa_core::Connector, bisa_core::CoreError> {
        Ok(bisa_core::Connector {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            tags: bisa_core::Tags::new(self.tags.iter().cloned())?,
            origin: bisa_core::Origin::Local,
            base_url: self.base_url.clone(),
            hosts: self.hosts.clone(),
            insecure_tls: self.insecure_tls,
            auth: self.auth.clone(),
            params: self.params.clone(),
            operations: self.operations.clone(),
            check: self.check.clone(),
            created_at: 0,
        })
    }

    pub fn into_new(self) -> Result<bisa_store::NewConnector, bisa_core::CoreError> {
        Ok(bisa_store::NewConnector {
            id: self.id,
            name: self.name,
            description: self.description,
            tags: bisa_core::Tags::new(self.tags)?,
            base_url: self.base_url,
            hosts: self.hosts,
            insecure_tls: self.insecure_tls,
            auth: self.auth,
            params: self.params,
            operations: self.operations,
            check: self.check,
        })
    }
}

/// One thing wrong with a definition, by the field it is about.
#[derive(Serialize, JsonSchema)]
#[schemars(rename = "ConnectorProblem")]
pub struct ConnectorProblemDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// The rule broken, as a message the reader renders (17 — Internationalisation).
    pub text: bisa_core::Text,
}

impl From<&bisa_core::ConnectorProblem> for ConnectorProblemDto {
    fn from(p: &bisa_core::ConnectorProblem) -> Self {
        Self {
            field: p.field.clone(),
            text: p.text.clone(),
        }
    }
}

/// `POST /connectors/validate`: every problem, or none.
#[derive(Serialize, JsonSchema)]
#[schemars(rename = "ConnectorValidation")]
pub struct ConnectorValidationDto {
    pub ok: bool,
    pub problems: Vec<ConnectorProblemDto>,
}

#[cfg(test)]
mod strip_tests {
    use super::{McpServerView, RunStrip};
    use bisa_core::{Join, OnFail, RunEvent, RunOutcome, Step, StepKind, Workflow, WorkflowRun};

    /// The strip names where a failed run failed and what the step recorded
    /// — the newest failure — and nothing while the run is not failed.
    #[test]
    fn a_failed_strip_names_the_newest_failed_step_and_its_error() {
        let sid = |s: &str| bisa_core::StepId::new(s).unwrap();
        let agent = |id: &str, name: &str, then: &[&str]| Step {
            id: sid(id),
            name: name.into(),
            kind: StepKind::Agent {
                instructions: "do".into(),
                assignee: None,
                project: None,
                harness: vec![],
                model: None,
                effort: None,
                output_schema: None,
                tier_ceiling: bisa_core::ToolTier::Write,
            },
            then: then.iter().map(|t| bisa_core::Flow::to(sid(t))).collect(),
            boundaries: vec![],
            join: Join::All,
            on_fail: OnFail::Fail,
            retries: 0,
            max_visits: 3,
            position: None,
        };
        let workflow = Workflow {
            id: "01BX5ZZKBKACTAV9WEVGEMMVRZ".parse().unwrap(),
            name: "Two".into(),
            description: String::new(),
            inputs: vec![],
            steps: vec![
                agent("build", "Build", &["review"]),
                agent("review", "Review", &[]),
            ],
            origin: bisa_core::WorkflowOrigin::Workspace,
            author: bisa_core::PrincipalId::new("ab".repeat(32)).unwrap(),
            tags: Default::default(),
            revision: 1,
            archived: None,
            decision_making: false,
            created_at: 0,
        };
        let mut run = WorkflowRun::new(
            "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            bisa_core::RunScope::Goal {
                goal: "01ARZ3NDEKTSV4RRFFQ69G5FAW".parse().unwrap(),
            },
            workflow,
            Default::default(),
            bisa_core::RunEntry::by_hand(),
            0,
        );
        run.apply(RunEvent::Start, 1).unwrap();
        assert!(RunStrip::from_parts(Some(&run), None).failure.is_none());
        run.apply(
            RunEvent::StepDone {
                step: sid("build"),
                output: serde_json::json!({"ok": true}),
            },
            2,
        )
        .unwrap();
        run.apply(
            RunEvent::StepFailed {
                step: sid("review"),
                error: "{steps.build.output.summary} has no value in this run: step `build` yielded `ok` and no `summary`".into(),
            },
            3,
        )
        .unwrap();
        assert_eq!(run.outcome, Some(RunOutcome::Failed));
        let strip = RunStrip::from_parts(Some(&run), None);
        let failure = strip.failure.expect("a failed run names its step");
        assert_eq!(failure.step, sid("review"));
        assert_eq!(failure.name, "Review");
        assert_eq!(
            failure.error.as_deref(),
            Some("{steps.build.output.summary} has no value in this run: step `build` yielded `ok` and no `summary`")
        );
    }

    /// The server view flattens a strict record beside its health and is
    /// read by hand: the health taken out, the rest read as the record — so
    /// a stray key reaches the record and is refused there, where a derived
    /// `flatten` would have dropped it in silence.
    #[test]
    fn the_server_view_refuses_a_key_nobody_knows_through_the_record_it_flattens() {
        let whole = serde_json::json!({
            "id": "srv", "name": "Server", "description": "", "transport": {"transport": "stdio", "name": "Server", "command": "x"},
            "enabled": true, "created_at": 1, "health": {"state": "unknown"}
        });
        let view: McpServerView =
            serde_json::from_value(whole.clone()).expect("the whole view reads");
        assert_eq!(view.server.id.to_string(), "srv");
        let mut spoilt = whole;
        spoilt["zzz_nobody_knows"] = serde_json::json!(1);
        let err =
            serde_json::from_value::<McpServerView>(spoilt).expect_err("a stray key is refused");
        assert!(err.to_string().contains("zzz_nobody_knows"), "{err}");
    }
}
