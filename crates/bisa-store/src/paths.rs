//! Where everything goes, in one place.
//!
//! **This module is the only place that knows the workspace layout.** Nothing
//! anywhere else joins a workspace directory name by hand — a test in
//! `tests/it/layout.rs` greps every crate for the literals below and fails on any
//! occurrence outside this file. The failure mode that rule prevents is silent:
//! the sync layer once walked `root.join("goals")` inside an `if let Ok`, so a
//! renamed directory made it publish nothing and report success.
//!
//! # Anything that becomes a filename is a type
//!
//! `GoalId`, `ProjectId`, `WorkstreamId`, `NoteId` are ULIDs; `Slug`, `AgentId`,
//! `TeamId`, `SkillId`, `McpId`, `ChannelId` are parsed allowlists; an
//! attachment address is a validated digest. Every accessor here takes one of
//! those, so the compiler refuses a `String` that happened to be checked
//! somewhere upstream. The ids the domain still carries as plain strings — a
//! pet's, a session's, a conversation scope — are checked at the door by
//! [`Paths::stem`]; a listener's file name is built from its typed key.
//!
//! # Containment
//!
//! [`resolve_within`] is the one boundary check, and it is deliberately not a
//! string prefix test. A `..` component, an absolute path and a symlink
//! pointing out of the tree are three ways to ask the same question, and only
//! canonicalization answers all three.
//!
//! It takes a base rather than being fixed to the root because an adopted
//! project's root is legitimately outside the workspace
//! ([`bisa_core::ProjectRoot::External`]): the boundary for a path is the
//! root it was reached through.
//!
//! # The layout
//!
//! See `docs/architecture/04-workspace-project-goal.md` — the tree there and
//! the accessors here are the same list.

use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use bisa_core::{
    AccountId, AddonId, AgentId, ConnectorId, ConversationId, DrawingId, GoalId, Home, ListenerKey,
    McpId, NoteId, RunId, Sha256, SkillId, Slug, TeamId, WorkItemId, WorkflowId, WorkstreamId,
};

use crate::error::StoreError;

/// Every path in a workspace, derived from its root.
///
/// Cheap to clone and to hand around; it holds a `PathBuf` and no state.
#[derive(Clone, Debug)]
pub struct Paths {
    root: PathBuf,
}

