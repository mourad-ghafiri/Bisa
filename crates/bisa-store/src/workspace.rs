//! The `Workspace` facade: the typed API the engine consumes.
//!
//! Filesystem is truth; the SQLite index is a rebuildable cache. The layout is
//! [`crate::paths`]'s alone — see `docs/architecture/04-workspace-project-goal.md` for the tree and
//! `08` for the schema and the rebuild order.
//!
//! **Nothing here writes a goal's status.** A goal has no lifecycle: where it
//! stands is a projection of its current run ([`Goal::status`]), and the run
//! changes only through [`Workspace::record_run_event`] (`runs.rs`), which
//! hands the event to [`bisa_core::WorkflowRun::apply`] and writes what
//! comes back. The same shape holds for work items
//! ([`Workspace::transition_work_item`]) and workstreams
//! ([`Workspace::transition_workstream`]).

use crate::error::StoreError;
use crate::identity::{AutoKeyStore, Identity, KeyStore};
use crate::index::{
    ActivityCursor, ActivityRecord, GoalRow, HomeKey, Index, SessionKind, SessionRow,
    SessionStatus, WorkItemRow,
};
use crate::journal::{EventLog, JournalAddr, JsonlEventLog};
use crate::paths::Paths;
use crate::problems::{ProblemKind, ProblemSink, WorkspaceProblem};
use crate::snapshots::SnapshotStore;
use crate::workstreams::WorkstreamFilter;
use bisa_core::event::{JournalEvent, JournalPayload};
use bisa_core::goal::{Budget, BudgetSpent};
use bisa_core::kind::{KIND_GOAL, KIND_WORKFLOW_RUN, KIND_WORK_ITEM};
use bisa_core::workitem::{WorkItemSpec, WorkItemTransition};
use bisa_core::{
    ActivityConcept, ActivityFact, ActivitySource, ActivitySourceKind, Answer, ApprovalId,
    Assignee, Gate, Goal, GoalEdgeKind, GoalId, GoalMode, GoalOrigin, GoalStatus, Home,
    PrincipalId, RunId, SessionId, Tags, WorkItemId, WorkflowRun,
};
use bisa_core::{CancelCause, RunEvent};
use nostr::event::Tag;
use nostr::key::Keys;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, RwLock};

thread_local! {
    /// Whether this thread holds the index guard — the re-entry tell.
    static INDEX_HELD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The index, locked: a [`MutexGuard`] that also marks the thread as holding
/// it, so a second `idx()` on the same thread is caught (see [`Workspace::idx`]).
pub(crate) struct IndexGuard<'a>(MutexGuard<'a, Index>);

impl std::ops::Deref for IndexGuard<'_> {
    type Target = Index;
    fn deref(&self) -> &Index {
        &self.0
    }
}

impl std::ops::DerefMut for IndexGuard<'_> {
    fn deref_mut(&mut self) -> &mut Index {
        &mut self.0
    }
}

impl Drop for IndexGuard<'_> {
    fn drop(&mut self) {
        INDEX_HELD.set(false);
    }
}
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn mint_ulid() -> ulid::Ulid {
    ulid::Ulid::from_datetime(SystemTime::now())
}

#[derive(Serialize, Deserialize)]
struct LedgerLine {
    tokens: u64,
    usd_cents: u64,
    wall_secs: u64,
    at: u64,
}

#[derive(Serialize, Deserialize, Default)]
struct EdgeFile {
    edges: Vec<(String, String)>, // (to_id, kind)
}

/// Who may read an event on the wire. `Restricted` events are pairwise-wrapped
/// to the listed participants only — never the workspace key.
#[derive(Clone, Debug, PartialEq)]
pub enum EventAudience {
    Workspace,
    Restricted(Vec<PrincipalId>),
}

impl From<&bisa_core::Audience> for EventAudience {
    fn from(a: &bisa_core::Audience) -> Self {
        match a {
            bisa_core::Audience::Workspace => EventAudience::Workspace,
            bisa_core::Audience::Restricted(p) => EventAudience::Restricted(p.clone()),
        }
    }
}

/// Whether a post is somebody asking, or the platform announcing.
///
/// **The one thing that decides whether an unaddressed message wakes anybody.**
/// Local only: never written to the log, never serialized, never synced. It is
/// a required argument at every call site, so anything that posts is silent by
/// default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostOrigin {
    /// Somebody asked. An unaddressed message may wake the general agent.
    Asked,
    /// The platform announced something. Never triages.
    Announced,
}

/// A locally-authored write, for the net layer to publish outward. Ingested
/// remote events never re-emit (that would echo loops).
#[derive(Clone, Debug)]
pub enum StoreEvent {
    /// A fact was appended to a home's journal — a goal's, or a run of the
    /// workspace's own.
    JournalAppended {
        home: Home,
        event: nostr::event::Event,
    },
    /// A snapshot filed under a home was written: a goal, its runs and work
    /// items and designs, or a run of the workspace and its work items.
    SnapshotWritten {
        kind: u16,
        home: Home,
        event: nostr::event::Event,
    },
    /// A conversation fact (message / reaction / retraction) in a scope.
    ConversationAppended {
        scope: String,
        event: nostr::event::Event,
        audience: EventAudience,
        origin: PostOrigin,
    },
    /// A message from a person on another node landed in a scope
    /// (14-collaboration): applied to the log like any other, and said here
    /// so the engine may read it before an agent does. Never re-published —
    /// the host relays facts by its own pump, not by echo.
    RemoteMessageArrived {
        scope: String,
        event: nostr::event::Event,
        audience: EventAudience,
        author: bisa_core::PrincipalId,
        role: bisa_core::MemberRole,
    },
    /// A peer's drawing landed (19 — Drawings): adopted into the index and
    /// the repository like any other, and said here so the engine may tell
    /// an open canvas what the store now holds. Never re-published.
    RemoteDrawingArrived { drawing: bisa_core::Drawing },
    /// A person joined, left or changed role (14-collaboration): the host's
    /// pump tells them and the people they share a room with; the engine
    /// tells the screens.
    PeopleChanged {
        pubkey: bisa_core::PrincipalId,
        change: PeopleChange,
    },
    /// An invitation moved: made, claimed, admitted, refused, withdrawn.
    InviteChanged { invite: bisa_core::Invite },
    /// A scope was marked read or unread. Local, but it has to reach this
    /// machine's other windows.
    ReadMarkerSet { scope: String },
    /// A non-goal addressable snapshot (channel / agent / team /
    /// skill / project / workflow).
    ConversationSnapshot {
        kind: u16,
        d: String,
        event: nostr::event::Event,
        audience: EventAudience,
    },
}

/// What moved for a person, for [`StoreEvent::PeopleChanged`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "change")]
pub enum PeopleChange {
    Joined {
        role: bisa_core::MemberRole,
    },
    Left,
    RoleChanged {
        role: bisa_core::MemberRole,
    },
    /// The person's label or face moved — the owner's own row included.
    ProfileChanged,
}

pub struct Workspace {
    pub(crate) paths: Paths,
    pub identity: Identity,
    pub(crate) owner: Keys,
    pub(crate) log: JsonlEventLog,
    /// One snapshot store for every namespace; the namespace strings come from
    /// [`Paths`].
    pub(crate) snapshots: SnapshotStore,
    index: Mutex<Index>,
    /// Serialises every write of a work item — `put_work_item`,
    /// `transition_work_item`, `claim_work_item`: each reads, checks and
    /// writes under it, so two claimers cannot both read `open`, and a cancel
    /// and the executor's settling cannot each replace the state the other
    /// just wrote. The engine lock makes one process the invariant; this
    /// makes one writer at a time the invariant inside it.
    item_writes: Mutex<()>,
    /// Serialises `record_run_event`: two step completions on one run read,
    /// apply and write under it, so neither can overwrite the other's record.
    /// A writer outside this process is caught by the snapshot's
    /// compare-and-swap instead.
    pub(crate) run_writes: Mutex<()>,
    /// A settings write is read the layer, change one key, write the layer:
    /// two at once to one scope would each write the layer as it was before
    /// the other, and one key would be lost. Held across the three steps.
    pub(crate) settings_writes: Mutex<()>,
    /// A held message is held, restated or released by reading the list,
    /// changing one row and writing the list — and every message from
    /// another node is read in a task of its own. Two at once would each
    /// write the list as it was before the other: a verdict lost to a
    /// release, or a released row written back. Held across the three steps.
    pub(crate) held_writes: Mutex<()>,
    /// A note is written by two hands — the person's editor through the node
    /// and an agent's tool through the intake — each reading the note,
    /// checking the hash and writing. Held across the three steps, so two
    /// writes at once never each write the body as it was before the other.
    pub(crate) notes_writes: Mutex<()>,
    /// The same for a drawing: the canvas's autosave and the bridge's save
    /// each read, compare the hash and write under it.
    pub(crate) drawings_writes: Mutex<()>,
    /// What the runtime around this store can launch — the harness ids,
    /// which of them take an effort and, per harness, the models it lists.
    /// The engine fills it at boot and again when a model list is refreshed;
    /// validation reads it. Empty until an engine has described itself (an
    /// offline tool), and then the harness checks are skipped.
    known_runtime: RwLock<KnownRuntime>,
    pub(crate) store_events: tokio::sync::broadcast::Sender<StoreEvent>,
    /// What this open and its rebuild found wrong and worked around — a
    /// file quarantined, a record skipped, a row the index could not take.
    /// Recomputed at every open; the quarantine folder is the durable record.
    /// Shared with the snapshot store, which names a file it moves aside.
    problems: ProblemSink,
}

/// The runtime as validation needs to know it. See
/// [`Workspace::set_known_runtime`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnownRuntime {
    /// Harness ids the engine can launch.
    pub harnesses: Vec<String>,
    /// Per harness id, the model ids it lists. A harness with no entry has
    /// an unknown list, and model pins on it are not checked.
    pub models: Vec<(String, Vec<String>)>,
    /// The harnesses, among `harnesses`, that take an effort — the ones
    /// whose adapter declares `HarnessCaps::EFFORT`. A step that pins an
    /// effort and names none of them is a problem.
    pub effort_harnesses: Vec<String>,
    /// The topics the engine's bus emits — what a `platform` start or wait
    /// may name. Empty until an engine has spoken, and then the topic check
    /// is skipped.
    pub topics: &'static [&'static str],
}

