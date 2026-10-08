//! Zero-I/O foundation for Bisa.
//!
//! This crate is the single source of truth for the domain model: identifiers,
//! the workflow definition and its validation, the events a workflow starts
//! on, waits for and hears on a live step (their filters, listeners and the
//! causal chain), the run machine that executes one, work items and workstreams, projects and their attachment to goals,
//! channels and membership, agents and teams, the GEP kind registry with its
//! access-control sets, the settings registry, capability flags, and the
//! shared error taxonomy.
//!
//! It performs no I/O by design (the manifest forbids tokio/rusqlite/axum/
//! reqwest), so every rule here is a pure function with a unit test. Every
//! domain invariant is enforced here, once — see `docs/architecture/02-domain-model.md`.

pub mod activity;
pub mod addon;
pub mod agent;
pub mod archive;
pub mod artifact;
pub mod ask;
pub mod assignee;
pub mod attachment;
pub mod board;
pub mod boundary;
pub mod browser;
pub mod cache_settings;
pub mod caps;
pub mod changes;
pub mod channel;
pub mod collab_settings;
pub mod connector;
pub mod conversation;
pub mod decision;
pub mod draw;
pub mod effort;
pub mod error;
pub mod error_text;
pub mod event;
pub mod event_settings;
pub mod gate;
pub mod git_profile;
pub mod git_settings;
pub mod goal;
pub mod home;
pub mod id;
pub mod invite;
pub mod kind;
pub mod listen;
pub mod mcp;
pub mod member;
pub mod message;
pub mod mobile_development;
pub mod model_plan;
pub mod network_settings;
pub mod note;
pub mod origin;
pub mod owner_scope;
pub mod path;
pub mod pet;
pub mod photo;
pub mod placement;
pub mod project;
pub mod review_note;
pub mod run;
pub mod session;
pub mod settings;
pub mod signal;
pub mod skill;
pub mod start;
pub mod sync;
pub mod tags;
pub mod team;
pub mod template;
pub mod text;
pub mod workflow;
pub mod workitem;
pub mod workstream;
pub use board::{
    rank_at, renumbered, BoardColumn, BoardError, DueDate, Placement, WorkstreamBoard, RANK_STEP,
};