impl Paths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    // ------------------------------------------------------------------
    // Top-level directories
    // ------------------------------------------------------------------

    pub fn goals_dir(&self) -> PathBuf {
        self.root.join("goals")
    }

    /// Projects are workspace citizens: nothing under here names a goal.
    pub fn projects_dir(&self) -> PathBuf {
        self.root.join("projects")
    }

    pub fn agents_dir(&self) -> PathBuf {
        self.root.join(Self::NS_AGENTS)
    }

    pub fn teams_dir(&self) -> PathBuf {
        self.root.join(Self::NS_TEAMS)
    }

    pub fn skills_dir(&self) -> PathBuf {
        self.root.join(Self::NS_SKILLS)
    }

    pub fn mcp_dir(&self) -> PathBuf {
        self.root.join("mcp")
    }

    /// Connector definitions — `connectors/<id>.json`, synced like a skill.
    pub fn connectors_dir(&self) -> PathBuf {
        self.root.join(Self::NS_CONNECTORS)
    }

    pub fn connector_file(&self, id: &ConnectorId) -> PathBuf {
        self.connectors_dir().join(format!("{id}.json"))
    }

    /// A connector's accounts on this machine — the non-secret records,
    /// one per account, under `identity/connectors/<connector>/`; the
    /// secrets themselves are in the keystore. Never synced, like
    /// everything under `identity/`.
    pub fn connector_accounts_dir(&self, connector: &ConnectorId) -> PathBuf {
        self.identity_dir()
            .join("connectors")
            .join(connector.as_str())
    }

    pub fn connector_account_file(&self, connector: &ConnectorId, account: AccountId) -> PathBuf {
        self.connector_accounts_dir(connector)
            .join(format!("{account}.json"))
    }

    /// The **notes repository**: every note, whatever it is about, as a
    /// Markdown file under one root that is a git repository the person
    /// commits and pushes — `notes/workspace/`, `notes/goals/<id>/`,
    /// `notes/projects/<slug>/`, `notes/workflows/<id>/`,
    /// `notes/channels/<id>/`, `notes/node/` ([`Self::scoped_dir`]). Here
    /// rather than beside the goal or the project, so one repository holds
    /// them all, and never inside a project's own tree.
    pub fn notes_dir(&self) -> PathBuf {
        self.root.join("notes")
    }

    /// The **drawings repository** (19 — Drawings): every drawing's
    /// `.excalidraw` file under one root laid out like the notes', a git
    /// repository the person commits and pushes; the records themselves
    /// are the namespace's snapshots in `drawings/state/`, which the
    /// repository ignores.
    pub fn drawings_dir(&self) -> PathBuf {
        self.root.join(Self::NS_DRAWINGS)
    }

    /// Where a scope's files sit under a repository root — the one layout
    /// notes and drawings share: `<base>/workspace/`, `<base>/node/`,
    /// `<base>/goals/<id>/`, `<base>/projects/<slug>/`,
    /// `<base>/workflows/<id>/`, `<base>/channels/<id>/`. `kind` is one of
    /// [`OwnerScope::KINDS`]; `id` is the record's — a project's **slug**, so
    /// the folder reads in a clone — and none for the two standalone kinds.
    pub fn scoped_dir(base: &Path, kind: &str, id: Option<&str>) -> PathBuf {
        match (kind, id) {
            ("goal", Some(id)) => base.join("goals").join(id),
            ("project", Some(slug)) => base.join("projects").join(slug),
            ("workflow", Some(id)) => base.join("workflows").join(id),
            ("channel", Some(id)) => base.join("channels").join(id),
            (kind, _) => base.join(kind),
        }
    }

    /// A note's file inside its scope's directory: `<id>.md`.
    pub fn note_file_in(dir: &Path, id: NoteId) -> PathBuf {
        dir.join(format!("{id}.md"))
    }

    /// The name a note's file carries, for a listing: `<id>.md`, or `None`.
    pub fn note_id_of_file(name: &str) -> Option<NoteId> {
        name.strip_suffix(".md").and_then(|stem| stem.parse().ok())
    }

    /// A drawing's file inside its scope's directory: `<id>.excalidraw` —
    /// the standard file, so it opens anywhere Excalidraw does.
    pub fn drawing_file_in(dir: &Path, id: DrawingId) -> PathBuf {
        dir.join(format!("{id}.excalidraw"))
    }

    /// Installed pet packages, one directory per pet, stored exactly as they
    /// arrived so what is here stays a valid package.
    pub fn pets_dir(&self) -> PathBuf {
        self.root.join("pets")
    }

    /// Installed addons: one directory per addon — its record and its
    /// bundle — beside the namespace's `state/` (18 — Addons).
    pub fn addons_dir(&self) -> PathBuf {
        self.root.join(Self::NS_ADDONS)
    }

    /// The name of the bundle folder inside an addon's directory.
    pub const ADDON_FILES_DIR: &'static str = "files";

    pub fn addon_dir(&self, id: &AddonId) -> PathBuf {
        self.addons_dir().join(id.as_str())
    }

    /// The record: the manifest as installed, its origin, enabled, grants.
    pub fn addon_record(&self, id: &AddonId) -> PathBuf {
        self.addon_dir(id).join("addon.json")
    }

    /// The bundle the node serves, exactly as it arrived.
    pub fn addon_files_dir(&self, id: &AddonId) -> PathBuf {
        self.addon_dir(id).join(Self::ADDON_FILES_DIR)
    }

    /// Where an install is assembled before one rename puts it in place: a
    /// dotted name, which no addon id can take and every listing skips.
    pub fn addon_staging_dir(&self) -> PathBuf {
        self.addons_dir().join(format!(
            ".staging-{}-{}",
            std::process::id(),
            WRITE_SEQ.fetch_add(1, Ordering::Relaxed)
        ))
    }

    /// The listeners' own folder — this machine's, never synced: the durable
    /// queue of occurrences ([`Self::signal_queue`]), each listener's runtime
    /// memory (`listeners/`), and the folders a check start runs in when it
    /// names no project (`scratch/`).
    pub fn events_dir(&self) -> PathBuf {
        self.root.join("events")
    }

    /// Workflow definitions. A workflow's truth is its signed snapshot under
    /// `workflows/state/`, like a channel's — no JSON twin.
    pub fn workflows_dir(&self) -> PathBuf {
        self.root.join(Self::NS_WORKFLOWS)
    }

    /// The runs of the workspace, one folder each — `workflows/runs/<RunId>/`,
    /// beside the library's `workflows/state/`. A goal's runs are filed under
    /// the goal instead.
    pub fn workspace_runs_dir(&self) -> PathBuf {
        self.workflows_dir().join(Self::RUNS)
    }

    /// Whether the library's workflows listen — one record per workflow that
    /// is On, `workflows/listening/<WorkflowId>.json`. This machine's: turning
    /// a workflow On is no revision of it, and nothing here syncs.
    pub fn listening_dir(&self) -> PathBuf {
        self.workflows_dir().join("listening")
    }

    pub fn listening_file(&self, wf: WorkflowId) -> PathBuf {
        self.listening_dir().join(format!("{wf}.json"))
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.root.join("sessions")
    }

    /// The activity log: the engine's own facts, one month per file
    /// (`activity_log.rs`).
    pub fn activity_dir(&self) -> PathBuf {
        self.root.join("activity")
    }

    pub fn conversation_dir(&self) -> PathBuf {
        self.root.join("conversation")
    }

    /// Message files: content-addressed, sharded one level, immutable.
    pub fn attachments_dir(&self) -> PathBuf {
        self.root.join("attachments")
    }

    pub fn identity_dir(&self) -> PathBuf {
        self.root.join("identity")
    }

    /// The code host tokens: one directory per kind under
    /// `identity/codehost/` — `github/`, `gitlab/`, `bitbucket/` — each holding
    /// one `0600` file per login plus the non-secret `accounts` list. Never
    /// synced, like everything under `identity/`.
    pub fn codehost_tokens_root(&self) -> PathBuf {
        self.identity_dir().join("codehost")
    }

    /// The git profiles the platform owns — one `<slug>.gitconfig` per
    /// profile, the files the person's global git config includes by remote
    /// (ide/04 §Profiles by organization): `identity/git/profiles/`.
    pub fn git_profiles_dir(&self) -> PathBuf {
        self.identity_dir().join("git").join("profiles")
    }

    /// Sockets, the bearer token, the engine lock, terminal scrollback: per-run
    /// state that never syncs and is never a session directory.
    pub fn run_dir(&self) -> PathBuf {
        self.root.join("run")
    }

    /// IDE window furniture and caches — rebuildable or disposable, never
    /// synced.
    pub fn ide_dir(&self) -> PathBuf {
        self.root.join("ide")
    }

    /// The diagnostic log: what the node, the CLI, the MCP servers and the
    /// desktop write about themselves. This is the one name the store owns
    /// here; everything inside it — one folder per process family, the
    /// crash reports, the run markers — is `bisa-log`'s to name, a
    /// leaf that depends on no crate of ours and so sits outside the layout
    /// guard by construction. This machine's, disposable, never indexed,
    /// never synced.
    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    // ------------------------------------------------------------------
    // Workspace-level files
    // ------------------------------------------------------------------

    pub fn index_db(&self) -> PathBuf {
        self.root.join("index.sqlite")
    }

    pub fn members_file(&self) -> PathBuf {
        self.root.join("members.json")
    }

    pub fn governance_file(&self) -> PathBuf {
        self.root.join("governance.json")
    }

    /// The invitations this host made (14-collaboration): secrets' hashes
    /// alone, mode 0600.
    pub fn invites_file(&self) -> PathBuf {
        self.root.join("invites.json")
    }

    /// Messages from outside held back from agents, with the reason.
    pub fn held_file(&self) -> PathBuf {
        self.root.join("held.json")
    }

    pub fn seen_file(&self) -> PathBuf {
        self.root.join("seen.jsonl")
    }

    /// The durable signal queue: an append-only log of enqueues and state
    /// changes, last line per id wins.
    pub fn signal_queue(&self) -> PathBuf {
        self.events_dir().join("queue.jsonl")
    }

    /// What every listener file of one host begins with:
    /// `<host kind>-<host id>-` — a ULID, so nothing escapes.
    pub fn host_stem_prefix(host: &bisa_core::ListenerHost) -> String {
        format!("{}-{}-", host.kind(), host.id())
    }

    /// The file-name stem of one listener: the host's prefix and its step
    /// word, both allowlists.
    fn listener_stem(key: &ListenerKey) -> String {
        format!("{}{}", Self::host_stem_prefix(&key.host), key.step)
    }

    /// The listeners' runtime memories, one file each.
    pub fn listeners_dir(&self) -> PathBuf {
        self.events_dir().join("listeners")
    }

    /// One listener's runtime memory — when it next comes due, what a poll
    /// has seen, a project's last heads. It names this machine's state, so it
    /// is never snapshotted, and turning the host On starts it afresh.
    pub fn listener_runtime_file(&self, key: &ListenerKey) -> PathBuf {
        self.listeners_dir()
            .join(format!("{}.json", Self::listener_stem(key)))
    }

    /// Where a check start that names no project runs its command.
    pub fn listener_scratch_dir(&self, key: &ListenerKey) -> PathBuf {
        self.events_dir()
            .join("scratch")
            .join(Self::listener_stem(key))
    }

    /// Machine-scope settings. Never synced: it belongs to this machine.
    pub fn machine_settings(&self) -> PathBuf {
        self.root.join("machine.json")
    }

    /// Workspace-scope settings. Syncs.
    pub fn workspace_settings(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    /// The control-plane bearer token, mode 0600.
    pub fn token_file(&self) -> PathBuf {
        self.run_dir().join("token")
    }

    /// One engine per workspace: flock + PID + start time.
    pub fn engine_lock(&self) -> PathBuf {
        self.run_dir().join("engine.lock")
    }

    pub fn node_socket(&self) -> PathBuf {
        self.run_dir().join("node.sock")
    }

    // ------------------------------------------------------------------
    // Quarantine
    // ------------------------------------------------------------------

    /// Where a file this build could not read is moved aside — never
    /// deleted: `quarantine/<unix-stamp>/<the file's path under the root>`.
    /// This machine's, never synced, never indexed; a person restores from
    /// it by hand.
    pub fn quarantine_dir(&self) -> PathBuf {
        self.root.join("quarantine")
    }

    /// Move `path` aside under a stamped quarantine folder, keeping its path
    /// under the root (a file from outside the root keeps its name alone),
    /// and answer where it went. A rename, so the bytes are untouched; the
    /// two folders are synced so the move survives the power going out.
    pub fn quarantine(&self, path: &Path, at: u64) -> Result<PathBuf, StoreError> {
        let relative: PathBuf = match path.strip_prefix(&self.root) {
            Ok(rel) => rel.to_path_buf(),
            Err(_) => PathBuf::from(path.file_name().unwrap_or(path.as_os_str())),
        };
        let stamped = self.quarantine_dir().join(at.to_string());
        let mut dest = stamped.join(&relative);
        // A second file of the same name in the same second keeps both.
        let mut n = 1;
        while dest.exists() {
            let mut name = relative
                .file_name()
                .map(|s| s.to_os_string())
                .unwrap_or_default();
            name.push(format!(".{n}"));
            dest = stamped.join(relative.with_file_name(name));
            n += 1;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| StoreError::io(parent.display().to_string(), e))?;
        } // LCOV_EXCL_LINE: a quarantined file lands under a stamped folder, which is its parent
        std::fs::rename(path, &dest).map_err(|e| StoreError::io(path.display().to_string(), e))?;
        if let Some(parent) = dest.parent() {
            sync_dir(parent);
        }
        if let Some(parent) = path.parent() {
            sync_dir(parent);
        }
        Ok(dest)
    }

    /// Where the desktop shell keeps a terminal tab's scrollback checkpoints
    /// (ide/06). The shell writes there by name — it cannot depend on this
    /// crate — so nothing in the workspace calls this; it stands as the
    /// layout's one declaration of the folder, beside the reference page.
    pub fn terminals_dir(&self) -> PathBuf {
        self.run_dir().join("terminals")
    }

    /// The saved workbench layout for one root: `ide/layout/<scope>-<id>.json`.
    /// The id is the caller's, already checked to be a file-name-safe token.
    pub fn ide_layout(&self, scope: crate::tree::FileScope, id: &str) -> PathBuf {
        self.ide_dir()
            .join("layout")
            .join(format!("{}-{id}.json", scope.as_str()))
    }

    /// What agents changed in a checkout, per conversation
    /// (`changes.rs`): `ide/changes/<conversation>/` holds the ledger, the
    /// blobs it names and the private git index its snapshots are taken
    /// through. This machine's — a checkout is — and never synced.
    pub fn changes_dir(&self, conversation: ConversationId) -> PathBuf {
        self.ide_dir()
            .join("changes")
            .join(conversation.to_string())
    }

    pub fn change_ledger(&self, conversation: ConversationId) -> PathBuf {
        self.changes_dir(conversation).join("ledger.json")
    }

    /// One blob of a conversation's changes, addressed by its digest — a
    /// [`Sha256`] is an allowlist, so the name cannot leave the directory.
    pub fn change_blob(&self, conversation: ConversationId, sha: &Sha256) -> PathBuf {
        self.change_blobs_dir(conversation).join(sha.as_str())
    }

    pub fn change_blobs_dir(&self, conversation: ConversationId) -> PathBuf {
        self.changes_dir(conversation).join("blobs")
    }

    /// The index file a snapshot of the checkout is staged through, so the
    /// person's own index is never touched.
    pub fn change_index(&self, conversation: ConversationId) -> PathBuf {
        self.changes_dir(conversation).join("index")
    }

    // ------------------------------------------------------------------
    // Addressed files
    // ------------------------------------------------------------------

    pub fn agent_file(&self, id: &AgentId) -> PathBuf {
        self.agents_dir().join(format!("{id}.json"))
    }

    pub fn team_file(&self, id: &TeamId) -> PathBuf {
        self.teams_dir().join(format!("{id}.json"))
    }

    pub fn skill_file(&self, id: &SkillId) -> PathBuf {
        self.skills_dir().join(format!("{id}.json"))
    }

    pub fn mcp_file(&self, id: &McpId) -> PathBuf {
        self.mcp_dir().join(format!("{id}.json"))
    }

    pub fn pet_dir(&self, id: &str) -> Result<PathBuf, StoreError> {
        bisa_core::validate_pet_id(id)?;
        Ok(self.pets_dir().join(id))
    }

    pub fn pet_manifest(&self, id: &str) -> Result<PathBuf, StoreError> {
        Ok(self.pet_dir(id)?.join("pet.json"))
    }

    pub fn session_meta_file(&self, adapter: &str, id: &str) -> Result<PathBuf, StoreError> {
        Ok(self
            .sessions_dir()
            .join(Self::stem("adapter", adapter)?)
            .join(format!("{}.json", Self::stem("session id", id)?)))
    }

    /// One conversation's append-only fact log.
    pub fn conversation_log(&self, scope: &str) -> Result<PathBuf, StoreError> {
        Ok(self
            .conversation_dir()
            .join(format!("{}.jsonl", Self::stem("scope id", scope)?)))
    }

    /// The blob for a SHA-256 digest, or `None` when the digest is not one.
    ///
    /// An allowlist — 64 lowercase hex characters — for the same reason a
    /// project slug is: a digest containing `/` or `..` would otherwise
    /// address a file outside the store, and the caller is an HTTP route.
    pub fn attachment(&self, sha256: &str) -> Option<PathBuf> {
        if !bisa_core::AttachmentRef::is_valid_hash(sha256) {
            return None;
        }
        let (shard, rest) = sha256.split_at(2);
        Some(self.attachments_dir().join(shard).join(rest))
    }

    /// Where a blob's real-named copies live: `attachments/named/<sha>/<name>`.
    /// `named` is never two hex characters, so it cannot collide with a shard.
    pub fn attachments_named_dir(&self) -> PathBuf {
        self.attachments_dir().join("named")
    }

    /// A blob under the name its maker gave it — the one file on disk a file
    /// manager can reveal and the default application can open, since the
    /// blob itself has no extension. `None` when the digest is not one or the
    /// name sanitises to nothing.
    pub fn attachment_named(&self, sha256: &str, name: &str) -> Option<PathBuf> {
        if !bisa_core::AttachmentRef::is_valid_hash(sha256) {
            return None;
        }
        let safe = sanitise_file_name(name)?;
        Some(self.attachments_named_dir().join(sha256).join(safe))
    }

    // ------------------------------------------------------------------
    // Snapshot namespaces
    //
    // A `SnapshotStore` addresses by a relative namespace path and keeps its
    // events under `<ns>/state/`. The namespaces are strings because the
    // store joins them; they live here so a rename is one edit.
    // ------------------------------------------------------------------

    pub const NS_AGENTS: &'static str = "agents";
    pub const NS_TEAMS: &'static str = "teams";
    pub const NS_SKILLS: &'static str = "skills";
    pub const NS_CONNECTORS: &'static str = "connectors";
    pub const NS_CHANNELS: &'static str = "channels";
    pub const NS_CONVERSATIONS: &'static str = "conversations";
    pub const NS_PROJECTS: &'static str = "projects";
    pub const NS_WORKFLOWS: &'static str = "workflows";
    /// Addon records — `addons/state/33407-<id>.json`; the bundles sit
    /// beside them under `addons/<id>/`.
    pub const NS_ADDONS: &'static str = "addons";
    /// Drawings: the records as snapshots in `drawings/state/`, the files beside them (19).
    pub const NS_DRAWINGS: &'static str = "drawings";

    /// A library workflow — `workflows/state/33412-<wf>.json`. A goal's design
    /// is filed under its goal instead ([`GoalPaths::workflow_snapshot`]).
    pub fn library_workflow_snapshot(&self, wf: WorkflowId) -> PathBuf {
        self.state_dir(Self::NS_WORKFLOWS)
            .join(format!("{}-{wf}.json", bisa_core::kind::KIND_WORKFLOW))
    }

    /// The folder under `workflows/` the runs of the workspace live in.
    const RUNS: &'static str = "runs";

    /// A goal's own namespace: `goals/<id>`.
    pub fn ns_goal(id: GoalId) -> String {
        format!("goals/{id}")
    }

    /// A run of the workspace's own namespace: `workflows/runs/<id>`.
    pub fn ns_run(id: RunId) -> String {
        format!("{}/{}/{id}", Self::NS_WORKFLOWS, Self::RUNS)
    }

    /// The namespace a home files its snapshots under: the goal's, or the
    /// run of the workspace's own.
    pub fn ns_home(home: &Home) -> String {
        match home {
            Home::Goal { goal } => Self::ns_goal(*goal),
            Home::Run { run } => Self::ns_run(*run),
        }
    }

    /// An agent's Recall namespace: `agents/<id>/recall`.
    pub fn ns_recall(agent: &AgentId) -> String {
        format!("agents/{agent}/recall")
    }

    /// Where a namespace keeps its addressable snapshots.
    pub fn state_dir(&self, ns: &str) -> PathBuf {
        self.root.join(ns).join("state")
    }

    /// The project namespace keeps its snapshots in `projects/state/`, which
    /// is why no project may take that slug.
    pub const RESERVED_PROJECT_SLUGS: &'static [&'static str] = &["state"];

    // ------------------------------------------------------------------
    // Sub-views
    // ------------------------------------------------------------------

    /// The folder a home files a run's truth in — the same shape for a goal
    /// and for a run of the workspace.
    pub fn home(&self, home: &Home) -> HomePaths {
        HomePaths {
            home: *home,
            dir: self.root.join(Self::ns_home(home)),
        }
    }

    pub fn goal(&self, id: GoalId) -> GoalPaths {
        GoalPaths {
            id,
            home: self.home(&Home::Goal { goal: id }),
        }
    }

    pub fn project(&self, slug: &Slug) -> ProjectPaths {
        ProjectPaths {
            dir: self.projects_dir().join(slug),
        }
    }

    /// Every project folder on disk, by the folders alone — for a reader
    /// that must not open the index (`check.rs`). An absent `projects/` is
    /// no project; a folder that cannot be listed is the error.
    pub fn project_dirs(&self) -> Result<Vec<ProjectPaths>, StoreError> {
        let projects = self.projects_dir();
        if !projects.is_dir() {
            return Ok(Vec::new());
        }
        let mut out: Vec<ProjectPaths> = std::fs::read_dir(&projects)
            .map_err(|e| StoreError::io(projects.display().to_string(), e))?
            .flatten()
            .map(|entry| entry.path())
            .filter(|dir| dir.is_dir())
            .map(|dir| ProjectPaths { dir })
            .collect();
        out.sort_by(|a, b| a.dir.cmp(&b.dir));
        Ok(out)
    }

    pub fn agent(&self, id: &AgentId) -> AgentPaths {
        AgentPaths {
            dir: self.agents_dir().join(id.as_str()),
        }
    }

    // ------------------------------------------------------------------
    // Containment
    // ------------------------------------------------------------------

    /// Resolve a caller-supplied relative path inside the workspace. Use the
    /// free function directly when the boundary is a project root.
    pub fn resolve(&self, relative: &str) -> Result<PathBuf, StoreError> {
        resolve_within(&self.root, relative)
    }

    /// A string that is about to become one path component.
    ///
    /// The ids the domain carries as plain strings — sessions, adapters,
    /// terminal keys, conversation scopes — pass through here before
    /// they are joined. Anything with a separator, a `..`, a leading dot or a
    /// NUL is refused by name.
    pub fn stem<'a>(what: &str, s: &'a str) -> Result<&'a str, StoreError> {
        let bad = |why: &str| {
            StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-cannot-be-used-as-file-name",
                what = what.to_string(),
                s = format!("{s:?}"),
                why = why.to_string()
            ))
        };
        if s.is_empty() {
            return Err(bad("it is empty"));
        }
        if s.len() > 200 {
            return Err(bad("it is longer than 200 bytes"));
        }
        if s.starts_with('.') {
            return Err(bad("it starts with a dot"));
        }
        if s.contains("..") {
            return Err(bad("it contains '..'"));
        }
        if s.bytes()
            .any(|b| b == b'/' || b == b'\\' || b == 0 || b.is_ascii_control())
        {
            return Err(bad("it contains a path separator or a control character"));
        }
        Ok(s)
    }
}