impl Workspace {
    /// Open (or initialize) a workspace with the OS keyring + file fallback.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        Self::open_observed(root, &crate::boot::Quiet)
    }

    /// [`Self::open`], saying each phase of the open to `observer`.
    pub fn open_observed(
        root: impl Into<PathBuf>,
        observer: &dyn crate::boot::BootObserver,
    ) -> Result<Self, StoreError> {
        let root: PathBuf = root.into();
        let ks = AutoKeyStore::new(Paths::new(&root).identity_dir());
        Self::open_with_keystore_observed(root, Box::new(ks), observer)
    }

    /// Open with an injected key store (tests use `MemoryKeyStore`).
    pub fn open_with_keystore(
        root: impl Into<PathBuf>,
        ks: Box<dyn KeyStore>,
    ) -> Result<Self, StoreError> {
        Self::open_with_keystore_observed(root, ks, &crate::boot::Quiet)
    }

    /// [`Self::open_with_keystore`], saying each phase of the open to
    /// `observer` — what a process waiting on the open relays to a person,
    /// so a long rebuild reads as work and not as a node that wedged.
    pub fn open_with_keystore_observed(
        root: impl Into<PathBuf>,
        ks: Box<dyn KeyStore>,
        observer: &dyn crate::boot::BootObserver,
    ) -> Result<Self, StoreError> {
        observer.phase(crate::boot::BootPhase::OpeningWorkspace);
        let root: PathBuf = root.into();
        let paths = Paths::new(&root);
        for dir in [
            paths.goals_dir(),
            paths.projects_dir(),
            paths.sessions_dir(),
        ] {
            std::fs::create_dir_all(&dir)
                .map_err(|e| StoreError::io(dir.display().to_string(), e))?;
        }
        crate::identity::ensure_private_dir(&paths.run_dir())?;

        let identity = Identity::new(ks);
        let owner = identity.owner()?;
        let index = Index::open(&paths.index_db())?;
        let rebuild = index.is_fresh();
        let (store_events, _) = tokio::sync::broadcast::channel(1024);
        let problems = ProblemSink::default();
        let ws = Self {
            log: JsonlEventLog::new(paths.clone()),
            snapshots: SnapshotStore::with_problems(paths.clone(), problems.clone()),
            index: Mutex::new(index),
            item_writes: Mutex::new(()),
            run_writes: Mutex::new(()),
            settings_writes: Mutex::new(()),
            held_writes: Mutex::new(()),
            notes_writes: Mutex::new(()),
            drawings_writes: Mutex::new(()),
            known_runtime: RwLock::new(KnownRuntime::default()),
            paths,
            identity,
            owner,
            store_events,
            problems,
        };
        // Bootstrap, in this order and nowhere else: the owner is a member,
        // the cache is rebuilt if it was discarded, then the three permanent
        // objects are ensured. `ensure_*` is idempotent and runs on every
        // open, which is what makes it the guarantee rather than the schema's
        // triggers.
        //
        // **The owner key alone stops an open.** Every other file this build
        // cannot read is quarantined or skipped and named in
        // [`Self::problems`]: a torn member file is moved aside and the
        // owner is a member again, a governance document is read as the
        // owner's alone until it is fixed, a `general` channel is made again,
        // a settings layer costs its values and not the resolution, and a
        // rebuild skips the records it cannot read. A crash never costs the
        // workspace, and never asks for the folder to be deleted.
        ws.ensure_owner_member()?;
        if rebuild {
            ws.rebuild_index_observed(observer)?;
        }
        // A snapshot written a moment before a crash can be ahead of its
        // projection; the live runs are re-indexed from truth at every open,
        // so the engine's first read never trusts a row the crash left stale.
        ws.reconcile_live()?;
        ws.governance_or_default_at_open()?;
        ws.ensure_core_agents()?;
        ws.ensure_general_channel()?;
        // A built-in addon this binary ships at a version other than the one
        // installed takes the new bundle; the person's switch and grants stay.
        ws.refresh_builtin_addons()?;
        Ok(ws)
    }

    /// Describe the runtime around this store, for validation: which harness
    /// ids exist, which take an effort and which models each lists. The
    /// engine calls this at boot
    /// and whenever a harness's model list is refreshed; the whole value is
    /// replaced, so a harness that disappeared disappears here too.
    pub fn set_known_runtime(&self, runtime: KnownRuntime) {
        let mut known = self.known_runtime.write().unwrap_or_else(|poisoned| {
            tracing::warn!("known-runtime lock was poisoned; continuing");
            poisoned.into_inner()
        });
        *known = runtime;
    }

    /// The runtime as last described. Empty until an engine has spoken.
    pub fn known_runtime(&self) -> KnownRuntime {
        self.known_runtime
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Subscribe to locally-authored writes. Lagging receivers miss events.
    pub fn subscribe_store_events(&self) -> tokio::sync::broadcast::Receiver<StoreEvent> {
        self.store_events.subscribe()
    }

    pub(crate) fn emit_store_event(&self, event: StoreEvent) {
        // A send fails only when nobody is listening, which is fine.
        if self.store_events.send(event).is_err() {
            tracing::trace!("no store event listeners");
        }
    }

    /// The index, recovering from a poisoned lock.
    ///
    /// A panic while the lock was held (in another thread, in a route
    /// handler) used to poison it for every later caller, turning one bug into
    /// a workspace that answers nothing. The index is a cache of files, so the
    /// worst case after recovery is a stale row the next rebuild fixes.
    ///
    /// **Never re-entered.** The lock is a plain mutex: a method that holds the
    /// guard and calls another that locks again hangs the thread forever. So a
    /// method that runs under a guard takes the `&Index` it was handed (the
    /// `*_in` methods), a loop never iterates a locked read (bind the rows,
    /// then loop), and a debug build turns the mistake into a panic naming
    /// the rule rather than a hang (`tests/it/locking.rs` reads the sources too).
    pub(crate) fn idx(&self) -> IndexGuard<'_> {
        if INDEX_HELD.replace(true) {
            if cfg!(debug_assertions) {
                panic!(
                    "the index lock is already held by this thread — a method under the guard \
                     must take the `&Index` it was handed (`*_in`), never lock again"
                );
            }
            tracing::error!(
                "the index lock is re-entered on one thread; this call will not return"
            );
        }
        IndexGuard(self.index.lock().unwrap_or_else(|poisoned| {
            tracing::warn!("index lock was poisoned; continuing with the cache as it stands");
            poisoned.into_inner()
        }))
    }

    pub fn root(&self) -> &Path {
        self.paths.root()
    }

    /// Where anything in this workspace goes.
    pub fn paths(&self) -> &Paths {
        &self.paths
    }

    pub fn owner_keys(&self) -> &Keys {
        &self.owner
    }

    pub fn owner_principal(&self) -> PrincipalId {
        PrincipalId::new(self.owner.public_key().to_hex()).expect("owner pubkey is valid hex")
    }

    /// The address a goal's journal facts hang off: the goal's, by its
    /// author.
    pub(crate) fn goal_addr(&self, goal: &Goal) -> JournalAddr {
        JournalAddr {
            author_pubkey_hex: goal.author.as_hex().to_string(),
            home: Home::Goal { goal: goal.id },
        }
    }

    /// The address a run of the workspace's journal facts hang off: the
    /// run's, by this workspace's owner, who makes every one.
    pub(crate) fn run_addr(&self, run: RunId) -> JournalAddr {
        JournalAddr {
            author_pubkey_hex: self.owner.public_key().to_hex(),
            home: Home::Run { run },
        }
    }

    /// The address a home's journal hangs off: a goal is read for its
    /// author, a run of the workspace's is known from its id alone.
    pub(crate) fn journal_addr(&self, home: &Home) -> Result<JournalAddr, StoreError> {
        match home {
            Home::Goal { goal } => Ok(self.goal_addr(&self.get_goal(*goal)?)),
            Home::Run { run } => Ok(self.run_addr(*run)),
        }
    }

    /// That a home is there to write to: the goal, or the run of the
    /// workspace, it names.
    fn check_home(&self, home: &Home) -> Result<(), StoreError> {
        match home {
            Home::Goal { goal } => self.get_goal(*goal).map(|_| ()),
            Home::Run { run } => self.get_run(*run).map(|_| ()),
        }
    }

    // ------------------------------------------------------------------
    // Goals
    // ------------------------------------------------------------------

    /// Capture a goal. `origin` is the caller's to state — the engine says
    /// `Spawned` or `Run`, a person's capture is `Captured` — because a
    /// goal's provenance is decided by whoever made it, once. A spawned goal
    /// gets its `refines` edge to the parent here, so the relation and the
    /// origin cannot disagree. A new goal listens to nothing: it listens once
    /// its work begins on a design with event starts.
    pub fn create_goal(&self, new: NewGoal) -> Result<Goal, StoreError> {
        if new.statement.trim().is_empty() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-goal-needs-statement"
            )));
        }
        if let Some(parent) = new.origin.parent() {
            self.get_goal(parent)?;
        }
        let id = GoalId::from_ulid(mint_ulid());
        let at = now_secs();
        let statement = new.statement.trim().to_string();
        let goal = Goal {
            id,
            statement: statement.clone(),
            // A title of spaces is no title: the lists fall back to the statement.
            title: new
                .title
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty()),
            author: self.owner_principal(),
            workflow: None,
            run: None,
            runs: vec![],
            listening: None,
            closed: None,
            origin: new.origin,
            budget: Default::default(),
            mode: new.mode,
            assignees: new.assignees,
            tags: new.tags,
            revision: 1,
            archived: None,
            created_at: at,
        };
        let addr = self.goal_addr(&goal);
        let je = JournalEvent {
            home: addr.home,
            author: goal.author.clone(),
            at,
            payload: JournalPayload::Note {
                text: format!("goal captured: {statement}"),
            },
        };
        let captured = self
            .log
            .append(&addr, &je, &self.owner, &self.owner.public_key(), None)?;
        self.emit_store_event(StoreEvent::JournalAppended {
            home: addr.home,
            event: captured.clone(),
        });
        self.write_goal_snapshot(&goal, at)?;
        self.index_goal(&goal, None)?;
        // The same seams every journal event passes — the search index and
        // the feed — so a rebuild, which replays the journal, reproduces
        // exactly the rows the capture wrote.
        if let Some(text) = searchable_text(&je.payload) {
            self.idx()
                .index_text(&captured.id.to_hex(), &id.to_string(), &text)?;
        }
        self.record_journal_activity(&je)?;
        if let Some(parent) = goal.origin.parent() {
            self.add_edge(id, parent, GoalEdgeKind::Refines)?;
        }
        Ok(goal)
    }

    pub(crate) fn write_goal_snapshot(&self, goal: &Goal, at: u64) -> Result<(), StoreError> {
        let d = goal.id.to_string();
        let event = self.snapshots.put(
            &Paths::ns_goal(goal.id),
            KIND_GOAL,
            &d,
            goal,
            goal.revision,
            &self.owner,
            at,
            None,
            goal.tags.as_slice(),
        )?;
        self.emit_store_event(StoreEvent::SnapshotWritten {
            kind: KIND_GOAL,
            home: Home::Goal { goal: goal.id },
            event,
        });
        Ok(())
    }

    /// Index a goal's row. `run` is its current run when the caller holds it;
    /// when it does not, the run is read so the cached status is the truth's.
    pub(crate) fn index_goal(
        &self,
        goal: &Goal,
        run: Option<&WorkflowRun>,
    ) -> Result<(), StoreError> {
        let loaded;
        let run = match (run, goal.run) {
            (Some(r), _) => Some(r),
            (None, Some(id)) => {
                loaded = self.get_run(id).ok();
                loaded.as_ref()
            }
            (None, None) => None,
        };
        let idx = self.idx();
        idx.in_transaction(|| self.index_goal_in(&idx, goal, run))
    }

    /// The goal's projection on an index the caller already holds — inside
    /// the caller's transaction when it has one.
    ///
    /// `run` is what the caller resolved *before* locking ([`Self::index_goal`]
    /// does); nothing is read through the workspace here, since that would
    /// lock the index this method already holds.
    pub(crate) fn index_goal_in(
        &self,
        idx: &Index,
        goal: &Goal,
        run: Option<&WorkflowRun>,
    ) -> Result<(), StoreError> {
        idx.upsert_goal(&goal_row(goal, run))?;
        idx.set_tags(
            bisa_core::TagEntity::Goal,
            &goal.id.to_string(),
            goal.tags.as_slice(),
        )?;
        idx.index_text(
            &format!("{}:statement", goal.id),
            &goal.id.to_string(),
            &goal.statement,
        )
    }

    pub fn get_goal(&self, id: GoalId) -> Result<Goal, StoreError> {
        let d = id.to_string();
        match self
            .snapshots
            .get::<Goal>(&Paths::ns_goal(id), KIND_GOAL, &d)?
        {
            Some((goal, _)) => Ok(goal),
            None => Err(StoreError::GoalNotFound(d)),
        }
    }

    /// The goals put away, newest first — what a list shows only when asked.
    pub fn list_archived_goals(&self) -> Result<Vec<Goal>, StoreError> {
        let rows = self.idx().list_archived_goals()?;
        self.goals_of_rows(rows)
    }

    /// The goals behind index rows, in the rows' order. **A list tolerates
    /// one broken file**: a row whose snapshot is gone, was written by
    /// another shape of the code, or names no goal is skipped with an
    /// `error` naming it — one file must never take the inbox, the goals
    /// screen and the guided resume down with it. A single read
    /// ([`Self::get_goal`]) stays strict, and `rebuild_index` is the one
    /// reconciler of the index against the disk; a list never repairs.
    fn goals_of_rows(&self, rows: Vec<crate::index::GoalRow>) -> Result<Vec<Goal>, StoreError> {
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let read =
                r.id.parse::<GoalId>()
                    .map_err(|e| {
                        StoreError::Invalid(bisa_core::text!(
                            "error-store-invalid-bad-id-index",
                            e = e.to_string()
                        ))
                    })
                    .and_then(|id| self.get_goal(id));
            if let Some(goal) = tolerated("goal", &r.id, read)? {
                out.push(goal);
            }
        }
        Ok(out)
    }

    /// Goals with one status (`None` = every goal) that are not archived, by
    /// id. The status is the cached projection in the index — the one the
    /// rows and screens read; an archived goal is [`Self::list_archived_goals`]'s.
    pub fn list_goals(&self, status: Option<GoalStatus>) -> Result<Vec<Goal>, StoreError> {
        let rows = {
            let idx = self.idx();
            idx.list_goals(status.map(GoalStatus::as_str))?
        };
        self.goals_of_rows(rows)
    }

    /// Every goal directory on disk, by id. The filesystem is the truth the
    /// rebuild walks.
    pub(crate) fn goal_ids_on_disk(&self) -> Result<Vec<GoalId>, StoreError> {
        let dir = self.paths.goals_dir();
        let mut out = Vec::new();
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(StoreError::io(dir.display().to_string(), e)),
        };
        for entry in entries.flatten() {
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            match entry.file_name().to_string_lossy().parse::<GoalId>() {
                Ok(id) => out.push(id),
                Err(_) => tracing::warn!("{}: not a goal id, skipping", entry.path().display()),
            }
        }
        out.sort();
        Ok(out)
    }

    /// Persist a modified goal (statement/title/budget/assignees/tags/mode
    /// edits), bumping the revision. What an edit may not do: close or reopen
    /// (`set_goal_closed`), point at a workflow (`set_goal_workflow`), start
    /// or forget a run (`create_run`), begin or stop listening
    /// (`set_listening`), or rewrite where the goal came from.
    pub fn update_goal(&self, mut goal: Goal) -> Result<Goal, StoreError> {
        let current = self.get_goal(goal.id)?;
        if current.closed != goal.closed {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-closing-reopening-go-through-set-goal-closed"
            )));
        }
        if current.archived != goal.archived {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-archiving-unarchiving-go-through-set-goal-archived"
            )));
        }
        if current.workflow != goal.workflow {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-workflow-goal-runs-set-through-set-goal"
            )));
        }
        if current.run != goal.run || current.runs != goal.runs {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-runs-started-through-create-run-never-edited"
            )));
        }
        if current.listening != goal.listening {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-listening-set-through-set-listening"
            )));
        }
        if current.origin != goal.origin {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-goal-s-origin-recorded-capture-never-edited"
            )));
        }
        if goal.statement.trim().is_empty() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-goal-needs-statement"
            )));
        }
        goal.author = current.author;
        goal.created_at = current.created_at;
        goal.revision = current.revision + 1;
        let at = now_secs();
        self.write_goal_snapshot(&goal, at)?;
        self.index_goal(&goal, None)?;
        Ok(goal)
    }

    /// Record a signed gate decision on the journal of the gate's home — the
    /// goal it is about, or the run of the workspace. Returns the approval to
    /// hand to the step it decides (`RunEvent::Decided`), or to keep as the
    /// record of an adoption, an amendment or an answer.
    pub fn record_decision(
        &self,
        home: &Home,
        gate: Gate,
        approve: bool,
        subject: &str,
        rationale: Option<&str>,
        answer: Option<&Answer>,
    ) -> Result<ApprovalId, StoreError> {
        // The local signer is always the owner key, and it must still pass
        // the gate's policy — a Listed policy can exclude even the owner.
        let signer = self.owner_principal();
        if !self.gate_policy_allows_for(gate, &signer, home.goal())? {
            return Err(StoreError::GatePolicy(bisa_core::text!(
                "error-store-gate-policy-may-not-decide-gate-under-current-governance",
                signer = signer.to_string(),
                gate = gate.to_string()
            )));
        }
        let addr = self.journal_addr(home)?;
        let at = now_secs();
        let je = JournalEvent {
            home: *home,
            author: signer.clone(),
            at,
            payload: JournalPayload::Decision {
                gate,
                approve,
                subject: subject.to_string(),
                rationale: rationale.map(str::to_string),
                answer: answer.cloned(),
            },
        };
        let event = self
            .log
            .append(&addr, &je, &self.owner, &self.owner.public_key(), None)?;
        self.emit_store_event(StoreEvent::JournalAppended {
            home: *home,
            event: event.clone(),
        });
        self.idx().add_approval(
            &HomeKey::from(home),
            gate.as_str(),
            subject,
            signer.as_hex(),
            approve,
            at,
        )?;
        // The feed's row: the same seam a rebuild replays
        // (`index_journal_fact`), so a rebuilt feed holds what this one did.
        self.record_journal_activity(&je)?;
        Ok(ApprovalId(event.id.to_hex()))
    }

    /// The signer of the decision `approval` names — iff a journal event with
    /// that id exists, is a Decision for `gate` with the verdict `approve`,
    /// carries `subject` when one is asked for, and its author passes the
    /// gate's governance policy. Anything else is refused by name.
    pub(crate) fn decision_author(
        &self,
        home: &Home,
        gate: Gate,
        approval: &ApprovalId,
        subject: Option<&str>,
        approve: bool,
    ) -> Result<PrincipalId, StoreError> {
        let addr = self.journal_addr(home)?;
        let events = self.log.replay(&addr, &self.owner)?;
        let found = events.iter().find(|(ev, je)| {
            ev.id.to_hex() == approval.0
                && matches!(
                    &je.payload,
                    JournalPayload::Decision { gate: g, approve: a, subject: s, .. }
                        if *g == gate && *a == approve && subject.is_none_or(|want| want == s)
                )
        });
        match found {
            Some((_, je)) if self.gate_policy_allows_for(gate, &je.author, home.goal())? => {
                Ok(je.author.clone())
            }
            Some((_, je)) => Err(StoreError::GatePolicy(bisa_core::text!(
                "error-store-gate-policy-decision-signed-who-may-not-decide-gate",
                a0 = (approval.0).to_string(),
                a1 = (je.author).to_string(),
                gate = gate.to_string()
            ))),
            None => Err(StoreError::GateDecisionInvalid(approval.0.clone())),
        }
    }

    /// Does the local journal hold an approving Decision for `gate` — on
    /// `subject`, when one is asked for — whose author passes the governance
    /// policy? Used when a remote run snapshot records an `approval` step as
    /// done.
    pub(crate) fn has_authorized_decision(
        &self,
        home: &Home,
        gate: Gate,
        subject: Option<&str>,
    ) -> Result<bool, StoreError> {
        let addr = self.journal_addr(home)?;
        for (_, je) in self.log.replay(&addr, &self.owner)? {
            if let JournalPayload::Decision {
                gate: g,
                approve: true,
                subject: s,
                ..
            } = &je.payload
            {
                if *g == gate
                    && subject.is_none_or(|want| want == s)
                    && self.gate_policy_allows_for(gate, &je.author, home.goal())?
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Delete a goal entirely: its directory AND its index rows.
    ///
    /// Its attached projects are **detached, never deleted** (I14): a project
    /// is a workspace citizen. What it listened with goes with it — its
    /// listeners' memories, its hook secrets, its queued signals; workstreams
    /// made for it keep their record under their project and lose the
    /// reference.
    pub fn delete_goal(&self, id: GoalId) -> Result<(), StoreError> {
        let goal = self.get_goal(id)?;
        // Read while its design is still indexed: the hook starts whose
        // secrets go with the goal.
        let hook_steps = match goal.workflow {
            Some(wf) => self
                .get_workflow(wf)
                .map(|wf| crate::workflows::hook_steps(&wf))
                .unwrap_or_default(),
            None => vec![],
        };
        // The goal's own designs go with its folder below; what has to happen
        // first is the refusal — anything else that still names one refuses
        // the deletion by name — and what has to happen at all is their rows.
        self.release_goal_designs(id)?;
        self.forget_host(&bisa_core::ListenerHost::Goal { goal: id }, &hook_steps)?;
        let dir = self.paths.goal(id).dir().to_path_buf();
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(StoreError::io(dir.display().to_string(), e)),
        }
        // Its notes and drawings live in their repositories, not its folder:
        // they leave with it.
        let scope = bisa_core::OwnerScope::Goal { id };
        let scoped = |base: std::path::PathBuf| {
            crate::paths::Paths::scoped_dir(&base, "goal", Some(&id.to_string()))
        };
        self.remove_notes_of(scope.clone(), &scoped(self.paths.notes_dir()))?;
        self.remove_drawings_of(scope, &scoped(self.paths.drawings_dir()))?;
        // So do the conversations about it.
        self.remove_conversations_of("goal", &id.to_string())?;
        self.idx().delete_goal(&id.to_string())
    }

    // ------------------------------------------------------------------
    // Journal
    // ------------------------------------------------------------------

    /// Append a fact authored by `signer` (owner or attested agent) to a
    /// home's journal — a goal's, or a run of the workspace's.
    pub fn append_journal(
        &self,
        home: &Home,
        payload: JournalPayload,
        signer: &Keys,
        attestation: Option<Tag>,
    ) -> Result<String, StoreError> {
        let addr = self.journal_addr(home)?;
        if let Home::Run { run } = home {
            // A run of the workspace's journal exists once the run does.
            self.get_run(*run)?;
        }
        let at = now_secs();
        let author = PrincipalId::new(signer.public_key().to_hex())?;
        let searchable = searchable_text(&payload);
        let je = JournalEvent {
            home: *home,
            author,
            at,
            payload,
        };
        let event = self
            .log
            .append(&addr, &je, signer, &self.owner.public_key(), attestation)?;
        self.emit_store_event(StoreEvent::JournalAppended {
            home: *home,
            event: event.clone(),
        });
        // Search finds goals: a goal's facts are indexed for it.
        if let (Some(text), Some(goal)) = (searchable, home.goal()) {
            self.idx()
                .index_text(&event.id.to_hex(), &goal.to_string(), &text)?;
        }
        // The feed's row: the same seam a rebuild replays (`index_journal_fact`).
        self.record_journal_activity(&je)?;
        Ok(event.id.to_hex())
    }

    // ------------------------------------------------------------------
    // The activity feed (the Pulse)
    // ------------------------------------------------------------------

    /// One fact into the feed's index, and — for a fact with no truth file
    /// of its own (the engine's) — into the activity log first, so a rebuilt
    /// index holds it again. A journal fact and a message are already truth
    /// elsewhere and are indexed alone.
    pub fn record_activity(&self, fact: &ActivityFact, durable: bool) -> Result<i64, StoreError> {
        if durable {
            crate::activity_log::append(&self.paths.activity_dir(), fact)?;
        }
        self.idx().record_activity(fact)
    }

    /// A journal fact's row: the concept the core decides, the payload
    /// verbatim, the source its home files under — the goal, or a run of the
    /// workspace's workflow, whose row the Inbox and the Pulse read it on.
    /// Called on append and on rebuild.
    pub(crate) fn record_journal_activity(&self, je: &JournalEvent) -> Result<(), StoreError> {
        let event = serde_json::to_value(&je.payload)?;
        let fact = ActivityFact {
            at: je.at,
            concept: ActivityConcept::of_journal(&je.home, &je.payload),
            kind: bisa_core::tag_of(&event),
            source: self.activity_source_of(&je.home)?,
            author: Some(je.author.to_string()),
            event,
        };
        self.idx().record_activity(&fact).map(|_| ())
    }

    /// Where a home's feed rows file: the goal itself, or the workflow a run
    /// of the workspace runs — read from its row, else from its folder.
    fn activity_source_of(&self, home: &Home) -> Result<ActivitySource, StoreError> {
        match home {
            Home::Goal { goal } => Ok(ActivitySource::new(
                ActivitySourceKind::Goal,
                goal.to_string(),
            )),
            Home::Run { run } => {
                let indexed = self.idx().get_run(&run.to_string())?.map(|r| r.workflow_id);
                let workflow = match indexed {
                    Some(workflow) => workflow,
                    None => self
                        .workspace_run_snapshot(*run)?
                        .map(|r| r.workflow.id.to_string())
                        .ok_or_else(|| StoreError::RunNotFound(run.to_string()))?,
                };
                Ok(ActivitySource::new(ActivitySourceKind::Workflow, workflow))
            }
        }
    }

    /// One page of the feed, newest first, by keyset (`index::activity_page`).
    pub fn activity_page(
        &self,
        concept: Option<ActivityConcept>,
        before: Option<ActivityCursor>,
        limit: usize,
    ) -> Result<Vec<ActivityRecord>, StoreError> {
        self.idx().activity_page(concept, before, limit)
    }

    /// The newest `limit` activity rows under any of `kinds`, newest first
    /// — what the Inbox classifies into notices. One bounded read.
    pub fn activity_by_kinds(
        &self,
        kinds: &[&str],
        limit: usize,
    ) -> Result<Vec<ActivityRecord>, StoreError> {
        self.idx().activity_by_kinds(kinds, limit)
    }

    /// The activity log's facts back into the index — the engine's own,
    /// whose truth is the log alone. Journal and message rows are replayed
    /// by their own passes.
    pub(crate) fn reindex_activity_log(&self) -> Result<(), StoreError> {
        for fact in crate::activity_log::read_all(&self.paths.activity_dir())? {
            self.idx().record_activity(&fact)?;
        }
        Ok(())
    }

    /// A home's journal, oldest first: a goal's history, or a run of the
    /// workspace's.
    pub fn journal(&self, home: &Home) -> Result<Vec<JournalEvent>, StoreError> {
        let addr = self.journal_addr(home)?;
        Ok(self
            .log
            .replay(&addr, &self.owner)?
            .into_iter()
            .map(|(_, je)| je)
            .collect())
    }

    // ------------------------------------------------------------------
    // Work items
    // ------------------------------------------------------------------

    /// Create or edit a work item's spec. **Not its state**: an existing item
    /// whose state differs from the stored one is refused — state moves only
    /// through [`Self::transition_work_item`].
    pub fn put_work_item(&self, spec: &WorkItemSpec) -> Result<(), StoreError> {
        let _one_writer = self.item_writer();
        self.check_home(&spec.home)?;
        if let Some(project) = spec.project {
            self.get_project(project)?;
        }
        if let Some(stored) = self.read_work_item(&spec.home, spec.id)? {
            if stored.state != spec.state {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-work-item-state-changes-go-through-transition",
                    a0 = (spec.id).to_string()
                )));
            }
        }
        self.write_work_item(spec)
    }

    /// The one writer of work items at a time. A poisoned lock is taken
    /// over: the item on disk is whatever the last write left, and the next
    /// write reads it again.
    fn item_writer(&self) -> MutexGuard<'_, ()> {
        self.item_writes.lock().unwrap_or_else(|poisoned| {
            tracing::warn!(
                "the work item write lock was poisoned; continuing with the item as it stands"
            );
            poisoned.into_inner()
        })
    }

    fn write_work_item(&self, spec: &WorkItemSpec) -> Result<(), StoreError> {
        let ns = Paths::ns_home(&spec.home);
        let d = spec.id.to_string();
        let at = now_secs();
        // Specs carry no revision; the snapshot's own revision tag is the
        // monotonic authority.
        let existing_rev = self.snapshots.current_revision(&ns, KIND_WORK_ITEM, &d)?;
        let event = self.snapshots.put(
            &ns,
            KIND_WORK_ITEM,
            &d,
            spec,
            existing_rev + 1,
            &self.owner,
            at,
            None,
            &[],
        )?;
        self.emit_store_event(StoreEvent::SnapshotWritten {
            kind: KIND_WORK_ITEM,
            home: spec.home,
            event,
        });
        self.index_work_item(spec, at)
    }

    pub(crate) fn index_work_item(&self, spec: &WorkItemSpec, at: u64) -> Result<(), StoreError> {
        let idx = self.idx();
        idx.in_transaction(|| self.index_work_item_in(&idx, spec, at))
    }

    /// The item's projection on an index the caller already holds.
    pub(crate) fn index_work_item_in(
        &self,
        idx: &Index,
        spec: &WorkItemSpec,
        at: u64,
    ) -> Result<(), StoreError> {
        // An item of a run of the workspace is filed under its run: its row
        // names the run and no goal.
        let run_id = match spec.home {
            Home::Run { run } => Some(run),
            Home::Goal { .. } => spec.run,
        };
        idx.upsert_work_item(&WorkItemRow {
            id: spec.id.to_string(),
            goal_id: spec.home.goal().map(|g| g.to_string()),
            project_id: spec.project.map(|p| p.to_string()),
            run_id: run_id.map(|r| r.to_string()),
            step_id: spec.step.as_ref().map(|s| s.to_string()),
            state: spec.state.as_str().to_string(),
            harness: spec.harness_candidates.first().cloned(),
            assignees: workitem_assignee_keys(spec),
            updated_at: at,
        })?;
        // Search finds goals: a goal's items are indexed for it.
        match spec.home.goal() {
            Some(goal) => idx.index_text(
                &format!("{}:instructions", spec.id),
                &goal.to_string(),
                &spec.instructions,
            ),
            None => Ok(()),
        }
    }

    fn read_work_item(
        &self,
        home: &Home,
        id: WorkItemId,
    ) -> Result<Option<WorkItemSpec>, StoreError> {
        Ok(self
            .snapshots
            .get::<WorkItemSpec>(&Paths::ns_home(home), KIND_WORK_ITEM, &id.to_string())?
            .map(|(spec, _)| spec))
    }

    pub fn get_work_item(&self, home: &Home, id: WorkItemId) -> Result<WorkItemSpec, StoreError> {
        self.read_work_item(home, id)?
            .ok_or_else(|| StoreError::WorkItemNotFound(id.to_string()))
    }

    /// A home's work items: a goal's, or a run of the workspace's.
    pub fn list_work_items(&self, home: &Home) -> Result<Vec<WorkItemSpec>, StoreError> {
        let ns = Paths::ns_home(home);
        self.snapshots
            .list_ds(&ns, KIND_WORK_ITEM)?
            .into_iter()
            .map(|d| {
                self.snapshots
                    .get::<WorkItemSpec>(&ns, KIND_WORK_ITEM, &d)?
                    .map(|(spec, _)| spec)
                    .ok_or_else(|| StoreError::WorkItemNotFound(d))
            })
            .collect()
    }

    /// [`list_work_items`](Self::list_work_items) for the boot walks: the
    /// items this build can read, and the ids of the ones it cannot — each
    /// of those one error line (`tolerated`), never a failed open. A read
    /// that fails for any other reason still fails.
    pub fn list_work_items_readable(
        &self,
        home: &Home,
    ) -> Result<(Vec<WorkItemSpec>, Vec<String>), StoreError> {
        let ns = Paths::ns_home(home);
        let mut items = Vec::new();
        let mut unreadable = Vec::new();
        for d in self.snapshots.list_ds(&ns, KIND_WORK_ITEM)? {
            let read = self.snapshots.get::<WorkItemSpec>(&ns, KIND_WORK_ITEM, &d);
            match tolerated("work item", &d, read)?.flatten() {
                Some((spec, _)) => items.push(spec),
                None => unreadable.push(d),
            }
        }
        Ok((items, unreadable))
    }

    /// Move a work item. **The only writer of a work item's state**, one
    /// move at a time: the state a transition is checked against is the state
    /// it replaces.
    pub fn transition_work_item(
        &self,
        home: &Home,
        id: WorkItemId,
        transition: &WorkItemTransition,
    ) -> Result<WorkItemSpec, StoreError> {
        let _one_writer = self.item_writer();
        self.transition_held(home, id, transition)
    }

    /// [`Self::transition_work_item`] for a caller that holds the writer.
    fn transition_held(
        &self,
        home: &Home,
        id: WorkItemId,
        transition: &WorkItemTransition,
    ) -> Result<WorkItemSpec, StoreError> {
        let mut spec = self.get_work_item(home, id)?;
        spec.state = spec.state.clone().apply(transition)?;
        self.write_work_item(&spec)?;
        Ok(spec)
    }

    /// Forget a work item: snapshot and rows. Its sessions' rows cascade.
    pub fn delete_work_item(&self, home: &Home, id: WorkItemId) -> Result<(), StoreError> {
        let spec = self.get_work_item(home, id)?;
        if spec.state.is_unsettled() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-work-item-let-settle-cancel-before-deleting",
                id = id.to_string(),
                a0 = (spec.state.as_str()).to_string()
            )));
        }
        self.snapshots
            .delete_snapshot(&Paths::ns_home(home), KIND_WORK_ITEM, &id.to_string())?;
        self.idx().delete_work_item(&id.to_string())
    }

    /// Where a work item is filed, from its id alone (the index answers): its
    /// goal, or the run of the workspace it serves.
    pub fn home_of_work_item(&self, id: WorkItemId) -> Result<Home, StoreError> {
        let d = id.to_string();
        let filed = self.idx().home_of_work_item(&d)?;
        filed
            .and_then(|key| key.home())
            .ok_or(StoreError::WorkItemNotFound(d))
    }

    /// Where a work item's work actually happens: the workstream opened for it
    /// when one is on disk, otherwise its home's own `scratch/`. Read-only: it
    /// never creates the directory.
    pub fn work_item_root(&self, id: WorkItemId) -> Result<PathBuf, StoreError> {
        let home = self.home_of_work_item(id)?;
        let checkout = self
            .list_workstreams(WorkstreamFilter::WorkItem(id))?
            .into_iter()
            .filter_map(|w| self.workstream_checkout(&w).ok())
            .find(|p| p.is_dir());
        Ok(checkout.unwrap_or_else(|| self.paths().home(&home).scratch()))
    }

    /// Claim an open work item. First valid claim wins — atomically: the
    /// read, the transition check, the journaled fact and the write happen
    /// under one lock, so a second claimer arriving during the first sees
    /// `claimed` and is refused by the transition table rather than
    /// journaling a second claim on the same item.
    pub fn claim_work_item(
        &self,
        home: &Home,
        id: WorkItemId,
        harness: &str,
        session: SessionId,
        claimer: &Keys,
        attestation: Option<Tag>,
    ) -> Result<WorkItemSpec, StoreError> {
        let _one_writer = self.item_writer();
        let spec = self.get_work_item(home, id)?;
        let by = PrincipalId::new(claimer.public_key().to_hex())?;
        // Refuse before journaling: the transition table decides, and a
        // refused claim must not leave a Claim fact behind.
        spec.state
            .clone()
            .apply(&WorkItemTransition::Claim { by: by.clone() })?;
        self.append_journal(
            home,
            JournalPayload::Claim {
                work_item: id,
                harness: harness.to_string(),
                session,
            },
            claimer,
            attestation,
        )?;
        self.transition_held(home, id, &WorkItemTransition::Claim { by })
    }

    // ------------------------------------------------------------------
    // Sessions
    // ------------------------------------------------------------------

    pub fn record_session(&self, row: &SessionRow) -> Result<(), StoreError> {
        let path = self.paths.session_meta_file(&row.adapter, &row.id)?;
        let json = serde_json::json!({
            "id": row.id,
            "adapter": row.adapter,
            "kind": row.kind,
            "work_item": row.work_item,
            "workstream": row.workstream,
            "conversation": row.conversation,
            "agent_id": row.agent_id,
            "transcript_path": row.transcript_path,
            "resume_token_json": row.resume_token_json,
            "status": row.status,
            "parked_at": row.parked_at,
            "pid": row.pid,
            "pid_seen_at": row.pid_seen_at,
            "ended_at": row.ended_at,
        });
        crate::paths::write_atomic(&path, &serde_json::to_vec_pretty(&json)?)?;
        self.idx().upsert_session(row)
    }

    pub fn park_session(&self, id: &str, at: u64) -> Result<(), StoreError> {
        let mut row = self
            .session_by_id(id)?
            .ok_or_else(|| StoreError::SessionNotFound(id.to_string()))?;
        row.status = SessionStatus::Parked;
        row.parked_at = Some(at);
        self.record_session(&row)
    }

    /// The session is over: its row says so, with when, and its child's pid
    /// is forgotten — there is nothing left to terminate.
    pub fn end_session(&self, id: &str, at: u64) -> Result<(), StoreError> {
        let mut row = self
            .session_by_id(id)?
            .ok_or_else(|| StoreError::SessionNotFound(id.to_string()))?;
        row.status = SessionStatus::Ended;
        row.ended_at = Some(at);
        row.pid = None;
        row.pid_seen_at = None;
        self.record_session(&row)
    }

    /// The session is over for its driver but its harness child is not
    /// gone yet — it ignored its stop, or is still leaving: the row ends,
    /// its pid kept, so a boot after a crash can still end the process.
    pub fn end_session_keeping_process(&self, id: &str, at: u64) -> Result<(), StoreError> {
        let mut row = self
            .session_by_id(id)?
            .ok_or_else(|| StoreError::SessionNotFound(id.to_string()))?;
        row.status = SessionStatus::Ended;
        row.ended_at = Some(at);
        self.record_session(&row)
    }

    /// Every session row still naming a harness child, whatever its
    /// status — what a boot terminates besides the live rows' children.
    pub fn list_sessions_with_process(&self) -> Result<Vec<SessionRow>, StoreError> {
        self.idx().sessions_with_process()
    }

    /// The harness child a live session is driving, as the driver announced
    /// it — recorded with the moment, so a later boot can tell this process
    /// from another that inherited its pid.
    pub fn set_session_process(
        &self,
        id: &str,
        pid: Option<u32>,
        at: u64,
    ) -> Result<(), StoreError> {
        let mut row = self
            .session_by_id(id)?
            .ok_or_else(|| StoreError::SessionNotFound(id.to_string()))?;
        row.pid = pid;
        row.pid_seen_at = pid.map(|_| at);
        self.record_session(&row)
    }

    pub fn session_by_id(&self, id: &str) -> Result<Option<SessionRow>, StoreError> {
        self.idx().get_session(id)
    }

    /// Every session still recorded as live — after a boot, the ones the
    /// last process was driving when it died.
    pub fn list_live_sessions(&self) -> Result<Vec<SessionRow>, StoreError> {
        self.idx().live_sessions()
    }

    // ------------------------------------------------------------------
    // Budget ledger
    // ------------------------------------------------------------------

    /// Charge a home's ledger: a goal's, or a run of the workspace's.
    pub fn add_spend(
        &self,
        home: &Home,
        tokens: u64,
        usd_cents: u64,
        wall_secs: u64,
    ) -> Result<(), StoreError> {
        self.check_home(home)?;
        let at = now_secs();
        let path = self.paths.home(home).ledger();
        let line = serde_json::to_string(&LedgerLine {
            tokens,
            usd_cents,
            wall_secs,
            at,
        })?;
        crate::paths::append_line(&path, &line)?;
        self.idx()
            .add_spend(&HomeKey::from(home), tokens, usd_cents, wall_secs, at)
    }

    pub fn spent(&self, home: &Home) -> Result<BudgetSpent, StoreError> {
        let (tokens, usd_cents, wall_clock_secs) = self.idx().total_spend(&HomeKey::from(home))?;
        Ok(BudgetSpent {
            tokens,
            usd_cents,
            wall_clock_secs,
        })
    }

    /// Preflight: does the home's ceiling allow more spend right now? A goal
    /// spends against its budget; a run of the workspace against the one its
    /// scope carries.
    pub fn budget_allows(&self, home: &Home) -> Result<bool, StoreError> {
        let budget = match home {
            Home::Goal { goal } => self.get_goal(*goal)?.budget,
            Home::Run { run } => self
                .get_run(*run)?
                .scope
                .budget()
                .cloned()
                .unwrap_or_default(),
        };
        Ok(budget.allows(&self.spent(home)?))
    }

    /// The budget a goal, or a run of the workspace, made without one of its
    /// own is given: the three `budget.default.*` settings, a zero reading as
    /// no ceiling. The ceiling standing work — a run a schedule starts daily
    /// for a year — spends against before it stops.
    pub fn default_budget(&self) -> Result<Budget, StoreError> {
        let ceiling = |key: &str| -> Result<Option<u64>, StoreError> {
            let n = self.setting(key, None)?.value.as_u64().unwrap_or(0);
            Ok((n > 0).then_some(n))
        };
        Ok(Budget {
            max_tokens: ceiling(Budget::SETTING_TOKENS)?,
            max_usd_cents: ceiling(Budget::SETTING_USD_CENTS)?,
            max_wall_clock_secs: ceiling(Budget::SETTING_WALL_CLOCK_SECS)?,
        })
    }

    // ------------------------------------------------------------------
    // Edges
    // ------------------------------------------------------------------

    pub fn add_edge(&self, from: GoalId, to: GoalId, kind: GoalEdgeKind) -> Result<(), StoreError> {
        self.get_goal(from)?;
        self.get_goal(to)?;
        let path = self.paths.goal(from).edges();
        // Absent is no edges yet; anything else is read whole or refused —
        // a file this build cannot read, or cannot parse, is never
        // overwritten with the one edge being added.
        let mut file: EdgeFile = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| StoreError::unreadable(&path, "goal edges", e))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => EdgeFile::default(),
            Err(e) => return Err(StoreError::io(path.display().to_string(), e)),
        };
        let entry = (to.to_string(), kind.as_str().to_string());
        if !file.edges.contains(&entry) {
            file.edges.push(entry);
        }
        crate::paths::write_atomic(&path, &serde_json::to_vec_pretty(&file)?)?;
        self.idx()
            .add_edge(&from.to_string(), &to.to_string(), kind.as_str())
    }

    pub fn edges_from(&self, from: GoalId) -> Result<Vec<(GoalId, GoalEdgeKind)>, StoreError> {
        Ok(self
            .idx()
            .edges_from(&from.to_string())?
            .into_iter()
            .filter_map(|(to, kind)| {
                let to = to.parse().ok()?;
                let kind = match kind.as_str() {
                    "refines" => GoalEdgeKind::Refines,
                    _ => return None,
                };
                Some((to, kind))
            })
            .collect())
    }

    // ------------------------------------------------------------------
    // Search & rebuild
    // ------------------------------------------------------------------

    pub fn search(&self, query: &str) -> Result<Vec<GoalId>, StoreError> {
        self.idx()
            .search(query)?
            .into_iter()
            .map(|s| {
                s.parse::<GoalId>().map_err(|e| {
                    StoreError::Invalid(bisa_core::text!(
                        "error-store-invalid-bad-id-fts",
                        e = e.to_string()
                    ))
                })
            })
            .collect()
    }

    /// Rebuild the entire index from the filesystem, in foreign-key order
    /// (`docs/architecture/08-persistence.md` § Rebuild order). Read markers are local-only
    /// and deliberately lost.
    pub fn rebuild_index(&self) -> Result<(), StoreError> {
        self.rebuild_index_observed(&crate::boot::Quiet)
    }

    /// [`Self::rebuild_index`], saying how far it is to `observer` — at the
    /// start, every few goals, and at the end — and to the log.
    ///
    /// One bad record never stops it: a file this build cannot read is
    /// skipped and named in [`Self::problems`], the way the goal and run
    /// walks always did, so an upgrade that rebuilds the index never turns a
    /// torn note or session record into a workspace that will not open. The
    /// stamp comes off first and goes back on last, so a rebuild cut short
    /// is rebuilt again at the next open rather than trusted half-empty.
    pub fn rebuild_index_observed(
        &self,
        observer: &dyn crate::boot::BootObserver,
    ) -> Result<(), StoreError> {
        use crate::boot::BootPhase;
        let started = std::time::Instant::now();
        {
            let idx = self.idx();
            idx.mark_building()?;
            idx.clear()?;
        }
        // 1. agents · teams · skills · mcp_servers · members · channels
        self.reindex_agents()?;
        self.reindex_teams()?;
        self.reindex_skills()?;
        self.reindex_connectors()?;
        self.reindex_mcps()?;
        self.reindex_members()?;
        self.reindex_channels()?;
        // 2. projects
        self.reindex_projects()?;
        // 3. workflows — a goal row points at one — then the library's
        // listening column, from the records beside them.
        self.reindex_workflows()?;
        self.reindex_listening()?;
        // 4–5. goals, then everything under them: runs and their steps, work
        // items (which point at a run), the journal's facts, the ledger, the
        // edges. A goal's row is built with its current run so the cached
        // status is the truth's.
        let goal_ids = self.goal_ids_on_disk()?;
        let of = goal_ids.len();
        observer.phase(BootPhase::RebuildingIndex { done: 0, of });
        tracing::info!(target: "bisa_store::rebuild", goals = of, "rebuilding the index from the files");
        for id in &goal_ids {
            let ns = Paths::ns_goal(*id);
            let Some((goal, _)) = self
                .tolerated_record(
                    "goal",
                    &id.to_string(),
                    self.snapshots.get::<Goal>(&ns, KIND_GOAL, &id.to_string()),
                )?
                .flatten()
            else {
                tracing::warn!("goal dir {id} has no goal snapshot; skipping");
                continue;
            };
            // The current run this build cannot read is a goal row with no
            // run — the read routes say `run_unreadable` — never a failed
            // rebuild.
            let current = match goal.run {
                Some(run) => self
                    .tolerated_record(
                        "run",
                        &run.to_string(),
                        self.snapshots.get::<WorkflowRun>(
                            &ns,
                            bisa_core::kind::KIND_WORKFLOW_RUN,
                            &run.to_string(),
                        ),
                    )?
                    .flatten()
                    .map(|(r, _)| r),
                None => None,
            };
            self.index_goal(&goal, current.as_ref())?;
        }
        for (n, id) in goal_ids.iter().enumerate() {
            if n > 0 && n % 25 == 0 {
                observer.phase(BootPhase::RebuildingIndex { done: n, of });
            }
            let ns = Paths::ns_goal(*id);
            let Some((goal, _)) = self
                .tolerated_record(
                    "goal",
                    &id.to_string(),
                    self.snapshots.get::<Goal>(&ns, KIND_GOAL, &id.to_string()),
                )?
                .flatten()
            else {
                continue;
            };
            let d = id.to_string();
            self.reindex_runs_of(*id)?;
            let home = Home::Goal { goal: *id };
            self.reindex_home(&home, &self.goal_addr(&goal))?;
            let edges_path = self.paths.goal(*id).edges();
            if let Ok(bytes) = std::fs::read(&edges_path) {
                if let Ok(file) = serde_json::from_slice::<EdgeFile>(&bytes) {
                    for (to, kind) in file.edges {
                        // An edge to a goal that is gone is a stale file, not a
                        // reason to fail the rebuild.
                        if self.idx().goal_exists(&to)? {
                            self.idx().add_edge(&d, &to, &kind)?;
                        }
                    }
                }
            }
        }
        // 5b. the runs of the workspace, each its own home: the run, then its
        // work items, its journal's facts and its ledger — after the goals,
        // since a goal a run's spawn step made may be named by nothing else.
        for run in self.workspace_run_ids_on_disk()? {
            let Some(snapshot) = self.workspace_run_snapshot(run)? else {
                tracing::warn!("run dir {run} has no run snapshot; skipping");
                continue;
            };
            self.index_run(&snapshot)?;
            self.reindex_home(&snapshot.home(), &self.run_addr(run))?;
        }
        // 6. notes, then drawings — from the files and from the snapshots.
        self.reindex_notes()?;
        self.reindex_drawings()?;
        // 7. workstreams, then the conversations that stand in them and
        // elsewhere — a session row and a message stream both point at one.
        self.reindex_workstreams()?;
        self.reindex_conversations()?;
        // 8. sessions
        self.reindex_sessions()?;
        // 9. messages → reactions → …
        self.reindex_conversation()?;
        self.reindex_activity_log()?;
        // 10. signals — each names its listener's host, a workflow or a goal,
        // so after both.
        self.reindex_signals()?;
        // 11. seen events (tags were written beside each object)
        self.reindex_seen()?;
        // Stamped last: an index whose rebuild a crash cut short reads as
        // unstamped and is rebuilt again at the next open.
        self.idx().mark_built()?;
        observer.phase(BootPhase::RebuildingIndex { done: of, of });
        tracing::info!(target: "bisa_store::rebuild", goals = of, secs = started.elapsed().as_secs_f64(), "the index is rebuilt");
        Ok(())
    }

    /// What one home holds besides its runs, back into the index: its work
    /// items, its journal's facts, its ledger — a goal's folder and a run of
    /// the workspace's alike.
    fn reindex_home(&self, home: &Home, addr: &JournalAddr) -> Result<(), StoreError> {
        let ns = Paths::ns_home(home);
        for wid in self.snapshots.list_ds(&ns, KIND_WORK_ITEM)? {
            let read = self
                .snapshots
                .get::<WorkItemSpec>(&ns, KIND_WORK_ITEM, &wid);
            if let Some((spec, ev)) = tolerated("work item", &wid, read)?.flatten() {
                self.index_work_item(&spec, ev.created_at.as_secs())?;
            }
        }
        for (ev, je) in self.log.replay(addr, &self.owner)? {
            self.index_journal_fact(&ev, &je)?;
        }
        let ledger_path = self.paths.home(home).ledger();
        if let Ok(content) = std::fs::read_to_string(&ledger_path) {
            let key = HomeKey::from(home);
            for line in content.lines().filter(|l| !l.trim().is_empty()) {
                match serde_json::from_str::<LedgerLine>(line) {
                    Ok(l) => {
                        self.idx()
                            .add_spend(&key, l.tokens, l.usd_cents, l.wall_secs, l.at)?
                    }
                    Err(e) => tracing::warn!("{}: bad ledger line: {e}", ledger_path.display()),
                }
            }
        }
        Ok(())
    }

    /// End every **orphan run**: a run still running whose goal never
    /// recorded it — a crash fell between the run's snapshot and the goal's,
    /// before `create_goal_run` wrote the goal's first — so nothing waits on
    /// it and the signal that began it is not dispatched twice. Indexed
    /// first, since the funnel resolves a run by its row; ended as stopped
    /// through the funnel's locked path; named ([`ProblemKind::OrphanRun`]).
    /// Answers the runs it ended.
    ///
    /// **The engine's, under its lock — never an open's.** A workspace is
    /// opened by every verb, beside a running node as well, and a run that
    /// node is in the middle of starting — its snapshot written, its goal's
    /// a moment behind — looks exactly like an orphan to a second process.
    /// Only the process that holds `run/engine.lock` knows nothing else is
    /// starting runs here.
    pub fn end_orphan_runs(&self) -> Result<Vec<RunId>, StoreError> {
        let mut ended_runs = Vec::new();
        for goal in self.list_goals(None)? {
            if goal.is_closed() {
                continue;
            }
            for run in self.list_runs(goal.id)? {
                if run.is_finished()
                    || run.is_queued()
                    || goal.run == Some(run.id)
                    || goal.runs.contains(&run.id)
                {
                    continue;
                }
                let indexed = self.index_run(&run);
                self.tolerated_index("run", &run.id.to_string(), indexed)?;
                let ended = {
                    let _one_writer = self.run_writer();
                    self.record_run_event_locked(
                        run.id,
                        RunEvent::Cancel {
                            cause: CancelCause::Stopped { rationale: None },
                        },
                    )
                };
                if let Err(e) = ended {
                    tracing::warn!(goal = %goal.id, run = %run.id, "an orphan run could not be ended: {e}");
                }
                self.record_problem(WorkspaceProblem::new(
                    ProblemKind::OrphanRun,
                    run.id.to_string(),
                    bisa_core::text!(
                        "error-store-problem-orphan-run",
                        run = run.id.to_string(),
                        goal = goal.id.to_string()
                    ),
                    None,
                ));
                ended_runs.push(run.id);
            }
        }
        Ok(ended_runs)
    }

    /// Re-index, from their snapshots, every unfinished run — of every open
    /// goal, and of the workspace — and the work items that run owns.
    /// Bounded by the live set — a finished run is never touched — and
    /// idempotent: the projection of a run that was already right is written
    /// again as the same rows. This is what makes a crash between a snapshot
    /// write and its index transaction cost nothing: the next open reads the
    /// truth and repairs the cache.
    pub fn reconcile_live(&self) -> Result<(), StoreError> {
        let mut runs = 0usize;
        for goal in self.list_goals(None)? {
            if goal.is_closed() {
                continue;
            }
            let Some(run_id) = goal.run else {
                continue;
            };
            // Straight from the goal's namespace, never through `get_run`:
            // that resolves the goal by the index row, and the row this
            // reconcile exists to write may not be there yet.
            let read = self.snapshots.get::<WorkflowRun>(
                &Paths::ns_goal(goal.id),
                KIND_WORKFLOW_RUN,
                &run_id.to_string(),
            );
            let run = match read {
                Ok(Some((run, _))) => run,
                Ok(None) => {
                    tracing::warn!(goal = %goal.id, run = %run_id, "reconcile found no snapshot for the goal's run");
                    continue;
                }
                Err(e) => {
                    tracing::warn!(goal = %goal.id, run = %run_id, "reconcile skipped a run it cannot read: {e}");
                    continue;
                }
            };
            if run.is_finished() {
                continue;
            }
            // An item this build cannot read is named and skipped: the run
            // is still projected, and the open still opens.
            let (items, unreadable) =
                self.list_work_items_readable(&Home::Goal { goal: goal.id })?;
            if !unreadable.is_empty() {
                tracing::warn!(goal = %goal.id, items = ?unreadable, "reconcile skipped work items it cannot read");
            }
            let idx = self.idx();
            let indexed = idx.in_transaction(|| {
                self.index_run_in(&idx, &run)?;
                self.index_goal_in(&idx, &goal, Some(&run))?;
                for spec in items.iter().filter(|s| s.run == Some(run.id)) {
                    self.index_work_item_in(&idx, spec, now_secs())?;
                }
                Ok(())
            });
            drop(idx);
            self.tolerated_index("run", &run.id.to_string(), indexed)?;
            runs += 1;
        }
        // The runs of the workspace, straight from their folders: the index
        // row that would list them is what this reconcile exists to write.
        for run_id in self.workspace_run_ids_on_disk()? {
            let Some(run) = self.workspace_run_snapshot(run_id)? else {
                continue;
            };
            if run.is_finished() {
                continue;
            }
            let (items, unreadable) = self.list_work_items_readable(&run.home())?;
            if !unreadable.is_empty() {
                tracing::warn!(run = %run.id, items = ?unreadable, "reconcile skipped work items it cannot read");
            }
            let idx = self.idx();
            let indexed = idx.in_transaction(|| {
                self.index_run_in(&idx, &run)?;
                for spec in &items {
                    self.index_work_item_in(&idx, spec, now_secs())?;
                }
                Ok(())
            });
            drop(idx);
            self.tolerated_index("run", &run.id.to_string(), indexed)?;
            runs += 1;
        }
        // A run no goal names — the orphan a crash between the run's snapshot
        // and the goal's leaves — is not touched here: an open is made by
        // every verb, beside a running node as well, and the run that node is
        // in the middle of starting looks exactly like one. The engine ends
        // orphans once it holds the lock ([`Self::end_orphan_runs`]).
        // Stale rows: the index says a run is live; its record says
        // otherwise — a finished run whose index update a crash lost, a run
        // whose record is gone. The row is brought back in step with the
        // record, so a listener's guard never counts it forever.
        let live_rows = self.idx().live_run_rows()?;
        for row in live_rows {
            let Ok(id) = row.id.parse::<RunId>() else {
                continue;
            };
            let what = match self.get_run(id) {
                Ok(run) if run.is_finished() => {
                    let indexed = self.index_run(&run);
                    self.tolerated_index("run", &row.id, indexed)?;
                    "finished"
                }
                Ok(_) => continue,
                Err(StoreError::RunNotFound(_)) => {
                    let deleted = self.idx().delete_run_row(&row.id);
                    self.tolerated_index("run", &row.id, deleted)?;
                    "gone"
                }
                Err(e) => {
                    // An unreadable record is named by the list reads; the
                    // row is left as the only trace of the run.
                    tracing::warn!(run = %row.id, "a live row's record could not be read: {e}");
                    continue;
                }
            };
            self.record_problem(WorkspaceProblem::new(
                ProblemKind::StaleRow,
                row.id.clone(),
                bisa_core::text!(
                    "error-store-problem-stale-row",
                    run = row.id.clone(),
                    what = what.to_string()
                ),
                None,
            ));
            runs += 1;
        }
        if runs > 0 {
            tracing::debug!(
                runs,
                "reconciled the live runs' projections from their snapshots"
            );
        }
        Ok(())
    }

    /// Rebuild support: the work items of one home, from their snapshots.
    /// Split out so a run that lands after its items can re-point them.
    pub(crate) fn reindex_work_items_of(&self, home: &Home) -> Result<(), StoreError> {
        let ns = Paths::ns_home(home);
        for wid in self.snapshots.list_ds(&ns, KIND_WORK_ITEM)? {
            if let Some((spec, ev)) =
                self.snapshots
                    .get::<WorkItemSpec>(&ns, KIND_WORK_ITEM, &wid)?
            {
                self.index_work_item(&spec, ev.created_at.as_secs())?;
            }
        }
        Ok(())
    }

    /// The index rows one journal fact produces — shared by ingest and the
    /// rebuild so both derive exactly the same cache. The goal-only facts —
    /// an attachment, a document, the search text — land for a goal's
    /// journal alone.
    pub(crate) fn index_journal_fact(
        &self,
        ev: &nostr::event::Event,
        je: &JournalEvent,
    ) -> Result<(), StoreError> {
        self.record_journal_activity(je)?;
        match &je.payload {
            JournalPayload::Decision {
                gate,
                approve,
                subject,
                ..
            } => {
                self.idx().add_approval(
                    &HomeKey::from(&je.home),
                    gate.as_str(),
                    subject,
                    je.author.as_hex(),
                    *approve,
                    je.at,
                )?;
            }
            JournalPayload::Attachment { project, attached } => {
                let Some(goal) = je.home.goal() else {
                    return Ok(());
                };
                let goal_d = goal.to_string();
                let idx = self.idx();
                if *attached {
                    // A project this node does not (yet) hold cannot be
                    // attached in the cache; the next rebuild after it arrives
                    // will. The journal is the truth either way.
                    if idx.project_slug(&project.to_string())?.is_some() {
                        idx.attach(&goal_d, &project.to_string(), je.at, je.author.as_hex())?;
                    } else {
                        tracing::debug!(
                            "goal {goal_d}: attachment to unknown project {project}; deferred"
                        );
                    }
                } else {
                    idx.detach(&goal_d, &project.to_string())?;
                }
            }
            // The folder is derived from the facts: a rebuild and a peer's
            // ingest both bring it up to date here, for the bytes held.
            JournalPayload::Document { .. } => {
                if let Some(goal) = je.home.goal() {
                    self.materialise_goal_documents(goal)?;
                }
            }
            _ => {}
        }
        // Search finds goals: a goal's facts are indexed for it.
        if let (Some(text), Some(goal)) = (searchable_text(&je.payload), je.home.goal()) {
            self.idx()
                .index_text(&ev.id.to_hex(), &goal.to_string(), &text)?;
        }
        Ok(())
    }

    fn reindex_sessions(&self) -> Result<(), StoreError> {
        let sessions_root = self.paths.sessions_dir();
        let Ok(adapters) = std::fs::read_dir(&sessions_root) else {
            return Ok(());
        };
        for adapter_dir in adapters.flatten() {
            if !adapter_dir.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let Ok(files) = std::fs::read_dir(adapter_dir.path()) else {
                continue;
            };
            for f in files.flatten() {
                let name = f.file_name().to_string_lossy().into_owned();
                if !name.ends_with(".json") {
                    continue;
                }
                let path = f.path();
                let bytes = std::fs::read(&path)
                    .map_err(|e| StoreError::io(path.display().to_string(), e))?;
                let Some(v) = self.tolerated_record::<serde_json::Value>(
                    "session record",
                    &path.display().to_string(),
                    serde_json::from_slice(&bytes)
                        .map_err(|e| StoreError::unreadable(&path, "session record", e)),
                )?
                else {
                    continue;
                };
                let row = SessionRow {
                    id: v["id"].as_str().unwrap_or_default().to_string(),
                    adapter: v["adapter"].as_str().unwrap_or_default().to_string(),
                    kind: SessionKind::parse(v["kind"].as_str().unwrap_or_default()),
                    work_item: v["work_item"].as_str().map(str::to_string),
                    workstream: v["workstream"].as_str().map(str::to_string),
                    conversation: v["conversation"].as_str().map(str::to_string),
                    agent_id: v["agent_id"].as_str().map(str::to_string),
                    transcript_path: v["transcript_path"].as_str().map(str::to_string),
                    resume_token_json: v["resume_token_json"].as_str().map(str::to_string),
                    status: SessionStatus::parse(v["status"].as_str().unwrap_or_default()),
                    parked_at: v["parked_at"].as_u64(),
                    pid: v["pid"].as_u64().map(|p| p as u32),
                    pid_seen_at: v["pid_seen_at"].as_u64(),
                    ended_at: v["ended_at"].as_u64(),
                };
                if !row.id.is_empty() {
                    self.idx().upsert_session(&row)?;
                }
            }
        }
        Ok(())
    }

    /// Comparable dump for tests.
    pub fn dump_goal_rows(&self) -> Result<Vec<GoalRow>, StoreError> {
        self.idx().dump_goals()
    }
}

