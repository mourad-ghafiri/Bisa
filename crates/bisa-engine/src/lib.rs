//! The Bisa orchestrator.
//!
//! Transport-free by design: the engine exposes a typed facade ([`Engine`])
//! plus a broadcast event bus; `bisa-node` and the CLI put wires on it.
//!
//! **Every mutation of the workspace goes through here** (`docs/architecture/
//! 07-layering.md`). The node reads the store; it writes through the engine.

/// The decision provider's failure kinds, as `EngineError::Provider` carries them.
pub use bisa_decision::ProviderError;

pub mod activity;
pub mod addons;
pub mod admin;
pub mod ask;
pub mod assign;
pub mod browser;
pub mod cache;
pub mod changes;
pub mod classifier;
pub mod codehost;
pub mod collab;
pub mod config;
pub mod connector_health;
pub mod connectors;
pub mod content;
pub mod conversation;
pub mod conversations;
pub mod decider;
pub mod directory;
pub mod documents;
pub mod drawings;
pub mod effects;
pub mod effort;
pub mod ending;
pub mod error_text;
pub mod events;
pub mod executor;
pub mod folder_git;
pub mod framing;
pub mod gates;
pub mod gitprofiles;
pub mod guided;
pub mod harness_usage;
pub mod ide;
pub mod identity;
pub mod import;
pub mod inputs;
pub mod intake;
pub mod interactive;
pub mod lifecycle;
pub mod listen;
pub mod lock;
pub mod logging;
pub mod lsp;
pub mod mcp_health;
pub mod membership;
pub mod messaging;
pub mod mobile_development;
pub mod models;
pub mod network;
pub mod notes;
pub mod notices;
pub mod ops;
pub mod parked;
pub mod presence;
pub mod projects;
pub mod readiness;
pub mod recovery;
pub mod registry;
pub mod retire;
pub mod scheduler;
pub mod scripts;
pub mod security;
pub mod sessions;
pub mod settings;
pub mod ssh;
pub mod staff;
pub mod updates;
pub mod waits;

pub use bisa_core::run::INTERRUPTED;
pub use config::EngineConfig;
pub use events::ExecutionOutcome;
pub use events::{EngineEvent, EnginePayload, EventScope, FiredOutcome};
pub use gates::{GateEntry, GateResolution, Gates, PauseGate};
pub use identity::{Committer, CommitterOverview, CommitterReason, PendingCommitter};
pub use listen::hooks::{HookDoor, HookRefusal};
pub use listen::turn::{HookSecret, TurnedOn};
pub use lock::{EngineLock, LockHolder};
pub use models::{ModelHealthRow, ModelLedger};
pub use ops::{Begin, Begun, DecideOutcome, HomeStatus, Stopped, SubmitRequest, Submitted};
pub use presence::{SessionPresence, SessionState, SubagentPresence, WaitingOn};
pub use projects::{
    Created, NewProjectRequest, ProjectSource, Workplace, WorkstreamEdit, WorkstreamRequest,
};
pub use recovery::{Recovered, ORPHANED};
pub use registry::LiveRunId;
pub use registry::{AgentRef, AgentRegistry, AgentStatus};
pub use scheduler::ScheduleRejection;
pub use waits::WaitState;