/// A home's folder — where a run's truth is filed: its goal's folder, or a
/// run of the workspace's own under `workflows/runs/<RunId>/`. The same
/// shape either way, so everything a run writes finds its place through one
/// type whatever the run is for.
#[derive(Clone, Debug)]
pub struct HomePaths {
    home: Home,
    dir: PathBuf,
}

impl HomePaths {
    pub fn home(&self) -> Home {
        self.home
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The signed event log — the truth this whole tree is an index over.
    pub fn journal(&self) -> PathBuf {
        self.dir.join("journal.jsonl")
    }

    /// Addressable snapshots: `state/<wire kind>-<d>.json`.
    pub fn state(&self) -> PathBuf {
        self.dir.join("state")
    }

    /// One run's snapshot, `state/33413-<run>.json`.
    pub fn run_snapshot(&self, run: RunId) -> PathBuf {
        self.state()
            .join(format!("{}-{run}.json", bisa_core::kind::KIND_WORKFLOW_RUN))
    }

    /// What the home's sessions spent, one line per charge.
    pub fn ledger(&self) -> PathBuf {
        self.dir.join("ledger.jsonl")
    }

    /// A work item's captured results: the patch a copy workstream left
    /// behind when its item settled.
    pub fn results(&self) -> PathBuf {
        self.dir.join("results")
    }

    pub fn result(&self, item: WorkItemId) -> PathBuf {
        self.results().join(format!("{item}.patch"))
    }

    /// The home's scratch folder: where an agent step with no project runs
    /// and leaves its deliverable, where a `check` command runs when the run
    /// has no project, where a goal's Workflow Agent design session runs, and
    /// the parent of every session's `TMPDIR`.
    pub fn scratch(&self) -> PathBuf {
        self.dir.join("scratch")
    }

    /// `TMPDIR` for every session of this home, inside [`Self::scratch`].
    pub fn tmp(&self) -> PathBuf {
        self.scratch().join(".tmp")
    }
}

/// The paths a goal owns: a home, and the three things only a goal has — its
/// own designs, its edges and its documents.
#[derive(Clone, Debug)]
pub struct GoalPaths {
    id: GoalId,
    home: HomePaths,
}

/// A goal's folder *is* a home: every run-truth path reads through.
impl std::ops::Deref for GoalPaths {
    type Target = HomePaths;