fn goal_row(goal: &Goal, run: Option<&WorkflowRun>) -> GoalRow {
    let (closure, superseded_by) = match goal.closed.as_ref().map(|c| &c.reason) {
        Some(reason) => (
            Some(reason.as_str().to_string()),
            match reason {
                bisa_core::ClosureReason::Superseded { by } => Some(by.to_string()),
                bisa_core::ClosureReason::Abandoned { .. } => None,
            },
        ),
        None => (None, None),
    };
    GoalRow {
        id: goal.id.to_string(),
        status: goal.status(run).as_str().to_string(),
        closure,
        superseded_by,
        origin: goal.origin.as_str().to_string(),
        workflow_id: goal.workflow.map(|w| w.to_string()),
        run_id: goal.run.map(|r| r.to_string()),
        author: goal.author.as_hex().to_string(),
        title: goal.title.clone(),
        assignees: goal.assignees.iter().map(|a| a.to_string()).collect(),
        revision: goal.revision,
        archived_at: goal.archived.map(|a| a.at),
        listening_since: goal.listening.as_ref().map(|l| l.since),
        created_at: goal.created_at,
    }
}

/// What a caller supplies to capture a goal.
#[derive(Clone, Debug, PartialEq)]
pub struct NewGoal {
    pub statement: String,
    pub title: Option<String>,
    pub origin: GoalOrigin,
    /// How the goal moves: who designs, who adopts and starts.
    pub mode: GoalMode,
    pub assignees: Vec<Assignee>,
    pub tags: Tags,
}

