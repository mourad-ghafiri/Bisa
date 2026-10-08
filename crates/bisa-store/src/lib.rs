//! Workspace storage for Bisa.
//!
//! **The filesystem is truth. `index.sqlite` is a cache and can be deleted at
//! any moment without loss** (`docs/architecture/08-persistence.md`). Truth
//! lives as signed Nostr events (journal JSONL + addressable snapshots) plus
//! JSON records for the objects that have no journal; the SQLite index is
//! rebuilt from them in foreign-key order.
//!
//! The domain lives in `bisa-core`. This crate owns where things go
//! ([`paths`]), how they are written ([`paths::write_atomic`]), and the cache.

pub mod activity_log;
pub mod addons;
pub mod agents;
pub mod boot;
pub mod catalog;
pub mod changes;
pub mod channels;
pub mod check;
pub mod connectors;
pub mod conversation;
pub mod conversations;
pub mod core_agents;
pub mod decision_making_agent;
pub mod drawings;
pub mod error;
pub mod error_text;
pub mod goal_documents;
pub mod governance;
pub mod held;
pub mod identity;
pub mod inbox;
pub mod index;
pub mod ingest;
pub mod invites;
pub mod journal;
pub mod listening;
pub mod mcp;
pub mod members;
pub mod notes;
pub mod owner;
pub mod paths;
pub mod pets;
pub mod problems;
pub use pets::PetSprite;
pub mod projects;
pub mod recall;
pub mod review;
pub mod runs;
pub mod settings;
pub mod signals;
pub mod skills;
pub mod snapshots;
pub mod syntax;
pub mod tags;
pub mod teams;
pub mod tree;
pub mod usage;
pub mod workflows;
pub mod workspace;
pub mod workstreams;

pub use addons::{AddonEntry, AddonOffer};
pub use agents::NewAgent;
pub use boot::{BootObserver, BootPhase, Quiet};
pub use catalog::{
    BuiltinAddon, BuiltinPet, Catalog, CatalogDescription, CatalogDetail, CatalogEntry,
    CatalogKind, Installed, Refreshed, CATALOG,
};
pub use changes::{MAX_CHANGE_BLOB, UNREADABLE_LEDGER};
pub use check::{check_files, Finding, FindingKind};
pub use connectors::{AccountSecrets, NewConnector, NewConnectorAccount, SecretSource};
pub use conversation::{MessageArtifact, MessageAttachment, PageBefore, ScopeRef};
pub use conversations::{ConversationFilter, NewConversation};
pub use decision_making_agent::{decision_making_agent, DecisionMakingAgent};
pub use drawings::{scene_hash, DrawingPatch, NewDrawing};
pub use error::StoreError;
pub use goal_documents::{distinct_name, GoalDocument};
pub use governance::{GatePolicy, Governance};
pub use held::{HeldMessage, HeldReason};
pub use identity::{
    attest_agent, verify_attestation, AutoKeyStore, FileKeyStore, Identity, KeyStore,
    KeyStoreChoice, KeyringStore, MemoryKeyStore, KEYSTORE_ENV,
};
pub use inbox::{DecisionRow, ReadMarker};
pub use index::{
    ActivityCursor, ActivityRecord, ArtifactListRow, ConversationRow, GoalRow, HomeKey, MessageRow,
    ReactionRow, RunRow, RunStepRow, SessionKind, SessionRow, SessionStatus, SignalRow,
    WorkItemRow, WorkflowRow, SCHEMA, SCHEMA_VERSION,
};
pub use ingest::{remote_goal_admissible, run_snapshot_admissible, IngestOutcome};
pub use invites::Claimed;
pub use journal::{EventLog, JournalAddr, JsonlEventLog};
pub use listening::ListenerRuntime;
pub use mcp::NewMcp;
pub use members::Admission;
pub use notes::{body_hash, NewNote, NotePatch};
pub use owner::OwnerFilter;
pub use paths::{
    append_line, resolve_within, resolve_within_new, sanitise_file_name, write_atomic, AgentPaths,
    GoalPaths, HomePaths, Paths, ProjectPaths,
};
pub use problems::{ProblemKind, WorkspaceProblem};
pub use projects::NewProject;
pub use recall::RecallRecord;
pub use review::NewReviewNote;
pub use runs::{approval_subject, CancelledRun};
pub use signals::{QueuedSignal, SignalState};
pub use skills::NewSkill;
pub use snapshots::SnapshotStore;
pub use syntax::{normalize_cron, StoreSyntaxChecks};
pub use tags::TagCount;
pub use tree::{
    content_hash, ignore_rules, EntryKind, FileContent, FileEntry, FileScope, FileTree, Placement,
    ReadCap, FILE_MAX_BYTES, TREE_DEFAULT_DEPTH, TREE_MAX_DEPTH, TREE_MAX_ENTRIES,
};
pub use usage::{Reference, ReferenceKind, Usage, UsageKind};
pub use workflows::{NewWorkflow, WorkflowScope};
pub use workspace::PeopleChange;
pub use workspace::{EventAudience, KnownRuntime, NewGoal, PostOrigin, StoreEvent, Workspace};
pub use workstreams::{primary_is_the_project, WorkstreamFilter};