    fn deref(&self) -> &HomePaths {
        &self.home
    }
}

impl GoalPaths {
    pub fn id(&self) -> GoalId {
        self.id
    }

    /// A design drawn for this goal — `state/33412-<wf>.json`, beside the
    /// goal's runs. It travels and dies with the goal.
    pub fn workflow_snapshot(&self, wf: WorkflowId) -> PathBuf {
        self.state()
            .join(format!("{}-{wf}.json", bisa_core::kind::KIND_WORKFLOW))
    }

    pub fn edges(&self) -> PathBuf {
        self.dir().join("edges.json")
    }

    /// The files a person gave this goal as context (`goal_documents.rs`):
    /// materialised from the journal's `document` facts, named as given,
    /// a taken name numbered. Read through the goal's file scope; written
    /// by the store alone.
    pub fn documents(&self) -> PathBuf {
        self.dir().join("documents")
    }

    /// One document by the name it was materialised under.
    pub fn document(&self, name: &str) -> PathBuf {
        self.documents().join(name)
    }
}

/// The paths a project owns. Everything here is a directory Bisa made;
/// an adopted (`External`) root is somewhere else and is never written to.
#[derive(Clone, Debug)]
pub struct ProjectPaths {
    dir: PathBuf,
}

impl ProjectPaths {
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The truth file.
    pub fn record(&self) -> PathBuf {
        self.dir.join("project.json")
    }