impl NewGoal {
    /// A person's capture: auto, unassigned, untagged.
    pub fn captured(statement: &str) -> Self {
        Self {
            statement: statement.to_string(),
            title: None,
            origin: GoalOrigin::Captured,
            mode: GoalMode::Auto,
            assignees: vec![],
            tags: Tags::default(),
        }
    }

    pub fn title(mut self, title: &str) -> Self {
        self.title = Some(title.to_string());
        self
    }

    pub fn mode(mut self, mode: GoalMode) -> Self {
        self.mode = mode;
        self
    }
}

/// Everyone a work item names, in `Assignee`'s wire form — the `assignees`
/// index column: the request (teams unexpanded), the runner the engine picked,
/// and the spawn allowlist. A wildcard entry names nobody and is dropped.
pub(crate) fn workitem_assignee_keys(spec: &WorkItemSpec) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    let mut push = |key: String| {
        if !keys.contains(&key) {
            keys.push(key);
        }
    };
    for a in &spec.assignees {
        push(a.to_string());
    }
    for agent in spec.agent.iter().chain(spec.spawn_allowlist.iter()) {
        if agent == "*" {
            continue;
        }
        push(Assignee::Agent(agent.clone()).to_string());
    }
    keys
}

pub(crate) fn searchable_text(payload: &JournalPayload) -> Option<String> {
    match payload {
        JournalPayload::Note { text } => Some(text.clone()),
        JournalPayload::Question { text, .. } => Some(text.clone()),
        JournalPayload::Progress {
            verb,
            object,
            outcome,
            ..
        } => Some(match outcome {
            Some(o) => format!("{verb} {object}: {o}"),
            None => format!("{verb} {object}"),
        }),
        JournalPayload::Result { output, .. } => Some(output.to_string()),
        JournalPayload::Document { file } => Some(file.name.clone()),
        _ => None,
    }
}

