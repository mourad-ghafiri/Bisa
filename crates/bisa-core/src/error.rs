//! Shared error taxonomy for the zero-I/O layer.

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("invalid principal id (expected 64 lowercase hex chars): {0:?}")]
    InvalidPrincipal(String),
    #[error(transparent)]
    Run(#[from] crate::run::RunError),
    #[error(transparent)]
    Template(#[from] crate::template::TemplateError),
    #[error(transparent)]
    WorkItem(#[from] crate::workitem::WorkItemError),
    #[error(transparent)]
    Workstream(#[from] crate::workstream::WorkstreamError),
    #[error(transparent)]
    Channel(#[from] crate::channel::ChannelError),
    #[error(transparent)]
    Agent(#[from] crate::agent::AgentError),
    #[error(transparent)]
    Team(#[from] crate::team::TeamError),
    #[error(transparent)]
    Skill(#[from] crate::skill::SkillError),
    #[error(transparent)]
    Mcp(#[from] crate::mcp::McpError),
    #[error(transparent)]
    Note(#[from] crate::note::NoteError),
    #[error(transparent)]
    Draw(#[from] crate::draw::DrawError),
    #[error(transparent)]
    Conversation(#[from] crate::conversation::ConversationError),
    #[error(transparent)]
    Pet(#[from] crate::pet::PetError),
    #[error(transparent)]
    Addon(#[from] crate::addon::AddonError),
    #[error(transparent)]
    Member(#[from] crate::member::MemberError),
    #[error(transparent)]
    ReviewNote(#[from] crate::review_note::ReviewNoteError),
    #[error(transparent)]
    GitProfile(#[from] crate::git_profile::GitProfileError),
    #[error(transparent)]
    Settings(#[from] crate::settings::SettingsError),
    #[error(transparent)]
    Answer(#[from] crate::ask::AnswerError),
    #[error(transparent)]
    Decision(#[from] crate::decision::DecisionContractError),
    #[error("unknown {what} {value:?}")]
    UnknownDecisionWord { what: &'static str, value: String },
    #[error("unknown effort {0:?}: expected auto, minimal, low, medium, high, xhigh or max")]
    UnknownEffort(String),
    #[error("unknown GEP kind: {0}")]
    UnknownKind(u16),
    #[error("unknown gate {0:?}: expected approval, escalation or publish")]
    UnknownGate(String),
    #[error("unknown goal status {0:?}: expected draft, running, waiting, done, failed or closed")]
    UnknownGoalStatus(String),
    #[error("unknown holder {0:?}: expected you, agents, world, finished or design")]
    UnknownHolder(String),
    #[error("unknown message scope {0:?}: expected channel, goal or conversation")]
    UnknownScopeKind(String),
    #[error("a reply's thinking is {bytes} bytes; the most kept is {max}", max = crate::message::MAX_THINKING_BYTES)]
    ThinkingTooLarge { bytes: usize },
    #[error("a message of {bytes} bytes is too long; the most is {max} — send what is a file as an attachment", max = crate::message::MAX_TEXT_BYTES)]
    TextTooLarge { bytes: usize },
    #[error("unknown file scope {0:?}: expected goal, workstream or work_item")]
    UnknownFileScope(String),
    #[error("unknown artifact kind {0:?}")]
    UnknownArtifactKind(String),
    #[error("unknown activity concept {0:?}: expected workspace, goals, workflows, projects, channels, agents or node")]
    UnknownActivityConcept(String),
    #[error("a message carries at most {max} artifacts, not {0}", max = crate::artifact::MAX_ARTIFACTS_PER_MESSAGE)]
    TooManyArtifacts(usize),
    #[error("invalid artifact name {0:?}: a file name, not a path")]
    InvalidArtifactName(String),
    #[error("invalid artifact title {0:?}: a title is a line of words")]
    InvalidArtifactTitle(String),
    #[error("artifact title is {bytes} bytes; the most is {max}", max = crate::artifact::MAX_ARTIFACT_TITLE_BYTES)]
    ArtifactTitleTooLong { bytes: usize },
    #[error("artifact is {bytes} bytes; the most is {max}", max = crate::attachment::MAX_ATTACHMENT_BYTES)]
    ArtifactTooLarge { bytes: u64 },
    #[error("unknown settings scope {0:?}: expected machine, workspace or project")]
    UnknownScope(String),
    #[error("unknown goal mode {0:?}: expected auto, guided or manual")]
    UnknownGoalMode(String),
    #[error("invalid assignee {0:?}: expected agent:<id>, human:<64 hex> or team:<id>")]
    InvalidAssignee(String),
    #[error(
        "invalid project slug {0:?}: expected 1-64 chars of lowercase a-z, 0-9, \
         '-' or '_', starting with a letter or digit"
    )]
    InvalidSlug(String),
    #[error("invalid {what} id {value:?}: expected 1-64 chars of a-z, 0-9, '-', '_', '.', ':', starting with a letter or digit")]
    InvalidId { what: String, value: String },
    #[error("invalid {what} {value:?}: expected 1-32 chars of a-z, 0-9, '-', '_', starting with a letter")]
    InvalidWord { what: String, value: String },
    #[error("invalid branch {0:?}: expected 1-64 chars, not blank")]
    InvalidBranch(String),
    #[error("invalid relative path {0:?}: no leading slash, no '..' component")]
    InvalidRelPath(String),
    #[error("invalid sha256 {0:?}: expected 64 lowercase hex chars")]
    InvalidHash(String),
    #[error("invalid line range {start}..={end}: lines are 1-based and end >= start")]
    InvalidRange { start: u32, end: u32 },
    #[error(
        "message context is {bytes} bytes; the cap is {}",
        crate::message::MAX_CONTEXT_BYTES
    )]
    ContextTooLarge { bytes: usize },
    #[error(
        "invalid tag {0:?}: expected letters, digits and separators only, \
         e.g. `engineering` or `go-to-market`"
    )]
    InvalidTag(String),
    #[error("unknown tag entity {0:?}: expected one of agent, team, channel, skill, mcp, project, goal, workflow, connector")]
    UnknownTagEntity(String),
    #[error("too many tags ({0}): at most {max} per object", max = crate::tags::MAX_TAGS)]
    TooManyTags(usize),
}