    /// Project-scope settings. Syncs; belongs to the code.
    pub fn settings(&self) -> PathBuf {
        self.dir.join("settings.json")
    }

    /// The repository itself — only when the root is `Managed`.
    pub fn tree(&self) -> PathBuf {
        self.dir.join("tree")
    }

    /// Review notes on diffs — local, never synced.
    pub fn review(&self) -> PathBuf {
        self.dir.join("review")
    }

    pub fn review_note(&self, id: NoteId) -> PathBuf {
        self.review().join(format!("{id}.json"))
    }

    pub fn workstreams(&self) -> PathBuf {
        self.dir.join("workstreams")
    }

    /// The workstream's record. Its checkout is [`Self::workstream_dir`] — they
    /// are siblings sharing a ULID stem, one `.json` and one directory.
    pub fn workstream_record(&self, id: WorkstreamId) -> PathBuf {
        self.workstreams().join(format!("{id}.json"))
    }

    pub fn workstream_dir(&self, id: WorkstreamId) -> PathBuf {
        self.workstreams().join(id.to_string())
    }
}

/// The paths an agent owns.
#[derive(Clone, Debug)]
pub struct AgentPaths {
    dir: PathBuf,
}

impl AgentPaths {
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The agent's scratch folder — where a chat instance of this agent runs.
    /// Not a repository; nothing commits it.
    pub fn scratch(&self) -> PathBuf {
        self.dir.join("scratch")
    }

    pub fn recall_index(&self) -> PathBuf {
        self.dir.join("recall_index.json")
    }
}

// ----------------------------------------------------------------------
// Containment and atomic writes
// ----------------------------------------------------------------------

/// Resolve `relative` under `base`, or refuse.
///
/// Three ways to leave a directory, all closed here: a `..` component
/// (rejected by component, before any I/O), an absolute path (a `RootDir` or
/// `Prefix` component, same filter) and a symlink pointing out (survives both,
/// which is why this canonicalizes rather than comparing strings).
///
/// A file name a person or an agent chose, made safe to join under one
/// directory: the last path component alone, no control characters, no
/// separators, never `.`, `..` or a dotfile, at most 255 bytes. `None` when
/// nothing safe is left — the caller then refuses by name rather than
/// inventing one.
pub fn sanitise_file_name(name: &str) -> Option<String> {
    let last = name.rsplit(['/', '\\']).next().unwrap_or(name);
    // A control character is never part of a name somebody meant: a NUL or
    // an escape inside one is refused, not quietly dropped into a lookalike.
    if last.chars().any(char::is_control) {
        return None;
    }
    let cleaned = last.trim().to_string();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." || cleaned.starts_with('.') {
        return None;
    }
    let mut out = cleaned;
    while out.len() > 255 {
        out.pop();
    }
    Some(out)
}

/// A path that does not exist yet still resolves: the deepest existing
/// ancestor is canonicalized and the remainder appended, so a caller can check
/// where a file *would* land before creating it.
pub fn resolve_within(base: &Path, relative: &str) -> Result<PathBuf, StoreError> {
    // Canonicalize the boundary before anything is compared against it or
    // named in a refusal, so every refusal spells the boundary one way.
    let base = base.canonicalize().map_err(|e| {
        StoreError::io(
            base.display().to_string(),
            std::io::Error::new(e.kind(), format!("resolving the boundary: {e}")),
        )
    })?;

    let requested = Path::new(relative);
    for component in requested.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            _ => {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-path-leaves",
                    relative = format!("{relative:?}"),
                    a0 = (base.display()).to_string()
                )))
            }
        }
    }

    let joined = base.join(requested);
    let mut existing = joined.as_path();
    let mut tail = PathBuf::new();
    let resolved = loop {
        match existing.canonicalize() {
            Ok(real) if tail.as_os_str().is_empty() => break real,
            Ok(real) => break real.join(&tail),
            Err(_) => {
                let name = existing.file_name().ok_or_else(|| {
                    // LCOV_EXCL_START: the base is canonical and exists, so the walk up from a path under it meets an existing ancestor with a name before any parent runs out
                    StoreError::Invalid(bisa_core::text!(
                        "error-store-invalid-path-leaves",
                        relative = format!("{relative:?}"),
                        a0 = (base.display()).to_string()
                    ))
                })?;
                // LCOV_EXCL_STOP
                // `PathBuf::join("")` appends a separator, so the first (innermost)
                // name stands alone: `README.md`, never `README.md/`.
                tail = if tail.as_os_str().is_empty() {
                    PathBuf::from(name)
                } else {
                    Path::new(name).join(&tail)
                };
                existing = existing.parent().ok_or_else(|| {
                    // LCOV_EXCL_START: the base is canonical and exists, so the walk up from a path under it meets an existing ancestor with a name before any parent runs out
                    StoreError::Invalid(bisa_core::text!(
                        "error-store-invalid-path-leaves",
                        relative = format!("{relative:?}"),
                        a0 = (base.display()).to_string()
                    ))
                })?;
                // LCOV_EXCL_STOP
            }
        }
    };

    if !resolved.starts_with(&base) {
        return Err(StoreError::Invalid(bisa_core::text!(
            "error-store-invalid-path-leaves",
            relative = format!("{relative:?}"),
            a0 = (base.display()).to_string()
        )));
    }
    Ok(resolved)
}