pub use activity::{tag_of, ActivityConcept, ActivityFact, ActivitySource, ActivitySourceKind};
pub use addon::{
    check_bundle_listing, is_bundle_path, is_semver, served_content_type, AddonDock, AddonError,
    AddonFrame, AddonManifest, AddonPermission, AddonProblem, AddonRecord, AddonWindow,
    DEFAULT_ENTRY as ADDON_DEFAULT_ENTRY, MANIFEST_FILE_NAME as ADDON_MANIFEST_FILE_NAME,
    MAX_ADDON_BYTES, MAX_ADDON_FILES, MAX_ADDON_FILE_BYTES, MAX_BUNDLE_DEPTH, MAX_MANIFEST_BYTES,
    SDK_FILE_NAME as ADDON_SDK_FILE_NAME,
};
pub use agent::{
    Agent, AgentError, RespondPolicy, DECISION_MAKING_AGENT_NAME, GENERAL_AGENT_NAME,
    WORKFLOW_AGENT_NAME,
};
pub use archive::Archived;
pub use artifact::{
    mime_of_name, ArtifactKind, ArtifactRef, ArtifactSource, MAX_ARTIFACTS_PER_MESSAGE,
    MAX_ARTIFACT_TITLE_BYTES,
};
pub use ask::{Answer, AnswerError, AskKind, AskOption};
pub use assignee::Assignee;
pub use attachment::{
    AttachmentRef, MAX_ATTACHMENT_BYTES, MAX_FACE_BYTES, MAX_PHOTO_BYTES, SHA256_HEX_LEN,
};
pub use boundary::{may_carry_boundaries, Boundary, BoundaryAct, BoundaryOn, DEFAULT_REMINDERS};
pub use cache_settings::CacheSettings;
pub use caps::{HarnessCaps, ToolTier};
pub use changes::{
    ChangeKind, ChangeLedger, ChangeState, FileChange, FileDiff, Hunk, LineSpan, Merged,
    RestoreTarget, ReviewFile, TurnChanges,
};
pub use channel::{
    events_for_enablement, members, reaches, Audience, Channel, ChannelError, ChannelKind,
    ChannelOrigin, DeletableChannel, Member, MembershipCause, MembershipChange, MembershipEvent,
    RosterPolicy,
};
pub use collab_settings::{
    is_relay_url, CollabSettings, CollaborationSecurity, JoinPolicy, ManualPeer, OutsiderTools,
    SyncSettings,
};
pub use connector::{
    is_media_type, AccountAuth, AuthScheme, ChallengeEncoding, Connector, ConnectorAccount,
    ConnectorProblem, Expect, HttpMethod, Idempotency, JwtAlg, KeyPlace, Operation, OperationBody,
    OutputSpec, Paging, ParamDef, ParamKind, Part, PartSource, ScopeJoin, SecretField,
    TemplateSite, DEFAULT_CLIENT_ID_PARAM, DEFAULT_JWT_TTL_SECS, MAX_CONNECTOR_BYTES,
    MAX_JWT_TTL_SECS, MAX_OPERATION_TIMEOUT_SECS, MAX_PAGES,
};
pub use conversation::{
    validate_title as validate_conversation_title, Conversation, ConversationError,
    ConversationMode, ConversationOrigin, MAX_TITLE_CHARS as MAX_CONVERSATION_TITLE_CHARS,
};
pub use decision::{
    answers_schema, DecisionAnswer, DecisionContractError, DecisionPoint, DecisionProviderKind,
    DecisionQuestion, DecisionRequest, DecisionResponse, DecisionUsage, Judgement,
    JudgementOutcome, NoulCriteria, MAX_REQUEST_BYTES,
};
pub use draw::{
    validate_skeleton, DrawError, Drawing, DrawingSummary, Scene, SceneAppState, DRAW_NOTE,
    DRAW_TOOLS, MAX_DRAWING_ELEMENTS, MAX_SCENE_BYTES, MAX_SKELETON_PER_CALL,
};
pub use effort::{resolve as resolve_effort, Effort, EffortChoice, Resolved as ResolvedEffort};
pub use error::CoreError;
pub use event::{
    GepEventKind, GuidancePhase, GuidanceStatus, JournalEvent, JournalPayload, RunFact, StepFact,
};
pub use event_settings::{EventSettings, ScanBounds, MIN_PR_POLL_SECS};
pub use gate::{ApprovalId, Gate};
pub use git_profile::{
    is_key_path, is_login, is_owner, slug_for_profile, GitProfile, GitProfileError, ProfileSpec,
};
pub use git_settings::CommitterPolicy;
pub use goal::{
    Budget, BudgetSpent, Closure, ClosureReason, Goal, GoalEdge, GoalEdgeKind, GoalMode,
    GoalOrigin, GoalStatus, Holder, GOAL_RUNS_KEPT,
};
pub use home::Home;
pub use id::{
    validate_slug_id, AccountId, AddonId, AgentId, ChannelId, CommitIdStr, ConnectorId,
    ConversationId, DrawingId, GoalId, InviteId, McpId, NoteId, OperationId, PrincipalId,
    ProjectId, RunId, SessionId, Sha256, SkillId, TeamId, TurnId, WorkItemId, WorkflowId,
    WorkspaceId, WorkstreamId,
};
pub use invite::{
    ClaimRefusal, Invite, InviteError, InviteState, INVITE_TTL_HOURS_DEFAULT, INVITE_TTL_HOURS_MAX,
    INVITE_TTL_HOURS_MIN,
};
pub use listen::{
    field_matches, fields_match, glob_match, valid_signal_name, Chain, ChainRefusal, Heard, Hearer,
    ListenerHost, ListenerKey, Listening, MessageFilter, MessageFrom, PauseReason, Paused,
    PlatformFilter, ProjectChange, ProjectFilter, RunEnd, RunFilter, SignalFilter, SignalScope,
    SignalSource,
};
pub use mcp::{
    McpError, McpMount, McpProvenance, McpServer, McpServerConfig, MASK as MCP_MASK,
    RESERVED_MCP_NAME,
};
pub use member::{
    check_member_change, clean_label, MemberError, MemberRole, Permission, WorkspaceMember,
    MAX_LABEL_CHARS,
};
pub use message::{
    split_text, ContextRef, DiffScope, LineRange, Mark, MessageBody, PageRef, ScopeKind,
    MAX_CONTEXT_BYTES, MAX_TEXT_BYTES, MAX_THINKING_BYTES,
};
pub use model_plan::{AllHealthy, ModelChoice, ModelHealthView, ModelPlan, ModelStrategy};
pub use network_settings::{ChildEnv, NetworkSettings, ProxyMode};
pub use note::{Note, NoteError, MAX_NOTE_BYTES};
pub use origin::{AgentOrigin, Origin, ProjectOrigin, StepRef, WorkflowOrigin};
pub use owner_scope::OwnerScope;
pub use path::{FileScope, RelPath};
pub use pet::{
    is_webp, validate_pet_id, webp_dimensions, Pet, PetAnimation, PetError, PetFlavour, PetOrigin,
    CELL_HEIGHT, CELL_WIDTH, MAX_SPRITESHEET_BYTES, SHEET_COLS, SHEET_ROWS, STATES,
};
pub use photo::{check_photo, image_type, is_face, PhotoProfile, PhotoRefusal};
pub use placement::{resolve_step_project, StepProject};
pub use project::{
    validate_slug, Attachment, CodeHost, Project, ProjectRoot, PublishPolicy, Slug, Vcs,
};
pub use review_note::{ReviewNote, ReviewNoteError};
pub use run::{
    CancelCause, Fired, LoopCursor, NoWayIn, RunEffect, RunEntry, RunError, RunEvent, RunScope,
    RunStatus, StepRecord, StepState, WayIn, WorkflowRun, AMBIGUOUS_WRITE, ENDED_FAILED,
};
pub use session::{AskPurpose, SessionOrigin};
pub use settings::{
    check_write as check_setting_write, resolve as resolve_setting,
    resolve_all as resolve_settings, Resolved as ResolvedSetting, Scope as SettingScope,
    SettingDef, SettingsError, REGISTRY as SETTINGS,
};
pub use signal::{Signal, MAX_SIGNAL_PAYLOAD_BYTES};
pub use skill::{Skill, SkillError, MAX_SKILL_BYTES};
pub use start::{
    map_event, Admission, Cadence, Dropped, FireOn, Guard, MapError, Overlap, ResolveError,
    Schedule, StartOn,
};
pub use tags::{TagEntity, TagMatch, Tags};
pub use team::{Team, TeamError};
pub use template::{
    json_path, no_values, placeholders, placeholders_in, render as render_template,
    render_in as render_template_in, render_with as render_template_with, shell_quote, Absence,
    GoalText, Grammar, Placeholder, RenderContext, TemplateCtx, TemplateError,
};
pub use text::{Arg, Localize, Text};
pub use workflow::{
    branch, start_steps_in, Branch, Case, CheckKind, Condition, ConditionCtx, Family, Finish, Flow,
    InputDef, InputError, InputKind, InputName, Join, JudgeOption, NoSyntaxChecks, OnFail, Pick,
    Point, Problem, ProblemKind, Rule, RunOutcome, Step, StepId, StepKind, SyntaxChecks,
    ValidationCtx, ValueRef, WaitFor, Workflow, DEFAULT_MAX_ITERATIONS, DEFAULT_MAX_VISITS,
};
pub use workitem::{
    WorkItemError, WorkItemSpec, WorkItemState, WorkItemTransition, DEFAULT_HARNESS,
};
pub use workstream::{
    branch_name_for, SourceSpecError, Workstream, WorkstreamError, WorkstreamKind,
    WorkstreamSource, WorkstreamState, WorkstreamTransition,
};
