//! Store error taxonomy.
//!
//! Domain refusals arrive as the core's own error types, transparently: a
//! caller matching on `StoreError::Run(_)` sees exactly what
//! `WorkflowRun::apply` said. `Invalid` carries the store's own considered
//! refusals, rendered verbatim, because every one of them reaches a person.

use bisa_core::{
    AgentError, ChannelError, CoreError, InputError, Problem, RunError, SettingsError, TeamError,
    WorkItemError, WorkstreamError,
};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("io error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("key store error: {0}")]
    KeyStore(String),
    #[error("key not found: {0}")]
    KeyNotFound(String),
    #[error("nostr error: {0}")]
    Nostr(String),
    #[error("encryption required for kind {0} but no owner key available")]
    EncryptionRequired(u16),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("goal not found: {0}")]
    GoalNotFound(String),
    #[error("work item not found: {0}")]
    WorkItemNotFound(String),
    #[error("session not found: {0}")]
    SessionNotFound(String),
    #[error("workstream not found: {0}")]
    WorkstreamNotFound(String),
    #[error("project not found: {0}")]
    ProjectNotFound(String),
    #[error("conversation not found: {0}")]
    ConversationNotFound(String),
    #[error("workflow not found: {0}")]
    WorkflowNotFound(String),
    #[error("run not found: {0}")]
    RunNotFound(String),
    /// An agent, a team, a skill, an MCP server or a channel nobody defined
    /// under that id — the node's 404, never the 400 of a malformed body.
    #[error("{kind} not found: {id}")]
    DefinitionNotFound { kind: &'static str, id: String },
    /// A goal runs one workflow at a time: a run of it is live or queued.
    #[error("goal {goal} has an unfinished run {run}; finish or stop it first")]
    RunNotFinished { goal: String, run: String },
    /// Only a queued run can be started from the queue or withdrawn.
    #[error("run {run} of goal {goal} is not queued")]
    RunNotQueued { goal: String, run: String },
    /// The queued signal already began its run: one occurrence makes one
    /// run, however often its dispatch is replayed.
    #[error("signal {signal} already began run {run}")]
    AlreadyDispatched { signal: String, run: String },
    /// A definition that cannot start. Every problem, so the caller can fix
    /// them all at once rather than one per save.
    #[error("{}", describe_problems(.0))]
    WorkflowInvalid(Vec<Problem>),
    /// The object moved under the caller: the stored copy is at `actual`,
    /// the caller edited `expected`. The one answer to two writers, whatever
    /// the clock said; the caller reloads and merges.
    #[error(
        "{kind} {id} is at revision {actual}; you edited revision {expected} — reload and merge"
    )]
    RevisionConflict {
        kind: &'static str,
        id: String,
        expected: u64,
        actual: u64,
    },
    /// `promote` on something that is already in the library.
    #[error("workflow {0} is already in the library")]
    AlreadyLibrary(String),
    /// A `remove_*` while something still points at the record — a team at
    /// an agent, an agent at a skill. The request is well-formed and what it
    /// is about is in use: a conflict, never a malformed body.
    #[error("{0}")]
    StillUsed(bisa_core::Text),
    /// A run's inputs do not fit its definition (kind, required, unknown).
    #[error(transparent)]
    Inputs(#[from] InputError),
    /// A record on disk that this build cannot read. There is no migration
    /// path: it was written by another shape of the code and is skipped by a
    /// list, moved aside by the open when it must be, never converted.
    #[error("{path}: not a {what} this build can read ({reason}). It was written by another shape of the code: a list skips it, the open moves it aside under quarantine/ when it must, and `bisa workspace check` names every such file")]
    Unreadable {
        path: String,
        what: &'static str,
        reason: String,
    },
    /// The owner's key is there and is not a key. The one file that stops an
    /// open: every record here is signed by it, and a new key would orphan
    /// them all — so none is minted, and nothing else is touched.
    #[error("{path}: not a key this build can read ({reason}); no new key is minted, since every record here is signed by the old one — restore the file from a backup or the keyring, nothing else is touched")]
    OwnerKeyUnreadable { path: String, reason: String },
    #[error(transparent)]
    Run(#[from] RunError),
    #[error(transparent)]
    WorkItem(#[from] WorkItemError),
    #[error(transparent)]
    Workstream(#[from] WorkstreamError),
    #[error(transparent)]
    Channel(#[from] ChannelError),
    #[error(transparent)]
    Agent(#[from] AgentError),
    #[error(transparent)]
    Team(#[from] TeamError),
    #[error(transparent)]
    Settings(#[from] SettingsError),
    /// Every other domain rule the core states: an invalid id, a bad slug, a
    /// context list over its cap, a nested team.
    #[error(transparent)]
    Core(CoreError),
    #[error("gate decision {0} not found or does not authorize this step")]
    GateDecisionInvalid(String),
    #[error("{0}")]
    GatePolicy(bisa_core::Text),
    #[error("budget exhausted for goal {0}")]
    BudgetExhausted(String),
    #[error("recall conflict: the stored value changed (current hash {current_hash})")]
    RecallConflict {
        current_value: String,
        current_hash: String,
    },
    /// A note, a drawing — or a file, through the IDE — was edited under the
    /// caller. Carries what is there now (a note's body as a string, a
    /// drawing's scene as an object) and its hash, so the caller can merge or
    /// adopt rather than re-read and guess.
    #[error("{what} conflict: it changed since you read it (current hash {current_hash})")]
    EditConflict {
        what: &'static str,
        current: serde_json::Value,
        current_hash: String,
    },
    /// A refusal the caller is meant to read. Rendered verbatim, with no
    /// prefix, because a prefix like "invalid data:" made a considered answer
    /// read as a malfunction.
    #[error("{0}")]
    Invalid(bisa_core::Text),
}

pub(crate) fn describe_problems(problems: &[Problem]) -> String {
    let list: Vec<String> = problems
        .iter()
        .map(|p| match &p.step {
            Some(step) => format!("{step}: {}", p.text),
            None => p.text.to_string(),
        })
        .collect();
    format!(
        "the workflow has {} problem{}: {}",
        problems.len(),
        if problems.len() == 1 { "" } else { "s" },
        list.join("; ")
    )
}

impl StoreError {
    pub fn io(path: impl Into<String>, source: std::io::Error) -> Self {
        StoreError::Io {
            path: path.into(),
            source,
        }
    }

    pub fn nostr(e: impl std::fmt::Display) -> Self {
        StoreError::Nostr(e.to_string())
    }

    /// A record at `path` that should have been a `what` and did not parse.
    pub fn unreadable(
        path: &std::path::Path,
        what: &'static str,
        reason: impl std::fmt::Display,
    ) -> Self {
        StoreError::Unreadable {
            path: path.display().to_string(),
            what,
            reason: reason.to_string(),
        }
    }

    /// Whether this is a refusal a person should read as an answer rather
    /// than as a failure — the store's own, or one of the core's rules.
    pub fn is_refusal(&self) -> bool {
        matches!(
            self,
            StoreError::Invalid(_)
                | StoreError::Run(_)
                | StoreError::WorkItem(_)
                | StoreError::Workstream(_)
                | StoreError::Channel(_)
                | StoreError::Agent(_)
                | StoreError::Team(_)
                | StoreError::Settings(_)
                | StoreError::Core(_)
                | StoreError::GatePolicy(_)
                | StoreError::GateDecisionInvalid(_)
                | StoreError::WorkflowInvalid(_)
                | StoreError::RunNotFinished { .. }
                | StoreError::RunNotQueued { .. }
                | StoreError::AlreadyDispatched { .. }
                | StoreError::RevisionConflict { .. }
                | StoreError::AlreadyLibrary(_)
                | StoreError::StillUsed(_)
                | StoreError::Inputs(_)
        )
    }
}

/// The remaining core errors convert through `CoreError`, so `?` works on any
/// domain rule without a variant per type.
macro_rules! via_core {
    ($($t:ty),* $(,)?) => {
        $(impl From<$t> for StoreError {
            fn from(e: $t) -> Self {
                StoreError::from(CoreError::from(e))
            }
        })*
    };
}

via_core!(
    bisa_core::AddonError,
    bisa_core::McpError,
    bisa_core::NoteError,
    bisa_core::DrawError,
    bisa_core::ConversationError,
    bisa_core::ReviewNoteError,
    bisa_core::MemberError,
    bisa_core::SkillError,
    bisa_core::AnswerError,
    bisa_core::TemplateError,
);

/// The core's transparent variants each get their own arm above so a caller
/// can match on them; everything else the core can say lands in `Core`.
impl From<CoreError> for StoreError {
    fn from(e: CoreError) -> Self {
        match e {
            CoreError::Run(r) => StoreError::Run(r),
            CoreError::WorkItem(w) => StoreError::WorkItem(w),
            CoreError::Workstream(w) => StoreError::Workstream(w),
            CoreError::Channel(c) => StoreError::Channel(c),
            CoreError::Agent(a) => StoreError::Agent(a),
            CoreError::Team(t) => StoreError::Team(t),
            CoreError::Settings(s) => StoreError::Settings(s),
            other => StoreError::Core(other),
        }
    }
}