/// A counter that makes every temporary name in this process unique. Two
/// threads writing one path used to target the same `<name>.tmp.<pid>` and
/// could interleave into a byte-mix of two serializations.
static WRITE_SEQ: AtomicU64 = AtomicU64::new(0);

/// Write a file so a reader never sees half of one, and a crash never loses
/// what was acknowledged.
///
/// Write-then-rename, with three properties the shape it replaces did not have:
///
/// 1. **A unique temporary name per write**, not per process.
/// 2. **`sync_all` on the temporary before the rename, and an fsync of the
///    parent directory after.** Atomic without durable is atomic with respect
///    to other readers only, which is not what *the filesystem is truth* needs.
/// 3. **A failed rename removes its temporary**, so a workspace does not fill
///    with orphans.
///
/// The temporary name is appended rather than substituted for the extension,
/// so a `journal.jsonl` neighbour cannot be clobbered by a `journal.json.tmp`.
/// [`write_atomic`], and the file made the owner's alone (`0600`) — for a
/// file that holds a secret's hash or a person's word about people.
pub fn write_private(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    write_atomic(path, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| StoreError::io(path.display().to_string(), e))?;
    }
    Ok(())
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let parent = path.parent().ok_or_else(|| {
        StoreError::Invalid(bisa_core::text!(
            "error-store-invalid-not-file-path",
            a0 = (path.display()).to_string()
        ))
    })?;
    std::fs::create_dir_all(parent).map_err(|e| StoreError::io(parent.display().to_string(), e))?;
    let name = path.file_name().ok_or_else(|| {
        StoreError::Invalid(bisa_core::text!(
            "error-store-invalid-not-file-path",
            a0 = (path.display()).to_string()
        ))
    })?;
    let mut tmp_name = name.to_os_string();
    tmp_name.push(format!(
        ".tmp.{}.{}",
        std::process::id(),
        WRITE_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let tmp = path.with_file_name(tmp_name);

    let written = (|| -> std::io::Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        std::io::Write::write_all(&mut f, bytes)?;
        f.sync_all()
    })();
    if let Err(e) = written {
        if let Err(rm) = std::fs::remove_file(&tmp) {
            if rm.kind() != std::io::ErrorKind::NotFound {
                // LCOV_EXCL_START: a temporary this process just made is its own to remove; a removal refused here is the disk's, said and never fatal
                tracing::warn!(
                    "{}: could not remove a failed temporary: {rm}",
                    tmp.display()
                );
            }
        }
        // LCOV_EXCL_STOP
        return Err(StoreError::io(tmp.display().to_string(), e));
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        // LCOV_EXCL_START: a temporary this process just made is its own to remove; a removal refused here is the disk's, said and never fatal
        if let Err(rm) = std::fs::remove_file(&tmp) {
            if rm.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(
                    "{}: could not remove a failed temporary: {rm}",
                    tmp.display()
                );
            }
        }
        // LCOV_EXCL_STOP
        return Err(StoreError::io(path.display().to_string(), e));
    }
    sync_dir(parent);
    Ok(())
}

/// Append one line to a log, creating it if absent. `O_APPEND`, one write,
/// one fsync: an append-only log is truth — the journal, the ledger, the
/// signal queue — and a line the caller was told landed has to survive the
/// power going out a moment later, the same promise [`write_atomic`] keeps
/// for a snapshot.
///
/// A tail a crash tore — a line without its newline — is mended first: the
/// write starts a new line when the file does not end in one, so the torn
/// line stays one unparsable line a reader skips, and the next fact is never
/// glued to it and lost with it.
pub fn append_line(path: &Path, line: &str) -> Result<(), StoreError> {
    append_line_with(path, line, true)
}

/// [`append_line`] without the `sync_data`: for the activity log, whose
/// rows are the Pulse's history and not a fact a caller was told was kept —
/// an fsync per engine fact would cost every stop and every settle a disk
/// round-trip, and the engine's stop reads its rows against the clock. The
/// torn tail is mended all the same.
pub(crate) fn append_line_unsynced(path: &Path, line: &str) -> Result<(), StoreError> {
    append_line_with(path, line, false)
}

fn append_line_with(path: &Path, line: &str, sync: bool) -> Result<(), StoreError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| StoreError::io(parent.display().to_string(), e))?;
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .read(true)
        .open(path)
        .map_err(|e| StoreError::io(path.display().to_string(), e))?;
    let torn =
        tail_lacks_newline(&mut f).map_err(|e| StoreError::io(path.display().to_string(), e))?;
    let mut bytes = Vec::with_capacity(line.len() + 2);
    if torn {
        bytes.push(b'\n');
    }
    bytes.extend_from_slice(line.as_bytes());
    bytes.push(b'\n');
    // `O_APPEND`: the write lands at the end whatever the read position.
    std::io::Write::write_all(&mut f, &bytes)
        .map_err(|e| StoreError::io(path.display().to_string(), e))?;
    if sync {
        f.sync_data()
            .map_err(|e| StoreError::io(path.display().to_string(), e))?;
    }
    Ok(())
}

/// Whether a non-empty file's last byte is not a newline.
pub(crate) fn tail_lacks_newline(f: &mut std::fs::File) -> std::io::Result<bool> {
    use std::io::{Read, Seek, SeekFrom};
    if f.metadata()?.len() == 0 {
        return Ok(false);
    }
    f.seek(SeekFrom::End(-1))?;
    let mut last = [0u8; 1];
    f.read_exact(&mut last)?;
    Ok(last[0] != b'\n')
}

/// Make a rename durable: the directory entry lives in the parent, and the
/// parent has to reach the disk too. Best effort — a filesystem that refuses
/// to fsync a directory is not a reason to fail a write that already landed.
pub(crate) fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    {
        if let Ok(d) = std::fs::File::open(dir) {
            if let Err(e) = d.sync_all() {
                // LCOV_EXCL_START: a directory fsync is best effort: a filesystem that refuses it (some network mounts) is said at debug, never a failed write
                tracing::debug!("{}: directory fsync not honoured: {e}", dir.display());
            }
        }
        // LCOV_EXCL_STOP
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
    }
}