impl Workspace {
    /// What this open and its rebuild found wrong and worked around, in the
    /// order found. Empty for a workspace nothing is wrong with.
    pub fn problems(&self) -> Vec<WorkspaceProblem> {
        self.problems.all()
    }

    /// Note one problem, once: a read made many times a boot — a settings
    /// layer's — says its trouble once.
    pub(crate) fn record_problem(&self, problem: WorkspaceProblem) {
        self.problems.record(problem);
    }

    /// [`tolerated`], and the skipped record named in [`Self::problems`]:
    /// the rebuild's and the reconcile's reads, which a person should hear
    /// about once rather than find in a log.
    pub(crate) fn tolerated_record<T>(
        &self,
        what: &'static str,
        id: &str,
        read: Result<T, StoreError>,
    ) -> Result<Option<T>, StoreError> {
        match read {
            Err(StoreError::Unreadable { path, what, reason }) => {
                self.record_problem(WorkspaceProblem::new(
                    ProblemKind::RebuildSkipped,
                    path.clone(),
                    bisa_core::text!(
                        "error-store-problem-rebuild-skipped",
                        path = path,
                        what = what.to_string(),
                        reason = reason
                    ),
                    None,
                ));
                Ok(None)
            }
            other => tolerated(what, id, other),
        }
    }