use bisa_core::workitem::WorkItemSpec;
use bisa_core::{
    AgentId, Answer, ApprovalId, ClosureReason, GoalId, Home, Problem, RunEntry, RunId, StepId,
    TeamId, WorkItemId, WorkItemTransition, Workflow, WorkflowId, WorkflowRun,
};
use bisa_harness::HarnessCatalog;
use bisa_store::Workspace;
use bisa_store::{KnownRuntime, NewWorkflow};
use dashmap::DashMap;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error(transparent)]
    Store(#[from] bisa_store::StoreError),
    #[error("unknown gate: {0}")]
    UnknownGate(String),
    #[error("gate already decided: {0}")]
    GateAlreadyDecided(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Vcs(#[from] bisa_vcs::VcsError),
    #[error(transparent)]
    CodeHost(#[from] bisa_codehost::CodeHostError),
    /// A connector call, an OAuth flow or an account check refused or failed
    /// — the crate's own words, already scrubbed of every secret it exposed.
    #[error(transparent)]
    Connector(#[from] bisa_connectors::ConnectorError),
    /// SSH for git hosts refused or failed — a key name taken, a key that
    /// wants a passphrase, a program that is missing (ide/04).
    #[error(transparent)]
    Ssh(#[from] bisa_ssh::SshError),
    /// SSH was not configured for this engine (`EngineConfig::ssh` is `None`):
    /// the SSH keys panel and the connection's transport facts are not
    /// available here. The node renders it as 503.
    #[error("SSH is not configured for this node: {0}")]
    SshUnavailable(String),
    /// The mobile tools refused or failed — a program missing, a device
    /// that is not there, a boot that timed out (ide/19).
    #[error(transparent)]
    MobileDevelopment(#[from] bisa_mobile_development::MobileDevelopmentError),
    /// The mobile tools were not configured for this engine
    /// (`EngineConfig::mobile_development` is `None`). The node renders it as 503.
    #[error("{0}")]
    MobileDevelopmentUnavailable(bisa_core::Text),
    /// No isolation backend is available here — a refusal the caller reads
    /// as the machine's state, not a malfunction.
    #[error("isolation unavailable: {0}")]
    Iso(String),
    /// A backend that is there failed to make the copy — disk, permissions:
    /// this node's failure, never the caller's.
    #[error("isolation failed: {0}")]
    IsoFailed(String),
    /// The Tool & Commands Guard refused a command, or the classifier gave no
    /// verdict where one was required. The sentence names the rule or the
    /// reason; it never carries the secret.
    #[error("refused by the guard: {0}")]
    Security(String),
    /// The decision provider could not answer — as it is set up, or now.
    /// Its kind says whether asking again may help, and the node answers
    /// with the status of that kind rather than a flat 400.
    #[error("{0}")]
    Provider(#[from] bisa_decision::ProviderError),
    /// An approval or a change request on a pull request the connected
    /// account itself opened: every code host refuses it, so the platform
    /// says so first. A comment is welcome; a verdict is a reviewer's.
    #[error("@{author} opened this pull request, so the code host takes a comment from you but not an approval or a change request — ask a reviewer, or an agent")]
    OwnPullRequest { author: String },
    /// A commit was asked for on a clean tree, or on a selection that matched
    /// nothing changed. The ordinary outcome of a work item that only read or
    /// answered; a caller settling a run treats it as "nothing to do".
    #[error("{0} has nothing to commit")]
    NothingToCommit(String),
    /// An agent step names no project and the goal has several, so there is
    /// no one place for its work: refused at run start (or at the step, when
    /// attachments changed mid-run) rather than guessed.
    #[error(
        "step `{step}` names no project and the goal has {count}; name one with a project input"
    )]
    ProjectAmbiguous {
        step: bisa_core::StepId,
        count: usize,
    },
    /// Nobody is set to commit in this repository: neither the repository's
    /// own config nor the global one names a `user.name`/`user.email`. The
    /// message names both fixes, because the platform never writes global
    /// config on anybody's behalf.
    #[error("nobody is set to commit in project {project}: set who commits in this repository — Git → Repository in the IDE, or `bisa project identity {project} --name … --email …`")]
    IdentityUnset { project: String },
    /// A folder repository — the notes' or the drawings' — has nobody to
    /// commit as: neither its own config nor the global one names a pair.
    /// The door is the overlay's own.
    #[error("nobody is set to commit {what}: set who commits under the panel's Who commits…, or your global git identity under Settings › Git & code hosts › Identity")]
    RepoIdentityUnset { what: &'static str },
    /// A folder repository that does not pull — the drawings': its records
    /// arrive by sync, and a pull that changed a file would be overwritten
    /// by the next ingest.
    #[error("the {what} repository offers no pull: its records arrive by sync")]
    PullNotOffered { what: &'static str },
    /// The project publishes manually: pushing and opening pull requests are a
    /// person's job here, and no gate can widen that.
    #[error("project {project} publishes manually; a person must {what}")]
    PublishManual { project: String, what: String },
    /// The project is gated, but the workstream belongs to no goal, so there
    /// is nobody on the bus to ask. Not the manual policy — a person can
    /// attach the project to a goal, or set the policy, and be asked.
    #[error("project {project} is gated and workstream {workstream} belongs to no goal, so there is nobody to ask; {what} yourself, attach the project to a goal, or set publishing to automatic")]
    PublishNoGoal {
        project: String,
        workstream: String,
        what: String,
    },
    /// The `Publish` gate was declined. Nothing left this machine.
    #[error("the publish gate was declined: {what}")]
    PublishDeclined { what: String },
    /// A push or a pull request was asked for a branch with nothing on it
    /// beyond its base: there is nothing to publish until something is
    /// committed.
    #[error("{branch} has no commits beyond {base}; commit first (workstream {workstream})")]
    NothingToPublish {
        workstream: String,
        branch: String,
        base: String,
    },
    #[error("invalid: {0}")]
    Invalid(bisa_core::Text),
    /// Refused for now, not for good — the workspace's state, not the
    /// request: a checkout busy elsewhere, an act that must wait its turn.
    /// The node answers 409 with the sentence.
    #[error("{0}")]
    Conflict(bisa_core::Text),
    /// A workstream was asked for from a pull request that is not open: a
    /// closed or merged one has no branch to work on. The node answers 409
    /// `pull_request_state` with the state in `detail`.
    #[error("pull request #{number} is {state}, not open — only an open pull request can be taken up as a workstream")]
    PullRequestNotOpen { number: u64, state: String },
    /// A workstream script stood in the way (ide/07 §Workstream scripts): the
    /// pre-create script refused the checkout, or the clean script refused to
    /// let it go — non-zero, timed out, unstartable, or not approved on this
    /// machine. `output` is the tail of what it printed.
    #[error("the {phase} script {reason}")]
    WorkstreamScript {
        phase: scripts::Phase,
        reason: String,
        output: String,
    },
    /// A compare-and-swap save lost the race: the file changed since it was
    /// read. Carries what is there now, so the client can merge without a
    /// second round trip.
    #[error("{path} changed since you read it")]
    FileConflict {
        path: String,
        current_hash: String,
        current_text: String,
    },
    /// Another engine holds this workspace. Starting a second one would have
    /// two schedulers claiming the same work items; the answer is to use the
    /// running one, or stop it.
    #[error("another engine holds this workspace ({path}{})", match holder { Some(h) => format!(", {h}"), None => String::new() })]
    Locked {
        path: String,
        holder: Option<LockHolder>,
    },
    /// The answer did not fit the question that was asked.
    #[error(transparent)]
    Answer(#[from] bisa_core::AnswerError),
    /// A domain rule the core states, reached from the engine directly.
    #[error(transparent)]
    Core(#[from] bisa_core::CoreError),
    /// The run refused the event: a step that is not live, an amendment that
    /// touches a started step. A refusal, never a malfunction.
    #[error(transparent)]
    Run(#[from] bisa_core::RunError),
    /// A template a step carries could not be rendered against the run.
    #[error(transparent)]
    Template(#[from] bisa_core::TemplateError),
    /// A design was asked for and there is nothing to design, or somebody is
    /// already at it.
    #[error(transparent)]
    DesignRefused(#[from] guided::DesignRefusal),
}

impl EngineError {
    /// What an isolation backend said, as the engine reports it: a backend
    /// that is not on this machine is the machine's state ([`Self::Iso`] —
    /// the next candidate is tried, and the node answers 409); one that is
    /// here and failed is this node's failure ([`Self::IsoFailed`], a 500),
    /// so a full disk is never read as *try something else*.
    pub(crate) fn of_iso(e: bisa_iso::IsoError) -> Self {
        if e.is_unavailable() {
            Self::Iso(e.to_string())
        } else {
            Self::IsoFailed(e.to_string())
        }
    }

    /// Whether this is a refusal a person should read as an answer rather
    /// than as a malfunction.
    pub fn is_refusal(&self) -> bool {
        match self {
            EngineError::Store(e) => e.is_refusal(),
            EngineError::Connector(e) => e.is_refusal(),
            EngineError::UnknownGate(_)
            | EngineError::GateAlreadyDecided(_)
            | EngineError::NothingToCommit(_)
            | EngineError::IdentityUnset { .. }
            | EngineError::RepoIdentityUnset { .. }
            | EngineError::PullNotOffered { .. }
            | EngineError::ProjectAmbiguous { .. }
            | EngineError::PublishManual { .. }
            | EngineError::PublishNoGoal { .. }
            | EngineError::PublishDeclined { .. }
            | EngineError::NothingToPublish { .. }
            | EngineError::Invalid(_)
            | EngineError::Conflict(_)
            | EngineError::PullRequestNotOpen { .. }
            | EngineError::WorkstreamScript { .. }
            | EngineError::Answer(_)
            | EngineError::FileConflict { .. }
            | EngineError::Core(_)
            | EngineError::Run(_)
            | EngineError::Template(_)
            | EngineError::Security(_)
            | EngineError::OwnPullRequest { .. }
            | EngineError::DesignRefused(_) => true,
            EngineError::CodeHost(e) => !matches!(e, bisa_codehost::CodeHostError::Transport(_)),
            // What the provider is set up as, what it refused and what it
            // answered are answers; a provider out of reach or out of time is
            // the machine's moment, not a refusal.
            EngineError::Provider(e) => !e.is_transient(),
            EngineError::Ssh(e) => matches!(e, bisa_ssh::SshError::Refused(_)),
            // A device that is not there, or a call that makes no sense for
            // it, is the caller's; a missing program and a timeout are the
            // machine's.
            EngineError::MobileDevelopment(e) => matches!(
                e,
                bisa_mobile_development::MobileDevelopmentError::NoSuchDevice(_)
                    | bisa_mobile_development::MobileDevelopmentError::Unsupported(_)
            ),
            EngineError::MobileDevelopmentUnavailable(_) => false,
            EngineError::IsoFailed(_) => false,
            EngineError::SshUnavailable(_)
            | EngineError::Io(_)
            | EngineError::Vcs(_)
            | EngineError::Iso(_)
            | EngineError::Locked { .. } => false,
        }
    }
}

/// The harnesses that take an effort: the ones whose adapter says so. What
/// the store checks a step's effort pin against.
fn effort_harnesses(inner: &Inner) -> Vec<String> {
    inner
        .catalog
        .adapters()
        .iter()
        .filter(|a| a.caps().contains(bisa_core::HarnessCaps::EFFORT))
        .map(|a| a.id().to_string())
        .collect()
}

/// Ask every adapter for its model list and hand the whole runtime to the
/// store, so a model pin is validated against what the harness really lists.
/// A harness that does not answer keeps an empty list and its pins are not
/// checked (the one soft check).
async fn describe_runtime(inner: Arc<Inner>) {
    let mut harnesses = Vec::new();
    let mut models = Vec::new();
    for adapter in inner.catalog.adapters() {
        harnesses.push(adapter.id().to_string());
        let listed: Vec<String> = adapter.models().await.into_iter().map(|m| m.id).collect();
        if !listed.is_empty() {
            models.push((adapter.id().to_string(), listed));
        }
    }
    inner.ws.set_known_runtime(KnownRuntime {
        harnesses,
        models,
        effort_harnesses: effort_harnesses(&inner),
        topics: events::TOPICS,
    });
}

/// Log a best-effort write that failed, and move on.
///
/// The one sanctioned way to drop a `Result` in this crate. `let _ = ` is
/// denied: a journal append that fails silently is how a record ends up
/// lying, and a warning naming the write is the least a reader deserves.
pub(crate) fn warn_on_err<T, E: std::fmt::Display>(result: Result<T, E>, what: &str) {
    if let Err(e) = result {
        tracing::warn!("{what} failed: {e}");
    }
}

/// [`warn_on_err`] for a failure that is about a home — a goal, or a run of
/// the workspace — and a work item when there is one: the ids ride as
/// fields, so a log read for one goal or run finds the write that did not
/// land for it.
pub(crate) fn warn_on_err_for<T, E: std::fmt::Display>(
    home: Home,
    work_item: Option<WorkItemId>,
    result: Result<T, E>,
    what: &str,
) {
    if let Err(e) = result {
        match work_item {
            Some(item) => tracing::warn!(%home, work_item = %item, "{what} failed: {e}"),
            None => tracing::warn!(%home, "{what} failed: {e}"),
        }
    }
}

/// Note a bookkeeping write that failed, at debug level: registry status
/// flips and similar, where a refusal is the expected shape of a race.
pub(crate) fn debug_on_err<T, E: std::fmt::Display>(result: Result<T, E>, what: &str) {
    if let Err(e) = result {
        tracing::debug!("{what}: {e}");
    }
}

/// Shared state across engine subsystems (crate-public: subsystems live in
/// sibling modules).
pub struct Inner {
    pub ws: Arc<Workspace>,
    pub catalog: HarnessCatalog,
    pub config: EngineConfig,
    /// The resolved `cache.*` settings every cache reads its TTL from,
    /// refreshed on a `cache.*` write.
    pub cache: cache::CacheState,
    /// The process's diagnostic log, when it installed one
    /// (`EngineConfig::log`): the `logging.*` settings are applied to it
    /// at start and on every write of one.
    pub log: Option<bisa_log::Handle>,
    pub registry: AgentRegistry,
    /// What every live session is doing — the one status that leaves the engine.
    pub presence: presence::Presence,
    /// The harnesses people opened in desktop terminals, and their secrets.
    pub interactive: interactive::InteractiveDesk,
    /// The `git` every identity read and every commit the engine makes goes
    /// through — the process's by default, a handle with its own environment
    /// under test so a developer's global config never reaches an assertion
    /// (`EngineConfig::git`).
    pub git: bisa_vcs::Git,
    /// SSH for git hosts, when this engine was given it (`EngineConfig::ssh`).
    pub ssh: Option<bisa_ssh::Ssh>,
    /// The projects nobody has said who commits in yet.
    pub committers: identity::CommitterDesk,
    pub gates: Gates,
    /// The browser tool calls parked for the desktop (ide/18).
    pub browser: browser::BrowserRequests,
    /// The drawing requests parked for the desktop (19 — Drawings).
    pub draw: parked::Desk<drawings::PendingDrawRequest>,
    /// The node's static server for a folder of a checkout, lent through
    /// [`Engine::set_folder_server`] — what `browser_serve` asks; none on
    /// an engine no node runs.
    pub folder_server: std::sync::RwLock<Option<Arc<dyn browser::FolderServer>>>,
    /// The last look at this machine's mobile toolchain (ide/19).
    pub mobile_development: mobile_development::MobileDevelopmentState,
    /// What the installed MCP servers answered when last checked (06 § MCP servers).
    pub mcp_health: mcp_health::McpHealth,
    pub pause: PauseGate,
    pub caps: scheduler::ConcurrencyCaps,
    pub lifecycle: lifecycle::Lifecycle,
    /// The stop of every session driven without a work item — the Workflow
    /// Agent's wakes, the one-shot asks — by its run: what ending the run's
    /// row tells its driver by. A worker's stop is its item's mark in
    /// `inflight`.
    pub driving: sessions::Drivers,
    /// The sessions told to stop whose harness process is not yet known to
    /// be gone: what a verb's wait for its stop watches, and terminates at
    /// its deadline (`sessions::await_stopped`).
    pub ending: sessions::Stopping,
    /// A weak handle on this very state, set once at start: for a path that
    /// holds a plain reference and must hand an owned one to a task — ending
    /// a roster row with its retention clock from a one-shot ask.
    me: std::sync::OnceLock<std::sync::Weak<Inner>>,
    /// The items executing right now, by the home each is filed in.
    pub active_items: DashMap<WorkItemId, Home>,
    /// Items reserved for execution right now (reservation happens
    /// synchronously at scheduling time; removed when the executor settles).
    /// Each mark carries the item's home and the signal that stops its
    /// session at once (`executor::InFlightMark`).
    pub inflight: DashMap<WorkItemId, executor::InFlightMark>,
    /// The executor tasks this engine spawned, so `shutdown` ends them: a
    /// task that outlived its engine would settle items in a store another
    /// engine has since taken over (two engines in one process is what a
    /// restart test does; a node restarting in place is the same shape).
    pub executors: std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>,
    /// Set by `shutdown` before the tasks are aborted: an executor that is
    /// mid-settle writes nothing more — the item is left as a restart's sweep
    /// finds it, interrupted, never failed by its own engine's ending.
    pub stopping: std::sync::atomic::AtomicBool,
    /// How many times each goal — or run of the workspace — has already been
    /// re-asked after an "I'm not sure". Bounded by
    /// [`EngineConfig::max_clarify_rounds`].
    pub clarify_rounds: DashMap<Home, u8>,
    /// What the engine knows right now about which `(harness, model)` pairs
    /// will actually run. In-memory and per-process on purpose.
    pub models: ModelLedger,
    pub guided: guided::GuidedState,
    pub conversation: conversation::ConversationState,
    /// The listening runtime: its settings, the armed listeners, the loop
    /// guard's memory.
    pub listen: listen::ListenState,
    /// The `wait` steps, waiting `spawn` steps and boundary events armed
    /// right now.
    pub waits: waits::WaitState,
    /// The roots the IDE is watching.
    pub ide_watch: ide::watch::WatchRegistry,
    pub ide_graph: ide::graph::GraphRegistry,
    pub ide_status: ide::git::StatusCache,
    /// The notes repository — status cached in one cell, dropped by every note write.
    pub notes_git: folder_git::FolderGit,
    /// The drawings repository, the same shape over `drawings/` (19).
    pub drawings_git: folder_git::FolderGit,
    /// The code hosts this build knows — GitHub, GitLab, Bitbucket, or a
    /// test's fakes — asked by remote URL.
    pub code_hosts: bisa_codehost::CodeHostRegistry,
    /// The builder behind them: the token store, the CLI and git's helpers
    /// per kind — what the account, health and sign-in routes read.
    pub hosts: Arc<bisa_codehost::hosts::Hosts>,
    /// Supervised language servers, one per root and language.
    pub lsp: lsp::LspRegistry,
    /// The redactor's vault, the guard's policy and the classifier's cache.
    pub security: Arc<security::SecurityState>,
    /// The Decision-Making Agent: the provider a test put in the settings'
    /// place.
    pub decider: decider::DeciderState,
    /// What agents changed in a checkout, per conversation (ide/20): the
    /// ledgers' locks and the asks waiting for a person's word.
    pub changes: changes::ChangesState,
    /// The HTTP client every connector step calls through, its credentials
    /// over the workspace's keystore, and the OAuth flows waiting for a
    /// browser to come back.
    pub connectors: connectors::ConnectorDesk,
    /// What this machine's connector accounts answered when last checked —
    /// in memory, for this engine's lifetime (`connector_health.rs`).
    pub connector_health: connector_health::ConnectorHealth,
    /// The `connector` steps calling out right now, by run and step, so a
    /// stopped run's call is aborted rather than left to its deadline.
    pub connector_calls: DashMap<(RunId, StepId), tokio::task::AbortHandle>,
    /// The `check` and `judge` tasks in flight, by run and step, so a run
    /// that is ended takes them with it (`effects::abort_step_tasks`) rather
    /// than leaving a shell command to its timeout.
    pub step_tasks: DashMap<(RunId, StepId), tokio::task::AbortHandle>,
    /// The clients every crate reaches the network through, built from the
    /// `network.*` settings and swapped on a write of one (`network.rs`).
    pub http: Arc<bisa_http::Clients>,
    /// Where the latest release is read from (`updates.rs`); `None` means
    /// nothing is ever asked.
    pub updates: Option<updates::UpdatesSource>,
    pub network: network::NetworkState,
    pub socket_path: PathBuf,
    bus: broadcast::Sender<EngineEvent>,
}

impl Inner {
    /// Subscribe to the engine bus.
    /// A handle on the engine's `git`, for a blocking closure, under the
    /// network policy in force — the proxy as curl reads it, `http.version`
    /// when HTTP/1.1 is the only version spoken. Named because `Git` has an
    /// inherent `clone` — the git verb — so `.clone()` on the field would not
    /// be this.
    pub fn git(&self) -> bisa_vcs::Git {
        network::git_under(Clone::clone(&self.git), &self.http.policy())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.bus.subscribe()
    }

    /// What an event about `home` says it is about: the goal, or the
    /// workflow of the run of the workspace — read from its folder.
    /// An owned handle on this state, for a path that holds only a
    /// reference to it; `None` only while the engine is still being built.
    pub(crate) fn arc(&self) -> Option<Arc<Inner>> {
        self.me.get().and_then(std::sync::Weak::upgrade)
    }

    pub(crate) fn home_scope(&self, home: &Home) -> EventScope {
        match home {
            Home::Goal { goal } => EventScope::of_goal(*goal),
            Home::Run { run } => match self.ws.get_run(*run) {
                Ok(run) => EventScope::of_run(&run),
                Err(e) => {
                    tracing::debug!(%run, "an event about a run it cannot read names no workflow: {e}");
                    EventScope::default()
                }
            },
        }
    }

    /// What everything a work item's session emits says it is about: its
    /// run's scope when it serves one, else its home's.
    pub(crate) fn item_scope(&self, spec: &WorkItemSpec) -> EventScope {
        match spec.run.and_then(|run| self.ws.get_run(run).ok()) {
            Some(run) => EventScope::of_run(&run),
            None => self.home_scope(&spec.home),
        }
    }

    /// Publish an event.
    pub fn emit(&self, event: EngineEvent) {
        // The feed's row first, then the frame: a reader that reads the feed
        // again on the frame finds what the frame announced.
        activity::record(self, &event);
        // A send fails only when nobody is subscribed, which is not a fault.
        if self.bus.send(event).is_err() {
            tracing::trace!("engine event dropped: no subscribers");
        }
    }
}

/// Run one iteration of a long-lived task. A panic in it is an `error`
/// line naming the task, never the end of the loop it runs in: a ticker
/// that died with one bad iteration stopped every timer in the process.
pub(crate) async fn survive<F: std::future::Future<Output = ()>>(what: &'static str, fut: F) {
    if let Err(panic) = futures::FutureExt::catch_unwind(std::panic::AssertUnwindSafe(fut)).await {
        tracing::error!(
            target: "bisa_engine",
            task = what,
            "a {what} iteration panicked: {}",
            bisa_log::panic_message(&*panic)
        );
    }
}

/// Spawn a task that **owes something a settlement** — a step's effect (a
/// check, a call to a platform, a judgement), the pump of an agent's reply.
/// Whatever happens inside it, what it owes is settled: a panic is caught
/// here and handed to `unwound` with the words of it. Left uncaught, a step
/// read *running* — a session *thinking* — with nothing at work on it until
/// the node was started again.
pub(crate) fn spawn_settling<F, U>(
    what: &'static str,
    work: F,
    unwound: U,
) -> tokio::task::JoinHandle<()>
where
    F: std::future::Future<Output = ()> + Send + 'static,
    U: FnOnce(String) + Send + 'static,
{
    tokio::spawn(async move {
        let done = futures::FutureExt::catch_unwind(std::panic::AssertUnwindSafe(work)).await;
        if let Err(panic) = done {
            let reason = format!("the {what} panicked: {}", bisa_log::panic_message(&*panic));
            tracing::error!(target: "bisa_engine", task = what, "{reason}");
            unwound(reason);
        }
    })
}

/// [`survive`] for a synchronous walk — the boot's recovery sweeps. A panic
/// is one error line naming the walk; the caller goes on to the next.
/// Answers what the walk answered, or `None` when it did not finish.
pub(crate) fn contain<T>(what: &'static str, walk: impl FnOnce() -> T) -> Option<T> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(walk)) {
        Ok(v) => Some(v),
        Err(panic) => {
            tracing::error!(
                target: "bisa_engine",
                task = what,
                "the {what} panicked and was skipped: {}",
                bisa_log::panic_message(&*panic)
            );
            None
        }
    }
}

pub struct Engine {
    inner: Arc<Inner>,
    /// Held for the engine's lifetime; dropping the engine releases the
    /// workspace to the next one.
    _lock: EngineLock,
    intake_task: tokio::task::JoinHandle<()>,
    conversation_task: tokio::task::JoinHandle<()>,
    collab_task: tokio::task::JoinHandle<()>,
    /// A peer's drawing landing here, relayed to the canvas (`drawings::spawn_listener`).
    drawings_task: tokio::task::JoinHandle<()>,
    /// The listening runtime: the ear, the ticker and the signal worker —
    /// none when `EngineConfig::events_enabled` is off.
    listen_tasks: Vec<tokio::task::JoinHandle<()>>,
    /// The run engine's own clock; never off, whatever listening does.
    wait_task: tokio::task::JoinHandle<()>,
    /// The roster's dead-process sweep (`presence::run_sweeper`).
    sweep_task: tokio::task::JoinHandle<()>,
}

impl Engine {
    /// Bind the intake socket and start the engine.
    pub fn start(
        ws: Workspace,
        catalog: HarnessCatalog,
        config: EngineConfig,
    ) -> Result<Self, EngineError> {
        // The lock comes first: a refused start must bind nothing and spawn
        // nothing, so a second engine leaves no trace but its error.
        let lock = EngineLock::acquire(ws.paths())?;
        let preferred = config.socket_path.clone().unwrap_or_else(|| {
            // Per-process socket name: an embedded engine (one-shot CLI
            // command) must never steal a running daemon's intake socket.
            ws.paths()
                .run_dir()
                .join(format!("engine-{}.sock", std::process::id()))
        });
        let (listener, socket_path) = intake::bind(&preferred)?;
        let (bus, _) = broadcast::channel(4096);
        // The three code hosts, each its CLI first and its API second, over
        // token stores under `identity/codehost/<kind>/`. The credential chain
        // ends at git's own helper, asked over the engine's `git` handle — the
        // configured one, so a test's isolated git is what gets asked. The CLI
        // layer exists only when a runner was handed in (`EngineConfig::cli`):
        // the node that serves a person gives the real one, a fixture none or
        // a fake, so no test ever spawns `gh` on the machine.
        let git_credentials = codehost::git_credentials(&config.git.clone().unwrap_or_default());
        // The clients under the `network.*` settings: the node's CLI built
        // them before the engine so the adapters hold the same handle; an
        // engine nobody handed one builds its own from the same settings.
        let http = config
            .http
            .clone()
            .unwrap_or_else(|| Arc::new(network::clients_from(&ws)));
        let hosts = {
            let root = ws.paths().codehost_tokens_root();
            let mut hosts = if config.code_hosts.is_some() {
                bisa_codehost::hosts::Hosts::file_only(root)
            } else {
                bisa_codehost::hosts::Hosts::new(root)
            }
            .with_git(git_credentials)
            .with_http(Arc::clone(&http));
            if let Some(cli) = &config.cli {
                hosts = hosts.with_cli(Arc::clone(cli));
            }
            Arc::new(hosts)
        };
        let code_hosts = config
            .code_hosts
            .clone()
            .unwrap_or_else(|| Arc::clone(&hosts).registry());
        let presence = presence::Presence::new(Duration::from_secs(config.retain_ended_secs));
        // Read before `config` moves into the struct: the real client unless a
        // test handed in a fake (06 § MCP servers).
        let mcp_probe = config
            .mcp_probe
            .clone()
            .unwrap_or_else(|| Arc::new(bisa_mcp_probe::RmcpProbe));
        // Resolve the cache settings once at startup; a later `cache.*` write
        // refreshes them. `ws` is still in scope here, before it moves
        // into the Inner below.
        let resolved_at_start = ws.settings(None).unwrap_or_default();
        let cache =
            cache::CacheState::new(bisa_core::CacheSettings::from_resolved(&resolved_at_start));
        // The listening runtime's knobs, seeded the same way; a later
        // `events.*` write refreshes them (`listen::refresh_for`).
        let listen_state = listen::ListenState::with_settings(
            bisa_core::EventSettings::from_resolved(&resolved_at_start),
        );
        let ws = Arc::new(ws);
        let security = Arc::new(security::SecurityState::new(Arc::clone(&ws), bus.clone()));
        let connectors =
            connectors::ConnectorDesk::new(Arc::clone(&ws), bus.clone(), Arc::clone(&http));
        let inner = Arc::new(Inner {
            caps: scheduler::ConcurrencyCaps::new(
                config.global_concurrency,
                config.per_adapter_concurrency,
            ),
            presence,
            interactive: interactive::InteractiveDesk::default(),
            git: config.git.clone().unwrap_or_default(),
            ssh: config.ssh.clone(),
            updates: config.updates.clone(),
            committers: identity::CommitterDesk::default(),
            ws,
            catalog,
            log: config.log.clone(),
            config,
            cache,
            registry: AgentRegistry::new(),
            gates: Gates::new(),
            browser: browser::BrowserRequests::new(),
            draw: drawings::desk(),
            folder_server: std::sync::RwLock::new(None),
            mobile_development: mobile_development::MobileDevelopmentState::default(),
            mcp_health: mcp_health::McpHealth::new(mcp_probe),
            pause: PauseGate::new(),
            lifecycle: lifecycle::Lifecycle::new(),
            driving: sessions::Drivers::default(),
            ending: sessions::Stopping::default(),
            me: std::sync::OnceLock::new(),
            active_items: DashMap::new(),
            inflight: DashMap::new(),
            executors: std::sync::Mutex::new(Vec::new()),
            stopping: std::sync::atomic::AtomicBool::new(false),
            clarify_rounds: DashMap::new(),
            models: ModelLedger::new(),
            guided: guided::GuidedState::default(),
            conversation: conversation::ConversationState::default(),
            listen: listen_state,
            waits: waits::WaitState::default(),
            ide_watch: ide::watch::WatchRegistry::default(),
            ide_graph: ide::graph::GraphRegistry::default(),
            ide_status: ide::git::StatusCache::default(),
            notes_git: folder_git::FolderGit::new(folder_git::Folder::Notes),
            drawings_git: folder_git::FolderGit::new(folder_git::Folder::Drawings),
            code_hosts,
            hosts,
            lsp: lsp::LspRegistry::default(),
            security,
            decider: decider::DeciderState::default(),
            changes: changes::ChangesState::default(),
            connectors,
            connector_health: connector_health::ConnectorHealth::default(),
            connector_calls: DashMap::new(),
            step_tasks: DashMap::new(),
            http,
            network: network::NetworkState::default(),
            socket_path,
            bus,
        });
        // Set once: the state knows itself, for the paths that hold only a
        // reference to it. The cell is fresh, so the set cannot have been
        // beaten to it.
        inner
            .me
            .set(Arc::downgrade(&inner))
            .expect("a fresh engine state has no self-reference yet");
        // The machine's `logging.*` settings reach the file layer now; until
        // here the process wrote errors only.
        logging::apply(&inner);
        tracing::info!(
            target: "bisa_engine",
            workspace = %inner.ws.root().display(),
            socket = %inner.socket_path.display(),
            harnesses = inner.catalog.adapters().len(),
            "engine started"
        );
        // What the store validates against: the harnesses this engine can
        // launch, now; each one's model list once it has answered.
        inner.ws.set_known_runtime(KnownRuntime {
            harnesses: inner
                .catalog
                .adapters()
                .iter()
                .map(|a| a.id().to_string())
                .collect(),
            models: vec![],
            effort_harnesses: effort_harnesses(&inner),
            topics: events::TOPICS,
        });
        tokio::spawn(describe_runtime(Arc::clone(&inner)));
        // The restart, in order. Every session the last process was driving
        // is ended — and the child it left behind, if it is still that
        // process, terminated — before anything is resumed into a checkout.
        // Each walk is contained (`contain`): every store read inside them
        // is already tolerated, and a panic on one persisted row must cost
        // that walk, never the start — a boot that dies on the same row at
        // every launch is a platform nobody can open.
        contain("stale session sweep", || sessions::end_stale(&inner));
        // And the files of the terminal sessions it knew, which this process
        // does not: the desk is in memory.
        contain("terminal session files", || {
            interactive::put_away_what_was_left(&inner)
        });
        // A run the last process started whose goal never recorded it — a
        // crash between the run's snapshot and the goal's — is ended here,
        // under the lock this process holds: a plain open must never end
        // one, since a verb opening the workspace beside a running node would
        // end the run that node is in the middle of starting.
        contain("orphan runs", || match inner.ws.end_orphan_runs() {
            Ok(ended) if !ended.is_empty() => {
                tracing::warn!(target: "bisa_engine", ended = ended.len(), "orphan runs ended at start");
            }
            Ok(_) => {}
            Err(e) => {
                tracing::error!(target: "bisa_engine", "the orphan runs could not be ended: {e}");
            }
        });
        // The catalog's own connectors, brought to the bundle's revision —
        // a truth write, so here under the lock and never at a plain open.
        // The bus has no listener yet; the activity feed is the record.
        contain("catalog connectors refresh", || {
            match inner.ws.refresh_catalog_connectors() {
                Ok(refreshed) if !refreshed.definitions.is_empty() => {
                    tracing::info!(
                        target: "bisa_engine",
                        connectors = ?refreshed.definitions,
                        accounts_moved = refreshed.accounts_moved,
                        "catalog connectors refreshed at start"
                    );
                    for slug in &refreshed.definitions {
                        if let Ok(id) = bisa_core::ConnectorId::new(slug) {
                            inner.connector_health.invalidate_connector(&id);
                        }
                    }
                    connectors::announce(&inner, crate::events::ConnectorsChange::Definitions);
                    if refreshed.accounts_moved > 0 {
                        connectors::announce(&inner, crate::events::ConnectorsChange::Accounts);
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::error!(target: "bisa_engine", "the catalog connectors could not be refreshed: {e}");
                }
            }
        });
        // Then one walk over the unfinished runs, from the snapshots: waits
        // re-armed, every live step interrupted through the funnel (the
        // machine resumes, re-runs or fails it), orphans cancelled, a note
        // per goal. Before the intake serves, so no stale session settles an
        // item mid-recovery.
        contain("recovery sweep", || recovery::sweep(&inner));
        // A design the last process was in the middle of died with it: say
        // so on the goal and wake again. After the walk, so a run it failed
        // wakes repair once.
        contain("guided resume", || guided::resume_interrupted(&inner));
        // The questions with nobody durable behind them are withdrawn, said.
        contain("dead question sweep", || {
            recovery::withdraw_dead_questions(&inner)
        });
        // Who commits, read from the repositories: off the boot path, under
        // the ticker guard, so a slow git costs nothing at start.
        tokio::spawn(survive(
            "committer rearm",
            identity::rearm(Arc::clone(&inner)),
        ));
        let wait_task = tokio::spawn(waits::run_ticker(Arc::clone(&inner)));
        let sweep_task = tokio::spawn(presence::run_sweeper(Arc::clone(&inner)));
        let intake_task = tokio::spawn(intake::serve(Arc::clone(&inner), listener));
        let conversation_task = conversation::spawn_listener(&inner);
        let collab_task = collab::spawn_listener(&inner);
        let drawings_task = drawings::spawn_listener(&inner);
        // The ear hears the bus whether or not the rest runs: a run's waits
        // and boundary events are its, never a person's switch's.
        let mut listen_tasks = vec![listen::ear::spawn(&inner)];
        if inner.config.events_enabled {
            listen_tasks.push(tokio::spawn(listen::sources::run_ticker(Arc::clone(
                &inner,
            ))));
            listen_tasks.push(tokio::spawn(listen::dispatch::run_worker(Arc::clone(
                &inner,
            ))));
        }
        Ok(Self {
            inner,
            _lock: lock,
            intake_task,
            conversation_task,
            collab_task,
            drawings_task,
            listen_tasks,
            wait_task,
            sweep_task,
        })
    }

    /// Who holds this workspace's engine lock, without taking it.
    pub fn lock_holder(paths: &bisa_store::Paths) -> Result<Option<LockHolder>, EngineError> {
        EngineLock::holder(paths)
    }

    pub fn workspace(&self) -> &Workspace {
        &self.inner.ws
    }

    pub fn inner(&self) -> &Arc<Inner> {
        &self.inner
    }

    pub fn socket_path(&self) -> &std::path::Path {
        &self.inner.socket_path
    }

    pub fn events(&self) -> broadcast::Receiver<EngineEvent> {
        self.inner.bus.subscribe()
    }

    // ------------------------------------------------------------------
    // Goals, workflows and runs
    // ------------------------------------------------------------------

    /// Capture a new goal — THE creation path for every surface.
    pub fn submit_goal(&self, req: SubmitRequest) -> Result<bisa_core::Goal, EngineError> {
        ops::submit(&self.inner, req)
    }

    /// Capture a new goal for a caller that shows a person what beginning it
    /// minted: the goal, and the public hook secrets of one that listens at
    /// once, shown this once.
    pub fn submit_goal_showing(&self, req: SubmitRequest) -> Result<Submitted, EngineError> {
        ops::submit_showing(&self.inner, req)
    }

    /// The mode a capture takes when it does not say: `goals.default_mode`.
    pub fn default_goal_mode(&self) -> bisa_core::GoalMode {
        ops::default_goal_mode(&self.inner)
    }

    /// Give a goal documents after capture: uploaded bytes, materialised
    /// under its `documents/`, each announced (`documents.rs`).
    pub fn add_goal_documents(
        &self,
        goal: GoalId,
        files: &[bisa_core::AttachmentRef],
    ) -> Result<Vec<bisa_store::GoalDocument>, EngineError> {
        documents::add_documents(&self.inner, goal, files)
    }

    /// Make a run of the goal's workflow by hand with these inputs: started
    /// at once when nothing is live on the goal, queued behind its live run
    /// otherwise. *Run now*, whatever else the workflow begins on.
    pub fn start_run(
        &self,
        goal: GoalId,
        inputs: BTreeMap<String, serde_json::Value>,
    ) -> Result<WorkflowRun, EngineError> {
        ops::begun_by_its_person(&self.inner, goal, &inputs.clone(), || {
            ops::start_run(&self.inner, goal, inputs, RunEntry::by_hand(), None)
        })
    }

    /// Begin a goal's work — a person's *Start*: it listens when its workflow
    /// begins on events, and runs by hand otherwise ([`ops::begin_goal`]).
    pub fn begin_goal(
        &self,
        goal: GoalId,
        inputs: BTreeMap<String, serde_json::Value>,
    ) -> Result<Begun, EngineError> {
        ops::begun_by_its_person(&self.inner, goal, &inputs.clone(), || {
            ops::begin_goal(&self.inner, goal, inputs, Begin::Auto)
        })
    }

    /// A test run of the goal's workflow: begun at `start` as if its event
    /// had happened with `payload`, the mapping read over it.
    pub fn test_run_goal(
        &self,
        goal: GoalId,
        start: &StepId,
        payload: serde_json::Value,
        inputs: BTreeMap<String, serde_json::Value>,
    ) -> Result<WorkflowRun, EngineError> {
        let g = self.inner.ws.get_goal(goal)?;
        let Some(wf) = g.workflow else {
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-goal-has-no-workflow-pick-one-let",
                goal_id = goal.to_string()
            )));
        };
        let wf = self.inner.ws.get_workflow(wf)?;
        // What its person gave, before the sample's mapping is read over
        // it: a project a sample names is a sample's, and attached by nobody.
        let given = inputs.clone();
        let (entry, inputs) = ops::test_entry(
            &wf,
            start,
            payload,
            bisa_core::SignalScope::Goal { goal },
            inputs,
        )?;
        ops::begun_by_its_person(&self.inner, goal, &given, || {
            ops::start_run(&self.inner, goal, inputs, entry, None)
        })
    }

    /// Stop a goal: its sessions ended, its queued runs withdrawn, its live
    /// run cancelled. The goal stays open, ready for a new run.
    pub async fn stop_goal(
        &self,
        goal: GoalId,
        rationale: Option<String>,
    ) -> Result<Stopped, EngineError> {
        ops::stop_goal(&self.inner, goal, rationale).await
    }

    /// Restart a goal: a new run of its last run's workflow and inputs,
    /// started at once ahead of the queue; a live run is cancelled first.
    pub async fn restart_goal(&self, goal: GoalId) -> Result<WorkflowRun, EngineError> {
        ops::restart_goal(&self.inner, goal).await
    }

    /// [`Self::restart_goal`], answering what the restart ended besides.
    pub async fn restart_goal_ended(
        &self,
        goal: GoalId,
    ) -> Result<(WorkflowRun, sessions::Ended), EngineError> {
        ops::restart_goal_ended(&self.inner, goal).await
    }

    /// Take a queued run out of the goal's queue.
    pub fn withdraw_run(&self, goal: GoalId, run: RunId) -> Result<WorkflowRun, EngineError> {
        ops::withdraw_run(&self.inner, goal, run)
    }

    /// Start a run of a library workflow in the workspace: no goal, its own
    /// folder, the workspace's default budget, started at once beside any
    /// other run of it. The library's *Run…*.
    pub fn start_workspace_run(
        &self,
        workflow: WorkflowId,
        inputs: BTreeMap<String, serde_json::Value>,
    ) -> Result<WorkflowRun, EngineError> {
        ops::start_workspace_run(
            &self.inner,
            workflow,
            inputs,
            RunEntry::by_hand(),
            None,
            None,
        )
    }

    /// A test run of a library workflow in the workspace: begun at `start` as
    /// if its event had happened with `payload` — *Run… › Test*.
    pub fn test_run_workflow(
        &self,
        workflow: WorkflowId,
        start: &StepId,
        payload: serde_json::Value,
        inputs: BTreeMap<String, serde_json::Value>,
    ) -> Result<WorkflowRun, EngineError> {
        let wf = self.inner.ws.get_workflow(workflow)?;
        let (entry, inputs) = ops::test_entry(
            &wf,
            start,
            payload,
            bisa_core::SignalScope::Workspace,
            inputs,
        )?;
        ops::start_workspace_run(&self.inner, workflow, inputs, entry, None, None)
    }

    /// Stop one run of the workspace: cancelled, its sessions ended. A
    /// goal's run is stopped from its goal (a conflict here).
    pub async fn stop_run(
        &self,
        run: RunId,
        rationale: Option<String>,
    ) -> Result<WorkflowRun, EngineError> {
        ops::stop_run(&self.inner, run, rationale).await
    }

    /// Restart one run of the workspace: cancelled when it still goes, and
    /// a new run of its workflow started with its inputs, at its start and on
    /// its event.
    pub async fn restart_run(&self, run: RunId) -> Result<WorkflowRun, EngineError> {
        ops::restart_run(&self.inner, run).await
    }

    /// [`Self::stop_run`], answering what the stop ended besides.
    pub async fn stop_run_ended(
        &self,
        run: RunId,
        rationale: Option<String>,
    ) -> Result<(WorkflowRun, sessions::Ended), EngineError> {
        ops::stop_run_ended(&self.inner, run, rationale).await
    }

    /// [`Self::restart_run`], answering what the restart ended besides.
    pub async fn restart_run_ended(
        &self,
        run: RunId,
    ) -> Result<(WorkflowRun, sessions::Ended), EngineError> {
        ops::restart_run_ended(&self.inner, run).await
    }

    /// Stop every run of the workspace of the workflow that is going;
    /// answers the runs stopped. A goal's run of it is the goal's.
    pub async fn stop_workflow(&self, workflow: WorkflowId) -> Result<Vec<RunId>, EngineError> {
        ops::stop_workflow(&self.inner, workflow).await
    }

    /// Restart every run of the workspace of the workflow that is going;
    /// answers the new runs.
    pub async fn restart_workflow(&self, workflow: WorkflowId) -> Result<Vec<RunId>, EngineError> {
        ops::restart_workflow(&self.inner, workflow).await
    }

    /// [`Self::stop_workflow`], answering what the stops ended besides.
    pub async fn stop_workflow_ended(
        &self,
        workflow: WorkflowId,
    ) -> Result<(Vec<RunId>, sessions::Ended), EngineError> {
        ops::stop_workflow_ended(&self.inner, workflow).await
    }

    /// [`Self::restart_workflow`], answering what the restarts ended besides.
    pub async fn restart_workflow_ended(
        &self,
        workflow: WorkflowId,
    ) -> Result<(Vec<RunId>, sessions::Ended), EngineError> {
        ops::restart_workflow_ended(&self.inner, workflow).await
    }

    /// Every run of the goal in queue order, oldest first.
    pub fn runs(&self, goal: GoalId) -> Result<Vec<WorkflowRun>, EngineError> {
        Ok(self.inner.ws.list_runs(goal)?)
    }

    /// Every run of the workspace the workflow has, oldest first.
    pub fn workflow_runs(&self, workflow: WorkflowId) -> Result<Vec<WorkflowRun>, EngineError> {
        Ok(self.inner.ws.list_workflow_runs(workflow)?)
    }

    /// Replace the not-yet-started steps of the goal's running workflow.
    pub fn amend_run(&self, goal: GoalId, draft: NewWorkflow) -> Result<WorkflowRun, EngineError> {
        ops::amend_run(&self.inner, goal, draft, None)
    }

    /// Point a goal at the workflow its next run will use.
    /// Lend the engine the node's folder server, so a session's
    /// `browser_serve` has one to ask (ide/18 §Serving a folder).
    pub fn set_folder_server(&self, server: Arc<dyn browser::FolderServer>) {
        *self
            .inner
            .folder_server
            .write()
            .unwrap_or_else(|e| e.into_inner()) = Some(server);
    }

    pub fn set_workflow(
        &self,
        goal: GoalId,
        workflow: Option<WorkflowId>,
    ) -> Result<bisa_core::Goal, EngineError> {
        ops::set_workflow(&self.inner, goal, workflow)
    }

    /// Answer a waiting `human` step of a run — a goal's or the workspace's.
    pub fn answer_step(
        &self,
        run: RunId,
        step: &StepId,
        answer: &Answer,
    ) -> Result<WorkflowRun, EngineError> {
        let run = self.inner.ws.get_run(run)?;
        // Through the gate when one is open, so the decision is signed and
        // the waiter resolves; straight onto the run otherwise.
        if let Some(gate) = self
            .inner
            .gates
            .pending_for_step(run.id, step)
            .into_iter()
            .next()
        {
            ops::decide(&self.inner, &gate.id, true, None, Some(answer), None)?;
            return Ok(self.inner.ws.get_run(run.id)?);
        }
        ops::record_run_event(
            &self.inner,
            run.id,
            bisa_core::RunEvent::Answered {
                step: step.clone(),
                answer: answer.clone(),
            },
        )
    }

    /// Release a `wait` step a person — or a call — is holding; a payload,
    /// when given, is the step's output.
    pub fn release_step(
        &self,
        run: RunId,
        step: &StepId,
        payload: Option<serde_json::Value>,
    ) -> Result<WorkflowRun, EngineError> {
        waits::release(&self.inner, run, step, payload)?;
        Ok(self.inner.ws.get_run(run)?)
    }

    /// A person did a `human` step by hand: it is done.
    pub fn mark_step_done(&self, run: RunId, step: &StepId) -> Result<WorkflowRun, EngineError> {
        self.answer_step(run, step, &Answer::text("done"))
    }

    /// The goal's current run, when it has one.
    pub fn current_run(&self, goal: GoalId) -> Result<Option<WorkflowRun>, EngineError> {
        ops::current_run(&self.inner, goal)
    }

    /// Record a local workflow. Refused with its problems when it cannot start.
    pub fn create_workflow(&self, new: NewWorkflow) -> Result<Workflow, EngineError> {
        ops::create_workflow(&self.inner, new)
    }

    /// Record a local workflow as a draft, kept with its problems.
    pub fn create_workflow_draft(
        &self,
        new: NewWorkflow,
    ) -> Result<(Workflow, Vec<Problem>), EngineError> {
        ops::create_workflow_draft(&self.inner, new)
    }

    /// Save an edited workflow as the next revision of `id`; the problems
    /// come back rather than refusing the save. `expected_revision` is the
    /// revision the editor holds; a copy that moved is a revision conflict.
    pub fn save_workflow(
        &self,
        id: WorkflowId,
        body: NewWorkflow,
        expected_revision: u64,
    ) -> Result<(Workflow, Vec<Problem>), EngineError> {
        ops::save_workflow(&self.inner, id, body, expected_revision)
    }

    /// The Workflow Agent's write to a library workflow: the next revision,
    /// refused with its problems, refused when the stored copy moved past
    /// `expected_revision`; announced as the agent's hand.
    pub fn revise_workflow(
        &self,
        id: WorkflowId,
        body: NewWorkflow,
        expected_revision: u64,
    ) -> Result<Workflow, EngineError> {
        ops::revise_workflow(&self.inner, id, body, expected_revision)
    }

    /// Describe the runtime to the store again: every harness's model list,
    /// asked of the adapters. Called once at boot in the background; a
    /// surface that refreshes model lists calls it after.
    pub async fn describe_runtime(&self) {
        describe_runtime(Arc::clone(&self.inner)).await;
    }

    /// Ask the Workflow Agent to design the goal's workflow again — after a
    /// stall, a failure or a restart. Refused by name when there is nothing
    /// to design or it is already at work.
    pub fn redesign(&self, goal: GoalId) -> Result<(), EngineError> {
        guided::request_design(&self.inner, goal)
    }

    /// Forget a workflow nobody uses.
    pub fn delete_workflow(&self, id: WorkflowId) -> Result<(), EngineError> {
        ops::delete_workflow(&self.inner, id)
    }

    /// Put a workflow away, or take it back out (`ops::archive_workflow`).
    pub fn archive_workflow(
        &self,
        id: WorkflowId,
        archived: bool,
    ) -> Result<bisa_core::Workflow, EngineError> {
        ops::archive_workflow(&self.inner, id, archived)
    }

    /// What retiring a goal would touch (`retire::preview_goal`).
    pub fn preview_goal_retirement(&self, goal: GoalId) -> Result<retire::Retirement, EngineError> {
        retire::preview_goal(&self.inner, goal)
    }

    /// Retire a goal on a plan: stop its work, close it, settle the projects born of it, archive or delete it.
    pub async fn retire_goal(
        &self,
        goal: GoalId,
        plan: retire::GoalPlan,
    ) -> Result<retire::Retired, EngineError> {
        retire::retire_goal(&self.inner, goal, plan).await
    }

    /// Take an archived goal back out; it stays closed.
    pub fn unarchive_goal(&self, goal: GoalId) -> Result<bisa_core::Goal, EngineError> {
        retire::unarchive_goal(&self.inner, goal)
    }

    /// What retiring a workflow would touch (`retire::preview_workflow`).
    pub fn preview_workflow_retirement(
        &self,
        workflow: WorkflowId,
    ) -> Result<retire::Retirement, EngineError> {
        retire::preview_workflow(&self.inner, workflow)
    }

    /// Retire a workflow on a plan; deletion refused while anything uses it.
    pub async fn retire_workflow(
        &self,
        workflow: WorkflowId,
        plan: retire::WorkflowPlan,
    ) -> Result<retire::Retired, EngineError> {
        retire::retire_workflow(&self.inner, workflow, plan).await
    }

    /// Put a project away, or take it back out (`projects::archive`).
    pub fn archive_project(
        &self,
        project: bisa_core::ProjectId,
        archived: bool,
    ) -> Result<bisa_core::Project, EngineError> {
        projects::archive(&self.inner, project, archived)
    }

    /// Turn a plain-folder project's tree into a git repository on a person's
    /// explicit ask — an adopted folder too (`projects::init_repository`).
    pub async fn init_repository(
        &self,
        project: bisa_core::ProjectId,
        git_config: Vec<(String, String)>,
    ) -> Result<projects::Initialised, EngineError> {
        projects::init_repository(&self.inner, project, git_config).await
    }

    /// Delete a project's records and, with `tree`, its managed folder to the Trash (`projects::delete`).
    pub async fn delete_project(
        &self,
        project: bisa_core::ProjectId,
        tree: bool,
    ) -> Result<projects::Deleted, EngineError> {
        projects::delete(&self.inner, project, tree).await
    }

    /// Every problem a definition has, without recording it.
    pub fn validate_workflow(&self, wf: &Workflow) -> Result<Vec<Problem>, EngineError> {
        ops::validate_workflow(&self.inner, wf)
    }

    /// Copy a goal's design into the library.
    pub fn promote_workflow(&self, id: WorkflowId) -> Result<Workflow, EngineError> {
        ops::promote_workflow(&self.inner, id)
    }

    /// Record a person's design as the goal's own draft, with its problems,
    /// and point the goal at it. `expected_revision` names the revision
    /// edited when the goal already has a design.
    pub fn design_workflow(
        &self,
        goal: GoalId,
        draft: NewWorkflow,
        expected_revision: Option<u64>,
    ) -> Result<(Workflow, Vec<Problem>), EngineError> {
        ops::design_workflow(&self.inner, goal, draft, expected_revision)
    }

    /// Close a goal: its live run and its queued runs are cancelled, its
    /// questions withdrawn, its workstreams released.
    pub fn close_goal(
        &self,
        goal_id: GoalId,
        reason: ClosureReason,
    ) -> Result<bisa_core::Goal, EngineError> {
        ops::close_goal(&self.inner, goal_id, reason)
    }

    /// [`Self::close_goal`], waited for: every session of the goal and of
    /// the goals it spawned is gone when this answers, and it says what
    /// it ended.
    pub async fn close_goal_settled(
        &self,
        goal_id: GoalId,
        reason: ClosureReason,
    ) -> Result<(bisa_core::Goal, sessions::Ended), EngineError> {
        ops::close_goal_settled(&self.inner, goal_id, reason).await
    }

    /// Cancel, reset or otherwise move one work item, filed in `home`.
    pub fn transition_work_item(
        &self,
        home: &Home,
        item: WorkItemId,
        transition: &WorkItemTransition,
    ) -> Result<WorkItemSpec, EngineError> {
        let spec = self.inner.ws.transition_work_item(home, item, transition)?;
        if spec.state.is_terminal() {
            // The mark is stopped before it goes: its session's driver
            // aborts the harness on the spot, never at its next event.
            executor::stop_item(&self.inner, item);
            self.inner.inflight.remove(&item);
            self.inner.active_items.remove(&item);
        }
        Ok(spec)
    }

    /// Forget a settled work item, filed in `home`.
    pub fn delete_work_item(&self, home: &Home, item: WorkItemId) -> Result<(), EngineError> {
        self.inner.ws.delete_work_item(home, item)?;
        executor::stop_item(&self.inner, item);
        self.inner.inflight.remove(&item);
        self.inner.active_items.remove(&item);
        Ok(())
    }

    /// Decide a pending gate. Records the signed decision, resolves waiters,
    /// and applies the consequence: an `approval` step is decided, a `human`
    /// step is answered, an adoption starts the run with `inputs`, an
    /// amendment is applied, an agent's own question wakes it.
    pub fn decide(
        &self,
        gate_id: &str,
        approve: bool,
        rationale: Option<&str>,
        answer: Option<&Answer>,
        inputs: Option<BTreeMap<String, serde_json::Value>>,
    ) -> Result<ApprovalId, EngineError> {
        ops::decide(&self.inner, gate_id, approve, rationale, answer, inputs)
    }

    /// Durable-mirror decide for surfaces without a live gate handle: prefers
    /// a pending in-memory gate of `home` (waiters resolve), else applies the
    /// decision from durable state alone — a goal's, or a run of the
    /// workspace's.
    #[allow(clippy::too_many_arguments)]
    pub fn decide_durable(
        &self,
        home: &Home,
        approve: bool,
        rationale: Option<&str>,
        answer: Option<&Answer>,
        inputs: Option<BTreeMap<String, serde_json::Value>>,
        gate_id: Option<&str>,
        step: Option<&StepId>,
    ) -> Result<DecideOutcome, EngineError> {
        // A step named picks the gate opened for it: two steps waiting at
        // once are two questions, and the person said which.
        let open: Vec<_> = self
            .inbox()
            .into_iter()
            .filter(|g| g.home == *home && g.resolution.is_none())
            .filter(|g| step.is_none_or(|s| g.step.as_ref() == Some(s)))
            .collect();
        // Without a gate named, one pending gate is unambiguous; two are a
        // guess this method refuses to make — deciding the older question
        // when the person meant the adoption was how a proposal got lost.
        if gate_id.is_none() && open.len() > 1 {
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-home-has-pending-gates-name-one",
                home = home.to_string(),
                a0 = (open.len()).to_string(),
                a1 = (open
                    .iter()
                    .map(|g| format!("{} ({})", g.id, g.subject))
                    .collect::<Vec<_>>()
                    .join(", "))
                .to_string()
            )));
        }
        // A gate named and live is decided as itself or not at all: one that
        // is another home's, or decided already, never falls through to
        // whatever else this one owes. An id no live gate has — a question
        // rebuilt from the journal after a restart — is the mirror's below.
        if let Some(named) = gate_id.and_then(|id| self.inner.gates.get(id)) {
            if named.home != *home {
                return Err(EngineError::Invalid(bisa_core::text!(
                    "error-engine-invalid-gate-not-this-home-s",
                    a0 = (named.id).to_string(),
                    home = home.to_string()
                )));
            }
            if named.resolution.is_some() {
                return Err(EngineError::GateAlreadyDecided(named.id));
            }
        }
        let pending = open
            .into_iter()
            .find(|g| gate_id.map(|id| g.id == id).unwrap_or(true));
        if let Some(gate) = pending {
            let decided =
                ops::decide_showing(&self.inner, &gate.id, approve, rationale, answer, inputs)?;
            return Ok(
                ops::outcome_of(&self.inner, home, gate.gate, approve)?.showing(decided.secrets)
            );
        }
        ops::decide_without_engine(&self.inner, home, approve, rationale, answer, inputs, step)
    }

    /// The `wait` steps armed right now: `(run, step, what it waits for)`.
    pub fn armed_waits(&self) -> Vec<(RunId, StepId, bisa_core::WaitFor)> {
        self.inner.waits.armed()
    }

    /// The boundary events armed on one live step of a run, by name.
    pub fn armed_boundaries(&self, run: RunId, step: &StepId) -> Vec<bisa_core::Branch> {
        self.inner.waits.boundaries_of(run, step)
    }

    // ------------------------------------------------------------------
    // Agents, teams, projects: the writes the node routes through here
    // ------------------------------------------------------------------

    /// Flip an agent's enablement and announce the membership change.
    pub fn set_agent_enabled(
        &self,
        id: &AgentId,
        enabled: bool,
    ) -> Result<bisa_core::Agent, EngineError> {
        membership::set_agent_enabled(&self.inner, id, enabled)
    }

    /// Flip a team's enablement and announce the membership change.
    pub fn set_team_enabled(
        &self,
        id: &TeamId,
        enabled: bool,
    ) -> Result<bisa_core::Team, EngineError> {
        membership::set_team_enabled(&self.inner, id, enabled)
    }

    /// Attach a project to a goal: the journal fact, and the event that
    /// tells every open screen. Idempotent in the store.
    pub fn attach_project(
        &self,
        goal: GoalId,
        project: bisa_core::ProjectId,
    ) -> Result<bisa_core::Attachment, EngineError> {
        let attachment = self.inner.ws.attach(goal, project)?;
        self.inner.emit(EngineEvent::scoped(
            goal,
            None,
            EnginePayload::AttachmentChanged {
                project,
                attached: true,
            },
        ));
        Ok(attachment)
    }

    /// Detach a project from a goal. Moves no bytes (invariant I14).
    pub fn detach_project(
        &self,
        goal: GoalId,
        project: bisa_core::ProjectId,
    ) -> Result<(), EngineError> {
        self.inner.ws.detach(goal, project)?;
        self.inner.emit(EngineEvent::scoped(
            goal,
            None,
            EnginePayload::AttachmentChanged {
                project,
                attached: false,
            },
        ));
        Ok(())
    }

    // ------------------------------------------------------------------
    // Inbox / gates / pause
    // ------------------------------------------------------------------

    /// Number of work items of this home — a goal, or a run of the
    /// workspace — currently reserved or executing.
    pub fn active_item_count(&self, home: &Home) -> usize {
        self.inner
            .inflight
            .iter()
            .filter(|e| e.value().home == *home)
            .count()
    }

    pub fn inbox(&self) -> Vec<GateEntry> {
        self.inner.gates.pending()
    }

    /// One gate by id, pending or resolved.
    pub fn gate(&self, id: &str) -> Option<GateEntry> {
        self.inner.gates.get(id)
    }

    pub fn model_health(&self) -> Vec<ModelHealthRow> {
        self.inner
            .models
            .snapshot_cached(self.inner.cache.settings().model_health_ttl())
    }

    /// What a harness's account has left — its usage limits from its own
    /// source, cached (`harness_usage`); `refresh` asks the source again.
    pub async fn harness_usage(&self, harness: &str, refresh: bool) -> bisa_harness::UsageState {
        harness_usage::usage(&self.inner, harness, refresh).await
    }

    /// The latest release of the platform as GitHub lists it — facts, never
    /// a comparison — cached (`updates`); `refresh` asks GitHub again. *Off*
    /// when the engine was started without a source.
    pub async fn check_update(&self, refresh: bool) -> updates::UpdateCheck {
        updates::check(&self.inner, refresh).await
    }

    pub fn pause(&self) {
        self.inner.pause.pause();
        self.inner.emit(EngineEvent::global(EnginePayload::Paused));
    }

    pub fn resume(&self) {
        self.inner.pause.resume();
        self.inner.emit(EngineEvent::global(EnginePayload::Resumed));
    }

    pub fn is_paused(&self) -> bool {
        self.inner.pause.is_paused()
    }

    // ------------------------------------------------------------------
    // Settings — the one write path
    // ------------------------------------------------------------------

    /// Write one setting at one scope. The registry refuses a scope the key
    /// does not allow and a value outside its kind; a success is announced
    /// on the bus so an open editor re-reads without a reload.
    pub fn set_setting(
        &self,
        scope: bisa_core::SettingScope,
        project: Option<bisa_core::ProjectId>,
        key: &str,
        value: serde_json::Value,
    ) -> Result<bisa_core::ResolvedSetting, EngineError> {
        settings::set(&self.inner, scope, project, key, value)
    }

    /// Remove one key from one scope; the value falls back to the next layer.
    pub fn unset_setting(
        &self,
        scope: bisa_core::SettingScope,
        project: Option<bisa_core::ProjectId>,
        key: &str,
    ) -> Result<bisa_core::ResolvedSetting, EngineError> {
        settings::unset(&self.inner, scope, project, key)
    }

    /// The `network.*` settings as the panel reads them, with what is in
    /// force and what keeps it from being (ide/13 §Capabilities › Network).
    pub fn network_status(&self) -> network::NetworkStatus {
        network::status(&self.inner)
    }

    /// One request to `url` through the outbound client, to see whether the
    /// way out works; refused for a URL that is not `http(s)` or is this
    /// machine.
    pub async fn network_check(&self, url: &str) -> Result<network::NetworkCheck, EngineError> {
        network::check(&self.inner, url).await
    }

    /// A snapshot of the resolved `cache.*` settings, for callers
    /// that own a cache the engine does not — the node's harness listing, say.
    pub fn cache_settings(&self) -> bisa_core::CacheSettings {
        self.inner.cache.settings()
    }

    // ------------------------------------------------------------------
    // Security
    // ------------------------------------------------------------------

    /// The policy as this node runs it, its problems, and the last decisions.
    pub fn security_status(&self) -> security::SecurityStatus {
        security::status(&self.inner)
    }

    /// The Decision-Making Agent as it stands on this node: who answers,
    /// whether it can be asked, which points the switch reaches.
    pub fn decider_status(&self) -> Result<decider::DeciderStatus, EngineError> {
        decider::status(&self.inner)
    }

    /// What the platform needs before it can work, as the setup gate and
    /// `bisa doctor` read it (`readiness::readiness`).
    pub async fn readiness(&self) -> readiness::Readiness {
        readiness::readiness(&self.inner).await
    }

    /// The newest judgements this node asked for, newest first.
    pub fn recent_judgements(
        &self,
        limit: usize,
    ) -> Result<Vec<decider::JudgementRecord>, EngineError> {
        decider::recent(&self.inner, limit)
    }

    /// Put `request` to the Decision-Making Agent as it is set up, with
    /// nothing decided and nothing recorded — the *Try it* of Settings ›
    /// Decision Making.
    pub async fn try_decision(
        &self,
        request: &bisa_core::DecisionRequest,
    ) -> Result<bisa_core::DecisionResponse, EngineError> {
        request.validate().map_err(|e| {
            EngineError::Invalid(bisa_core::text!(
                "error-engine-lib-refused",
                detail = e.to_string()
            ))
        })?;
        let (_, result) = decider::ask(&self.inner, request).await;
        result.map_err(EngineError::Provider)
    }

    /// Keep, or forget, the API key of a remote decision provider. The key
    /// goes to this machine's keystore and is never read back out.
    pub fn set_decision_key(
        &self,
        provider: bisa_core::DecisionProviderKind,
        key: Option<&str>,
    ) -> Result<(), EngineError> {
        match key {
            Some(key) => decider::set_key(&self.inner, provider, key),
            None => decider::clear_key(&self.inner, provider),
        }
    }

    /// Answer every judgement from `provider` instead of the one the settings
    /// name — a test's scripted provider; `None` puts the settings back.
    pub fn stand_in_decision_provider(
        &self,
        provider: Option<Arc<dyn bisa_decision::DecisionProvider>>,
    ) {
        self.inner.decider.stand_in(provider);
    }

    /// What the redactor would do to `text` — on a scratch vault.
    pub fn redact_preview(&self, text: &str) -> security::RedactPreview {
        security::redact_preview(&self.inner, text)
    }

    /// `text` through the real redactor — for a route that serves what a
    /// harness wrote in its own words (a transcript).
    pub fn redact(&self, text: &str) -> String {
        self.inner.security.redact(text).text
    }

    /// What the guard rules alone say about a call.
    pub fn guard_preview(&self, tool: &str, input: &serde_json::Value) -> security::GuardPreview {
        security::guard_preview(&self.inner, tool, input, None)
    }

    // ------------------------------------------------------------------
    // Listening
    // ------------------------------------------------------------------

    /// Turn a host on: a library workflow's events start runs of the
    /// workspace, a goal's start runs on the goal. Answers what it listens
    /// with and the public hook secrets minted, shown this once.
    pub fn set_listening(
        &self,
        host: bisa_core::ListenerHost,
        inputs: BTreeMap<String, serde_json::Value>,
        budget: Option<bisa_core::goal::Budget>,
    ) -> Result<TurnedOn, EngineError> {
        match host {
            bisa_core::ListenerHost::Goal { goal } => {
                ops::begun_by_its_person(&self.inner, goal, &inputs.clone(), || {
                    listen::turn::turn_on(&self.inner, host, inputs, budget)
                })
            }
            // A run of the workspace works in the project it names:
            // attachment is a goal's relation.
            bisa_core::ListenerHost::Workspace { .. } => {
                listen::turn::turn_on(&self.inner, host, inputs, budget)
            }
        }
    }

    /// Turn a host off; what its events queued settles.
    pub fn stop_listening(&self, host: bisa_core::ListenerHost) -> Result<(), EngineError> {
        listen::turn::turn_off(&self.inner, host)
    }

    /// A paused goal hears again — with new inputs when given.
    pub fn listen_again(
        &self,
        goal: GoalId,
        inputs: Option<BTreeMap<String, serde_json::Value>>,
    ) -> Result<TurnedOn, EngineError> {
        let given = inputs.clone().unwrap_or_default();
        ops::begun_by_its_person(&self.inner, goal, &given, || {
            listen::turn::listen_again(&self.inner, goal, inputs)
        })
    }

    /// Mint a public hook start's secret anew, shown this once.
    pub fn rotate_hook_secret(
        &self,
        key: &bisa_core::ListenerKey,
    ) -> Result<HookSecret, EngineError> {
        listen::turn::rotate_secret(&self.inner, key)
    }

    /// A hook call — local, or public with its secret already verified by
    /// the caller. Answers the signal written.
    pub fn call_hook(
        &self,
        key: &bisa_core::ListenerKey,
        body: serde_json::Value,
        delivery: Option<&str>,
        door: HookDoor,
    ) -> Result<String, HookRefusal> {
        listen::hooks::call(&self.inner, key, body, delivery, door)
    }

    /// Raise a named signal — a person's, through the node or the CLI.
    pub fn emit_signal(
        &self,
        name: &str,
        payload: serde_json::Value,
        scope: bisa_core::SignalScope,
    ) -> Result<listen::emit::Emitted, EngineError> {
        listen::emit::emit(
            &self.inner,
            name,
            payload,
            scope,
            bisa_core::Chain::default(),
            None,
        )
    }

    /// Raise a named signal for a payload that came from outside this
    /// machine — an A2A task: redacted, and read by the content screen before
    /// anything hears it ([`listen::emit::emit_from_outside`]).
    pub fn emit_outside_signal(
        &self,
        name: &str,
        payload: serde_json::Value,
        scope: bisa_core::SignalScope,
        source: &str,
    ) -> Result<(), EngineError> {
        listen::emit::emit_from_outside(&self.inner, name, payload, scope, source.to_string())
    }

    /// Let a held signal through — a person read what the content screen
    /// would not pass.
    pub fn release_signal(&self, id: &str) -> Result<(), EngineError> {
        let Some(queued) = self.inner.ws.signal(id)? else {
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-store-invalid-signal-not-found",
                id = id.to_string()
            )));
        };
        if queued.state != bisa_store::SignalState::Held {
            return Err(EngineError::Conflict(bisa_core::text!(
                "error-engine-conflict-signal-not-held",
                id = id.to_string(),
                state = queued.state.as_str().to_string()
            )));
        }
        self.inner.ws.release_signal(id)?;
        self.inner.listen.wake();
        Ok(())
    }

    /// The listeners armed right now.
    pub fn armed_listeners(&self) -> Arc<listen::registry::Registry> {
        listen::armed(&self.inner)
    }

    /// One pass of the listening ticker at an explicit `now` — the test door
    /// a schedule, a poll, a check or a project change is driven through.
    pub async fn tick_listeners_at(&self, now: u64) {
        listen::sources::tick(&self.inner, now).await;
    }

    /// One tick of the wait clock at an explicit `now` — the test door a
    /// `delay`, a `schedule` or a boundary timer is driven through.
    pub fn tick_waits_at(&self, now: u64) {
        waits::tick(&self.inner, now);
    }

    /// Settle the signal queue to a standstill.
    pub async fn drain_signals(&self) -> usize {
        listen::dispatch::drain_signals(&self.inner).await
    }

    /// Hear one event as the ear would — the test door, without racing the
    /// bus.
    pub fn hear(&self, event: &EngineEvent) {
        listen::ear::on_event(&self.inner, event);
    }

    /// Stop the engine and let go of it: nothing of it runs once this
    /// returns, and what it held of the workspace is released.
    pub async fn shutdown(self) {
        self.stop().await;
    }

    /// Stop everything this engine runs — its executors, its walks, its
    /// intake — for a holder that cannot give the engine up: the node, whose
    /// state a task that outlived its request may still hold. Saying it
    /// twice stops nothing twice.
    pub async fn stop(&self) {
        self.inner
            .stopping
            .store(true, std::sync::atomic::Ordering::SeqCst);
        // Every session this engine drives is told to stop and waited for —
        // its process gone, or terminated at the deadline — before the tasks
        // are torn down: a node that stops leaves no harness behind.
        let settled = sessions::end_all(&self.inner).await;
        if settled.stopped + settled.terminated + settled.still_live > 0 {
            tracing::info!(target: "bisa_engine", stopped = settled.stopped, terminated = settled.terminated, still_live = settled.still_live, "the engine's sessions were ended with it");
        }
        let executors = std::mem::take(
            &mut *self
                .inner
                .executors
                .lock()
                .unwrap_or_else(|e| e.into_inner()),
        );
        let live = executors.iter().filter(|h| !h.is_finished()).count();
        // Aborted, then awaited: a task ends at its next await point, and a
        // synchronous store write it was in the middle of finishes first —
        // so nothing of this engine runs once `shutdown` returns.
        for handle in &executors {
            handle.abort();
        }
        for handle in executors {
            // Cancelled is the abort's own outcome; a panic was already
            // contained and settled inside the task, so anything else is news.
            if let Err(e) = handle.await {
                if !e.is_cancelled() {
                    tracing::warn!(target: "bisa_engine", "an executor task ended oddly at shutdown: {e}");
                }
            }
        }
        if live > 0 {
            tracing::info!(target: "bisa_engine", tasks = live, "executor tasks ended with the engine; their items are the restart's");
        }
        self.wait_task.abort();
        self.sweep_task.abort();
        self.intake_task.abort();
        self.conversation_task.abort();
        self.collab_task.abort();
        self.drawings_task.abort();
        for task in &self.listen_tasks {
            task.abort();
        }
        if let Err(e) = std::fs::remove_file(&self.inner.socket_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::debug!("intake socket not removed: {e}");
            }
        }
    }
}

#[cfg(test)]
mod contain_tests {
    use super::contain;

    #[test]
    fn a_walk_that_panics_is_skipped_and_the_next_one_runs() {
        let quiet = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let first = contain("a walk that dies", || -> usize { panic!("one bad row") });
        let second = contain("the walk after it", || 3usize);
        std::panic::set_hook(quiet);
        assert_eq!(first, None);
        assert_eq!(second, Some(3));
    }
}

#[cfg(test)]
mod iso_tests {
    use super::EngineError;

    /// A backend that is not on this machine is the machine's state; one
    /// that is here and failed is this node's failure — never read as *try
    /// something else*.
    #[test]
    fn a_missing_backend_is_told_from_one_that_failed() {
        let missing = EngineError::of_iso(bisa_iso::IsoError::unavailable("no APFS here"));
        assert!(matches!(&missing, EngineError::Iso(why) if why.contains("no APFS here")));
        let failed = EngineError::of_iso(bisa_iso::IsoError::other("no space left on device"));
        assert!(matches!(
            &failed,
            EngineError::IsoFailed(why) if why.contains("no space left on device")
        ));
    }
}