/// Resolve `relative` under a `base` that may not exist yet: the lexical
/// containment check only, for a destination that is about to be created.
pub fn resolve_within_new(base: &Path, relative: &str) -> Result<PathBuf, StoreError> {
    let requested = Path::new(relative);
    for component in requested.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            _ => {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-path-leaves",
                    relative = format!("{relative:?}"),
                    a0 = (base.display()).to_string()
                )))
            }
        }
    }
    Ok(base.join(requested))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(dir: &Path) -> Paths {
        Paths::new(dir)
    }

    fn goal_id() -> GoalId {
        GoalId::from_ulid(ulid::Ulid::from_datetime(std::time::SystemTime::now()))
    }

    #[test]
    fn a_goal_owns_its_scratch_directory() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        let goal = p.goal(goal_id());
        assert_eq!(goal.scratch(), goal.dir().join("scratch"));
        assert_eq!(goal.tmp(), goal.scratch().join(".tmp"));
        assert!(goal.scratch().starts_with(p.goals_dir()));
        assert_ne!(goal.scratch(), p.root());
    }

    /// A run of the workspace is its own home: a folder of a goal's shape
    /// under `workflows/runs/`, beside the library and never inside a goal —
    /// and a goal's run is filed with its goal.
    #[test]
    fn a_workspace_run_is_a_home_of_its_own_under_workflows() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        let run = RunId::from_ulid(ulid::Ulid::from_parts(5, 1));
        let home = p.home(&Home::Run { run });
        assert_eq!(home.home(), Home::Run { run });
        assert_eq!(
            home.dir(),
            p.workflows_dir().join("runs").join(run.to_string())
        );
        assert_eq!(home.dir(), p.workspace_runs_dir().join(run.to_string()));
        assert_eq!(Paths::ns_run(run), format!("workflows/runs/{run}"));
        assert_eq!(
            p.state_dir(&Paths::ns_home(&Home::Run { run })),
            home.state()
        );
        assert_eq!(home.journal(), home.dir().join("journal.jsonl"));
        assert_eq!(home.ledger(), home.dir().join("ledger.jsonl"));
        assert_eq!(home.tmp(), home.scratch().join(".tmp"));
        assert_eq!(
            home.run_snapshot(run),
            home.state().join(format!("33413-{run}.json"))
        );
        assert!(
            !home.dir().starts_with(p.goals_dir()),
            "no goal in its path"
        );
        assert_ne!(
            home.state(),
            p.state_dir(Paths::NS_WORKFLOWS),
            "never inside the library's snapshots"
        );
        let goal = goal_id();
        assert_eq!(
            p.home(&Home::Goal { goal }).dir(),
            p.goal(goal).dir(),
            "a goal's run is filed with its goal"
        );
    }

    #[test]
    fn a_design_is_filed_under_its_goal_and_a_library_workflow_under_workflows() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        let wf = WorkflowId::from_ulid(ulid::Ulid::from_parts(9, 9));
        let goal = p.goal(goal_id());
        assert_eq!(
            goal.workflow_snapshot(wf),
            goal.dir().join("state").join(format!("33412-{wf}.json"))
        );
        assert_eq!(
            p.library_workflow_snapshot(wf),
            p.root()
                .join("workflows")
                .join("state")
                .join(format!("33412-{wf}.json"))
        );
    }

    #[test]
    fn a_project_is_a_sibling_of_goals_and_names_none() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        let slug = Slug::new("web-app").unwrap();
        let proj = p.project(&slug);
        assert_eq!(proj.dir(), p.projects_dir().join("web-app"));
        assert!(!proj.record().to_string_lossy().contains("goals"));
        let ws = WorkstreamId::from_ulid(ulid::Ulid::from_parts(4, 1));
        assert_eq!(
            proj.workstream_record(ws).parent(),
            proj.workstream_dir(ws).parent()
        );
        assert!(proj.tree().starts_with(proj.dir()));
        assert!(proj.review().starts_with(proj.dir()));
    }

    #[test]
    fn a_listeners_memory_and_scratch_are_this_machines_and_named_by_its_key() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        let wf = WorkflowId::from_ulid(ulid::Ulid::from_parts(9, 1));
        let key = ListenerKey {
            host: bisa_core::ListenerHost::Workspace { workflow: wf },
            step: bisa_core::StepId::new("nightly").unwrap(),
        };
        let runtime = p.listener_runtime_file(&key);
        assert!(runtime.starts_with(p.events_dir()));
        assert_eq!(
            runtime.file_name().unwrap().to_string_lossy(),
            format!("workspace-{wf}-nightly.json")
        );
        assert!(p.listener_scratch_dir(&key).starts_with(p.events_dir()));
        assert_eq!(p.signal_queue(), p.events_dir().join("queue.jsonl"));
        assert_eq!(
            p.listening_file(wf),
            p.workflows_dir()
                .join("listening")
                .join(format!("{wf}.json"))
        );
        assert!(
            !p.listening_file(wf)
                .starts_with(p.state_dir(Paths::NS_WORKFLOWS)),
            "turning a workflow On is no snapshot of it"
        );
    }

    #[test]
    fn plain_string_ids_are_checked_before_they_become_a_path() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        for bad in ["", "../x", "a/b", ".hidden", "a\\b", "x\0y", "a..b"] {
            assert!(p.conversation_log(bad).is_err(), "{bad:?}");
            assert!(p.session_meta_file("claude-code", bad).is_err(), "{bad:?}");
        }
        assert!(p.conversation_log("01J8ABCDEF").is_ok());
        assert!(p.pet_dir("../escape").is_err());
        assert!(p.pet_dir("bisa-pets.moon").is_ok());
    }

    #[test]
    fn dot_dot_and_absolute_paths_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        for bad in ["../secret", "a/../../secret", "/etc/passwd", ".."] {
            let err = resolve_within(dir.path(), bad).unwrap_err();
            assert!(
                err.to_string().contains("leaves"),
                "{bad:?} was allowed: {err}"
            );
        }
    }

    #[test]
    #[cfg(unix)]
    fn a_symlink_out_of_the_tree_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret"), b"x").unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
        let err = resolve_within(dir.path(), "link/secret").unwrap_err();
        assert!(err.to_string().contains("leaves"), "{err}");
    }

    #[test]
    #[cfg(unix)]
    fn every_refusal_names_the_boundary_the_same_way() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
        let canonical = dir.path().canonicalize().unwrap();
        for bad in ["../secret", "/etc/passwd", "link/secret"] {
            let msg = resolve_within(dir.path(), bad).unwrap_err().to_string();
            assert!(
                msg.ends_with(&format!("leaves {}", canonical.display())),
                "{bad:?} named the boundary as {msg:?}"
            );
        }
    }

    #[test]
    fn a_path_that_does_not_exist_yet_still_resolves() {
        let dir = tempfile::tempdir().unwrap();
        let resolved = resolve_within(dir.path(), "not/here/yet.txt").unwrap();
        assert!(resolved.starts_with(dir.path().canonicalize().unwrap()));
        assert!(resolved.ends_with("not/here/yet.txt"));
        // Byte for byte, not component for component: a trailing separator
        // would make every creating write miss (`README.md/`).
        assert_eq!(
            resolved.as_os_str(),
            dir.path()
                .canonicalize()
                .unwrap()
                .join("not/here/yet.txt")
                .as_os_str()
        );
        let one = resolve_within(dir.path(), "new.txt").unwrap();
        assert!(!one.to_string_lossy().ends_with('/'), "{}", one.display());
    }

    #[test]
    fn an_ordinary_path_resolves_to_itself() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("a/b")).unwrap();
        std::fs::write(dir.path().join("a/b/c.txt"), b"hello").unwrap();
        let resolved = resolve_within(dir.path(), "a/b/c.txt").unwrap();
        assert_eq!(std::fs::read(resolved).unwrap(), b"hello");
        assert!(resolve_within(dir.path(), "./a/./b").is_ok());
        assert_eq!(
            resolve_within(dir.path(), "").unwrap(),
            dir.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn write_atomic_creates_parents_and_leaves_no_temporary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deep/nested/file.json");
        write_atomic(&path, b"{}").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"{}");
        write_atomic(&path, b"[]").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"[]");
        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp."))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temporaries left behind: {leftovers:?}"
        );
    }

    /// Two writers on one path, at once, must each land a whole file.
    #[test]
    fn concurrent_writers_never_share_a_temporary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hot.json");
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let path = path.clone();
                std::thread::spawn(move || {
                    let body = vec![b'a' + i as u8; 4096];
                    for _ in 0..20 {
                        write_atomic(&path, &body).unwrap();
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        let got = std::fs::read(&path).unwrap();
        assert_eq!(got.len(), 4096);
        assert!(
            got.iter().all(|b| *b == got[0]),
            "a byte-mix of two writers"
        );
    }

    #[test]
    fn append_line_appends() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log/x.jsonl");
        append_line(&path, "one").unwrap();
        append_line(&path, "two").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\ntwo\n");
    }

    #[test]
    fn a_snapshot_namespace_has_one_spelling() {
        assert_eq!(Paths::NS_AGENTS, "agents");
        assert_eq!(Paths::NS_CHANNELS, "channels");
        assert_eq!(Paths::NS_WORKFLOWS, "workflows");
        {
            let dir = tempfile::tempdir().unwrap();
            let p = paths(dir.path());
            assert_eq!(p.workflows_dir(), p.root().join("workflows"));
            let goal = p.goal(goal_id());
            let run = RunId::from_ulid(ulid::Ulid::from_parts(4, 1));
            assert_eq!(
                goal.run_snapshot(run).parent(),
                Some(goal.state().as_path())
            );
            assert!(goal.run_snapshot(run).to_string_lossy().contains("33413-"));
        }
        let a = AgentId::new("scribe").unwrap();
        assert_eq!(Paths::ns_recall(&a), "agents/scribe/recall");
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        let id = goal_id();
        assert_eq!(p.state_dir(&Paths::ns_goal(id)), p.goal(id).state());
        assert_eq!(
            p.state_dir(Paths::NS_PROJECTS),
            p.projects_dir().join("state")
        );
        assert!(Paths::RESERVED_PROJECT_SLUGS.contains(&"state"));
    }

    // added by the coverage pass: s1-paths.rs
    #[test]
    fn a_file_from_outside_the_root_is_quarantined_by_its_name_and_a_twin_in_one_second_keeps_both()
    {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(&dir.path().join("ws"));
        let outside = tempfile::tempdir().unwrap();
        let stray = outside.path().join("stray.json");
        std::fs::write(&stray, b"1").unwrap();
        let to = p.quarantine(&stray, 7).unwrap();
        assert_eq!(to, p.quarantine_dir().join("7").join("stray.json"));
        assert!(!stray.exists() && to.is_file());
        std::fs::write(&stray, b"2").unwrap();
        let twin = p.quarantine(&stray, 7).unwrap();
        assert_eq!(twin, p.quarantine_dir().join("7").join("stray.json.1"));
        assert_eq!(std::fs::read(&twin).unwrap(), b"2");
        assert_eq!(std::fs::read(&to).unwrap(), b"1");
    }

    #[test]
    fn the_smaller_accessors_say_their_folders() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        assert_eq!(p.terminals_dir(), p.run_dir().join("terminals"));
        assert_eq!(p.attachment("not a digest"), None);
        assert_eq!(p.attachment_named("not a digest", "a.txt"), None);
        assert!(
            p.project_dirs().unwrap().is_empty(),
            "no projects/ is no project"
        );
        assert_eq!(
            p.resolve("notes/a.md").unwrap(),
            dir.path().canonicalize().unwrap().join("notes/a.md")
        );
        let project = p.project(&Slug::new("web").unwrap());
        let note = NoteId::from_ulid(ulid::Ulid::from_parts(1, 1));
        assert_eq!(
            project.review_note(note),
            project.review().join(format!("{note}.json"))
        );
        let goal = goal_id();
        assert_eq!(p.goal(goal).id(), goal);
    }

    #[test]
    fn a_long_stem_and_a_long_file_name_are_bounded_and_a_path_without_a_file_is_refused() {
        assert!(Paths::stem("thing", &"a".repeat(201)).is_err());
        assert!(Paths::stem("thing", &"a".repeat(200)).is_ok());
        assert_eq!(sanitise_file_name(&"b".repeat(300)).unwrap().len(), 255);
        assert!(matches!(
            write_atomic(Path::new("/"), b"x"),
            Err(StoreError::Invalid(_))
        ));
        let dir = tempfile::tempdir().unwrap();
        let no_name = dir.path().join("x").join("..");
        assert!(matches!(
            write_atomic(&no_name, b"x"),
            Err(StoreError::Invalid(_))
        ));
        assert!(append_line(Path::new("/"), "x").is_err());
        assert!(matches!(
            resolve_within_new(dir.path(), "/abs"),
            Err(StoreError::Invalid(_))
        ));
        assert!(resolve_within_new(dir.path(), "a/b")
            .unwrap()
            .ends_with("a/b"));
    }

    // added by the coverage pass: paths.rs

    #[test]
    fn an_atomic_write_onto_a_folder_is_refused_by_the_folders_path_and_leaves_no_temporary() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("taken");
        std::fs::create_dir_all(folder.join("inside")).unwrap();
        let err = write_atomic(&folder, b"x").unwrap_err();
        assert!(
            matches!(&err, StoreError::Io { path, .. } if path.ends_with("taken")),
            "{err:?}"
        );
        let strays = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name() != "taken")
            .count();
        assert_eq!(strays, 0, "the temporary is gone");
    }
}