    /// One index transaction of the reconcile. A constraint the cache cannot
    /// keep for one record — two runs on one signal, a row the files no
    /// longer back — costs that record's row, said and named, never the open.
    pub(crate) fn tolerated_index(
        &self,
        what: &'static str,
        id: &str,
        result: Result<(), StoreError>,
    ) -> Result<(), StoreError> {
        match result {
            Err(StoreError::Sqlite(e)) => {
                self.record_problem(WorkspaceProblem::new(
                    ProblemKind::IndexDisagrees,
                    id,
                    bisa_core::text!(
                        "error-store-problem-index-disagrees",
                        what = what.to_string(),
                        id = id.to_string(),
                        reason = e.to_string()
                    ),
                    None,
                ));
                Ok(())
            }
            other => other,
        }
    }
}

/// One record read for a list. `Ok(None)` when the record is missing or
/// was written by another shape of the code — said once at `error`, naming
/// it, so the operator knows which file to repair or rebuild — and `Err`
/// for everything else (an I/O failure is the caller's). The rule every
/// list shares, so a broken file costs one row and never the list.
pub(crate) fn tolerated<T>(
    what: &'static str,
    id: &str,
    read: Result<T, StoreError>,
) -> Result<Option<T>, StoreError> {
    match read {
        Ok(v) => Ok(Some(v)),
        Err(
            e @ (StoreError::GoalNotFound(_)
            | StoreError::RunNotFound(_)
            | StoreError::DefinitionNotFound { .. }
            | StoreError::Unreadable { .. }),
        ) => {
            tracing::error!(target: "bisa_store", what, id, "skipping a {what} the list cannot read: {e}");
            Ok(None)
        }
        Err(e @ StoreError::Invalid(_)) if e.to_string().contains("bad id in index") => {
            tracing::error!(target: "bisa_store", what, id, "skipping an index row that names no {what}: {e}");
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::MemoryKeyStore;
    use crate::workflows::tests::{agent_step, notify_workflow, sid};
    use bisa_core::{ClosureReason, RunEvent, WorkItemState};
    use std::collections::BTreeMap;

    fn ws() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        (dir, ws)
    }

    /// An open item filed in `home` — a goal's, when given a goal id.
    fn item(home: impl Into<Home>, text: &str) -> WorkItemSpec {
        WorkItemSpec {
            id: WorkItemId::from_ulid(mint_ulid()),
            home: home.into(),
            run: None,
            step: None,
            instructions: text.into(),
            state: WorkItemState::Open,
            project: None,
            harness_candidates: vec!["claude-code".into()],
            model: None,
            effort: None,
            output_schema: None,
            budget: Default::default(),
            assignees: vec![],
            tier_ceiling: bisa_core::ToolTier::Write,
            agent: None,
            spawn_allowlist: vec![],
            depth_budget: 0,
            result_attempts: 0,
            interruptions: 0,
        }
    }

    /// A goal with a run under way: one agent step, running.
    fn with_run(ws: &Workspace) -> (Goal, WorkflowRun) {
        ws.add_agent(crate::agents::NewAgent {
            name: "Developer".into(),
            harness: "mock".into(),
            system_prompt: "build".into(),
            ..Default::default()
        })
        .unwrap();
        let mut new = notify_workflow("Build");
        new.steps
            .insert(0, agent_step("build", "developer", &["post"]));
        let wf = ws
            .create_workflow(new, bisa_core::WorkflowOrigin::Workspace)
            .unwrap();
        let goal = ws.create_goal(NewGoal::captured("build a parser")).unwrap();
        let (run, _) = ws
            .create_run(
                bisa_core::RunScope::Goal { goal: goal.id },
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        (ws.get_goal(goal.id).unwrap(), run)
    }

    #[test]
    fn a_fresh_workspace_opens_seeded_on_schema_one() {
        let (dir, ws) = ws();
        let agents = ws.list_agents().unwrap();
        assert_eq!(agents.len(), 2, "the General Agent and the Workflow Agent");
        assert!(agents.iter().all(|a| a.id.is_core_id()));
        assert!(ws.get_channel(&bisa_core::ChannelId::general()).is_ok());
        assert!(ws.list_goals(None).unwrap().is_empty());
        let stamped: i32 = rusqlite::Connection::open(dir.path().join("index.sqlite"))
            .unwrap()
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(stamped, crate::index::SCHEMA_VERSION);
    }

    #[test]
    fn a_stale_index_is_discarded_and_rebuilt_from_truth() {
        let dir = tempfile::tempdir().unwrap();
        let goal = {
            let ws = Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default()))
                .unwrap();
            let goal = ws
                .create_goal(NewGoal::captured("survive a wipe").title("Durable"))
                .unwrap();
            ws.add_agent(crate::agents::NewAgent {
                name: "Scout".into(),
                harness: "mock".into(),
                system_prompt: "look around".into(),
                ..Default::default()
            })
            .unwrap();
            goal.id
        };
        let db = dir.path().join("index.sqlite");
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.pragma_update(None, "user_version", 999_i32).unwrap();
        }
        let ws =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        assert_eq!(ws.get_goal(goal).unwrap().title.as_deref(), Some("Durable"));
        assert_eq!(ws.list_goals(None).unwrap().len(), 1);
        assert_eq!(ws.list_agents().unwrap().len(), 3);
        let stamped: i32 = rusqlite::Connection::open(&db)
            .unwrap()
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_ne!(stamped, 999);
    }

    #[test]
    fn create_get_list_search() {
        let (_dir, ws) = ws();
        let goal = ws
            .create_goal(NewGoal::captured("sync bookmarks across browsers").title("Bookmarks"))
            .unwrap();
        assert_eq!(ws.get_goal(goal.id).unwrap().statement, goal.statement);
        assert_eq!(goal.origin, GoalOrigin::Captured);
        assert_eq!(
            goal.mode,
            GoalMode::Auto,
            "a capture is auto unless it says otherwise"
        );
        assert_eq!(
            ws.create_goal(NewGoal::captured("by hand").mode(GoalMode::Manual))
                .unwrap()
                .mode,
            GoalMode::Manual
        );
        assert_eq!(goal.status(None), GoalStatus::Draft);
        assert_eq!(
            ws.list_goals(Some(GoalStatus::Draft)).unwrap().len(),
            2,
            "both captures are drafts until a workflow is chosen"
        );
        assert!(ws.list_goals(Some(GoalStatus::Running)).unwrap().is_empty());
        assert_eq!(
            ws.search("bookmarks").unwrap(),
            vec![goal.id],
            "{:?}",
            ws.list_goals(None)
                .unwrap()
                .iter()
                .map(|g| (g.id.to_string(), g.statement.clone(), g.title.clone()))
                .collect::<Vec<_>>()
        );
        assert!(ws.create_goal(NewGoal::captured("   ")).is_err());
        // A spawned goal records its parent and the edge in one act.
        let child = ws
            .create_goal(NewGoal {
                statement: "part of it".into(),
                title: None,
                origin: GoalOrigin::Spawned { parent: goal.id },
                mode: GoalMode::Manual,
                assignees: vec![],
                tags: Tags::default(),
            })
            .unwrap();
        assert_eq!(
            ws.edges_from(child.id).unwrap(),
            vec![(goal.id, GoalEdgeKind::Refines)]
        );
        let rows = ws.dump_goal_rows().unwrap();
        let child_row = rows
            .iter()
            .find(|r| r.id == child.id.to_string())
            .expect("the child's row");
        assert_eq!(child_row.origin, "spawned");
        let ghost = GoalId::from_ulid(ulid::Ulid::from_parts(9, 9));
        assert!(ws
            .create_goal(NewGoal {
                statement: "orphan".into(),
                title: None,
                origin: GoalOrigin::Spawned { parent: ghost },
                mode: GoalMode::Manual,
                assignees: vec![],
                tags: Tags::default(),
            })
            .is_err());
    }

    #[test]
    fn a_goal_cannot_be_closed_or_rerun_by_an_edit() {
        let (_dir, ws) = ws();
        let (goal, run) = with_run(&ws);
        let mut closed = goal.clone();
        closed.closed = Some(bisa_core::Closure {
            reason: ClosureReason::Abandoned { rationale: None },
            at: 1,
        });
        assert!(ws.update_goal(closed).is_err());
        let mut forgot = goal.clone();
        forgot.run = None;
        assert!(ws.update_goal(forgot).is_err());
        let mut swapped = goal.clone();
        swapped.workflow = None;
        assert!(ws.update_goal(swapped).is_err());
        let mut reborn = goal.clone();
        reborn.origin = GoalOrigin::Run {
            run: run.id,
            step: sid("build"),
        };
        assert!(ws.update_goal(reborn).is_err());
        let mut listening = goal.clone();
        listening.listening = Some(bisa_core::Listening {
            inputs: BTreeMap::new(),
            budget: None,
            since: 1,
            paused: None,
        });
        assert!(
            ws.update_goal(listening).is_err(),
            "listening moves through set_listening"
        );
        let mut fine = goal.clone();
        fine.title = Some("Parser".into());
        fine.budget.max_tokens = Some(10);
        let saved = ws.update_goal(fine).unwrap();
        assert_eq!(saved.revision, goal.revision + 1);
        assert_eq!(saved.run, Some(run.id), "an edit does not touch the run");
        assert_eq!(ws.dump_goal_rows().unwrap()[0].status, "running");
    }

    #[test]
    fn work_item_state_moves_only_through_its_transitions() {
        let (_dir, ws) = ws();
        let goal = ws.create_goal(NewGoal::captured("build a parser")).unwrap();
        let home = Home::from(goal.id);
        let wi = item(goal.id, "write the tokenizer");
        ws.put_work_item(&wi).unwrap();
        assert_eq!(ws.list_work_items(&home).unwrap().len(), 1);

        // A put that changes the state is refused.
        let mut sneaky = wi.clone();
        sneaky.state = WorkItemState::Accepted;
        assert!(ws.put_work_item(&sneaky).is_err());
        // An edit to the spec is fine.
        let mut edited = wi.clone();
        edited.instructions = "write the lexer".into();
        ws.put_work_item(&edited).unwrap();

        let agent = ws.identity.mint_agent().unwrap();
        let auth = crate::identity::attest_agent(ws.owner_keys(), &agent.public_key().to_hex(), "")
            .unwrap();
        let claimed = ws
            .claim_work_item(
                &home,
                wi.id,
                "claude-code",
                SessionId::from_ulid(mint_ulid()),
                &agent,
                Some(auth.clone()),
            )
            .unwrap();
        assert!(matches!(claimed.state, WorkItemState::Claimed { .. }));
        // Second claim refused by the transition table, and leaves no Claim fact.
        let before = ws.journal(&home).unwrap().len();
        assert!(matches!(
            ws.claim_work_item(
                &home,
                wi.id,
                "codex",
                SessionId::from_ulid(mint_ulid()),
                &agent,
                Some(auth)
            ),
            Err(StoreError::WorkItem(_))
        ));
        assert_eq!(ws.journal(&home).unwrap().len(), before);

        let started = ws
            .transition_work_item(&home, wi.id, &WorkItemTransition::Start)
            .unwrap();
        assert!(matches!(started.state, WorkItemState::InProgress { .. }));
        assert!(ws.delete_work_item(&home, wi.id).is_err(), "unsettled");
        ws.transition_work_item(&home, wi.id, &WorkItemTransition::Cancel)
            .unwrap();
        // A settled item takes no more words — the same one twice included —
        // and a refusal writes nothing.
        let settled = ws.journal(&home).unwrap().len();
        for again in [
            WorkItemTransition::Cancel,
            WorkItemTransition::Start,
            WorkItemTransition::Accept,
        ] {
            assert!(
                matches!(
                    ws.transition_work_item(&home, wi.id, &again),
                    Err(StoreError::WorkItem(_))
                ),
                "{again:?} after the item settled"
            );
        }
        assert_eq!(ws.journal(&home).unwrap().len(), settled);
        assert_eq!(
            ws.get_work_item(&home, wi.id).unwrap().state,
            WorkItemState::Cancelled
        );
        ws.delete_work_item(&home, wi.id).unwrap();
        assert!(ws.get_work_item(&home, wi.id).is_err());
        assert!(ws.home_of_work_item(wi.id).is_err());
    }

    #[test]
    fn the_default_budget_reads_the_three_settings_and_zero_is_no_ceiling() {
        let (_dir, ws) = ws();
        assert!(ws.default_budget().unwrap().is_unlimited(), "nothing set");
        ws.set_setting(
            bisa_core::settings::Scope::Workspace,
            None,
            Budget::SETTING_USD_CENTS,
            serde_json::json!(2500),
        )
        .unwrap();
        ws.set_setting(
            bisa_core::settings::Scope::Machine,
            None,
            Budget::SETTING_WALL_CLOCK_SECS,
            serde_json::json!(3600),
        )
        .unwrap();
        ws.set_setting(
            bisa_core::settings::Scope::Workspace,
            None,
            Budget::SETTING_TOKENS,
            serde_json::json!(0),
        )
        .unwrap();
        assert_eq!(
            ws.default_budget().unwrap(),
            Budget {
                max_tokens: None,
                max_usd_cents: Some(2500),
                max_wall_clock_secs: Some(3600),
            }
        );
    }

    #[test]
    fn budget_ledger_enforcement() {
        let (_dir, ws) = ws();
        let mut goal = ws.create_goal(NewGoal::captured("cheap task")).unwrap();
        goal.budget.max_tokens = Some(100);
        let goal = ws.update_goal(goal).unwrap();
        let home = Home::from(goal.id);
        assert!(ws.budget_allows(&home).unwrap());
        ws.add_spend(&home, 60, 0, 0).unwrap();
        assert!(ws.budget_allows(&home).unwrap());
        ws.add_spend(&home, 60, 0, 0).unwrap();
        assert!(!ws.budget_allows(&home).unwrap());
    }

    #[test]
    fn rebuild_matches_incremental_with_foreign_keys_on() {
        let (_dir, ws) = ws();
        let (a, run) = with_run(&ws);
        let b = ws.create_goal(NewGoal::captured("beta statement")).unwrap();
        let home = Home::from(a.id);
        ws.add_spend(&home, 10, 1, 5).unwrap();
        ws.add_edge(b.id, a.id, GoalEdgeKind::Refines).unwrap();
        let p = ws
            .create_project(crate::projects::NewProject::managed("app").unwrap())
            .unwrap();
        ws.attach(a.id, p.id).unwrap();
        ws.record_session(&SessionRow {
            id: "s1".into(),
            adapter: "claude-code".into(),
            status: SessionStatus::Live,
            ..Default::default()
        })
        .unwrap();
        let mut spec = item(a.id, "the build step's item");
        spec.run = Some(run.id);
        spec.step = Some(sid("build"));
        ws.put_work_item(&spec).unwrap();
        ws.record_run_event(
            run.id,
            RunEvent::StepStarted {
                step: sid("build"),
                work_item: Some(spec.id),
            },
        )
        .unwrap();
        ws.record_decision(&home, Gate::Approval, true, "adopt:x", None, None)
            .unwrap();
        let wf = ws.list_workflows().unwrap().remove(0);
        let host = bisa_core::ListenerHost::Workspace { workflow: wf.id };
        let on = bisa_core::Listening {
            inputs: BTreeMap::new(),
            budget: None,
            since: 7,
            paused: None,
        };
        ws.set_listening(&host, Some(on.clone())).unwrap();
        let listener = bisa_core::ListenerKey {
            host,
            step: sid("nightly"),
        };
        ws.enqueue_signal(&bisa_core::Signal {
            id: "01SIGNAL".into(),
            listener: Some(listener.clone()),
            source: bisa_core::SignalSource::Schedule,
            name: None,
            at: 5,
            payload: serde_json::json!({ "at": 5 }),
            scope: bisa_core::SignalScope::Workspace,
            chain: bisa_core::Chain::default(),
            dedupe_key: Some("schedule:5".into()),
        })
        .unwrap();

        let before = ws.dump_goal_rows().unwrap();
        let spend_before = ws.spent(&home).unwrap();
        let run_before = ws.idx().get_run(&run.id.to_string()).unwrap();
        let steps_before = ws.idx().armed_steps().unwrap();
        assert!(ws.idx().foreign_keys_enabled().unwrap());
        ws.rebuild_index().unwrap();
        assert_eq!(ws.dump_goal_rows().unwrap(), before);
        assert_eq!(ws.spent(&home).unwrap(), spend_before);
        assert_eq!(ws.idx().get_run(&run.id.to_string()).unwrap(), run_before);
        assert_eq!(ws.idx().armed_steps().unwrap(), steps_before);
        assert_eq!(
            ws.edges_from(b.id).unwrap(),
            vec![(a.id, GoalEdgeKind::Refines)]
        );
        assert_eq!(
            ws.session_by_id("s1").unwrap().unwrap().status,
            SessionStatus::Live
        );
        assert_eq!(ws.search("parser").unwrap(), vec![a.id]);
        assert_eq!(ws.projects_for(a.id).unwrap().len(), 1);
        assert_eq!(ws.decisions().unwrap().len(), 1);
        let items = ws.idx().work_items_for(&a.id.to_string()).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].run_id.as_deref(),
            Some(run.id.to_string().as_str())
        );
        assert_eq!(ws.list_workflows().unwrap(), vec![wf.clone()]);
        assert_eq!(ws.listening(&host).unwrap(), Some(on));
        assert_eq!(
            ws.idx()
                .workflow_listening_since(&wf.id.to_string())
                .unwrap(),
            Some(7)
        );
        assert_eq!(
            ws.pending_signals(&listener)
                .unwrap()
                .iter()
                .map(|q| q.signal.id.as_str())
                .collect::<Vec<_>>(),
            vec!["01SIGNAL"],
            "the queue survives the rebuild"
        );
    }

    #[test]
    fn delete_goal_removes_fs_and_rows_and_detaches_projects() {
        let (_dir, ws) = ws();
        let a = ws.create_goal(NewGoal::captured("to be deleted")).unwrap();
        let p = ws
            .create_project(crate::projects::NewProject::managed("kept").unwrap())
            .unwrap();
        ws.attach(a.id, p.id).unwrap();
        ws.delete_goal(a.id).unwrap();
        assert!(ws.get_goal(a.id).is_err());
        assert!(
            ws.get_project(p.id).is_ok(),
            "a delete detaches, never destroys"
        );
        assert!(ws.goals_of_project(p.id).unwrap().is_empty());
        ws.rebuild_index().unwrap();
        assert!(ws.dump_goal_rows().unwrap().is_empty());
        assert!(ws.get_project(p.id).is_ok());
    }

    #[test]
    fn an_old_goal_snapshot_is_refused_as_unreadable() {
        let (_dir, ws) = ws();
        let goal = ws
            .create_goal(NewGoal::captured("from another era"))
            .unwrap();
        // Overwrite the snapshot with the lifecycle-shaped goal, signed by the
        // owner, at a later revision — exactly what an older build wrote.
        let old = serde_json::json!({
            "id": goal.id.to_string(),
            "statement": "from another era",
            "author": goal.author.as_hex(),
            "state": "shaping",
            "criteria": [],
            "budget": {},
            "mode": "guided",
            "revision": goal.revision + 1,
            "created_at": goal.created_at
        });
        ws.snapshots
            .put(
                &Paths::ns_goal(goal.id),
                KIND_GOAL,
                &goal.id.to_string(),
                &old,
                goal.revision + 1,
                &ws.owner,
                now_secs() + 5,
                None,
                &[],
            )
            .unwrap();
        match ws.get_goal(goal.id) {
            Err(StoreError::Unreadable { what, path, .. }) => {
                assert_eq!(what, "goal");
                assert!(
                    path.ends_with(&format!("{KIND_GOAL}-{}.json", goal.id)),
                    "{path}"
                );
            }
            other => panic!("expected Unreadable, got {other:?}"),
        }
    }

    /// One goal whose snapshot another shape of the code wrote must not
    /// empty the list: the lists skip it and say so, a single read refuses
    /// it, and a rebuild is the one thing that drops its row.
    #[test]
    fn list_goals_skips_an_unreadable_snapshot() {
        let (_dir, ws) = ws();
        let a = ws.create_goal(NewGoal::captured("readable")).unwrap();
        let b = ws
            .create_goal(NewGoal::captured("written elsewhere"))
            .unwrap();
        assert_eq!(ws.list_goals(None).unwrap().len(), 2);

        // `Goal` is `deny_unknown_fields`: a field this build never wrote is
        // the shape of a snapshot from another one.
        ws.snapshots
            .put(
                &Paths::ns_goal(b.id),
                KIND_GOAL,
                &b.id.to_string(),
                &serde_json::json!({"id": b.id.to_string(), "from_another_build": true}),
                99,
                &ws.owner,
                now_secs(),
                None,
                &[],
            )
            .unwrap();
        assert!(
            matches!(ws.get_goal(b.id), Err(StoreError::Unreadable { .. })),
            "a single read refuses it"
        );
        let listed: Vec<GoalId> = ws.list_goals(None).unwrap().iter().map(|g| g.id).collect();
        assert_eq!(listed, vec![a.id], "the list skips it and keeps the rest");
        assert_eq!(ws.list_archived_goals().unwrap().len(), 0);

        // A rebuild reads the disk and leaves the unreadable goal out of the index.
        ws.rebuild_index().unwrap();
        let listed: Vec<GoalId> = ws.list_goals(None).unwrap().iter().map(|g| g.id).collect();
        assert_eq!(listed, vec![a.id]);
    }

    #[test]
    fn the_index_lock_recovers_from_poison() {
        let (_dir, ws) = ws();
        let ws = std::sync::Arc::new(ws);
        let poisoner = std::sync::Arc::clone(&ws);
        let poisoned = std::thread::spawn(move || {
            let _guard = poisoner.idx();
            panic!("poison the lock on purpose");
        })
        .join();
        assert!(poisoned.is_err(), "the thread panicked on purpose");
        assert!(ws.list_goals(None).is_ok(), "a poisoned lock is recovered");
    }
}
