//! The SQLite index: a rebuildable cache over the filesystem truth.
//!
//! Nothing in here is authoritative. `Workspace::rebuild_index` reconstructs
//! every table from the truth files; deleting an object must remove its file
//! AND its rows, or the next rebuild resurrects it.
//!
//! The schema declares real foreign keys, checks and a trigger. In a cache
//! these cannot protect user data — the truth files can be re-read. **They
//! exist to catch our own bugs**, at the one moment a bug is cheapest to find:
//! a rebuild that inserts a child before its parent is a wrong rebuild, and it
//! now fails instead of leaving a dangling row. A constraint here is never the
//! definition of a domain rule; the rule lives in `bisa-core`.
//!
//! Rows are strings. `state` is `TEXT`, not an enum bound to Rust — the domain
//! lives in the snapshot, the index only finds things.

use crate::error::StoreError;
use bisa_core::tags::{TagEntity, TagMatch};
use bisa_core::{ActivityConcept, ActivityFact, ContextRef};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

/// Stamp for the shape below. Bump it whenever [`SCHEMA`] changes; the next
/// open then throws the cache away and rebuilds from truth. A
/// cache-invalidation marker, **not** a migration ladder.
pub const SCHEMA_VERSION: i32 = 26;

/// The row of a scope's last words — its newest live post: a retracted post
/// and a membership event are nobody's. The one spelling of the rule, read
/// for every room at once and for one room alone; `m` is the outer row.
const LATEST_OF_SCOPE: &str = "SELECT rowid FROM messages
                              WHERE scope_id = m.scope_id AND retracted = 0 AND body_kind = 'post'
                              ORDER BY created_at DESC, rowid DESC LIMIT 1";

/// The one current shape of the index — `docs/architecture/08-persistence.md`
/// verbatim, with the two reconciliations that document records (`messages`
/// carries the cached `context` chips; `sessions.status` is free text because
/// a settled session's status carries its reason).
pub const SCHEMA: &str = r#"
-- Workflows: definitions, then the goals that run them
CREATE TABLE workflows (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    origin       TEXT NOT NULL CHECK (origin IN ('workspace','catalog','goal')),
    catalog_slug TEXT,
    goal_id      TEXT,
    author       TEXT NOT NULL,
    revision     INTEGER NOT NULL DEFAULT 0,
    step_count   INTEGER NOT NULL CHECK (step_count >= 0),
    archived_at  INTEGER,
    listening_since INTEGER,
    created_at   INTEGER NOT NULL,
    CHECK ((origin = 'catalog') = (catalog_slug IS NOT NULL)),
    CHECK ((origin = 'goal') = (goal_id IS NOT NULL))
);
CREATE UNIQUE INDEX idx_workflows_catalog ON workflows(catalog_slug) WHERE catalog_slug IS NOT NULL;
CREATE INDEX idx_workflows_goal ON workflows(goal_id);

-- Goals
CREATE TABLE goals (
    id          TEXT PRIMARY KEY,
    status      TEXT NOT NULL CHECK (status IN
                  ('draft','running','waiting','done','failed','closed')),
    closure     TEXT     CHECK (closure IS NULL OR closure IN ('abandoned','superseded')),
    superseded_by TEXT   REFERENCES goals(id) ON DELETE SET NULL,
    origin      TEXT NOT NULL CHECK (origin IN ('captured','spawned','run')),
    workflow_id TEXT     REFERENCES workflows(id) ON DELETE SET NULL,
    run_id      TEXT,
    author      TEXT NOT NULL,
    title       TEXT,
    assignees   TEXT NOT NULL DEFAULT '[]',
    revision    INTEGER NOT NULL DEFAULT 0,
    archived_at INTEGER,
    listening_since INTEGER,
    created_at  INTEGER NOT NULL,
    CHECK ((status = 'closed') = (closure IS NOT NULL)),
    CHECK (superseded_by IS NULL OR closure = 'superseded'),
    CHECK (archived_at IS NULL OR status = 'closed')
);
CREATE INDEX idx_goals_status   ON goals(status, created_at);
CREATE INDEX idx_goals_workflow ON goals(workflow_id);

CREATE TABLE workflow_runs (
    id          TEXT PRIMARY KEY,
    scope       TEXT NOT NULL CHECK (scope IN ('workspace','goal')),
    goal_id     TEXT     REFERENCES goals(id) ON DELETE CASCADE,
    workflow_id TEXT NOT NULL,
    status      TEXT NOT NULL CHECK (status IN ('queued','running','waiting','done','failed','cancelled')),
    revision    INTEGER NOT NULL DEFAULT 0,
    queued_at   INTEGER NOT NULL,
    started_at  INTEGER,
    finished_at INTEGER,
    listener    TEXT,
    dispatched  TEXT,
    CHECK ((scope = 'goal') = (goal_id IS NOT NULL)),
    CHECK (scope = 'goal' OR status != 'queued'),
    CHECK (dispatched IS NULL OR listener IS NOT NULL)
);
CREATE INDEX idx_runs_goal     ON workflow_runs(goal_id, queued_at);
CREATE INDEX idx_runs_workflow ON workflow_runs(workflow_id, scope, queued_at);
CREATE INDEX idx_runs_listener ON workflow_runs(listener, status);
CREATE UNIQUE INDEX idx_runs_dispatched ON workflow_runs(dispatched) WHERE dispatched IS NOT NULL;

CREATE TABLE goal_edges (
    from_id TEXT NOT NULL REFERENCES goals(id) ON DELETE CASCADE,
    to_id   TEXT NOT NULL REFERENCES goals(id) ON DELETE CASCADE,
    kind    TEXT NOT NULL CHECK (kind IN ('refines')),
    PRIMARY KEY (from_id, to_id, kind)
);

-- Projects, and the relation
CREATE TABLE projects (
    id          TEXT PRIMARY KEY,
    slug        TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    root_kind   TEXT NOT NULL CHECK (root_kind IN ('managed','external')),
    root_path   TEXT,
    vcs         TEXT NOT NULL CHECK (vcs IN ('none','git')),
    publish     TEXT NOT NULL CHECK (publish IN ('manual','gated','auto')),
    origin          TEXT NOT NULL CHECK (origin IN ('workspace','goal','step')),
    origin_goal     TEXT,
    origin_run      TEXT,
    origin_step     TEXT,
    origin_workflow TEXT,
    archived_at INTEGER,
    created_at  INTEGER NOT NULL,
    CHECK ((root_kind = 'external') = (root_path IS NOT NULL)),
    CHECK (origin != 'workspace' OR (origin_goal IS NULL AND origin_run IS NULL)),
    CHECK (origin != 'goal' OR origin_goal IS NOT NULL),
    CHECK ((origin_run IS NULL) = (origin_step IS NULL) AND (origin_run IS NULL) = (origin_workflow IS NULL)),
    CHECK (origin != 'step' OR origin_run IS NOT NULL)
);
CREATE INDEX idx_projects_origin_goal     ON projects(origin_goal);
CREATE INDEX idx_projects_origin_workflow ON projects(origin_workflow);

CREATE TABLE work_items (
    id          TEXT PRIMARY KEY,
    goal_id     TEXT     REFERENCES goals(id) ON DELETE CASCADE,
    project_id  TEXT     REFERENCES projects(id) ON DELETE SET NULL,
    run_id      TEXT     REFERENCES workflow_runs(id) ON DELETE CASCADE,
    step_id     TEXT,
    state       TEXT NOT NULL CHECK (state IN
                  ('open','claimed','in_progress','blocked','review','accepted','rejected','cancelled')),
    harness     TEXT,
    assignees   TEXT NOT NULL DEFAULT '[]',
    updated_at  INTEGER NOT NULL,
    CHECK (goal_id IS NOT NULL OR run_id IS NOT NULL)
);
CREATE INDEX idx_work_items_goal    ON work_items(goal_id);
CREATE INDEX idx_work_items_project ON work_items(project_id);
CREATE INDEX idx_work_items_run     ON work_items(run_id);

CREATE TABLE run_steps (
    run_id      TEXT NOT NULL REFERENCES workflow_runs(id) ON DELETE CASCADE,
    step_id     TEXT NOT NULL,
    kind        TEXT NOT NULL CHECK (kind IN
                  ('start','wait','emit','end','decide','if','switch','judge','parallel',
                   'for_each','while','agent','human','approval','check','connector','notify',
                   'spawn')),
    state       TEXT NOT NULL CHECK (state IN
                  ('pending','running','waiting','done','skipped','failed','cancelled',
                   'diverted')),
    wait_topic  TEXT,
    due_at      INTEGER,
    work_item   TEXT REFERENCES work_items(id) ON DELETE SET NULL,
    updated_at  INTEGER NOT NULL,
    PRIMARY KEY (run_id, step_id)
);
CREATE INDEX idx_run_steps_wait ON run_steps(state, wait_topic);
CREATE INDEX idx_run_steps_due  ON run_steps(state, due_at);

CREATE TABLE goal_projects (
    goal_id     TEXT NOT NULL REFERENCES goals(id)    ON DELETE CASCADE,
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    attached_at INTEGER NOT NULL,
    attached_by TEXT    NOT NULL,
    PRIMARY KEY (goal_id, project_id)
);
CREATE INDEX idx_goal_projects_project ON goal_projects(project_id);

-- Agents, teams, channels
CREATE TABLE agents (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    harness    TEXT NOT NULL,
    enabled    INTEGER NOT NULL CHECK (enabled IN (0,1)),
    pubkey     TEXT NOT NULL UNIQUE,
    origin     TEXT NOT NULL CHECK (origin IN ('local','catalog','core')),
    created_at INTEGER NOT NULL
);

CREATE TABLE teams (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    enabled    INTEGER NOT NULL CHECK (enabled IN (0,1)),
    origin     TEXT NOT NULL CHECK (origin IN ('local','catalog')),
    created_at INTEGER NOT NULL
);

CREATE TABLE channels (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    kind          TEXT NOT NULL CHECK (kind IN ('standing','direct')),
    topic         TEXT,
    audience_json TEXT NOT NULL,
    roster_policy TEXT NOT NULL CHECK (roster_policy IN ('everyone','listed')),
    origin        TEXT NOT NULL CHECK (origin IN ('local','catalog','core')),
    created_at    INTEGER NOT NULL,
    CHECK (roster_policy = 'listed' OR id = 'general')
);

CREATE TABLE channel_roster (
    channel_id  TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    member_kind TEXT NOT NULL CHECK (member_kind IN ('agent','team')),
    member_id   TEXT NOT NULL,
    PRIMARY KEY (channel_id, member_kind, member_id)
);


CREATE TABLE workstreams (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects(id)   ON DELETE CASCADE,
    kind        TEXT NOT NULL CHECK (kind IN ('primary','worktree','copy')),
    name        TEXT,
    goal_id     TEXT     REFERENCES goals(id)           ON DELETE SET NULL,
    work_item   TEXT     REFERENCES work_items(id)      ON DELETE SET NULL,
    branch      TEXT,
    agent_id    TEXT     REFERENCES agents(id)          ON DELETE SET NULL,
    state       TEXT NOT NULL CHECK (state IN
                  ('open','dirty','committed','pushed','pr_open','merged','closed')),
    created_at  INTEGER NOT NULL
);
CREATE INDEX idx_workstreams_project ON workstreams(project_id);
CREATE UNIQUE INDEX idx_workstreams_primary ON workstreams(project_id) WHERE kind = 'primary';
CREATE INDEX idx_workstreams_goal    ON workstreams(goal_id);
CREATE INDEX idx_workstreams_work_item ON workstreams(work_item);

-- Conversations: the records, then every message stream's rows
CREATE TABLE conversations (
    id              TEXT PRIMARY KEY,
    origin_kind     TEXT NOT NULL CHECK (origin_kind IN
                      ('node','workspace','goal','workflow','project','workstream','drawing','note')),
    origin_id       TEXT,
    project         TEXT     REFERENCES projects(id) ON DELETE CASCADE,
    title           TEXT,
    first_line      TEXT,
    author          TEXT NOT NULL,
    created_at      INTEGER NOT NULL,
    last_message_at INTEGER,
    message_count   INTEGER NOT NULL DEFAULT 0 CHECK (message_count >= 0),
    archived        INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0,1)),
    mode            TEXT NOT NULL CHECK (mode IN ('manual','auto','plan')),
    CHECK ((origin_kind IN ('node','workspace')) = (origin_id IS NULL))
);
CREATE INDEX idx_conversations_origin ON conversations(origin_kind, origin_id, last_message_at);
CREATE INDEX idx_conversations_project ON conversations(project, archived, last_message_at);

CREATE TABLE conversation_agents (
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    agent_id        TEXT NOT NULL,
    PRIMARY KEY (conversation_id, agent_id)
);

CREATE TABLE messages (
    id         TEXT PRIMARY KEY,
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('channel','goal','conversation')),
    scope_id   TEXT NOT NULL,
    author     TEXT NOT NULL,
    body_kind  TEXT NOT NULL CHECK (body_kind IN ('post','membership')),
    content    TEXT NOT NULL,
    thinking   TEXT,
    said       TEXT,
    context    TEXT NOT NULL DEFAULT '[]',
    reply_to   TEXT REFERENCES messages(id) ON DELETE SET NULL,
    retracted  INTEGER NOT NULL DEFAULT 0 CHECK (retracted IN (0,1)),
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_messages_scope ON messages(scope_id, created_at, id);

CREATE TABLE message_mentions (
    message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    pubkey     TEXT NOT NULL,
    scope_id   TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (message_id, pubkey)
);
CREATE INDEX idx_mentions_pubkey ON message_mentions(pubkey, created_at);

CREATE TABLE attachments (
    message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    ordinal    INTEGER NOT NULL,
    sha256     TEXT NOT NULL,
    name       TEXT NOT NULL,
    mime       TEXT NOT NULL,
    size       INTEGER NOT NULL CHECK (size >= 0),
    PRIMARY KEY (message_id, ordinal)
);
CREATE INDEX idx_attachments_sha ON attachments(sha256);

CREATE TABLE artifacts (
    message_id   TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    ordinal      INTEGER NOT NULL,
    sha256       TEXT NOT NULL,
    name         TEXT NOT NULL,
    mime         TEXT NOT NULL,
    size         INTEGER NOT NULL CHECK (size >= 0),
    kind         TEXT NOT NULL,
    title        TEXT NOT NULL,
    source_scope TEXT,
    source_id    TEXT,
    source_path  TEXT,
    PRIMARY KEY (message_id, ordinal)
);
CREATE INDEX idx_artifacts_sha ON artifacts(sha256);

CREATE TABLE reactions (
    id         TEXT PRIMARY KEY,
    target_id  TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    scope_id   TEXT NOT NULL,
    author     TEXT NOT NULL,
    emoji      TEXT NOT NULL,
    retracted  INTEGER NOT NULL DEFAULT 0 CHECK (retracted IN (0,1)),
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_reactions_target ON reactions(target_id);

CREATE TABLE read_markers (
    scope_id      TEXT PRIMARY KEY,
    last_read_at  INTEGER NOT NULL,
    forced_unread INTEGER NOT NULL DEFAULT 0 CHECK (forced_unread IN (0,1))
);

-- Library, work, governance
CREATE TABLE skills (
    id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT NOT NULL,
    origin TEXT NOT NULL CHECK (origin IN ('local','catalog')),
    created_at INTEGER NOT NULL
);

CREATE TABLE connectors (
    id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT NOT NULL,
    auth TEXT NOT NULL,
    origin TEXT NOT NULL CHECK (origin IN ('local','catalog')),
    created_at INTEGER NOT NULL
);

CREATE TABLE mcp_servers (
    id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT NOT NULL,
    transport TEXT NOT NULL CHECK (transport IN ('stdio','http','sse')),
    enabled INTEGER NOT NULL CHECK (enabled IN (0,1)),
    created_at INTEGER NOT NULL
);

CREATE TABLE sessions (
    id                TEXT PRIMARY KEY,
    adapter           TEXT NOT NULL,
    kind              TEXT NOT NULL CHECK (kind IN ('worker','guided','conversation','terminal')),
    work_item         TEXT REFERENCES work_items(id) ON DELETE CASCADE,
    workstream        TEXT REFERENCES workstreams(id) ON DELETE SET NULL,
    conversation      TEXT REFERENCES conversations(id) ON DELETE SET NULL,
    agent_id          TEXT REFERENCES agents(id)     ON DELETE SET NULL,
    transcript_path   TEXT,
    resume_token_json TEXT,
    status            TEXT NOT NULL CHECK (status IN ('live','parked','ended')),
    parked_at         INTEGER,
    -- The harness child this process spawned for the session, and when it
    -- was seen: what a restart terminates — if it is still that process —
    -- before it resumes the work.
    pid               INTEGER,
    pid_seen_at       INTEGER,
    ended_at          INTEGER
);
CREATE INDEX idx_sessions_status ON sessions(status);

CREATE TABLE approvals (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    goal_id    TEXT REFERENCES goals(id)         ON DELETE CASCADE,
    run_id     TEXT REFERENCES workflow_runs(id) ON DELETE CASCADE,
    gate       TEXT NOT NULL CHECK (gate IN ('approval','escalation','publish')),
    subject    TEXT NOT NULL,
    actor      TEXT NOT NULL,
    approve    INTEGER NOT NULL CHECK (approve IN (0,1)),
    at         INTEGER NOT NULL,
    CHECK ((goal_id IS NULL) != (run_id IS NULL))
);
CREATE INDEX idx_approvals_goal ON approvals(goal_id, at);
CREATE INDEX idx_approvals_run  ON approvals(run_id, at);

CREATE TABLE budget_ledger (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    goal_id    TEXT REFERENCES goals(id)         ON DELETE CASCADE,
    run_id     TEXT REFERENCES workflow_runs(id) ON DELETE CASCADE,
    tokens     INTEGER NOT NULL DEFAULT 0 CHECK (tokens    >= 0),
    usd_cents  INTEGER NOT NULL DEFAULT 0 CHECK (usd_cents >= 0),
    wall_secs  INTEGER NOT NULL DEFAULT 0 CHECK (wall_secs >= 0),
    at         INTEGER NOT NULL,
    CHECK ((goal_id IS NULL) != (run_id IS NULL))
);
CREATE INDEX idx_ledger_goal ON budget_ledger(goal_id);
CREATE INDEX idx_ledger_run  ON budget_ledger(run_id);

CREATE TABLE notes (
    id          TEXT PRIMARY KEY,
    scope_kind  TEXT NOT NULL CHECK (scope_kind IN ('workspace','goal','project','workflow','channel','node')),
    scope_id    TEXT,
    home_goal   TEXT REFERENCES goals(id) ON DELETE CASCADE,
    title       TEXT NOT NULL,
    pinned      INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0,1)),
    updated_at  INTEGER NOT NULL,
    CHECK ((scope_kind IN ('workspace','node')) = (scope_id IS NULL))
);
CREATE INDEX idx_notes_scope ON notes(scope_kind, scope_id);

-- Drawings: the locator and the listing's columns; the snapshot is the truth
CREATE TABLE drawings (
    id             TEXT PRIMARY KEY,
    scope_kind     TEXT NOT NULL CHECK (scope_kind IN ('workspace','goal','project','workflow','channel','node')),
    scope_id       TEXT,
    home_goal      TEXT REFERENCES goals(id) ON DELETE CASCADE,
    title          TEXT NOT NULL,
    pinned         INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0,1)),
    hash           TEXT NOT NULL,
    element_count  INTEGER NOT NULL DEFAULT 0 CHECK (element_count >= 0),
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL,
    CHECK ((scope_kind IN ('workspace','node')) = (scope_id IS NULL))
);
CREATE INDEX idx_drawings_scope ON drawings(scope_kind, scope_id);

-- Signals: the durable queue of occurrences, keyed by the listener each is
-- for (none for a named signal kept only for a wait to replay). A listener
-- is not a row, so nothing here refers to one; one occurrence is one row per
-- listener, whatever wrote it twice.
CREATE TABLE signals (
    id           TEXT PRIMARY KEY,
    host_kind    TEXT CHECK (host_kind IS NULL OR host_kind IN ('workspace','goal')),
    host_id      TEXT,
    step         TEXT,
    source       TEXT NOT NULL CHECK (source IN
                   ('schedule','hook','message','signal','project','run','platform','connector',
                    'check','test')),
    name         TEXT,
    scope_kind   TEXT NOT NULL CHECK (scope_kind IN ('workspace','goal')),
    scope_id     TEXT,
    dedupe_key   TEXT,
    state        TEXT NOT NULL CHECK (state IN
                   ('queued','running','waiting','held','done','skipped','failed')),
    at           INTEGER NOT NULL,
    started_at   INTEGER,
    attempts     INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    payload_json TEXT NOT NULL,
    chain_json   TEXT NOT NULL DEFAULT '{}',
    last_error   TEXT,
    CHECK ((host_kind IS NULL) = (host_id IS NULL) AND (host_kind IS NULL) = (step IS NULL)),
    CHECK ((scope_kind = 'goal') = (scope_id IS NOT NULL))
);
CREATE UNIQUE INDEX idx_signals_dedupe ON signals(host_kind, host_id, step, dedupe_key)
    WHERE dedupe_key IS NOT NULL AND host_kind IS NOT NULL;
CREATE INDEX idx_signals_state    ON signals(state, at);
CREATE INDEX idx_signals_listener ON signals(host_kind, host_id, step, state, at);
CREATE INDEX idx_signals_name     ON signals(source, name, at);
CREATE INDEX idx_signals_scope    ON signals(scope_kind, scope_id, at);

-- Workspace-wide
CREATE TABLE members (
    pubkey     TEXT PRIMARY KEY,
    role       TEXT NOT NULL CHECK (role IN ('owner','admin','member','guest')),
    label      TEXT,
    added_at   INTEGER NOT NULL
);

CREATE TABLE seen_events (
    event_id TEXT PRIMARY KEY,
    at       INTEGER NOT NULL
);
CREATE INDEX idx_seen_at ON seen_events(at);

CREATE TABLE tags (
    entity TEXT NOT NULL CHECK (entity IN
             ('agent','team','channel','skill','mcp','project','goal','workflow','connector')),
    id     TEXT NOT NULL,
    tag    TEXT NOT NULL,
    PRIMARY KEY (entity, id, tag)
);
CREATE INDEX idx_tags_tag ON tags(tag, entity);

CREATE VIRTUAL TABLE events_fts USING fts5(doc_id, goal_id, content);

-- The activity feed (the Pulse): one row per fact across every concept,
-- paged by keyset on (at, seq). Derived from the journals, the
-- conversations and the activity log.
CREATE TABLE activity (
    seq         INTEGER PRIMARY KEY,
    at          INTEGER NOT NULL,
    concept     TEXT NOT NULL CHECK (concept IN ('workspace','goals','workflows','projects','channels','agents','node')),
    kind        TEXT NOT NULL,
    source_kind TEXT NOT NULL,
    source_id   TEXT NOT NULL,
    author      TEXT,
    event       TEXT NOT NULL
);
CREATE INDEX idx_activity_at ON activity(at, seq);
CREATE INDEX idx_activity_concept ON activity(concept, at, seq);
CREATE INDEX idx_activity_kind ON activity(kind, at, seq);
"#;

/// Defence against our own bugs: the two permanent objects cannot leave the
/// cache through an ordinary delete. Created after the tables and dropped for
/// the duration of a rebuild's `clear`, which empties everything on purpose.
/// The guarantee that actually holds is `ensure_*` at every open.
pub const GUARD_TRIGGERS: &str = r#"
CREATE TRIGGER channels_general_undeletable
BEFORE DELETE ON channels WHEN OLD.id = 'general'
BEGIN SELECT RAISE(ABORT, 'the general channel cannot be deleted'); END;

CREATE TRIGGER agents_core_undeletable
BEFORE DELETE ON agents WHEN OLD.origin = 'core'
BEGIN SELECT RAISE(ABORT, 'a core agent cannot be deleted'); END;
"#;

const DROP_GUARD_TRIGGERS: &str = r#"
DROP TRIGGER IF EXISTS channels_general_undeletable;
DROP TRIGGER IF EXISTS agents_core_undeletable;
"#;

/// Every table, in dependency order: parents before children. `clear`
/// deletes in the reverse of this list, and `rebuild_index` inserts in it.
pub const TABLES_IN_FK_ORDER: &[&str] = &[
    "agents",
    "teams",
    "skills",
    "mcp_servers",
    "connectors",
    "members",
    "channels",
    "channel_roster",
    "projects",
    "workflows",
    "goals",
    "goal_projects",
    "goal_edges",
    "workflow_runs",
    "work_items",
    "run_steps",
    "approvals",
    "budget_ledger",
    "notes",
    "drawings",
    "workstreams",
    "conversations",
    "conversation_agents",
    "sessions",
    "messages",
    "attachments",
    "artifacts",
    "message_mentions",
    "reactions",
    "signals",
    "activity",
    "tags",
    "seen_events",
    "read_markers",
    "events_fts",
];

/// One `drawings` row: what a listing needs and where the snapshot is filed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrawingRow {
    pub id: String,
    pub scope_kind: String,
    pub scope_id: Option<String>,
    pub home_goal: Option<String>,
    pub title: String,
    pub pinned: bool,
    pub hash: String,
    pub element_count: u64,
    pub created_at: u64,
    pub updated_at: u64,
}

pub struct Index {
    conn: Connection,
    /// Set when this open laid down an empty schema, so the workspace knows it
    /// owes a rebuild from truth.
    fresh: bool,
}

/// Row shapes handed to/from the index. Strings, not domain types.
#[derive(Clone, Debug, PartialEq)]
pub struct GoalRow {
    pub id: String,
    /// `Goal::status` at the last write — a cached projection, like every
    /// other column here.
    pub status: String,
    pub closure: Option<String>,
    pub superseded_by: Option<String>,
    /// `captured` | `spawned` | `run`.
    pub origin: String,
    pub workflow_id: Option<String>,
    pub run_id: Option<String>,
    pub author: String,
    pub title: Option<String>,
    /// `Assignee` wire forms (`agent:<id>` / `human:<hex>` / `team:<id>`).
    pub assignees: Vec<String>,
    pub revision: u64,
    /// Put away — unix seconds; `None` for a goal in the lists.
    pub archived_at: Option<u64>,
    /// Since when the goal listens for its workflow's start events — paused
    /// or not; `None` for a goal whose work begins by hand.
    pub listening_since: Option<u64>,
    pub created_at: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkItemRow {
    pub id: String,
    /// The goal a goal's run's item is filed under; `None` for an item of a
    /// run of the workspace, filed under its run (`run_id`).
    pub goal_id: Option<String>,
    pub project_id: Option<String>,
    pub run_id: Option<String>,
    pub step_id: Option<String>,
    pub state: String,
    pub harness: Option<String>,
    /// Everyone the spec names, in `Assignee`'s wire form: the request, the
    /// runner the engine picked, and the spawn allowlist.
    pub assignees: Vec<String>,
    pub updated_at: u64,
}

/// A workflow definition's locator row.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowRow {
    pub id: String,
    pub name: String,
    /// `workspace` | `catalog` | `goal`.
    pub origin: String,
    pub catalog_slug: Option<String>,
    /// The goal a `goal`-origin design was drawn for.
    pub goal_id: Option<String>,
    pub author: String,
    pub revision: u64,
    pub step_count: u64,
    /// Put away — unix seconds; `None` for a workflow in the library.
    pub archived_at: Option<u64>,
    pub created_at: u64,
}

/// A project's locator row: the record's finding facts, provenance included.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectRow {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub root_kind: String,
    pub root_path: Option<String>,
    pub vcs: String,
    pub publish: String,
    /// `workspace` | `goal` | `step`.
    pub origin: String,
    pub origin_goal: Option<String>,
    pub origin_run: Option<String>,
    pub origin_step: Option<String>,
    pub origin_workflow: Option<String>,
    /// Put away — unix seconds; `None` for a project in the rail.
    pub archived_at: Option<u64>,
    pub created_at: u64,
}

/// A home as the index keys a row by it — a goal's id in `goal_id`, or a run
/// of the workspace's in `run_id`, exactly one of the two (`CHECK`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HomeKey {
    Goal(String),
    Run(String),
}

impl HomeKey {
    /// `(goal_id, run_id)` as the two columns hold them.
    fn columns(&self) -> (Option<&str>, Option<&str>) {
        match self {
            HomeKey::Goal(goal) => (Some(goal), None),
            HomeKey::Run(run) => (None, Some(run)),
        }
    }

    /// The key a row's two columns hold: the goal's where there is one — a
    /// goal's run's row carries both — the run's otherwise. Neither is no
    /// home at all.
    fn from_columns(goal_id: Option<String>, run_id: Option<String>) -> Option<Self> {
        match (goal_id, run_id) {
            (Some(goal), _) => Some(HomeKey::Goal(goal)),
            (None, Some(run)) => Some(HomeKey::Run(run)),
            (None, None) => None,
        }
    }

    /// The home the key names, when its id reads as one.
    pub fn home(&self) -> Option<bisa_core::Home> {
        match self {
            HomeKey::Goal(goal) => goal.parse().ok().map(|goal| bisa_core::Home::Goal { goal }),
            HomeKey::Run(run) => run.parse().ok().map(|run| bisa_core::Home::Run { run }),
        }
    }
}

impl From<&bisa_core::Home> for HomeKey {
    fn from(home: &bisa_core::Home) -> Self {
        match home {
            bisa_core::Home::Goal { goal } => HomeKey::Goal(goal.to_string()),
            bisa_core::Home::Run { run } => HomeKey::Run(run.to_string()),
        }
    }
}

/// A run's locator row. `status` is `queued|running|waiting|done|failed|cancelled`.
#[derive(Clone, Debug, PartialEq)]
pub struct RunRow {
    pub id: String,
    /// `workspace` | `goal` — what the run is for.
    pub scope: String,
    /// The goal a goal's run is for; `None` for a run of the workspace.
    pub goal_id: Option<String>,
    pub workflow_id: String,
    pub status: String,
    pub revision: u64,
    pub queued_at: u64,
    /// `None` while the run is queued.
    pub started_at: Option<u64>,
    pub finished_at: Option<u64>,
    /// The listener whose event began the run (`workspace:<wf>/<step>`,
    /// `goal:<goal>/<step>`); `None` for a run a person began.
    pub listener: Option<String>,
    /// The queued signal the run was dispatched from — unique, so one
    /// occurrence makes one run however often its dispatch is replayed.
    pub dispatched: Option<String>,
}

/// One step of a run, as the runtime polls it: which waits are armed on what
/// (`wait_topic` names the catch — `signal:<name>`, `message`, `project`,
/// `run`, `platform:<topic>`, `release`), which are due when, which work item
/// a step is bound to.
#[derive(Clone, Debug, PartialEq)]
pub struct RunStepRow {
    pub run_id: String,
    pub step_id: String,
    pub kind: String,
    pub state: String,
    pub wait_topic: Option<String>,
    pub due_at: Option<u64>,
    pub work_item: Option<String>,
    pub updated_at: u64,
}

/// One row of the durable signal queue. `state` is `queued` (ready),
/// `running` (claimed), `waiting` (held by its listener's guard behind a run
/// still live), `held` (waiting on a person — an outside payload the screen
/// would not pass), or settled: `done` (it began a run), `skipped` (dropped,
/// with the reason) or `failed`.
#[derive(Clone, Debug, PartialEq)]
pub struct SignalRow {
    pub id: String,
    /// The listener the signal is for, as its three columns; all `None` for
    /// a named signal kept only for a wait to replay.
    pub host_kind: Option<String>,
    pub host_id: Option<String>,
    pub step: Option<String>,
    pub source: String,
    pub name: Option<String>,
    pub scope_kind: String,
    pub scope_id: Option<String>,
    pub dedupe_key: Option<String>,
    pub state: String,
    pub at: u64,
    pub attempts: u32,
    pub payload_json: String,
    pub chain_json: String,
    pub last_error: Option<String>,
}

/// Where a harness session stands, as the index and its meta file hold it.
/// `Live` is a session this process drives; `Parked` one it put aside with a
/// resume token; `Ended` one that is over — the way it ended is the
/// journal's, not this row's. A row a dead process left `Live` is ended at
/// the next boot; nothing else moves a row backwards.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Live,
    Parked,
    #[default]
    Ended,
}

impl SessionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionStatus::Live => "live",
            SessionStatus::Parked => "parked",
            SessionStatus::Ended => "ended",
        }
    }

    /// The column's word back; anything else is `Ended` — a status the index
    /// cannot read is not a session anybody drives.
    pub fn parse(word: &str) -> Self {
        match word {
            "live" => SessionStatus::Live,
            "parked" => SessionStatus::Parked,
            _ => SessionStatus::Ended,
        }
    }
}

/// What a session is a run of: the durable answer to "what kind of session
/// was this?", written when the row is. One vocabulary for the store, the
/// engine's roster and the wire.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Default,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    /// An agent step's work item.
    #[default]
    Worker,
    /// A guided wake: the Workflow Agent designing or repairing a goal's workflow.
    Guided,
    /// One turn of a conversation, reached only through it.
    Conversation,
    /// A harness a person opened in a desktop terminal, reporting through its
    /// own hooks; never driven by the engine, never recorded here.
    Terminal,
    /// One bounded question put to a model and nothing else — the
    /// classifier's reading, the Decision-Making Agent's judgement, a
    /// suggested commit message: on the roster while it runs, never recorded
    /// here (it holds no door to the platform and ends with its deadline).
    Ask,
}

impl SessionKind {
    pub const ALL: [SessionKind; 5] = [
        SessionKind::Worker,
        SessionKind::Guided,
        SessionKind::Conversation,
        SessionKind::Terminal,
        SessionKind::Ask,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            SessionKind::Worker => "worker",
            SessionKind::Guided => "guided",
            SessionKind::Conversation => "conversation",
            SessionKind::Terminal => "terminal",
            SessionKind::Ask => "ask",
        }
    }

    /// The column's word back; a word the index cannot read is a worker —
    /// the kind every surface treats as plain work.
    pub fn parse(word: &str) -> Self {
        SessionKind::ALL
            .into_iter()
            .find(|k| k.as_str() == word)
            .unwrap_or_default()
    }

    /// Whether a session of this kind is a **work session** of the checkout
    /// it stands in — what the IDE's rail draws and a workstream's status
    /// counts. A conversation's turn is its conversation's, never the
    /// checkout's row.
    pub fn is_work(self) -> bool {
        matches!(self, SessionKind::Worker | SessionKind::Terminal)
    }
}

impl std::fmt::Display for SessionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct SessionRow {
    pub id: String,
    pub adapter: String,
    pub kind: SessionKind,
    pub work_item: Option<String>,
    /// The conversation whose turn this session runs, for a `conversation` kind.
    pub conversation: Option<String>,
    /// The workstream the session runs in: the checkout its cwd
    /// is. `None` for a session in an agent's or a goal's scratch folder.
    pub workstream: Option<String>,
    pub agent_id: Option<String>,
    pub transcript_path: Option<String>,
    pub resume_token_json: Option<String>,
    pub status: SessionStatus,
    pub parked_at: Option<u64>,
    /// The harness child's process id and the moment it was seen, when this
    /// process spawned one — so a boot after a crash can end what it left
    /// running, and only that: a recycled pid started at another time is
    /// somebody else's.
    pub pid: Option<u32>,
    pub pid_seen_at: Option<u64>,
    pub ended_at: Option<u64>,
}

/// A conversation as a list reads it: the record's fields with the facts
/// the index keeps beside them — when it last moved, how many messages, the
/// agents who took part.
#[derive(
    Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct ConversationRow {
    pub id: String,
    pub origin_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_id: Option<String>,
    /// The project a project's or a workstream's conversation stands in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The first line of the first post, for a list row with no title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_line: Option<String>,
    pub author: String,
    pub created_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_message_at: Option<u64>,
    pub message_count: u64,
    pub archived: bool,
    /// How far an agent goes on its own: `manual` · `auto` · `plan`
    /// ([`bisa_core::ConversationMode`]).
    pub mode: String,
    /// The agents who spoke or were addressed, in id order.
    pub agents: Vec<String>,
}

/// A message as read back from the index.
/// Where a page of the feed stops: the last row's `(at, seq)`, the next
/// page's exclusive bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ActivityCursor {
    pub at: u64,
    pub seq: i64,
}

/// One row of the feed as the index holds it: the fact's words are the
/// caller's to parse (`event` is the payload's JSON, verbatim).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ActivityRecord {
    pub seq: i64,
    pub at: u64,
    pub concept: String,
    pub kind: String,
    pub source_kind: String,
    pub source_id: String,
    pub author: Option<String>,
    pub event: String,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MessageRow {
    pub id: String,
    pub scope_id: String,
    pub author: String,
    /// `post` or `membership`.
    pub body_kind: String,
    pub content: String,
    /// An agent's reasoning before its words, when its harness said it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    /// The platform's own sentence, when the post is one it authored — the
    /// message of the catalog, for a reader in another language; `content`
    /// is its English.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub said: Option<bisa_core::Text>,
    /// The chips a post carried, cached from the event.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context: Vec<ContextRef>,
    pub created_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    pub retracted: bool,
    /// Filled by the `Workspace` reader, which can answer whether the bytes
    /// are on this disk.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<crate::conversation::MessageAttachment>,
    /// What the message's author made for the reader to look at, with
    /// presence answered the same way.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<crate::conversation::MessageArtifact>,
}

/// One artifact of a conversation, with the message it rode on — the
/// conversation's gallery, newest first.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ArtifactListRow {
    pub message_id: String,
    pub scope_id: String,
    pub author: String,
    pub created_at: u64,
    #[serde(flatten)]
    pub artifact: bisa_core::ArtifactRef,
    pub present: bool,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ReactionRow {
    pub id: String,
    pub target_id: String,
    pub author: String,
    pub emoji: String,
    pub retracted: bool,
}

impl Index {
    /// Open the index, discarding it first if it was built for another shape.
    ///
    /// There is deliberately **no migration path**. A schema that does not
    /// match is deleted and rebuilt from truth by
    /// [`crate::Workspace::rebuild_index`].
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let mut fresh = Self::discard_if_stale(path)?;
        let mut conn = Self::configured(path)?;
        // A file the right build wrote can still be damaged — a torn page, a
        // disk that lied. `quick_check` reads every page once; anything but
        // `ok` sends the file through the same door a stale schema does. The
        // index is a cache: throwing it away costs a rebuild, never a fact.
        if !fresh && !Self::passes_quick_check(&conn) {
            tracing::warn!(
                "{}: the index did not pass quick_check; discarding the cache and rebuilding from truth",
                path.display() // LCOV_EXCL_LINE: a field line of the macro; the macro's own line counts
            );
            drop(conn);
            Self::discard(path)?;
            conn = Self::configured(path)?;
            fresh = true;
        }
        let index = Self { conn, fresh };
        if fresh {
            index.create_schema()?;
        }
        Ok(index)
    }

    /// Open the file and set the pragmas. A file that will not open at all is
    /// discarded and opened again empty — the caller then rebuilds it.
    fn configured(path: &Path) -> Result<Connection, StoreError> {
        let conn = match Connection::open(path) {
            Ok(c) => c,
            // LCOV_EXCL_START: `discard_if_stale` already discarded a file that would not open; this arm is the race with a writer between the two opens
            Err(e) if path.exists() => {
                tracing::warn!(
                    "{}: the index would not open ({e}); discarding the cache and rebuilding from truth",
                    path.display()
                );
                Self::discard(path)?;
                Connection::open(path)?
            }
            // LCOV_EXCL_STOP
            Err(e) => return Err(e.into()),
        };
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        // Tuning for a node that stays open for weeks. With WAL the
        // default `synchronous = FULL` fsyncs on every commit; NORMAL fsyncs
        // only at a checkpoint — the last transaction can roll back on a power
        // loss, but WAL still guarantees no corruption, the right trade for a
        // cache that the truth files rebuild (every truth write is fsynced,
        // and `Workspace::reconcile_live` re-indexes the live runs at open).
        // The checkpoint interval is said out loud rather than inherited: a
        // thousand pages, so the WAL never grows past a few megabytes between
        // the fsyncs NORMAL relies on. The rest are pure reads-go-faster with
        // no durability cost: a memory-mapped read window, an in-memory page
        // cache and temp store, and a wait rather than an instant
        // `SQLITE_BUSY` under concurrent access.
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "wal_autocheckpoint", 1000)?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        conn.pragma_update(None, "mmap_size", 268_435_456i64)?; // 256 MiB
        conn.pragma_update(None, "cache_size", -65_536i64)?; // 64 MiB (negative = KiB)
        conn.pragma_update(None, "temp_store", "MEMORY")?;
        Ok(conn)
    }

    fn passes_quick_check(conn: &Connection) -> bool {
        conn.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
            .map(|v| v == "ok")
            .unwrap_or(false)
    }

    /// Whether this open created an empty index. When true the caller owes it
    /// a rebuild, or the workspace reads as empty while its truth sits there.
    pub fn is_fresh(&self) -> bool {
        self.fresh
    }

    /// Stamp the schema version. Called by the workspace **after** a rebuild
    /// landed, never at schema creation: an index whose rebuild was cut short
    /// by a crash reads as unstamped and is rebuilt again at the next open,
    /// rather than served half-empty at the right version.
    pub fn mark_built(&self) -> Result<(), StoreError> {
        self.conn
            .pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(())
    }

    /// Take the stamp off **before** a rebuild wipes the tables: a rebuild
    /// of an index that was stamped — `bisa workspace reindex` — that is cut
    /// short or fails half-way would otherwise leave an empty index the next
    /// open trusts, and the workspace would read as wiped with every file
    /// intact. Unstamped, it is rebuilt again at the next open.
    pub fn mark_building(&self) -> Result<(), StoreError> {
        self.conn.pragma_update(None, "user_version", 0)?;
        Ok(())
    }

    /// Run `f` inside one transaction — every statement it issues lands or
    /// none does. Joins the transaction already open on this connection when
    /// there is one, so a caller that batches several indexers is one unit
    /// and each indexer stays correct on its own.
    /// A test's door to the cache: one statement, for the rows a rebuild
    /// never writes — an id that is no id, a scope that names nothing — so
    /// the readers' tolerance of a corrupt cache is held by a test.
    #[cfg(test)]
    pub(crate) fn execute_for_test(&self, sql: &str) -> rusqlite::Result<usize> {
        self.conn.execute(sql, [])
    }

    pub fn in_transaction<T>(
        &self,
        f: impl FnOnce() -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        if !self.conn.is_autocommit() {
            return f();
        }
        let tx = self.conn.unchecked_transaction()?;
        let out = f()?;
        tx.commit()?;
        Ok(out)
    }

    fn discard_if_stale(path: &Path) -> Result<bool, StoreError> {
        let stamped = Connection::open(path).ok().and_then(|c| {
            c.query_row("PRAGMA user_version", [], |r| r.get::<_, i32>(0))
                .ok()
        });
        if stamped == Some(SCHEMA_VERSION) {
            return Ok(false);
        }
        if stamped.is_some() {
            tracing::info!(
                "index schema is stale or its rebuild never finished (found {:?}, want {SCHEMA_VERSION}); \
                 discarding the cache and rebuilding from truth",
                stamped
            );
        }
        Self::discard(path)?;
        Ok(true)
    }

    /// Remove the index and its `-wal` / `-shm` siblings, which belong to the
    /// file being dropped.
    fn discard(path: &Path) -> Result<(), StoreError> {
        for suffix in ["", "-wal", "-shm"] {
            let sibling = if suffix.is_empty() {
                path.to_path_buf()
            } else {
                let mut name = path.as_os_str().to_os_string();
                name.push(suffix);
                std::path::PathBuf::from(name)
            };
            match std::fs::remove_file(&sibling) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(StoreError::io(sibling.display().to_string(), e)),
            }
        }
        Ok(())
    }

    fn create_schema(&self) -> Result<(), StoreError> {
        self.conn.execute_batch(SCHEMA)?;
        self.conn.execute_batch(GUARD_TRIGGERS)?;
        Ok(())
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let index = Self { conn, fresh: true };
        index.create_schema()?;
        index.mark_built()?;
        Ok(index)
    }

    /// Whether `PRAGMA foreign_keys` is on — asserted by the rebuild test.
    pub fn foreign_keys_enabled(&self) -> Result<bool, StoreError> {
        Ok(self
            .conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0))?
            == 1)
    }

    /// Wipe every table, children first, before a rebuild scan. The guard
    /// triggers are lifted for the wipe and put back afterwards: a rebuild
    /// empties everything on purpose, and `ensure_*` restores the permanent
    /// objects right after.
    pub fn clear(&self) -> Result<(), StoreError> {
        self.conn.execute_batch(DROP_GUARD_TRIGGERS)?;
        let wiped: Result<(), StoreError> = (|| {
            for table in TABLES_IN_FK_ORDER.iter().rev() {
                self.conn.execute(&format!("DELETE FROM {table}"), [])?;
            }
            Ok(())
        })();
        self.conn.execute_batch(GUARD_TRIGGERS)?;
        wiped
    }

    // --- goals ---

    pub fn upsert_goal(&self, row: &GoalRow) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO goals (id, status, closure, superseded_by, origin, workflow_id, run_id,
                                author, title, assignees, revision, archived_at, listening_since,
                                created_at)
             VALUES (?1, ?2, ?3, (SELECT id FROM goals WHERE id = ?4), ?5,
                     (SELECT id FROM workflows WHERE id = ?6), ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(id) DO UPDATE SET
               status=excluded.status, closure=excluded.closure,
               superseded_by=excluded.superseded_by, origin=excluded.origin,
               workflow_id=excluded.workflow_id, run_id=excluded.run_id,
               author=excluded.author, title=excluded.title, assignees=excluded.assignees,
               revision=excluded.revision, archived_at=excluded.archived_at,
               listening_since=excluded.listening_since, created_at=excluded.created_at",
            params![
                row.id,
                row.status,
                row.closure,
                row.superseded_by,
                row.origin,
                row.workflow_id,
                row.run_id,
                row.author,
                row.title,
                serde_json::to_string(&row.assignees)?,
                row.revision as i64,
                row.archived_at.map(|a| a as i64),
                row.listening_since.map(|a| a as i64),
                row.created_at as i64
            ],
        )?;
        Ok(())
    }

    const GOAL_COLUMNS: &'static str =
        "id, status, closure, superseded_by, origin, workflow_id, run_id,
               author, title, assignees, revision, created_at, archived_at, listening_since";

    /// The goals that listen right now, oldest first — the hosts a registry
    /// rebuild arms besides the library's.
    pub fn listening_goal_ids(&self) -> Result<Vec<String>, StoreError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id FROM goals WHERE listening_since IS NOT NULL ORDER BY created_at, id",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_goal(&self, id: &str) -> Result<Option<GoalRow>, StoreError> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM goals WHERE id = ?1",
            Self::GOAL_COLUMNS
        ))?;
        Ok(stmt.query_row(params![id], row_to_goal).optional()?)
    }

    /// The goals in the lists — never an archived one — with one status, or every status.
    pub fn list_goals(&self, status: Option<&str>) -> Result<Vec<GoalRow>, StoreError> {
        let base = format!(
            "SELECT {} FROM goals WHERE archived_at IS NULL",
            Self::GOAL_COLUMNS
        );
        match status {
            Some(s) => {
                let mut stmt = self
                    .conn
                    .prepare(&format!("{base} AND status = ?1 ORDER BY id"))?;
                let rows = stmt.query_map(params![s], row_to_goal)?;
                Ok(rows.collect::<Result<Vec<_>, _>>()?)
            }
            None => {
                let mut stmt = self.conn.prepare(&format!("{base} ORDER BY id"))?;
                let rows = stmt.query_map([], row_to_goal)?;
                Ok(rows.collect::<Result<Vec<_>, _>>()?)
            }
        }
    }

    /// The goals put away, newest archived first.
    pub fn list_archived_goals(&self) -> Result<Vec<GoalRow>, StoreError> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM goals WHERE archived_at IS NOT NULL ORDER BY archived_at DESC, id",
            Self::GOAL_COLUMNS
        ))?;
        let rows = stmt.query_map([], row_to_goal)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Ids of the goals whose workflow is `workflow_id`.
    pub fn goals_using_workflow(&self, workflow_id: &str) -> Result<Vec<String>, StoreError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT id FROM goals WHERE workflow_id = ?1 ORDER BY id")?;
        let rows = stmt.query_map(params![workflow_id], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    // --- workflows ---

    pub fn upsert_workflow(&self, row: &WorkflowRow) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO workflows (id, name, origin, catalog_slug, goal_id, author, revision, step_count, archived_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
               name=excluded.name, origin=excluded.origin, catalog_slug=excluded.catalog_slug,
               goal_id=excluded.goal_id, author=excluded.author, revision=excluded.revision,
               step_count=excluded.step_count, archived_at=excluded.archived_at, created_at=excluded.created_at",
            params![
                row.id,
                row.name,
                row.origin,
                row.catalog_slug,
                row.goal_id,
                row.author,
                row.revision as i64,
                row.step_count as i64,
                row.archived_at.map(|a| a as i64),
                row.created_at as i64
            ],
        )?;
        Ok(())
    }

    /// The workflows put away, newest archived first.
    pub fn list_archived_workflow_ids(&self) -> Result<Vec<String>, StoreError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id FROM workflows WHERE archived_at IS NOT NULL ORDER BY archived_at DESC, id",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Ids of the designs drawn for one goal — what a goal deletion cascades
    /// over, and what its Workflow tab lists.
    pub fn workflows_of_goal(&self, goal_id: &str) -> Result<Vec<String>, StoreError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id FROM workflows WHERE goal_id = ?1 ORDER BY created_at, id",
        )?;
        let rows = stmt.query_map(params![goal_id], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_workflow(&self, id: &str) -> Result<Option<WorkflowRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, origin, catalog_slug, goal_id, author, revision, step_count, created_at, archived_at
             FROM workflows WHERE id = ?1",
        )?;
        Ok(stmt.query_row(params![id], row_to_workflow).optional()?)
    }

    /// Every workflow in the lists — never an archived one — oldest first.
    pub fn list_workflow_ids(&self) -> Result<Vec<String>, StoreError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id FROM workflows WHERE archived_at IS NULL ORDER BY created_at, id",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// The installed workflow that came from this catalog slug, if any. The
    /// unique partial index makes "at most one" the schema's promise.
    pub fn workflow_id_for_slug(&self, slug: &str) -> Result<Option<String>, StoreError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT id FROM workflows WHERE catalog_slug = ?1")?;
        Ok(stmt
            .query_row(params![slug], |r| r.get::<_, String>(0))
            .optional()?)
    }

    pub fn delete_workflow(&self, id: &str) -> Result<(), StoreError> {
        self.conn
            .execute("DELETE FROM workflows WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Mark a library workflow On (`since`) or Off. Its own column, apart
    /// from the definition's: saving a revision never turns it off, and the
    /// rebuild restores it from the listening records after the workflows.
    pub fn set_workflow_listening(&self, id: &str, since: Option<u64>) -> Result<(), StoreError> {
        self.conn.execute(
            "UPDATE workflows SET listening_since = ?2 WHERE id = ?1",
            params![id, since.map(|v| v as i64)],
        )?;
        Ok(())
    }

    /// Since when a library workflow listens, or `None` while it is Off.
    pub fn workflow_listening_since(&self, id: &str) -> Result<Option<u64>, StoreError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT listening_since FROM workflows WHERE id = ?1")?;
        Ok(stmt
            .query_row(params![id], |r| r.get::<_, Option<i64>>(0))
            .optional()?
            .flatten()
            .map(|v| v as u64))
    }

    /// The library workflows that are On, oldest first.
    pub fn listening_workflow_ids(&self) -> Result<Vec<String>, StoreError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id FROM workflows WHERE listening_since IS NOT NULL ORDER BY created_at, id",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    // --- runs ---

    pub fn upsert_run(&self, row: &RunRow) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO workflow_runs (id, scope, goal_id, workflow_id, status, revision, queued_at,
                                        started_at, finished_at, listener, dispatched)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET
               scope=excluded.scope, goal_id=excluded.goal_id, workflow_id=excluded.workflow_id,
               status=excluded.status, revision=excluded.revision, queued_at=excluded.queued_at,
               started_at=excluded.started_at, finished_at=excluded.finished_at,
               listener=excluded.listener, dispatched=excluded.dispatched",
            params![
                row.id,
                row.scope,
                row.goal_id,
                row.workflow_id,
                row.status,
                row.revision as i64,
                row.queued_at as i64,
                row.started_at.map(|v| v as i64),
                row.finished_at.map(|v| v as i64),
                row.listener,
                row.dispatched
            ],
        )?;
        Ok(())
    }

    const RUN_COLUMNS: &'static str =
        "id, scope, goal_id, workflow_id, status, revision, queued_at,
                started_at, finished_at, listener, dispatched";

    pub fn get_run(&self, id: &str) -> Result<Option<RunRow>, StoreError> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM workflow_runs WHERE id = ?1",
            Self::RUN_COLUMNS
        ))?;
        Ok(stmt.query_row(params![id], row_to_run).optional()?)
    }

    /// The run a queued signal was dispatched as, when one was: what makes a
    /// replayed dispatch a no-op.
    pub fn run_dispatched_from(&self, signal: &str) -> Result<Option<String>, StoreError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT id FROM workflow_runs WHERE dispatched = ?1")?;
        Ok(stmt
            .query_row(params![signal], |r| r.get::<_, String>(0))
            .optional()?)
    }

    /// The run a signal was dispatched as and when it was queued, for the
    /// clash two runs on one signal make at a rebuild.
    pub fn run_dispatched_from_with_queued_at(
        &self,
        signal: &str,
    ) -> Result<Option<(String, u64)>, StoreError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT id, queued_at FROM workflow_runs WHERE dispatched = ?1")?;
        Ok(stmt
            .query_row(params![signal], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?.max(0) as u64))
            })
            .optional()?)
    }

    /// Take the signal off a run's row — the second of two runs on one
    /// signal keeps its row and loses the dispatch.
    pub fn clear_run_dispatched(&self, id: &str) -> Result<(), StoreError> {
        self.conn.execute(
            "UPDATE workflow_runs SET dispatched = NULL WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// How many runs one listener began are unfinished — queued, running or
    /// waiting: what its guard counts.
    pub fn live_runs_of_listener(&self, listener: &str) -> Result<u32, StoreError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT count(*) FROM workflow_runs
             WHERE listener = ?1 AND status IN ('queued','running','waiting')",
        )?;
        Ok(stmt
            .query_row(params![listener], |r| r.get::<_, i64>(0))?
            .max(0) as u32)
    }

    /// The runs one listener began, newest first.
    pub fn runs_of_listener(
        &self,
        listener: &str,
        limit: usize,
    ) -> Result<Vec<RunRow>, StoreError> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM workflow_runs WHERE listener = ?1
             ORDER BY queued_at DESC, id DESC LIMIT ?2",
            Self::RUN_COLUMNS
        ))?;
        let rows = stmt.query_map(params![listener, limit as i64], row_to_run)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Every row that says its run is still going — queued, running or
    /// waiting — for the reconcile to read back against the records.
    pub fn live_run_rows(&self) -> Result<Vec<RunRow>, StoreError> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM workflow_runs WHERE status IN ('queued','running','waiting')
             ORDER BY queued_at, id",
            Self::RUN_COLUMNS
        ))?;
        let rows = stmt.query_map([], row_to_run)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Drop one run's row and its step rows — a row the records no longer
    /// back. The work items and decisions that name the run keep their rows:
    /// they are records of their own.
    pub fn delete_run_row(&self, id: &str) -> Result<(), StoreError> {
        self.replace_run_steps(id, &[])?;
        self.conn
            .execute("DELETE FROM workflow_runs WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Every run of a goal in queue order: `(queued_at, id)`.
    pub fn runs_for_goal(&self, goal_id: &str) -> Result<Vec<RunRow>, StoreError> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM workflow_runs WHERE goal_id = ?1 ORDER BY queued_at, id",
            Self::RUN_COLUMNS
        ))?;
        let rows = stmt.query_map(params![goal_id], row_to_run)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// The ids of a goal's queued runs, first to start first.
    pub fn queued_run_ids(&self, goal_id: &str) -> Result<Vec<String>, StoreError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id FROM workflow_runs WHERE goal_id = ?1 AND status = 'queued'
             ORDER BY queued_at, id",
        )?;
        let rows = stmt.query_map(params![goal_id], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// The ids of the workspace's runs — of one workflow, or of every one —
    /// in the order they were made: `(queued_at, id)`. `live` keeps the
    /// started and unfinished ones alone.
    pub fn workspace_run_ids(
        &self,
        workflow_id: Option<&str>,
        live: bool,
    ) -> Result<Vec<String>, StoreError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id FROM workflow_runs
             WHERE scope = 'workspace'
               AND (?1 IS NULL OR workflow_id = ?1)
               AND (?2 = 0 OR status IN ('running','waiting'))
             ORDER BY queued_at, id",
        )?;
        let rows = stmt.query_map(params![workflow_id, live as i64], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// The goal a run is for, from its id alone: `Some(Some(goal))` for a
    /// goal's run, `Some(None)` for a run of the workspace, `None` for an id
    /// the index does not hold.
    pub fn goal_of_run(&self, id: &str) -> Result<Option<Option<String>>, StoreError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT goal_id FROM workflow_runs WHERE id = ?1")?;
        Ok(stmt
            .query_row(params![id], |r| r.get::<_, Option<String>>(0))
            .optional()?)
    }

    /// Forget a run of the workspace's rows. Its steps, its work items, its
    /// decisions and its spend cascade; a goal's run is never removed here.
    pub fn delete_workspace_run(&self, id: &str) -> Result<(), StoreError> {
        self.conn.execute(
            "DELETE FROM workflow_runs WHERE id = ?1 AND scope = 'workspace'",
            params![id],
        )?;
        Ok(())
    }

    /// Replace every step row of a run with the records given — one unit: a
    /// crash between the delete and the last insert used to leave a run with
    /// no step rows, invisible to every wait and due query for good.
    pub fn replace_run_steps(&self, run_id: &str, rows: &[RunStepRow]) -> Result<(), StoreError> {
        self.in_transaction(|| self.replace_run_steps_statements(run_id, rows))
    }

    fn replace_run_steps_statements(
        &self,
        run_id: &str,
        rows: &[RunStepRow],
    ) -> Result<(), StoreError> {
        self.conn
            .execute("DELETE FROM run_steps WHERE run_id = ?1", params![run_id])?;
        for row in rows {
            self.conn.execute(
                "INSERT INTO run_steps (run_id, step_id, kind, state, wait_topic, due_at, work_item, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, (SELECT id FROM work_items WHERE id = ?7), ?8)",
                params![
                    row.run_id,
                    row.step_id,
                    row.kind,
                    row.state,
                    row.wait_topic,
                    row.due_at.map(|v| v as i64),
                    row.work_item,
                    row.updated_at as i64
                ],
            )?;
        }
        Ok(())
    }

    /// Steps waiting on a signal topic (every topic when `None`).
    pub fn waiting_steps(&self, topic: Option<&str>) -> Result<Vec<RunStepRow>, StoreError> {
        let base = "SELECT run_id, step_id, kind, state, wait_topic, due_at, work_item, updated_at
                    FROM run_steps WHERE state = 'waiting' AND wait_topic IS NOT NULL";
        match topic {
            Some(t) => {
                let mut stmt = self.conn.prepare(&format!(
                    "{base} AND wait_topic = ?1 ORDER BY run_id, step_id"
                ))?;
                let rows = stmt.query_map(params![t], row_to_run_step)?;
                Ok(rows.collect::<Result<Vec<_>, _>>()?)
            }
            None => {
                let mut stmt = self
                    .conn
                    .prepare(&format!("{base} ORDER BY run_id, step_id"))?;
                let rows = stmt.query_map([], row_to_run_step)?;
                Ok(rows.collect::<Result<Vec<_>, _>>()?)
            }
        }
    }

    /// Steps whose timer or schedule is due at or before `now`.
    pub fn due_steps(&self, now: u64) -> Result<Vec<RunStepRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT run_id, step_id, kind, state, wait_topic, due_at, work_item, updated_at
             FROM run_steps WHERE state = 'waiting' AND due_at IS NOT NULL AND due_at <= ?1
             ORDER BY due_at, run_id, step_id",
        )?;
        let rows = stmt.query_map(params![now as i64], row_to_run_step)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Every armed wait, due or not, for re-arming on start.
    pub fn armed_steps(&self) -> Result<Vec<RunStepRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT run_id, step_id, kind, state, wait_topic, due_at, work_item, updated_at
             FROM run_steps WHERE state = 'waiting' AND kind = 'wait'
             ORDER BY run_id, step_id",
        )?;
        let rows = stmt.query_map([], row_to_run_step)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn goal_exists(&self, id: &str) -> Result<bool, StoreError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT 1 FROM goals WHERE id = ?1")?;
        Ok(stmt.exists(params![id])?)
    }

    /// Ids of the goals naming `assignee` (compact wire form), newest first.
    pub fn goals_for_assignee(&self, assignee: &str) -> Result<Vec<String>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT goals.id FROM goals, json_each(goals.assignees)
             WHERE json_each.value = ?1
             ORDER BY goals.created_at DESC, goals.id DESC",
        )?;
        let rows = stmt.query_map(params![assignee], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Deletes the goal row; the foreign keys cascade to everything under it.
    /// Only the FTS rows, which have no key, are removed by hand.
    pub fn delete_goal(&self, id: &str) -> Result<(), StoreError> {
        self.in_transaction(|| {
            self.conn
                .execute("DELETE FROM goals WHERE id = ?1", params![id])?;
            self.conn
                .execute("DELETE FROM events_fts WHERE goal_id = ?1", params![id])?;
            Ok(())
        })
    }

    // --- work items ---

    pub fn upsert_work_item(&self, row: &WorkItemRow) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO work_items (id, goal_id, project_id, run_id, step_id, state, harness,
                                     assignees, updated_at)
             VALUES (?1, ?2, (SELECT id FROM projects WHERE id = ?3),
                     (SELECT id FROM workflow_runs WHERE id = ?4), ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
               goal_id=excluded.goal_id, project_id=excluded.project_id,
               run_id=excluded.run_id, step_id=excluded.step_id, state=excluded.state,
               harness=excluded.harness, assignees=excluded.assignees,
               updated_at=excluded.updated_at",
            params![
                row.id,
                row.goal_id,
                row.project_id,
                row.run_id,
                row.step_id,
                row.state,
                row.harness,
                serde_json::to_string(&row.assignees)?,
                row.updated_at as i64
            ],
        )?;
        Ok(())
    }

    pub fn delete_work_item(&self, id: &str) -> Result<(), StoreError> {
        self.conn
            .execute("DELETE FROM work_items WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn work_items_for(&self, goal_id: &str) -> Result<Vec<WorkItemRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, goal_id, project_id, run_id, step_id, state, harness, assignees, updated_at
             FROM work_items WHERE goal_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![goal_id], row_to_work_item)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Where a work item is filed, from its id alone: the goal for a goal's
    /// run's item, the run for a run of the workspace's.
    pub fn home_of_work_item(&self, id: &str) -> Result<Option<HomeKey>, StoreError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT goal_id, run_id FROM work_items WHERE id = ?1")?;
        let columns = stmt
            .query_row(params![id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        Ok(columns.and_then(|(goal_id, run_id)| HomeKey::from_columns(goal_id, run_id)))
    }

    /// Every item naming `assignee` with where it is filed, newest first.
    pub fn work_items_naming_assignee(
        &self,
        assignee: &str,
    ) -> Result<Vec<(String, HomeKey)>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT work_items.id, work_items.goal_id, work_items.run_id
             FROM work_items, json_each(work_items.assignees)
             WHERE json_each.value = ?1
             ORDER BY work_items.updated_at DESC, work_items.id DESC",
        )?;
        let rows = stmt.query_map(params![assignee], |r| {
            Ok((r.get::<_, String>(0)?, r.get(1)?, r.get(2)?))
        })?;
        let mut filed = Vec::new();
        for row in rows {
            let (id, goal_id, run_id) = row?;
            if let Some(home) = HomeKey::from_columns(goal_id, run_id) {
                filed.push((id, home));
            }
        }
        Ok(filed)
    }

    // --- goal edges ---

    pub fn add_edge(&self, from: &str, to: &str, kind: &str) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT OR IGNORE INTO goal_edges (from_id, to_id, kind) VALUES (?1, ?2, ?3)",
            params![from, to, kind],
        )?;
        Ok(())
    }

    pub fn edges_from(&self, from: &str) -> Result<Vec<(String, String)>, StoreError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT to_id, kind FROM goal_edges WHERE from_id = ?1 ORDER BY to_id",
        )?;
        let rows = stmt.query_map(params![from], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    // --- projects and the relation ---

    pub fn upsert_project(&self, row: &ProjectRow) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO projects (id, slug, name, root_kind, root_path, vcs, publish,
                                   origin, origin_goal, origin_run, origin_step, origin_workflow, archived_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(id) DO UPDATE SET
               slug=excluded.slug, name=excluded.name, root_kind=excluded.root_kind,
               root_path=excluded.root_path, vcs=excluded.vcs, publish=excluded.publish,
               origin=excluded.origin, origin_goal=excluded.origin_goal,
               origin_run=excluded.origin_run, origin_step=excluded.origin_step,
               origin_workflow=excluded.origin_workflow, archived_at=excluded.archived_at, created_at=excluded.created_at",
            params![
                row.id,
                row.slug,
                row.name,
                row.root_kind,
                row.root_path,
                row.vcs,
                row.publish,
                row.origin,
                row.origin_goal,
                row.origin_run,
                row.origin_step,
                row.origin_workflow,
                row.archived_at.map(|a| a as i64),
                row.created_at as i64
            ],
        )?;
        Ok(())
    }

    /// Ids of the projects born of a goal — from the goal itself, or from a
    /// step of one of its runs.
    pub fn projects_from_goal(&self, goal_id: &str) -> Result<Vec<String>, StoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM projects WHERE origin_goal = ?1 ORDER BY created_at, id")?;
        let rows = stmt.query_map(params![goal_id], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Ids of the projects a workflow's steps made.
    pub fn projects_from_workflow(&self, workflow_id: &str) -> Result<Vec<String>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM projects WHERE origin = 'step' AND origin_workflow = ?1 ORDER BY created_at, id",
        )?;
        let rows = stmt.query_map(params![workflow_id], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// The slug — enough to find the truth file.
    pub fn project_slug(&self, id: &str) -> Result<Option<String>, StoreError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT slug FROM projects WHERE id = ?1")?;
        Ok(stmt
            .query_row(params![id], |r| r.get::<_, String>(0))
            .optional()?)
    }

    /// Every project id in the lists — never an archived one — oldest first.
    pub fn list_project_ids(&self) -> Result<Vec<String>, StoreError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id FROM projects WHERE archived_at IS NULL ORDER BY created_at, id",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// The projects put away, newest archived first.
    pub fn list_archived_project_ids(&self) -> Result<Vec<String>, StoreError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id FROM projects WHERE archived_at IS NOT NULL ORDER BY archived_at DESC, id",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Deletes the project row; attachments and workstreams cascade, work items
    /// that named it lose the reference.
    pub fn delete_project(&self, id: &str) -> Result<(), StoreError> {
        self.conn
            .execute("DELETE FROM projects WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn attach(
        &self,
        goal_id: &str,
        project_id: &str,
        attached_at: u64,
        attached_by: &str,
    ) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT OR IGNORE INTO goal_projects (goal_id, project_id, attached_at, attached_by)
             VALUES (?1, ?2, ?3, ?4)",
            params![goal_id, project_id, attached_at as i64, attached_by],
        )?;
        Ok(())
    }

    pub fn detach(&self, goal_id: &str, project_id: &str) -> Result<(), StoreError> {
        self.conn.execute(
            "DELETE FROM goal_projects WHERE goal_id = ?1 AND project_id = ?2",
            params![goal_id, project_id],
        )?;
        Ok(())
    }

    /// Project ids attached to a goal, oldest attachment first.
    pub fn projects_of_goal(&self, goal_id: &str) -> Result<Vec<String>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT project_id FROM goal_projects WHERE goal_id = ?1
             ORDER BY attached_at, project_id",
        )?;
        let rows = stmt.query_map(params![goal_id], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Goal ids a project is attached to, oldest attachment first.
    pub fn goals_of_project(&self, project_id: &str) -> Result<Vec<String>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT goal_id FROM goal_projects WHERE project_id = ?1
             ORDER BY attached_at, goal_id",
        )?;
        let rows = stmt.query_map(params![project_id], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// `(project id, attached_at, attached_by)` for a goal, oldest first.
    pub fn attachments_of_goal(
        &self,
        goal_id: &str,
    ) -> Result<Vec<(String, u64, String)>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT project_id, attached_at, attached_by FROM goal_projects WHERE goal_id = ?1
             ORDER BY attached_at, project_id",
        )?;
        let rows = stmt.query_map(params![goal_id], |r| {
            Ok((r.get(0)?, r.get::<_, i64>(1)?.max(0) as u64, r.get(2)?))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn is_attached(&self, goal_id: &str, project_id: &str) -> Result<bool, StoreError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT 1 FROM goal_projects WHERE goal_id = ?1 AND project_id = ?2")?;
        Ok(stmt.exists(params![goal_id, project_id])?)
    }

    // --- workstreams ---

    /// The three optional references resolve through a subquery, so a
    /// workstream naming a goal, an item or an agent the index no longer holds
    /// records `NULL` rather than failing the write: the record on disk is the
    /// truth, and it may legitimately outlive what it points at.
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_workstream(
        &self,
        id: &str,
        project: &str,
        kind: &str,
        name: Option<&str>,
        goal: Option<&str>,
        work_item: Option<&str>,
        branch: Option<&str>,
        agent: Option<&str>,
        state: &str,
        created_at: u64,
    ) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO workstreams
               (id, project_id, kind, name, goal_id, work_item, branch, agent_id, state, created_at)
             VALUES (?1, ?2, ?3, ?4,
                     (SELECT id FROM goals WHERE id = ?5),
                     (SELECT id FROM work_items WHERE id = ?6),
                     ?7,
                     (SELECT id FROM agents WHERE id = ?8),
                     ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
               project_id=excluded.project_id, kind=excluded.kind, name=excluded.name,
               goal_id=excluded.goal_id, work_item=excluded.work_item, branch=excluded.branch,
               agent_id=excluded.agent_id, state=excluded.state, created_at=excluded.created_at",
            params![
                id,
                project,
                kind,
                name,
                goal,
                work_item,
                branch,
                agent,
                state,
                created_at as i64
            ],
        )?;
        Ok(())
    }

    /// The project whose folder holds a workstream — the locator for its file.
    pub fn workstream_project(&self, id: &str) -> Result<Option<String>, StoreError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT project_id FROM workstreams WHERE id = ?1")?;
        Ok(stmt
            .query_row(params![id], |r| r.get::<_, String>(0))
            .optional()?)
    }

    /// `(id, project id)` pairs for workstreams matching an optional
    /// single-column filter: each project's primary first, then oldest first.
    pub fn workstream_locators(
        &self,
        column: Option<&str>,
        value: Option<&str>,
    ) -> Result<Vec<(String, String)>, StoreError> {
        let sql = match column {
            None => "SELECT id, project_id FROM workstreams
                     ORDER BY project_id, (kind = 'primary') DESC, created_at, id"
                .to_string(),
            // A run is not a workstream's column: its work items name it.
            Some("run_id") => "SELECT workstreams.id, workstreams.project_id
                     FROM workstreams JOIN work_items ON work_items.id = workstreams.work_item
                     WHERE work_items.run_id = ?1
                     ORDER BY workstreams.project_id, (workstreams.kind = 'primary') DESC,
                              workstreams.created_at, workstreams.id"
                .to_string(),
            Some(col) => {
                if !matches!(col, "project_id" | "goal_id" | "work_item") {
                    return Err(StoreError::Invalid(bisa_core::text!(
                        "error-store-invalid-workstreams-cannot-be-filtered",
                        col = col.to_string()
                    )));
                }
                format!(
                    "SELECT id, project_id FROM workstreams WHERE {col} = ?1
                     ORDER BY project_id, (kind = 'primary') DESC, created_at, id"
                )
            }
        };
        let mut stmt = self.conn.prepare(&sql)?;
        let map = |r: &rusqlite::Row<'_>| Ok((r.get(0)?, r.get(1)?));
        let rows: Vec<(String, String)> = match value {
            Some(v) => stmt
                .query_map(params![v], map)?
                .collect::<Result<Vec<_>, _>>()?,
            None => stmt.query_map([], map)?.collect::<Result<Vec<_>, _>>()?,
        };
        Ok(rows)
    }

    pub fn delete_workstream(&self, id: &str) -> Result<(), StoreError> {
        self.conn
            .execute("DELETE FROM workstreams WHERE id = ?1", params![id])?;
        Ok(())
    }

    // --- agents / teams ---

    #[allow(clippy::too_many_arguments)]
    pub fn upsert_agent(
        &self,
        id: &str,
        name: &str,
        harness: &str,
        enabled: bool,
        pubkey: &str,
        origin: &str,
        created_at: u64,
    ) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO agents (id, name, harness, enabled, pubkey, origin, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
               name=excluded.name, harness=excluded.harness, enabled=excluded.enabled,
               pubkey=excluded.pubkey, origin=excluded.origin, created_at=excluded.created_at",
            params![
                id,
                name,
                harness,
                enabled as i64,
                pubkey,
                origin,
                created_at as i64
            ],
        )?;
        Ok(())
    }

    pub fn delete_agent(&self, id: &str) -> Result<(), StoreError> {
        self.conn
            .execute("DELETE FROM agents WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn upsert_team(
        &self,
        id: &str,
        name: &str,
        enabled: bool,
        origin: &str,
        created_at: u64,
    ) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO teams (id, name, enabled, origin, created_at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
               name=excluded.name, enabled=excluded.enabled, origin=excluded.origin,
               created_at=excluded.created_at",
            params![id, name, enabled as i64, origin, created_at as i64],
        )?;
        Ok(())
    }

    pub fn delete_team(&self, id: &str) -> Result<(), StoreError> {
        self.conn
            .execute("DELETE FROM teams WHERE id = ?1", params![id])?;
        Ok(())
    }

    // --- channels ---

    #[allow(clippy::too_many_arguments)]
    pub fn upsert_channel(
        &self,
        id: &str,
        name: &str,
        kind: &str,
        topic: Option<&str>,
        audience_json: &str,
        roster_policy: &str,
        origin: &str,
        created_at: u64,
    ) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO channels
               (id, name, kind, topic, audience_json, roster_policy, origin, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
               name=excluded.name, kind=excluded.kind, topic=excluded.topic,
               audience_json=excluded.audience_json, roster_policy=excluded.roster_policy,
               origin=excluded.origin, created_at=excluded.created_at",
            params![
                id,
                name,
                kind,
                topic,
                audience_json,
                roster_policy,
                origin,
                created_at as i64
            ],
        )?;
        Ok(())
    }

    /// Replace a `listed` channel's roster rows. `(member_kind, member_id)`.
    pub fn set_channel_roster(
        &self,
        channel_id: &str,
        members: &[(&str, &str)],
    ) -> Result<(), StoreError> {
        self.conn.execute(
            "DELETE FROM channel_roster WHERE channel_id = ?1",
            params![channel_id],
        )?;
        let mut stmt = self.conn.prepare(
            "INSERT OR IGNORE INTO channel_roster (channel_id, member_kind, member_id)
             VALUES (?1, ?2, ?3)",
        )?;
        for (kind, id) in members {
            stmt.execute(params![channel_id, kind, id])?;
        }
        Ok(())
    }

    /// Channel ids whose `listed` roster names `(member_kind, member_id)`.
    pub fn channels_rostering(
        &self,
        member_kind: &str,
        member_id: &str,
    ) -> Result<Vec<String>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT channel_id FROM channel_roster WHERE member_kind = ?1 AND member_id = ?2
             ORDER BY channel_id",
        )?;
        let rows = stmt.query_map(params![member_kind, member_id], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn delete_channel(&self, id: &str) -> Result<(), StoreError> {
        self.conn
            .execute("DELETE FROM channels WHERE id = ?1", params![id])?;
        Ok(())
    }

    // --- skills / mcp servers ---

    pub fn upsert_skill(
        &self,
        id: &str,
        name: &str,
        description: &str,
        origin: &str,
        created_at: u64,
    ) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO skills (id, name, description, origin, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
               name=excluded.name, description=excluded.description,
               origin=excluded.origin, created_at=excluded.created_at",
            params![id, name, description, origin, created_at as i64],
        )?;
        Ok(())
    }

    pub fn delete_skill(&self, id: &str) -> Result<(), StoreError> {
        self.conn
            .execute("DELETE FROM skills WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn upsert_mcp(
        &self,
        id: &str,
        name: &str,
        description: &str,
        transport: &str,
        enabled: bool,
        created_at: u64,
    ) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO mcp_servers (id, name, description, transport, enabled, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
               name=excluded.name, description=excluded.description,
               transport=excluded.transport, enabled=excluded.enabled,
               created_at=excluded.created_at",
            params![
                id,
                name,
                description,
                transport,
                enabled as i64,
                created_at as i64
            ],
        )?;
        Ok(())
    }

    pub fn delete_mcp(&self, id: &str) -> Result<(), StoreError> {
        self.conn
            .execute("DELETE FROM mcp_servers WHERE id = ?1", params![id])?;
        Ok(())
    }

    // --- connectors ---

    pub fn upsert_connector(
        &self,
        id: &str,
        name: &str,
        description: &str,
        auth: &str,
        origin: &str,
        created_at: u64,
    ) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO connectors (id, name, description, auth, origin, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
               name=excluded.name, description=excluded.description, auth=excluded.auth,
               origin=excluded.origin, created_at=excluded.created_at",
            params![id, name, description, auth, origin, created_at as i64],
        )?;
        Ok(())
    }

    pub fn delete_connector(&self, id: &str) -> Result<(), StoreError> {
        self.conn
            .execute("DELETE FROM connectors WHERE id = ?1", params![id])?;
        Ok(())
    }

    // --- sessions ---

    pub fn upsert_session(&self, row: &SessionRow) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO sessions
               (id, adapter, kind, work_item, workstream, conversation, agent_id, transcript_path,
                resume_token_json, status, parked_at, pid, pid_seen_at, ended_at)
             VALUES (?1, ?2, ?13,
                     (SELECT id FROM work_items WHERE id = ?3),
                     (SELECT id FROM workstreams WHERE id = ?9),
                     (SELECT id FROM conversations WHERE id = ?14),
                     (SELECT id FROM agents WHERE id = ?4),
                     ?5, ?6, ?7, ?8, ?10, ?11, ?12)
             ON CONFLICT(id) DO UPDATE SET
               adapter=excluded.adapter, kind=excluded.kind, work_item=excluded.work_item,
               workstream=excluded.workstream, conversation=excluded.conversation,
               agent_id=excluded.agent_id,
               transcript_path=excluded.transcript_path,
               resume_token_json=excluded.resume_token_json,
               status=excluded.status, parked_at=excluded.parked_at,
               pid=excluded.pid, pid_seen_at=excluded.pid_seen_at, ended_at=excluded.ended_at",
            params![
                row.id,
                row.adapter,
                row.work_item,
                row.agent_id,
                row.transcript_path,
                row.resume_token_json,
                row.status.as_str(),
                row.parked_at.map(|v| v as i64),
                row.workstream,
                row.pid.map(|v| v as i64),
                row.pid_seen_at.map(|v| v as i64),
                row.ended_at.map(|v| v as i64),
                row.kind.as_str(),
                row.conversation,
            ],
        )?;
        Ok(())
    }

    const SESSION_COLUMNS: &'static str =
        "id, adapter, work_item, agent_id, transcript_path, resume_token_json, status,
                    parked_at, workstream, pid, pid_seen_at, ended_at, kind, conversation";

    fn session_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRow> {
        Ok(SessionRow {
            id: r.get(0)?,
            adapter: r.get(1)?,
            work_item: r.get(2)?,
            agent_id: r.get(3)?,
            transcript_path: r.get(4)?,
            resume_token_json: r.get(5)?,
            status: SessionStatus::parse(&r.get::<_, String>(6)?),
            parked_at: r.get::<_, Option<i64>>(7)?.map(|v| v as u64),
            workstream: r.get(8)?,
            pid: r.get::<_, Option<i64>>(9)?.map(|v| v as u32),
            pid_seen_at: r.get::<_, Option<i64>>(10)?.map(|v| v as u64),
            ended_at: r.get::<_, Option<i64>>(11)?.map(|v| v as u64),
            kind: SessionKind::parse(&r.get::<_, String>(12)?),
            conversation: r.get(13)?,
        })
    }

    pub fn get_session(&self, id: &str) -> Result<Option<SessionRow>, StoreError> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM sessions WHERE id = ?1",
            Self::SESSION_COLUMNS
        ))?;
        Ok(stmt.query_row(params![id], Self::session_row).optional()?)
    }

    /// Every session the index still holds as live — after a boot, the ones
    /// the last process was driving when it died.
    pub fn live_sessions(&self) -> Result<Vec<SessionRow>, StoreError> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM sessions WHERE status = 'live' ORDER BY id",
            Self::SESSION_COLUMNS
        ))?;
        let rows = stmt.query_map([], Self::session_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Every session row that still names a harness child, whatever its
    /// status: an ended row keeps its pid while the child outlives it.
    pub fn sessions_with_process(&self) -> Result<Vec<SessionRow>, StoreError> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM sessions WHERE pid IS NOT NULL ORDER BY id",
            Self::SESSION_COLUMNS
        ))?;
        let rows = stmt.query_map([], Self::session_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    // --- approvals / ledger / fts ---

    pub fn add_approval(
        &self,
        home: &HomeKey,
        gate: &str,
        subject: &str,
        actor: &str,
        approve: bool,
        at: u64,
    ) -> Result<(), StoreError> {
        let (goal_id, run_id) = home.columns();
        self.conn.execute(
            "INSERT INTO approvals (goal_id, run_id, gate, subject, actor, approve, at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                goal_id,
                run_id,
                gate,
                subject,
                actor,
                approve as i64,
                at as i64
            ],
        )?;
        Ok(())
    }

    /// Gate decisions, newest first. The one durable answer to "has this goal
    /// — or this run of the workspace — ever asked a human for something?" —
    /// re-derived from the journals.
    pub fn decisions(&self, limit: usize) -> Result<Vec<crate::inbox::DecisionRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT goal_id, run_id, gate, subject, actor, approve, at FROM approvals
             ORDER BY at DESC, id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| {
            Ok(crate::inbox::DecisionRow {
                goal_id: r.get(0)?,
                run_id: r.get(1)?,
                gate: r.get(2)?,
                subject: r.get(3)?,
                actor: r.get(4)?,
                approve: r.get::<_, i64>(5)? != 0,
                at: r.get::<_, i64>(6)?.max(0) as u64,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn add_spend(
        &self,
        home: &HomeKey,
        tokens: u64,
        usd_cents: u64,
        wall_secs: u64,
        at: u64,
    ) -> Result<(), StoreError> {
        let (goal_id, run_id) = home.columns();
        self.conn.execute(
            "INSERT INTO budget_ledger (goal_id, run_id, tokens, usd_cents, wall_secs, at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                goal_id,
                run_id,
                tokens as i64,
                usd_cents as i64,
                wall_secs as i64,
                at as i64
            ],
        )?;
        Ok(())
    }

    /// What a home has spent: the sums of its ledger rows.
    pub fn total_spend(&self, home: &HomeKey) -> Result<(u64, u64, u64), StoreError> {
        let (goal_id, run_id) = home.columns();
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(SUM(tokens),0), COALESCE(SUM(usd_cents),0), COALESCE(SUM(wall_secs),0)
             FROM budget_ledger
             WHERE (?1 IS NOT NULL AND goal_id = ?1) OR (?2 IS NOT NULL AND run_id = ?2)",
        )?;
        Ok(stmt.query_row(params![goal_id, run_id], |r| {
            Ok((
                r.get::<_, i64>(0)? as u64,
                r.get::<_, i64>(1)? as u64,
                r.get::<_, i64>(2)? as u64,
            ))
        })?)
    }

    /// One fact into the feed. The row's `seq` is the insertion order — the
    /// tiebreak that makes a page exact when two facts share a second.
    pub fn record_activity(&self, fact: &ActivityFact) -> Result<i64, StoreError> {
        self.conn.execute(
            "INSERT INTO activity (at, concept, kind, source_kind, source_id, author, event)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                fact.at.min(i64::MAX as u64) as i64,
                fact.concept.as_str(),
                fact.kind,
                fact.source.kind.as_str(),
                fact.source.id,
                fact.author,
                serde_json::to_string(&fact.event)?,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// One page of the feed, newest first: every concept or one, the rows
    /// strictly before the cursor by `(at, seq)`, at most `limit`. Keyset
    /// paging — a page costs the same at any depth and never loses a row
    /// that shares a second with the last one shown.
    pub fn activity_page(
        &self,
        concept: Option<ActivityConcept>,
        before: Option<ActivityCursor>,
        limit: usize,
    ) -> Result<Vec<ActivityRecord>, StoreError> {
        let (at, seq) = match before {
            Some(c) => (c.at.min(i64::MAX as u64) as i64, c.seq),
            None => (i64::MAX, i64::MAX),
        };
        let concept = concept.map(|c| c.as_str().to_string());
        let mut stmt = self.conn.prepare_cached(
            "SELECT seq, at, concept, kind, source_kind, source_id, author, event
             FROM activity
             WHERE (?1 IS NULL OR concept = ?1)
               AND (at < ?2 OR (at = ?2 AND seq < ?3))
             ORDER BY at DESC, seq DESC LIMIT ?4",
        )?;
        let rows = stmt.query_map(params![concept, at, seq, limit as i64], |r| {
            Ok(ActivityRecord {
                seq: r.get(0)?,
                at: r.get::<_, i64>(1)? as u64,
                concept: r.get(2)?,
                kind: r.get(3)?,
                source_kind: r.get(4)?,
                source_id: r.get(5)?,
                author: r.get(6)?,
                event: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// The newest `limit` activity rows whose `kind` is one of `kinds` —
    /// the Inbox's one read of the feed, over `idx_activity_kind`. Nothing
    /// outside the kinds is read, whatever its concept.
    pub fn activity_by_kinds(
        &self,
        kinds: &[&str],
        limit: usize,
    ) -> Result<Vec<ActivityRecord>, StoreError> {
        if kinds.is_empty() {
            return Ok(Vec::new());
        }
        let marks = std::iter::repeat_n("?", kinds.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT seq, at, concept, kind, source_kind, source_id, author, event
             FROM activity
             WHERE kind IN ({marks})
             ORDER BY at DESC, seq DESC LIMIT ?"
        );
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = kinds
            .iter()
            .map(|k| Box::new(k.to_string()) as Box<dyn rusqlite::ToSql>)
            .collect();
        args.push(Box::new(limit as i64));
        let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), |r| {
            Ok(ActivityRecord {
                seq: r.get(0)?,
                at: r.get::<_, i64>(1)? as u64,
                concept: r.get(2)?,
                kind: r.get(3)?,
                source_kind: r.get(4)?,
                source_id: r.get(5)?,
                author: r.get(6)?,
                event: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn index_text(&self, doc_id: &str, goal_id: &str, content: &str) -> Result<(), StoreError> {
        self.conn
            .execute("DELETE FROM events_fts WHERE doc_id = ?1", params![doc_id])?;
        self.conn.execute(
            "INSERT INTO events_fts (doc_id, goal_id, content) VALUES (?1, ?2, ?3)",
            params![doc_id, goal_id, content],
        )?;
        Ok(())
    }

    pub fn search(&self, query: &str) -> Result<Vec<String>, StoreError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT DISTINCT goal_id FROM events_fts WHERE events_fts MATCH ?1")?;
        let rows = stmt.query_map(params![query], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    // --- members / seen events ---

    pub fn upsert_member(
        &self,
        pubkey: &str,
        role: &str,
        label: Option<&str>,
        added_at: u64,
    ) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO members (pubkey, role, label, added_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(pubkey) DO UPDATE SET
               role=excluded.role, label=excluded.label, added_at=excluded.added_at",
            params![pubkey, role, label, added_at as i64],
        )?;
        Ok(())
    }

    pub fn delete_member(&self, pubkey: &str) -> Result<(), StoreError> {
        self.conn
            .execute("DELETE FROM members WHERE pubkey = ?1", params![pubkey])?;
        Ok(())
    }

    pub fn is_member(&self, pubkey: &str) -> Result<bool, StoreError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT 1 FROM members WHERE pubkey = ?1")?;
        Ok(stmt.exists(params![pubkey])?)
    }

    pub fn mark_seen(&self, event_id: &str, at: u64) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT OR IGNORE INTO seen_events (event_id, at) VALUES (?1, ?2)",
            params![event_id, at as i64],
        )?;
        Ok(())
    }

    pub fn is_seen(&self, event_id: &str) -> Result<bool, StoreError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT 1 FROM seen_events WHERE event_id = ?1")?;
        Ok(stmt.exists(params![event_id])?)
    }

    /// Retention: forget seen ids older than `cutoff`. Returns how many went.
    pub fn prune_seen_before(&self, cutoff: u64) -> Result<usize, StoreError> {
        Ok(self.conn.execute(
            "DELETE FROM seen_events WHERE at < ?1",
            params![cutoff as i64],
        )?)
    }

    pub fn seen_count(&self) -> Result<u64, StoreError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM seen_events", [], |r| {
                r.get::<_, i64>(0)
            })? as u64)
    }

    // --- tags ---

    pub fn set_tags(&self, entity: TagEntity, id: &str, tags: &[String]) -> Result<(), StoreError> {
        self.clear_tags(entity, id)?;
        let mut stmt = self
            .conn
            .prepare_cached("INSERT OR IGNORE INTO tags (entity, id, tag) VALUES (?1, ?2, ?3)")?;
        for tag in tags {
            stmt.execute(params![entity.as_str(), id, tag])?;
        }
        Ok(())
    }

    pub fn clear_tags(&self, entity: TagEntity, id: &str) -> Result<(), StoreError> {
        self.conn.execute(
            "DELETE FROM tags WHERE entity = ?1 AND id = ?2",
            params![entity.as_str(), id],
        )?;
        Ok(())
    }

    pub fn tags_of(&self, entity: TagEntity, id: &str) -> Result<Vec<String>, StoreError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT tag FROM tags WHERE entity = ?1 AND id = ?2 ORDER BY tag")?;
        let rows = stmt.query_map(params![entity.as_str(), id], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Ids of one entity kind carrying `tags`, sorted. An empty `tags` returns
    /// nothing, not everything.
    pub fn ids_with_tags(
        &self,
        entity: TagEntity,
        tags: &[String],
        match_mode: TagMatch,
    ) -> Result<Vec<String>, StoreError> {
        if tags.is_empty() {
            return Ok(vec![]);
        }
        let placeholders = std::iter::repeat_n("?", tags.len())
            .collect::<Vec<_>>()
            .join(",");
        let having = match match_mode {
            TagMatch::Any => String::new(),
            TagMatch::All => format!(" HAVING COUNT(*) = {}", tags.len()),
        };
        let sql = format!(
            "SELECT id FROM tags WHERE entity = ? AND tag IN ({placeholders}) \
             GROUP BY id{having} ORDER BY id"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let entity_str = entity.as_str();
        let mut args: Vec<&dyn rusqlite::ToSql> = vec![&entity_str];
        for t in tags {
            args.push(t);
        }
        let rows = stmt.query_map(args.as_slice(), |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Facet counts: `(entity, tag, how many objects carry it)`, most-used
    /// first.
    pub fn tag_counts(
        &self,
        entity: Option<TagEntity>,
    ) -> Result<Vec<(String, String, u64)>, StoreError> {
        let (sql, arg): (&str, Option<&str>) = match entity {
            Some(e) => (
                "SELECT entity, tag, COUNT(*) FROM tags WHERE entity = ?1
                 GROUP BY entity, tag ORDER BY COUNT(*) DESC, tag",
                Some(e.as_str()),
            ),
            None => (
                "SELECT entity, tag, COUNT(*) FROM tags
                 GROUP BY entity, tag ORDER BY COUNT(*) DESC, entity, tag",
                None,
            ),
        };
        let mut stmt = self.conn.prepare(sql)?;
        let map = |r: &rusqlite::Row| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)? as u64,
            ))
        };
        let rows = match arg {
            Some(a) => stmt.query_map(params![a], map)?.collect::<Vec<_>>(),
            None => stmt.query_map([], map)?.collect::<Vec<_>>(),
        };
        Ok(rows.into_iter().collect::<Result<Vec<_>, _>>()?)
    }

    // --- conversations ---

    #[allow(clippy::too_many_arguments)]
    pub fn upsert_conversation(
        &self,
        id: &str,
        origin_kind: &str,
        origin_id: Option<&str>,
        project: Option<&str>,
        title: Option<&str>,
        author: &str,
        created_at: u64,
        archived: bool,
        mode: &str,
    ) -> Result<(), StoreError> {
        // The record's own fields are written; the facts beside them
        // (`first_line`, the counts, the agents) are the messages' and stay.
        self.conn.execute(
            "INSERT INTO conversations
               (id, origin_kind, origin_id, project, title, author, created_at, archived, mode)
             VALUES (?1, ?2, ?3, (SELECT id FROM projects WHERE id = ?4), ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
               origin_kind=excluded.origin_kind, origin_id=excluded.origin_id,
               project=excluded.project, title=excluded.title, author=excluded.author,
               created_at=excluded.created_at, archived=excluded.archived, mode=excluded.mode",
            params![
                id,
                origin_kind,
                origin_id,
                project,
                title,
                author,
                created_at as i64,
                archived as i64,
                mode
            ],
        )?;
        Ok(())
    }

    /// A message landed in a conversation: the counts move, the first post's
    /// first line is kept, and the agents who spoke or were addressed are
    /// noted.
    pub fn note_conversation_message(
        &self,
        id: &str,
        at: u64,
        first_line: Option<&str>,
        agents: &[String],
    ) -> Result<(), StoreError> {
        self.in_transaction(|| {
            self.conn.execute(
                "UPDATE conversations SET
                   message_count = message_count + 1,
                   last_message_at = MAX(COALESCE(last_message_at, 0), ?2),
                   first_line = COALESCE(first_line, ?3)
                 WHERE id = ?1",
                params![id, at as i64, first_line],
            )?;
            for agent in agents {
                self.conn.execute(
                    "INSERT OR IGNORE INTO conversation_agents (conversation_id, agent_id)
                     VALUES (?1, ?2)",
                    params![id, agent],
                )?;
            }
            Ok(())
        })
    }

    const CONVERSATION_COLUMNS: &'static str =
        "c.id, c.origin_kind, c.origin_id, c.project, c.title, c.first_line, c.author,
         c.created_at, c.last_message_at, c.message_count, c.archived, c.mode,
         (SELECT GROUP_CONCAT(agent_id, ' ') FROM
            (SELECT agent_id FROM conversation_agents WHERE conversation_id = c.id ORDER BY agent_id))";

    fn conversation_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ConversationRow> {
        let agents: Option<String> = r.get(12)?;
        Ok(ConversationRow {
            id: r.get(0)?,
            origin_kind: r.get(1)?,
            origin_id: r.get(2)?,
            project: r.get(3)?,
            title: r.get(4)?,
            first_line: r.get(5)?,
            author: r.get(6)?,
            created_at: r.get::<_, i64>(7)? as u64,
            last_message_at: r.get::<_, Option<i64>>(8)?.map(|v| v as u64),
            message_count: r.get::<_, i64>(9)?.max(0) as u64,
            archived: r.get::<_, i64>(10)? != 0,
            mode: r.get(11)?,
            agents: agents
                .unwrap_or_default()
                .split_whitespace()
                .map(str::to_string)
                .collect(),
        })
    }

    pub fn get_conversation(&self, id: &str) -> Result<Option<ConversationRow>, StoreError> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM conversations c WHERE c.id = ?1",
            Self::CONVERSATION_COLUMNS
        ))?;
        Ok(stmt
            .query_row(params![id], Self::conversation_row)
            .optional()?)
    }

    /// Conversations a filter admits, the most recently moved first (a
    /// conversation nothing was said in yet ranks by its birth), archived
    /// ones last. `agent` keeps those the agent spoke in or was addressed
    /// in; `ids` is the full-text search's answer, and `None` means no
    /// search was asked for. `before` and `limit` page by the ranking moment.
    #[allow(clippy::too_many_arguments)]
    /// `project` is the column a project's or a checkout's conversation
    /// carries: every conversation standing in that project, whatever its
    /// origin kind.
    #[allow(clippy::too_many_arguments)]
    pub fn conversations_matching(
        &self,
        origin_kind: Option<&str>,
        origin_id: Option<&str>,
        project: Option<&str>,
        agent: Option<&str>,
        archived: Option<bool>,
        ids: Option<&[String]>,
        before: Option<crate::conversation::PageBefore>,
        limit: usize,
    ) -> Result<Vec<ConversationRow>, StoreError> {
        // The ranking moment is a whole second: a cursor that names the last
        // row shown continues from exactly there (`PageBefore`).
        let mut sql = format!(
            "SELECT {} FROM conversations c
             WHERE (?1 IS NULL OR c.origin_kind = ?1)
               AND (?2 IS NULL OR c.origin_id = ?2)
               AND (?3 IS NULL OR c.archived = ?3)
               AND (?4 IS NULL OR EXISTS (SELECT 1 FROM conversation_agents a
                                          WHERE a.conversation_id = c.id AND a.agent_id = ?4))
               AND (COALESCE(c.last_message_at, c.created_at) < ?5
                    OR (?7 IS NOT NULL AND COALESCE(c.last_message_at, c.created_at) = ?5
                        AND c.id < ?7))
               AND (?6 IS NULL OR c.project = ?6)",
            Self::CONVERSATION_COLUMNS
        );
        let (at, before_id) = match before {
            Some(b) => (b.at.min(i64::MAX as u64) as i64, b.id),
            None => (i64::MAX, None),
        };
        let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![
            Box::new(origin_kind.map(str::to_string)),
            Box::new(origin_id.map(str::to_string)),
            Box::new(archived.map(|a| a as i64)),
            Box::new(agent.map(str::to_string)),
            Box::new(at),
            Box::new(project.map(str::to_string)),
            Box::new(before_id),
        ];
        if let Some(ids) = ids {
            if ids.is_empty() {
                return Ok(vec![]);
            }
            let marks = ids
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 8))
                .collect::<Vec<_>>()
                .join(", ");
            sql.push_str(&format!(" AND c.id IN ({marks})"));
            for id in ids {
                values.push(Box::new(id.clone()));
            }
        }
        sql.push_str(&format!(
            " ORDER BY c.archived ASC, COALESCE(c.last_message_at, c.created_at) DESC, c.id DESC
              LIMIT {}",
            limit as i64
        ));
        let mut stmt = self.conn.prepare(&sql)?;
        let params = values.iter().map(|v| v.as_ref()).collect::<Vec<_>>();
        let rows = stmt.query_map(params.as_slice(), Self::conversation_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Conversation ids whose title contains `needle` (case-insensitively).
    pub fn conversations_titled_like(&self, needle: &str) -> Result<Vec<String>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM conversations WHERE title IS NOT NULL AND LOWER(title) LIKE ?1",
        )?;
        let pattern = format!("%{}%", needle.to_lowercase().replace(['%', '_'], " "));
        let rows = stmt.query_map(params![pattern], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn delete_conversation(&self, id: &str) -> Result<(), StoreError> {
        self.conn
            .execute("DELETE FROM conversations WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Every fact of one message stream leaves the cache: its messages (and
    /// with them the mentions, files, artifacts and reactions), its read
    /// marker and its searchable text. What a scope's deletion calls.
    pub fn delete_scope_facts(&self, scope_id: &str) -> Result<(), StoreError> {
        self.in_transaction(|| {
            self.conn.execute(
                "DELETE FROM messages WHERE scope_id = ?1",
                params![scope_id],
            )?;
            self.conn.execute(
                "DELETE FROM read_markers WHERE scope_id = ?1",
                params![scope_id],
            )?;
            self.conn.execute(
                "DELETE FROM events_fts WHERE goal_id = ?1",
                params![scope_id],
            )?;
            Ok(())
        })
    }

    /// The agent an event's author pubkey belongs to, when it is an agent's.
    pub fn agent_id_for_pubkey(&self, pubkey: &str) -> Result<Option<String>, StoreError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT id FROM agents WHERE pubkey = ?1")?;
        Ok(stmt
            .query_row(params![pubkey], |r| r.get::<_, String>(0))
            .optional()?)
    }

    /// The last `limit` messages of a scope, oldest first — the transcript a
    /// fresh session is given; the harness holds the rest as its own context.
    pub fn transcript_tail(
        &self,
        scope_id: &str,
        limit: usize,
    ) -> Result<Vec<MessageRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, scope_id, author, body_kind, content, context, created_at, reply_to, retracted, thinking, said
             FROM messages WHERE scope_id = ?1
             ORDER BY created_at DESC, rowid DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![scope_id, limit as i64], row_to_message)?;
        let mut v = rows.collect::<Result<Vec<_>, _>>()?;
        v.reverse();
        Ok(v)
    }

    // --- messages / reactions ---

    #[allow(clippy::too_many_arguments)]
    pub fn upsert_message(
        &self,
        id: &str,
        scope_kind: &str,
        scope_id: &str,
        author: &str,
        body_kind: &str,
        content: &str,
        thinking: Option<&str>,
        said: Option<&bisa_core::Text>,
        context_json: &str,
        reply_to: Option<&str>,
        created_at: u64,
    ) -> Result<(), StoreError> {
        let said = said.map(serde_json::to_string).transpose()?;
        self.conn.execute(
            "INSERT OR IGNORE INTO messages
               (id, scope_kind, scope_id, author, body_kind, content, thinking, said, context, reply_to, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                id,
                scope_kind,
                scope_id,
                author,
                body_kind,
                content,
                thinking,
                said,
                context_json,
                reply_to,
                created_at as i64
            ],
        )?;
        Ok(())
    }

    pub fn upsert_mention(
        &self,
        message_id: &str,
        pubkey: &str,
        scope_id: &str,
        created_at: u64,
    ) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT OR IGNORE INTO message_mentions (message_id, pubkey, scope_id, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![message_id, pubkey, scope_id, created_at as i64],
        )?;
        Ok(())
    }

    /// Messages that mention `pubkey`, newest-last. Retracted ones excluded.
    pub fn mentions_for(&self, pubkey: &str, limit: usize) -> Result<Vec<MessageRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT m.id, m.scope_id, m.author, m.body_kind, m.content, m.context, m.created_at,
                    m.reply_to, m.retracted, m.thinking, m.said
             FROM messages m
             JOIN message_mentions x ON x.message_id = m.id
             WHERE x.pubkey = ?1 AND m.retracted = 0
             ORDER BY m.created_at DESC, m.rowid DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![pubkey, limit as i64], row_to_message)?;
        let mut v = rows.collect::<Result<Vec<_>, _>>()?;
        v.reverse();
        Ok(v)
    }

    /// Scope ids in which `pubkey` is mentioned by at least one live message.
    pub fn scopes_mentioning(&self, pubkey: &str) -> Result<Vec<String>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT x.scope_id FROM message_mentions x
             JOIN messages m ON m.id = x.message_id
             WHERE x.pubkey = ?1 AND m.retracted = 0",
        )?;
        let rows = stmt.query_map(params![pubkey], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// A scope's messages before the cursor, newest-last. Keyset paging by
    /// `(created_at, rowid)` when the cursor names a row, so two messages
    /// posted within one second are never split by a page; by the second
    /// alone, exclusive, when it does not.
    pub fn messages_for(
        &self,
        scope_id: &str,
        before: Option<crate::conversation::PageBefore>,
        limit: usize,
    ) -> Result<Vec<MessageRow>, StoreError> {
        let (at, id) = match before {
            Some(b) => (b.at.min(i64::MAX as u64) as i64, b.id),
            None => (i64::MAX, None),
        };
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, scope_id, author, body_kind, content, context, created_at, reply_to, retracted, thinking, said
             FROM messages
             WHERE scope_id = ?1
               AND (created_at < ?2
                    OR (?3 IS NOT NULL AND created_at = ?2
                        AND rowid < COALESCE((SELECT rowid FROM messages WHERE id = ?3), 0)))
             ORDER BY created_at DESC, rowid DESC LIMIT ?4",
        )?;
        let rows = stmt.query_map(params![scope_id, at, id, limit as i64], row_to_message)?;
        let mut v = rows.collect::<Result<Vec<_>, _>>()?;
        v.reverse();
        Ok(v)
    }

    pub fn upsert_attachment(
        &self,
        message_id: &str,
        ordinal: usize,
        a: &bisa_core::AttachmentRef,
    ) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO attachments (message_id, ordinal, sha256, name, mime, size)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                message_id,
                ordinal as i64,
                a.sha256,
                a.name,
                a.mime,
                a.size as i64
            ],
        )?;
        Ok(())
    }

    /// Every message's files, for one page of a conversation, in one query.
    pub fn attachments_for(
        &self,
        message_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<bisa_core::AttachmentRef>>, StoreError> {
        let mut out: std::collections::HashMap<String, Vec<bisa_core::AttachmentRef>> =
            std::collections::HashMap::new();
        if message_ids.is_empty() {
            return Ok(out);
        }
        let holes = std::iter::repeat_n("?", message_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT message_id, sha256, name, mime, size FROM attachments
             WHERE message_id IN ({holes}) ORDER BY message_id, ordinal"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(message_ids), |r| {
            Ok((
                r.get::<_, String>(0)?,
                bisa_core::AttachmentRef {
                    sha256: r.get(1)?,
                    name: r.get(2)?,
                    mime: r.get(3)?,
                    size: r.get::<_, i64>(4)? as u64,
                },
            ))
        })?;
        for row in rows {
            let (id, a) = row?;
            out.entry(id).or_default().push(a);
        }
        Ok(out)
    }

    pub fn upsert_artifact(
        &self,
        message_id: &str,
        ordinal: usize,
        a: &bisa_core::ArtifactRef,
    ) -> Result<(), StoreError> {
        let (source_scope, source_id, source_path) = match &a.source {
            Some(s) => (
                Some(s.scope.as_str().to_string()),
                Some(s.id.clone()),
                Some(s.path.as_str().to_string()),
            ),
            None => (None, None, None),
        };
        self.conn.execute(
            "INSERT OR REPLACE INTO artifacts
             (message_id, ordinal, sha256, name, mime, size, kind, title, source_scope, source_id, source_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                message_id,
                ordinal as i64,
                a.sha256,
                a.name,
                a.mime,
                a.size as i64,
                a.kind.as_str(),
                a.title,
                source_scope,
                source_id,
                source_path,
            ],
        )?;
        Ok(())
    }

    /// Every message's artifacts, for one page of a conversation, in one query.
    pub fn artifacts_for(
        &self,
        message_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<bisa_core::ArtifactRef>>, StoreError> {
        let mut out: std::collections::HashMap<String, Vec<bisa_core::ArtifactRef>> =
            std::collections::HashMap::new();
        if message_ids.is_empty() {
            return Ok(out);
        }
        let holes = std::iter::repeat_n("?", message_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT message_id, sha256, name, mime, size, kind, title, source_scope, source_id, source_path
             FROM artifacts WHERE message_id IN ({holes}) ORDER BY message_id, ordinal"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(message_ids), |r| {
            Ok((r.get::<_, String>(0)?, row_to_artifact(r, 1)?))
        })?;
        for row in rows {
            let (id, a) = row?;
            out.entry(id).or_default().push(a);
        }
        Ok(out)
    }

    /// A conversation's artifacts, newest message first, a retracted
    /// message's left out.
    pub fn artifacts_for_scope(
        &self,
        scope_id: &str,
        limit: usize,
    ) -> Result<Vec<ArtifactListRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT m.id, m.scope_id, m.author, m.created_at,
                    a.sha256, a.name, a.mime, a.size, a.kind, a.title,
                    a.source_scope, a.source_id, a.source_path
             FROM artifacts a JOIN messages m ON m.id = a.message_id
             WHERE m.scope_id = ?1 AND m.retracted = 0
             ORDER BY m.created_at DESC, m.rowid DESC, a.ordinal ASC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![scope_id, limit as i64], |r| {
            Ok(ArtifactListRow {
                message_id: r.get(0)?,
                scope_id: r.get(1)?,
                author: r.get(2)?,
                created_at: r.get::<_, i64>(3)? as u64,
                artifact: row_to_artifact(r, 4)?,
                present: false,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// One message by its id, whatever its scope.
    pub fn message_by_id(&self, id: &str) -> Result<Option<MessageRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, scope_id, author, body_kind, content, context, created_at, reply_to, retracted, thinking, said
             FROM messages WHERE id = ?1",
        )?;
        Ok(stmt.query_row(params![id], row_to_message).optional()?)
    }

    pub fn upsert_reaction(
        &self,
        id: &str,
        target_id: &str,
        scope_id: &str,
        author: &str,
        emoji: &str,
        created_at: u64,
    ) -> Result<(), StoreError> {
        // A reaction retracted and given again within one second is the
        // same event, byte for byte — a Nostr id hashes the author, the
        // second, the kind, the tags and the content. Indexed again, it
        // stands again: the log's order is the truth, a replay included.
        self.conn.execute(
            "INSERT INTO reactions (id, target_id, scope_id, author, emoji, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET retracted = 0",
            params![id, target_id, scope_id, author, emoji, created_at as i64],
        )?;
        Ok(())
    }

    pub fn has_reaction(
        &self,
        target_id: &str,
        author: &str,
        emoji: &str,
    ) -> Result<bool, StoreError> {
        Ok(self.reaction_id(target_id, author, emoji)?.is_some())
    }

    /// The live reaction `author` already has on `target_id` with `emoji`,
    /// by its event id — the one a second reaction answers instead of
    /// writing another.
    pub fn reaction_id(
        &self,
        target_id: &str,
        author: &str,
        emoji: &str,
    ) -> Result<Option<String>, StoreError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id FROM reactions
             WHERE target_id = ?1 AND author = ?2 AND emoji = ?3 AND retracted = 0
             LIMIT 1",
        )?;
        let id = stmt
            .query_row(params![target_id, author, emoji], |r| r.get::<_, String>(0))
            .optional()?;
        Ok(id)
    }

    pub fn reactions_for(&self, scope_id: &str) -> Result<Vec<ReactionRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, target_id, author, emoji, retracted FROM reactions
             WHERE scope_id = ?1 AND retracted = 0 ORDER BY created_at",
        )?;
        let rows = stmt.query_map(params![scope_id], |r| {
            Ok(ReactionRow {
                id: r.get(0)?,
                target_id: r.get(1)?,
                author: r.get(2)?,
                emoji: r.get(3)?,
                retracted: r.get::<_, i64>(4)? != 0,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// The author of a stored fact (message or reaction), by event id.
    pub fn fact_author(&self, event_id: &str) -> Result<Option<String>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT author FROM messages WHERE id = ?1
             UNION SELECT author FROM reactions WHERE id = ?1",
        )?;
        Ok(stmt
            .query_row(params![event_id], |r| r.get::<_, String>(0))
            .optional()?)
    }

    /// The scope a stored fact belongs to, by event id.
    pub fn fact_scope(&self, event_id: &str) -> Result<Option<String>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT scope_id FROM messages WHERE id = ?1
             UNION SELECT scope_id FROM reactions WHERE id = ?1",
        )?;
        Ok(stmt
            .query_row(params![event_id], |r| r.get::<_, String>(0))
            .optional()?)
    }

    pub fn retract_fact(&self, event_id: &str) -> Result<(), StoreError> {
        self.conn.execute(
            "UPDATE messages SET retracted = 1 WHERE id = ?1",
            params![event_id],
        )?;
        self.conn.execute(
            "UPDATE reactions SET retracted = 1 WHERE id = ?1",
            params![event_id],
        )?;
        Ok(())
    }

    // --- read markers (local-only; lost on rebuild by design) ---

    pub fn set_read_marker(
        &self,
        scope_id: &str,
        at: u64,
        forced_unread: bool,
    ) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO read_markers (scope_id, last_read_at, forced_unread)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(scope_id) DO UPDATE SET
               last_read_at=excluded.last_read_at, forced_unread=excluded.forced_unread",
            params![scope_id, at as i64, forced_unread as i64],
        )?;
        Ok(())
    }

    /// Unread counts per scope: messages from others after the marker;
    /// forced-unread scopes report at least 1.
    pub fn unread_counts(&self, own_pubkey: &str) -> Result<Vec<(String, u64)>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT m.scope_id,
                    MAX(SUM(CASE WHEN m.author != ?1 AND m.retracted = 0
                          AND m.created_at > COALESCE(r.last_read_at, 0)
                        THEN 1 ELSE 0 END),
                        MAX(COALESCE(r.forced_unread, 0)))
             FROM messages m LEFT JOIN read_markers r ON r.scope_id = m.scope_id
             GROUP BY m.scope_id",
        )?;
        let rows = stmt.query_map(params![own_pubkey], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?.max(0) as u64))
        })?;
        Ok(rows
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|(_, n)| *n > 0)
            .collect())
    }

    pub fn read_markers(&self) -> Result<Vec<crate::inbox::ReadMarker>, StoreError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT scope_id, last_read_at, forced_unread FROM read_markers")?;
        let rows = stmt.query_map([], |r| {
            Ok(crate::inbox::ReadMarker {
                scope_id: r.get(0)?,
                last_read_at: r.get::<_, i64>(1)?.max(0) as u64,
                forced_unread: r.get::<_, i64>(2)? != 0,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// The newest live post of every scope that has one — who, the words,
    /// when — for a list's preview of each room. A retracted post and a
    /// membership event are nobody's last words. One query over every scope;
    /// the caller keeps the rooms it lists.
    pub fn latest_messages(&self) -> Result<Vec<MessageRow>, StoreError> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT id, scope_id, author, body_kind, content, context, created_at, reply_to, retracted, thinking, said
             FROM messages m
             WHERE m.rowid = ({LATEST_OF_SCOPE})"
        ))?;
        let rows = stmt.query_map([], row_to_message)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// The newest live post of one scope, by the rule of
    /// [`Self::latest_messages`] — what a frame about that room carries.
    pub fn latest_message(&self, scope: &str) -> Result<Option<MessageRow>, StoreError> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT id, scope_id, author, body_kind, content, context, created_at, reply_to, retracted, thinking, said
             FROM messages m
             WHERE m.scope_id = ?1 AND m.rowid = ({LATEST_OF_SCOPE})"
        ))?;
        let mut rows = stmt.query_map(params![scope], row_to_message)?;
        Ok(rows.next().transpose()?)
    }

    /// `(scope, newest live message)` for every conversation that has one.
    pub fn scope_activity(&self) -> Result<Vec<(String, u64)>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT scope_id, MAX(created_at) FROM messages
             WHERE retracted = 0 GROUP BY scope_id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?.max(0) as u64))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    // --- notes ---

    #[allow(clippy::too_many_arguments)]
    pub fn upsert_note(
        &self,
        id: &str,
        scope_kind: &str,
        scope_id: Option<&str>,
        home_goal: Option<&str>,
        title: &str,
        updated_at: u64,
        pinned: bool,
    ) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO notes (id, scope_kind, scope_id, home_goal, title, updated_at, pinned)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
               scope_kind=excluded.scope_kind, scope_id=excluded.scope_id,
               home_goal=excluded.home_goal, title=excluded.title,
               updated_at=excluded.updated_at, pinned=excluded.pinned",
            params![
                id,
                scope_kind,
                scope_id,
                home_goal,
                title,
                updated_at as i64,
                pinned as i64
            ],
        )?;
        Ok(())
    }

    pub fn delete_note(&self, id: &str) -> Result<(), StoreError> {
        self.conn
            .execute("DELETE FROM notes WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// `(scope_kind, scope_id)` — where a note lives, so its file can be found.
    pub fn note_scope(&self, id: &str) -> Result<Option<(String, Option<String>)>, StoreError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT scope_kind, scope_id FROM notes WHERE id = ?1")?;
        Ok(stmt
            .query_row(params![id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?)
    }

    /// `(id, scope_kind, scope_id)` of every note a filter admits: no kind is
    /// every note, a kind alone is every note of that kind, a kind and an id
    /// is one scope's. Pinned first, newest first, so a caller that reads
    /// nothing else still lists in the order the overlay draws.
    pub fn notes_matching(
        &self,
        scope_kind: Option<&str>,
        scope_id: Option<&str>,
    ) -> Result<Vec<(String, String, Option<String>)>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, scope_kind, scope_id FROM notes
             WHERE (?1 IS NULL OR scope_kind = ?1) AND (?2 IS NULL OR scope_id = ?2)
             ORDER BY pinned DESC, updated_at DESC, id DESC",
        )?;
        let rows = stmt.query_map(params![scope_kind, scope_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    // --- drawings ---

    pub fn upsert_drawing(&self, row: &DrawingRow) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO drawings
               (id, scope_kind, scope_id, home_goal, title, pinned, hash, element_count, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
               scope_kind=excluded.scope_kind, scope_id=excluded.scope_id,
               home_goal=excluded.home_goal, title=excluded.title, pinned=excluded.pinned,
               hash=excluded.hash, element_count=excluded.element_count,
               created_at=excluded.created_at, updated_at=excluded.updated_at",
            params![
                row.id,
                row.scope_kind,
                row.scope_id,
                row.home_goal,
                row.title,
                row.pinned as i64,
                row.hash,
                row.element_count as i64,
                row.created_at as i64,
                row.updated_at as i64,
            ],
        )?;
        Ok(())
    }

    pub fn delete_drawing(&self, id: &str) -> Result<(), StoreError> {
        self.conn
            .execute("DELETE FROM drawings WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Every drawing a filter admits, as the listing draws it — pinned first,
    /// newest first — with no scene: a row is the index's columns alone.
    pub fn drawings_matching(
        &self,
        scope_kind: Option<&str>,
        scope_id: Option<&str>,
    ) -> Result<Vec<DrawingRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, scope_kind, scope_id, home_goal, title, pinned, hash, element_count,
                    created_at, updated_at
             FROM drawings
             WHERE (?1 IS NULL OR scope_kind = ?1) AND (?2 IS NULL OR scope_id = ?2)
             ORDER BY pinned DESC, updated_at DESC, id DESC",
        )?;
        let rows = stmt.query_map(params![scope_kind, scope_id], |r| {
            Ok(DrawingRow {
                id: r.get(0)?,
                scope_kind: r.get(1)?,
                scope_id: r.get(2)?,
                home_goal: r.get(3)?,
                title: r.get(4)?,
                pinned: r.get::<_, i64>(5)? != 0,
                hash: r.get(6)?,
                element_count: r.get::<_, i64>(7)?.max(0) as u64,
                created_at: r.get::<_, i64>(8)?.max(0) as u64,
                updated_at: r.get::<_, i64>(9)?.max(0) as u64,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    // --- signals (the durable queue) ---
    //
    // Within one second the order is the order of arrival (`rowid`), never
    // the id's: a signal a run raised is named after its dedupe key, so its
    // id says nothing of when it came. A rebuild inserts in the log's order,
    // which is the same order.

    const SIGNAL_COLUMNS: &'static str = "id, host_kind, host_id, step, source, name, scope_kind,
                scope_id, dedupe_key, state, at, attempts, payload_json, chain_json, last_error";

    /// Insert a signal as the row says. The id is the row's own and
    /// `(host, step, dedupe_key)` the occurrence's: a second write of either
    /// is a no-op, never a second copy. Whether the row is new.
    pub fn insert_signal(&self, row: &SignalRow) -> Result<bool, StoreError> {
        let n = self.conn.execute(
            "INSERT INTO signals
               (id, host_kind, host_id, step, source, name, scope_kind, scope_id, dedupe_key,
                state, at, started_at, attempts, payload_json, chain_json, last_error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, NULL, ?12, ?13, ?14, ?15)
             ON CONFLICT DO NOTHING",
            params![
                row.id,
                row.host_kind,
                row.host_id,
                row.step,
                row.source,
                row.name,
                row.scope_kind,
                row.scope_id,
                row.dedupe_key,
                row.state,
                row.at as i64,
                row.attempts as i64,
                row.payload_json,
                row.chain_json,
                row.last_error
            ],
        )?;
        Ok(n > 0)
    }

    /// The signal one listener already holds for an occurrence, by its key.
    pub fn signal_by_dedupe(
        &self,
        host_kind: &str,
        host_id: &str,
        step: &str,
        dedupe_key: &str,
    ) -> Result<Option<SignalRow>, StoreError> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM signals
             WHERE host_kind = ?1 AND host_id = ?2 AND step = ?3 AND dedupe_key = ?4",
            Self::SIGNAL_COLUMNS
        ))?;
        Ok(stmt
            .query_row(params![host_kind, host_id, step, dedupe_key], row_to_signal)
            .optional()?)
    }

    /// Atomically take the oldest ready signal into `running`. A signal kept
    /// only for replay has no listener and is never claimed.
    pub fn claim_next_signal(&self, now: u64) -> Result<Option<SignalRow>, StoreError> {
        let mut stmt = self.conn.prepare(&format!(
            "UPDATE signals SET state = 'running', started_at = ?1, attempts = attempts + 1
             WHERE id = (SELECT id FROM signals WHERE state = 'queued' AND host_kind IS NOT NULL
                         ORDER BY at, rowid LIMIT 1)
             RETURNING {}",
            Self::SIGNAL_COLUMNS
        ))?;
        Ok(stmt
            .query_row(params![now as i64], row_to_signal)
            .optional()?)
    }

    /// Force a signal's state, attempts and last error. Used by the rebuild,
    /// replaying the queue's `State` lines in order, and by every move the
    /// queue makes after its truth line is written.
    pub fn set_signal_state(
        &self,
        id: &str,
        state: &str,
        started_at: Option<u64>,
        attempts: u32,
        last_error: Option<&str>,
    ) -> Result<(), StoreError> {
        self.conn.execute(
            "UPDATE signals SET state = ?2, started_at = ?3, attempts = ?4, last_error = ?5
             WHERE id = ?1",
            params![
                id,
                state,
                started_at.map(|v| v as i64),
                attempts as i64,
                last_error
            ],
        )?;
        Ok(())
    }

    /// The `running` signals claimed at or before `cutoff` — what a crash
    /// left in flight. A read, so the truth log can say *queued* first.
    pub fn stale_running_signals(&self, cutoff: u64) -> Result<Vec<SignalRow>, StoreError> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM signals WHERE state = 'running' AND COALESCE(started_at, 0) <= ?1
             ORDER BY at, rowid",
            Self::SIGNAL_COLUMNS
        ))?;
        let rows = stmt.query_map(params![cutoff as i64], row_to_signal)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Put one signal back on the queue — a stale claim, or one its guard
    /// held that may start now.
    pub fn requeue_signal(&self, id: &str) -> Result<(), StoreError> {
        self.conn.execute(
            "UPDATE signals SET state = 'queued', started_at = NULL
             WHERE id = ?1 AND state IN ('running','waiting','held')",
            params![id],
        )?;
        Ok(())
    }

    pub fn get_signal(&self, id: &str) -> Result<Option<SignalRow>, StoreError> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM signals WHERE id = ?1",
            Self::SIGNAL_COLUMNS
        ))?;
        Ok(stmt.query_row(params![id], row_to_signal).optional()?)
    }

    /// The newest signals — of one host, or of every one.
    pub fn list_signals(
        &self,
        host: Option<(&str, &str)>,
        limit: usize,
    ) -> Result<Vec<SignalRow>, StoreError> {
        let rows = match host {
            Some((kind, id)) => {
                let mut stmt = self.conn.prepare(&format!(
                    "SELECT {} FROM signals WHERE host_kind = ?1 AND host_id = ?2
                     ORDER BY at DESC, rowid DESC LIMIT ?3",
                    Self::SIGNAL_COLUMNS
                ))?;
                let rows = stmt.query_map(params![kind, id, limit as i64], row_to_signal)?;
                rows.collect::<Result<Vec<_>, _>>()?
            }
            None => {
                let mut stmt = self.conn.prepare(&format!(
                    "SELECT {} FROM signals ORDER BY at DESC, rowid DESC LIMIT ?1",
                    Self::SIGNAL_COLUMNS
                ))?;
                let rows = stmt.query_map(params![limit as i64], row_to_signal)?;
                rows.collect::<Result<Vec<_>, _>>()?
            }
        };
        Ok(rows)
    }

    /// Named signals raised at or after `since`, oldest first — what a `wait`
    /// re-armed after a restart reads back.
    pub fn named_signals_since(
        &self,
        name: &str,
        since: u64,
        limit: usize,
    ) -> Result<Vec<SignalRow>, StoreError> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM signals WHERE source = 'signal' AND name = ?1 AND at >= ?2
               AND host_kind IS NULL
             ORDER BY at, rowid LIMIT ?3",
            Self::SIGNAL_COLUMNS
        ))?;
        let rows = stmt.query_map(params![name, since as i64, limit as i64], row_to_signal)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// The signals of one listener that have not settled — ready, held by
    /// its guard, or waiting on a person — oldest first: its backlog.
    pub fn pending_signals_of_listener(
        &self,
        host_kind: &str,
        host_id: &str,
        step: &str,
    ) -> Result<Vec<SignalRow>, StoreError> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM signals
             WHERE host_kind = ?1 AND host_id = ?2 AND step = ?3
               AND state IN ('queued','waiting','held')
             ORDER BY at, rowid",
            Self::SIGNAL_COLUMNS
        ))?;
        let rows = stmt.query_map(params![host_kind, host_id, step], row_to_signal)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// The signals of one host that have not settled, every listener of it.
    pub fn pending_signals_of_host(
        &self,
        host_kind: &str,
        host_id: &str,
    ) -> Result<Vec<SignalRow>, StoreError> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM signals
             WHERE host_kind = ?1 AND host_id = ?2 AND state IN ('queued','waiting','held')
             ORDER BY at, rowid",
            Self::SIGNAL_COLUMNS
        ))?;
        let rows = stmt.query_map(params![host_kind, host_id], row_to_signal)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// When a listener's last signal arrived, if it ever had one.
    pub fn last_signal_at(
        &self,
        host_kind: &str,
        host_id: &str,
        step: &str,
    ) -> Result<Option<u64>, StoreError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT max(at) FROM signals WHERE host_kind = ?1 AND host_id = ?2 AND step = ?3",
        )?;
        Ok(stmt
            .query_row(params![host_kind, host_id, step], |r| {
                r.get::<_, Option<i64>>(0)
            })?
            .map(|v| v as u64))
    }

    /// Forget every signal of a host that is gone — a workflow deleted, a
    /// goal deleted. The truth log keeps them; a rebuild skips them.
    pub fn delete_signals_of_host(&self, host_kind: &str, host_id: &str) -> Result<(), StoreError> {
        self.conn.execute(
            "DELETE FROM signals WHERE host_kind = ?1 AND host_id = ?2",
            params![host_kind, host_id],
        )?;
        Ok(())
    }

    /// Comparable row sets (used by rebuild-equivalence tests).
    pub fn dump_goals(&self) -> Result<Vec<GoalRow>, StoreError> {
        self.list_goals(None)
    }

    /// The `CHECK` constraint on `messages.scope_kind`, as the schema spells
    /// it — pinned to the core's `ScopeKind` by a test.
    pub fn scope_kind_check_values() -> Vec<&'static str> {
        check_values(SCHEMA, "scope_kind TEXT NOT NULL CHECK (scope_kind IN (")
    }

    pub fn goal_status_check_values() -> Vec<&'static str> {
        check_values(
            table(SCHEMA, "goals"),
            "status      TEXT NOT NULL CHECK (status IN\n                  (",
        )
    }

    /// The `CHECK` on `workflow_runs.status` — pinned to `RunStatus::ALL`.
    pub fn run_status_check_values() -> Vec<&'static str> {
        check_values(
            table(SCHEMA, "workflow_runs"),
            "status      TEXT NOT NULL CHECK (status IN (",
        )
    }

    /// The `CHECK` on `run_steps.kind` — pinned to `StepKind::NAMES`.
    pub fn step_kind_check_values() -> Vec<&'static str> {
        check_values(
            table(SCHEMA, "run_steps"),
            "kind        TEXT NOT NULL CHECK (kind IN\n                  (",
        )
    }

    /// The `CHECK` on `run_steps.state` — pinned to `StepState::NAMES`.
    pub fn step_state_check_values() -> Vec<&'static str> {
        check_values(
            table(SCHEMA, "run_steps"),
            "state       TEXT NOT NULL CHECK (state IN\n                  (",
        )
    }

    /// The `CHECK` on `goals.origin` — pinned to `GoalOrigin::NAMES`.
    pub fn goal_origin_check_values() -> Vec<&'static str> {
        check_values(
            table(SCHEMA, "goals"),
            "origin      TEXT NOT NULL CHECK (origin IN (",
        )
    }

    /// The `CHECK` on `workflow_runs.scope` — pinned to `RunScope::NAMES`.
    pub fn run_scope_check_values() -> Vec<&'static str> {
        check_values(
            table(SCHEMA, "workflow_runs"),
            "scope       TEXT NOT NULL CHECK (scope IN (",
        )
    }
}

/// The schema from one table's `CREATE` onward, so a needle finds that
/// table's constraint and not an earlier table's that happens to read alike.
fn table(schema: &'static str, name: &str) -> &'static str {
    let needle = format!("CREATE TABLE {name} (");
    match schema.find(&needle) {
        Some(at) => &schema[at..],
        None => "",
    }
}

/// The quoted values inside the first `IN (...)` that follows `needle`.
fn check_values(schema: &'static str, needle: &str) -> Vec<&'static str> {
    let Some(start) = schema.find(needle) else {
        return vec![];
    };
    let rest = &schema[start + needle.len()..];
    let Some(end) = rest.find(')') else {
        return vec![];
    };
    rest[..end]
        .split(',')
        .map(|s| s.trim().trim_matches('\''))
        .filter(|s| !s.is_empty())
        .collect()
}

fn row_to_signal(r: &rusqlite::Row<'_>) -> rusqlite::Result<SignalRow> {
    Ok(SignalRow {
        id: r.get(0)?,
        host_kind: r.get(1)?,
        host_id: r.get(2)?,
        step: r.get(3)?,
        source: r.get(4)?,
        name: r.get(5)?,
        scope_kind: r.get(6)?,
        scope_id: r.get(7)?,
        dedupe_key: r.get(8)?,
        state: r.get(9)?,
        at: r.get::<_, i64>(10)? as u64,
        attempts: r.get::<_, i64>(11)? as u32,
        payload_json: r.get(12)?,
        chain_json: r.get(13)?,
        last_error: r.get(14)?,
    })
}

fn row_to_goal(r: &rusqlite::Row<'_>) -> rusqlite::Result<GoalRow> {
    let assignees: String = r.get(9)?;
    Ok(GoalRow {
        id: r.get(0)?,
        status: r.get(1)?,
        closure: r.get(2)?,
        superseded_by: r.get(3)?,
        origin: r.get(4)?,
        workflow_id: r.get(5)?,
        run_id: r.get(6)?,
        author: r.get(7)?,
        title: r.get(8)?,
        // A malformed column is a broken cache, not a broken workspace.
        assignees: serde_json::from_str(&assignees).unwrap_or_default(),
        revision: r.get::<_, i64>(10)? as u64,
        created_at: r.get::<_, i64>(11)? as u64,
        archived_at: r.get::<_, Option<i64>>(12)?.map(|a| a as u64),
        listening_since: r.get::<_, Option<i64>>(13)?.map(|a| a as u64),
    })
}

fn row_to_work_item(r: &rusqlite::Row<'_>) -> rusqlite::Result<WorkItemRow> {
    let assignees: String = r.get(7)?;
    Ok(WorkItemRow {
        id: r.get(0)?,
        goal_id: r.get(1)?,
        project_id: r.get(2)?,
        run_id: r.get(3)?,
        step_id: r.get(4)?,
        state: r.get(5)?,
        harness: r.get(6)?,
        assignees: serde_json::from_str(&assignees).unwrap_or_default(),
        updated_at: r.get::<_, i64>(8)? as u64,
    })
}

fn row_to_workflow(r: &rusqlite::Row<'_>) -> rusqlite::Result<WorkflowRow> {
    Ok(WorkflowRow {
        id: r.get(0)?,
        name: r.get(1)?,
        origin: r.get(2)?,
        catalog_slug: r.get(3)?,
        goal_id: r.get(4)?,
        author: r.get(5)?,
        revision: r.get::<_, i64>(6)? as u64,
        step_count: r.get::<_, i64>(7)? as u64,
        created_at: r.get::<_, i64>(8)? as u64,
        archived_at: r.get::<_, Option<i64>>(9)?.map(|a| a as u64),
    })
}

fn row_to_run(r: &rusqlite::Row<'_>) -> rusqlite::Result<RunRow> {
    Ok(RunRow {
        id: r.get(0)?,
        scope: r.get(1)?,
        goal_id: r.get(2)?,
        workflow_id: r.get(3)?,
        status: r.get(4)?,
        revision: r.get::<_, i64>(5)? as u64,
        queued_at: r.get::<_, i64>(6)? as u64,
        started_at: r.get::<_, Option<i64>>(7)?.map(|v| v as u64),
        finished_at: r.get::<_, Option<i64>>(8)?.map(|v| v as u64),
        listener: r.get(9)?,
        dispatched: r.get(10)?,
    })
}

fn row_to_run_step(r: &rusqlite::Row<'_>) -> rusqlite::Result<RunStepRow> {
    Ok(RunStepRow {
        run_id: r.get(0)?,
        step_id: r.get(1)?,
        kind: r.get(2)?,
        state: r.get(3)?,
        wait_topic: r.get(4)?,
        due_at: r.get::<_, Option<i64>>(5)?.map(|v| v as u64),
        work_item: r.get(6)?,
        updated_at: r.get::<_, i64>(7)? as u64,
    })
}

fn row_to_message(r: &rusqlite::Row<'_>) -> rusqlite::Result<MessageRow> {
    let context: String = r.get(5)?;
    Ok(MessageRow {
        id: r.get(0)?,
        scope_id: r.get(1)?,
        author: r.get(2)?,
        body_kind: r.get(3)?,
        content: r.get(4)?,
        context: serde_json::from_str(&context).unwrap_or_default(),
        created_at: r.get::<_, i64>(6)? as u64,
        reply_to: r.get(7)?,
        retracted: r.get::<_, i64>(8)? != 0,
        thinking: r.get(9)?,
        said: r
            .get::<_, Option<String>>(10)?
            .and_then(|json| serde_json::from_str(&json).ok()),
        attachments: Vec::new(),
        artifacts: Vec::new(),
    })
}

/// An artifact from the ten columns starting at `from`: sha256, name, mime,
/// size, kind, title, source_scope, source_id, source_path.
fn row_to_artifact(r: &rusqlite::Row<'_>, from: usize) -> rusqlite::Result<bisa_core::ArtifactRef> {
    let kind: String = r.get(from + 4)?;
    let kind = kind.parse::<bisa_core::ArtifactKind>().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            from + 4,
            rusqlite::types::Type::Text,
            Box::new(e),
        )
    })?;
    let source_scope: Option<String> = r.get(from + 6)?;
    let source_id: Option<String> = r.get(from + 7)?;
    let source_path: Option<String> = r.get(from + 8)?;
    let source = match (source_scope, source_id, source_path) {
        (Some(scope), Some(id), Some(path)) => {
            let scope = scope.parse::<bisa_core::FileScope>().map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    from + 6,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?;
            let path = bisa_core::RelPath::new(path).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    from + 8,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?;
            Some(bisa_core::ArtifactSource { scope, id, path })
        }
        _ => None,
    };
    Ok(bisa_core::ArtifactRef {
        sha256: r.get(from)?,
        name: r.get(from + 1)?,
        mime: r.get(from + 2)?,
        size: r.get::<_, i64>(from + 3)? as u64,
        kind,
        title: r.get(from + 5)?,
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A transaction that fails mid-way leaves nothing of itself behind, and
    /// one opened inside another joins it rather than refusing.
    #[test]
    fn a_failed_transaction_leaves_no_row_and_a_nested_one_joins_the_outer() {
        let idx = Index::open_in_memory().unwrap();
        let insert = |pubkey: &str| {
            idx.conn.execute(
                "INSERT INTO members (pubkey, role, label, added_at) VALUES (?1, 'member', NULL, 1)",
                params![pubkey],
            )?;
            Ok(())
        };
        let count = || -> i64 {
            idx.conn
                .query_row("SELECT count(*) FROM members", [], |r| r.get(0))
                .unwrap()
        };
        let refused: Result<(), StoreError> = idx.in_transaction(|| {
            insert("aa")?;
            Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-cut-short"
            )))
        });
        assert!(refused.is_err());
        assert_eq!(count(), 0, "the insert rolled back with the transaction");
        idx.in_transaction(|| {
            idx.in_transaction(|| insert("bb"))?;
            assert!(
                !idx.conn.is_autocommit(),
                "the inner call joined the outer transaction"
            );
            Ok(())
        })
        .unwrap();
        assert!(idx.conn.is_autocommit());
        assert_eq!(count(), 1);
    }

    fn test_project_row(id: &str, slug: &str) -> ProjectRow {
        ProjectRow {
            id: id.into(),
            slug: slug.into(),
            name: "Web".into(),
            root_kind: "managed".into(),
            root_path: None,
            vcs: "git".into(),
            publish: "gated".into(),
            origin: "workspace".into(),
            origin_goal: None,
            origin_run: None,
            origin_step: None,
            origin_workflow: None,
            archived_at: None,
            created_at: 1,
        }
    }
    use bisa_core::{GoalOrigin, GoalStatus, ScopeKind, StepKind, StepState};

    fn goal(id: &str) -> GoalRow {
        GoalRow {
            id: id.into(),
            status: "draft".into(),
            closure: None,
            superseded_by: None,
            origin: "captured".into(),
            workflow_id: None,
            run_id: None,
            author: "aa".into(),
            title: None,
            assignees: vec![],
            revision: 1,
            archived_at: None,
            listening_since: None,
            created_at: 1,
        }
    }

    fn workflow(id: &str) -> WorkflowRow {
        WorkflowRow {
            goal_id: None,
            id: id.into(),
            name: "Flow".into(),
            origin: "workspace".into(),
            catalog_slug: None,
            author: "aa".into(),
            revision: 1,
            step_count: 2,
            archived_at: None,
            created_at: 1,
        }
    }

    fn run(id: &str, goal_id: &str, workflow_id: &str) -> RunRow {
        RunRow {
            id: id.into(),
            scope: "goal".into(),
            goal_id: Some(goal_id.into()),
            workflow_id: workflow_id.into(),
            status: "running".into(),
            revision: 1,
            queued_at: 1,
            started_at: Some(1),
            finished_at: None,
            listener: None,
            dispatched: None,
        }
    }

    #[test]
    fn fts_and_ledger() {
        let idx = Index::open_in_memory().unwrap();
        idx.upsert_goal(&goal("01A")).unwrap();
        idx.index_text("01A:statement", "01A", "sync bookmarks across browsers")
            .unwrap();
        assert_eq!(idx.search("bookmarks").unwrap(), vec!["01A"]);
        assert!(idx.search("nonexistent").unwrap().is_empty());
        idx.add_spend(&HomeKey::Goal("01A".into()), 100, 2, 30, 1)
            .unwrap();
        idx.add_spend(&HomeKey::Goal("01A".into()), 50, 1, 10, 2)
            .unwrap();
        assert_eq!(
            idx.total_spend(&HomeKey::Goal("01A".into())).unwrap(),
            (150, 3, 40)
        );
    }

    /// A run of the workspace keeps its own rows: no goal, never queued, its
    /// spend and its decisions under the run — and they go with it.
    #[test]
    fn a_workspace_run_is_keyed_by_itself_and_its_rows_go_with_it() {
        let idx = Index::open_in_memory().unwrap();
        idx.upsert_workflow(&workflow("W1")).unwrap();
        let workspace_run = |id: &str, status: &str| RunRow {
            id: id.into(),
            scope: "workspace".into(),
            goal_id: None,
            workflow_id: "W1".into(),
            status: status.into(),
            revision: 1,
            queued_at: 1,
            started_at: Some(1),
            finished_at: None,
            listener: None,
            dispatched: None,
        };
        idx.upsert_run(&workspace_run("R1", "running")).unwrap();
        idx.upsert_run(&workspace_run("R2", "done")).unwrap();
        assert!(
            idx.upsert_run(&workspace_run("R3", "queued")).is_err(),
            "a run of the workspace is never queued"
        );
        let mut named = workspace_run("R4", "running");
        named.goal_id = Some("01A".into());
        assert!(
            idx.upsert_run(&named).is_err(),
            "a run of the workspace names no goal"
        );
        let mut orphan = run("R5", "01A", "W1");
        orphan.goal_id = None;
        assert!(
            idx.upsert_run(&orphan).is_err(),
            "a goal's run names its goal"
        );
        assert_eq!(idx.goal_of_run("R1").unwrap(), Some(None));
        assert_eq!(idx.goal_of_run("nope").unwrap(), None);
        assert_eq!(
            idx.workspace_run_ids(Some("W1"), false).unwrap(),
            vec!["R1", "R2"]
        );
        assert_eq!(idx.workspace_run_ids(Some("W1"), true).unwrap(), vec!["R1"]);
        assert_eq!(idx.workspace_run_ids(None, true).unwrap(), vec!["R1"]);
        assert!(idx.workspace_run_ids(Some("W9"), false).unwrap().is_empty());
        idx.upsert_work_item(&WorkItemRow {
            id: "I1".into(),
            goal_id: None,
            project_id: None,
            run_id: Some("R1".into()),
            step_id: Some("build".into()),
            state: "open".into(),
            harness: None,
            assignees: vec![],
            updated_at: 1,
        })
        .unwrap();
        assert_eq!(
            idx.home_of_work_item("I1").unwrap(),
            Some(HomeKey::Run("R1".into()))
        );
        assert!(
            idx.upsert_work_item(&WorkItemRow {
                id: "I2".into(),
                goal_id: None,
                project_id: None,
                run_id: None,
                step_id: None,
                state: "open".into(),
                harness: None,
                assignees: vec![],
                updated_at: 1,
            })
            .is_err(),
            "an item is filed under a goal or a run"
        );
        idx.add_spend(&HomeKey::Run("R1".into()), 10, 1, 2, 1)
            .unwrap();
        assert_eq!(
            idx.total_spend(&HomeKey::Run("R1".into())).unwrap(),
            (10, 1, 2)
        );
        assert_eq!(
            idx.total_spend(&HomeKey::Run("R2".into())).unwrap(),
            (0, 0, 0)
        );
        idx.add_approval(
            &HomeKey::Run("R1".into()),
            "approval",
            "approval:R1/ship",
            "aa",
            true,
            3,
        )
        .unwrap();
        let decided = idx.decisions(10).unwrap();
        assert_eq!(decided[0].run_id.as_deref(), Some("R1"));
        assert_eq!(decided[0].goal_id, None);
        idx.delete_workspace_run("R1").unwrap();
        assert!(idx.get_run("R1").unwrap().is_none());
        assert_eq!(idx.home_of_work_item("I1").unwrap(), None, "its items go");
        assert_eq!(
            idx.total_spend(&HomeKey::Run("R1".into())).unwrap(),
            (0, 0, 0)
        );
        assert!(idx.decisions(10).unwrap().is_empty(), "its decisions go");
    }

    #[test]
    fn assignee_lookup_uses_the_wire_form() {
        let idx = Index::open_in_memory().unwrap();
        for (id, assignees) in [
            ("01A", vec!["team:t1".to_string(), "agent:a1".to_string()]),
            ("01B", vec!["agent:a1".to_string()]),
            ("01C", vec![]),
        ] {
            let mut g = goal(id);
            g.assignees = assignees;
            idx.upsert_goal(&g).unwrap();
        }
        assert_eq!(idx.goals_for_assignee("agent:a1").unwrap().len(), 2);
        assert_eq!(idx.goals_for_assignee("team:t1").unwrap(), vec!["01A"]);
        assert!(idx.goals_for_assignee("team:nope").unwrap().is_empty());
    }

    /// The `CHECK` lists are the Rust enums' spellings, exactly.
    #[test]
    fn check_constraints_mirror_the_core_enums() {
        assert_eq!(
            Index::scope_kind_check_values(),
            ScopeKind::ALL
                .iter()
                .map(|k| k.as_str())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            Index::goal_status_check_values(),
            GoalStatus::ALL
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            Index::goal_origin_check_values(),
            GoalOrigin::NAMES.to_vec()
        );
        assert_eq!(
            Index::run_scope_check_values(),
            bisa_core::RunScope::NAMES.to_vec()
        );
        assert_eq!(Index::step_kind_check_values(), StepKind::NAMES.to_vec());
        assert_eq!(Index::step_state_check_values(), StepState::NAMES.to_vec());
        assert_eq!(
            Index::run_status_check_values(),
            bisa_core::RunStatus::ALL
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
        );
        let idx = Index::open_in_memory().unwrap();
        let mut bad = goal("01Z");
        bad.status = "shaping".into();
        assert!(
            idx.upsert_goal(&bad).is_err(),
            "the lifecycle vocabulary is refused"
        );
        let version: i32 = idx
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        assert_eq!(
            SCHEMA_VERSION, 26,
            "the schema grew; still no ladder — 08-persistence.md says twenty-six too"
        );
    }

    #[test]
    fn a_closed_goal_carries_a_reason_and_only_then() {
        let idx = Index::open_in_memory().unwrap();
        let mut g = goal("01A");
        g.status = "closed".into();
        assert!(idx.upsert_goal(&g).is_err(), "closed without a closure");
        g.closure = Some("abandoned".into());
        idx.upsert_goal(&g).unwrap();
        let mut live = goal("01B");
        live.closure = Some("abandoned".into());
        assert!(
            idx.upsert_goal(&live).is_err(),
            "a live goal has no closure"
        );
        // A successor the index does not hold records NULL rather than failing.
        let mut sup = goal("01C");
        sup.status = "closed".into();
        sup.closure = Some("superseded".into());
        sup.superseded_by = Some("01NOPE".into());
        idx.upsert_goal(&sup).unwrap();
        assert_eq!(idx.get_goal("01C").unwrap().unwrap().superseded_by, None);
    }

    #[test]
    fn workflows_runs_and_steps_cascade_and_a_catalog_slug_is_installed_at_most_once() {
        let idx = Index::open_in_memory().unwrap();
        idx.upsert_workflow(&workflow("W1")).unwrap();
        let mut from_catalog = workflow("W2");
        from_catalog.origin = "catalog".into();
        from_catalog.catalog_slug = Some("bug-fix".into());
        idx.upsert_workflow(&from_catalog).unwrap();
        let mut twice = workflow("W3");
        twice.origin = "catalog".into();
        twice.catalog_slug = Some("bug-fix".into());
        assert!(
            idx.upsert_workflow(&twice).is_err(),
            "one install per catalog slug"
        );
        assert_eq!(
            idx.workflow_id_for_slug("bug-fix").unwrap().as_deref(),
            Some("W2")
        );
        let mut half = workflow("W4");
        half.origin = "catalog".into();
        assert!(
            idx.upsert_workflow(&half).is_err(),
            "a catalog workflow names its slug"
        );

        let mut g = goal("01A");
        g.workflow_id = Some("W1".into());
        g.run_id = Some("R1".into());
        g.status = "running".into();
        idx.upsert_goal(&g).unwrap();
        idx.upsert_run(&run("R1", "01A", "W1")).unwrap();
        idx.upsert_work_item(&WorkItemRow {
            id: "I1".into(),
            goal_id: Some("01A".into()),
            project_id: None,
            run_id: Some("R1".into()),
            step_id: Some("build".into()),
            state: "open".into(),
            harness: None,
            assignees: vec![],
            updated_at: 1,
        })
        .unwrap();
        idx.replace_run_steps(
            "R1",
            &[
                RunStepRow {
                    run_id: "R1".into(),
                    step_id: "build".into(),
                    kind: "agent".into(),
                    state: "running".into(),
                    wait_topic: None,
                    due_at: None,
                    work_item: Some("I1".into()),
                    updated_at: 1,
                },
                RunStepRow {
                    run_id: "R1".into(),
                    step_id: "hold".into(),
                    kind: "wait".into(),
                    state: "waiting".into(),
                    wait_topic: Some("deploy.finished".into()),
                    due_at: Some(50),
                    work_item: None,
                    updated_at: 1,
                },
            ],
        )
        .unwrap();
        assert_eq!(idx.goals_using_workflow("W1").unwrap(), vec!["01A"]);
        assert_eq!(idx.runs_for_goal("01A").unwrap().len(), 1);
        assert_eq!(
            idx.goal_of_run("R1").unwrap(),
            Some(Some("01A".to_string()))
        );
        assert_eq!(
            idx.waiting_steps(Some("deploy.finished")).unwrap()[0].step_id,
            "hold"
        );
        assert!(idx.waiting_steps(Some("other.topic")).unwrap().is_empty());
        assert_eq!(idx.due_steps(60).unwrap().len(), 1);
        assert!(idx.due_steps(10).unwrap().is_empty());
        assert_eq!(idx.armed_steps().unwrap().len(), 1);
        // A bad kind or state is refused by the schema.
        assert!(idx
            .replace_run_steps(
                "R1",
                &[RunStepRow {
                    run_id: "R1".into(),
                    step_id: "x".into(),
                    kind: "plan".into(),
                    state: "running".into(),
                    wait_topic: None,
                    due_at: None,
                    work_item: None,
                    updated_at: 1,
                }]
            )
            .is_err());
        // Deleting the goal takes its run, its steps and its items; the
        // workflow stays — a definition is not the goal's.
        idx.delete_goal("01A").unwrap();
        assert!(idx.get_run("R1").unwrap().is_none());
        assert!(idx.armed_steps().unwrap().is_empty());
        assert!(idx.work_items_for("01A").unwrap().is_empty());
        assert!(idx.get_workflow("W1").unwrap().is_some());
        // Deleting a workflow a goal names leaves the goal pointing nowhere.
        let mut g2 = goal("01B");
        g2.workflow_id = Some("W2".into());
        idx.upsert_goal(&g2).unwrap();
        idx.delete_workflow("W2").unwrap();
        assert_eq!(idx.get_goal("01B").unwrap().unwrap().workflow_id, None);
        assert_eq!(idx.list_workflow_ids().unwrap(), vec!["W1"]);
    }

    #[test]
    fn foreign_keys_are_on_and_cascade() {
        let idx = Index::open_in_memory().unwrap();
        assert!(idx.foreign_keys_enabled().unwrap());
        idx.upsert_goal(&goal("01B")).unwrap();
        idx.index_text("01B:x", "01B", "hello world").unwrap();
        idx.add_spend(&HomeKey::Goal("01B".into()), 1, 1, 1, 1)
            .unwrap();
        idx.upsert_project(&test_project_row("P1", "web")).unwrap();
        idx.attach("01B", "P1", 1, "aa").unwrap();
        idx.upsert_work_item(&WorkItemRow {
            id: "W1".into(),
            goal_id: Some("01B".into()),
            project_id: Some("P1".into()),
            run_id: None,
            step_id: None,
            state: "open".into(),
            harness: None,
            assignees: vec![],
            updated_at: 1,
        })
        .unwrap();
        // A child of a goal the index does not hold is a bug, and it says so.
        assert!(idx
            .upsert_work_item(&WorkItemRow {
                id: "W2".into(),
                goal_id: Some("01NOPE".into()),
                project_id: None,
                run_id: None,
                step_id: None,
                state: "open".into(),
                harness: None,
                assignees: vec![],
                updated_at: 1,
            })
            .is_err());
        idx.delete_goal("01B").unwrap();
        assert!(idx.get_goal("01B").unwrap().is_none());
        assert!(idx.search("hello").unwrap().is_empty());
        assert_eq!(
            idx.total_spend(&HomeKey::Goal("01B".into())).unwrap(),
            (0, 0, 0)
        );
        assert!(idx.work_items_for("01B").unwrap().is_empty());
        assert!(idx.goals_of_project("P1").unwrap().is_empty());
        // The project survives: a delete detaches, it never destroys.
        assert_eq!(idx.project_slug("P1").unwrap().as_deref(), Some("web"));
    }

    #[test]
    fn the_permanent_objects_cannot_be_deleted_from_the_cache() {
        let idx = Index::open_in_memory().unwrap();
        idx.upsert_channel(
            "general", "general", "standing", None, "{}", "everyone", "core", 1,
        )
        .unwrap();
        assert!(idx.delete_channel("general").is_err());
        assert!(
            idx.upsert_channel("eng", "eng", "standing", None, "{}", "everyone", "local", 1)
                .is_err(),
            "only general may be rostered everyone"
        );
        for (id, name, pk) in [
            ("general-agent", "General Agent", "pk1"),
            ("workflow-agent", "Workflow Agent", "pk2"),
        ] {
            idx.upsert_agent(id, name, "claude-code", true, pk, "core", 1)
                .unwrap();
            assert!(idx.delete_agent(id).is_err(), "{id}");
        }
    }

    #[test]
    fn clear_empties_every_table_in_fk_order() {
        let idx = Index::open_in_memory().unwrap();
        idx.upsert_goal(&goal("01A")).unwrap();
        idx.upsert_project(&test_project_row("P1", "web")).unwrap();
        idx.attach("01A", "P1", 1, "aa").unwrap();
        idx.mark_seen("e1", 1).unwrap();
        idx.clear().unwrap();
        assert!(idx.list_goals(None).unwrap().is_empty());
        assert!(idx.list_project_ids().unwrap().is_empty());
        assert!(!idx.is_seen("e1").unwrap());
        // Every table the schema creates is in the order list, and vice versa.
        let mut created: Vec<String> = idx
            .conn
            .prepare_cached("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE 'events_fts_%'")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        created.sort();
        let mut listed: Vec<String> = TABLES_IN_FK_ORDER.iter().map(|s| s.to_string()).collect();
        listed.sort();
        assert_eq!(created, listed);
    }

    fn signal_row(id: &str, step: Option<&str>, dedupe: Option<&str>, at: u64) -> SignalRow {
        SignalRow {
            id: id.into(),
            host_kind: step.map(|_| "workspace".to_string()),
            host_id: step.map(|_| "W1".to_string()),
            step: step.map(str::to_string),
            source: "schedule".into(),
            name: None,
            scope_kind: "workspace".into(),
            scope_id: None,
            dedupe_key: dedupe.map(str::to_string),
            state: "queued".into(),
            at,
            attempts: 0,
            payload_json: "{}".into(),
            chain_json: "{}".into(),
            last_error: None,
        }
    }

    /// One occurrence is one signal per listener, whatever wrote it twice;
    /// a signal kept for replay names no listener and is never claimed.
    #[test]
    fn an_occurrence_is_one_signal_per_listener_and_a_replay_record_is_never_claimed() {
        let idx = Index::open_in_memory().unwrap();
        assert!(idx
            .insert_signal(&signal_row(
                "S1",
                Some("nightly"),
                Some("schedule:100"),
                100
            ))
            .unwrap());
        assert!(
            !idx.insert_signal(&signal_row(
                "S2",
                Some("nightly"),
                Some("schedule:100"),
                100
            ))
            .unwrap(),
            "the same occurrence written twice is one row"
        );
        assert!(
            !idx.insert_signal(&signal_row("S1", Some("other"), None, 100))
                .unwrap(),
            "the same id twice is one row"
        );
        assert!(idx
            .insert_signal(&signal_row("S3", Some("other"), Some("schedule:100"), 100))
            .unwrap());
        assert_eq!(
            idx.signal_by_dedupe("workspace", "W1", "nightly", "schedule:100")
                .unwrap()
                .map(|r| r.id),
            Some("S1".to_string())
        );
        let mut replay = signal_row("S4", None, Some("emit:R1:e:3"), 50);
        replay.source = "signal".into();
        replay.name = Some("report.ready".into());
        replay.state = "done".into();
        assert!(idx.insert_signal(&replay).unwrap());
        assert!(
            idx.insert_signal(&SignalRow {
                id: "S5".into(),
                ..replay.clone()
            })
            .unwrap(),
            "a replay record has no listener to dedupe against"
        );
        let mut half = signal_row("S6", Some("x"), None, 1);
        half.host_id = None;
        assert!(
            idx.insert_signal(&half).is_err(),
            "a listener is whole or absent"
        );
        assert_eq!(idx.claim_next_signal(200).unwrap().unwrap().id, "S1");
        assert_eq!(idx.claim_next_signal(200).unwrap().unwrap().id, "S3");
        assert!(
            idx.claim_next_signal(200).unwrap().is_none(),
            "the replay records are never claimed"
        );
        assert_eq!(
            idx.named_signals_since("report.ready", 40, 10)
                .unwrap()
                .len(),
            2
        );
        assert!(idx
            .named_signals_since("report.ready", 60, 10)
            .unwrap()
            .is_empty());
        idx.set_signal_state("S3", "waiting", None, 1, None)
            .unwrap();
        assert_eq!(
            idx.pending_signals_of_listener("workspace", "W1", "other")
                .unwrap()
                .len(),
            1,
            "a signal its guard holds is still its backlog"
        );
        idx.requeue_signal("S3").unwrap();
        assert_eq!(idx.claim_next_signal(201).unwrap().unwrap().id, "S3");
        assert_eq!(
            idx.last_signal_at("workspace", "W1", "nightly").unwrap(),
            Some(100)
        );
        idx.delete_signals_of_host("workspace", "W1").unwrap();
        assert!(idx.get_signal("S1").unwrap().is_none());
        assert!(
            idx.get_signal("S4").unwrap().is_some(),
            "no host, not the host's"
        );
    }

    /// A signal dispatches into one run: the column is unique. The guard
    /// counts a listener's unfinished runs, queued ones included.
    #[test]
    fn one_signal_makes_one_run_and_a_listener_counts_its_live_runs() {
        let idx = Index::open_in_memory().unwrap();
        idx.upsert_workflow(&workflow("W1")).unwrap();
        let from = |id: &str, status: &str, signal: Option<&str>| RunRow {
            id: id.into(),
            scope: "workspace".into(),
            goal_id: None,
            workflow_id: "W1".into(),
            status: status.into(),
            revision: 1,
            queued_at: 1,
            started_at: Some(1),
            finished_at: None,
            listener: Some("workspace:W1/nightly".into()),
            dispatched: signal.map(str::to_string),
        };
        idx.upsert_run(&from("R1", "running", Some("S1"))).unwrap();
        assert!(
            idx.upsert_run(&from("R2", "running", Some("S1"))).is_err(),
            "a second run for one signal is refused"
        );
        idx.upsert_run(&from("R3", "done", Some("S2"))).unwrap();
        idx.upsert_run(&from("R4", "waiting", None)).unwrap();
        assert_eq!(
            idx.run_dispatched_from("S1").unwrap().as_deref(),
            Some("R1")
        );
        assert_eq!(idx.run_dispatched_from("S9").unwrap(), None);
        assert_eq!(
            idx.live_runs_of_listener("workspace:W1/nightly").unwrap(),
            2
        );
        assert_eq!(
            idx.runs_of_listener("workspace:W1/nightly", 10)
                .unwrap()
                .len(),
            3
        );
        let mut orphan = from("R5", "running", Some("S5"));
        orphan.listener = None;
        assert!(
            idx.upsert_run(&orphan).is_err(),
            "a dispatched run names the listener it came from"
        );
    }

    /// Whether a workflow listens is its own column: saving a revision keeps
    /// it; a goal's comes with its row.
    #[test]
    fn listening_survives_a_revision_and_lists_its_hosts() {
        let idx = Index::open_in_memory().unwrap();
        idx.upsert_workflow(&workflow("W1")).unwrap();
        idx.upsert_workflow(&workflow("W2")).unwrap();
        idx.set_workflow_listening("W1", Some(7)).unwrap();
        let mut next = workflow("W1");
        next.revision = 2;
        idx.upsert_workflow(&next).unwrap();
        assert_eq!(idx.workflow_listening_since("W1").unwrap(), Some(7));
        assert_eq!(idx.listening_workflow_ids().unwrap(), vec!["W1"]);
        idx.set_workflow_listening("W1", None).unwrap();
        assert!(idx.listening_workflow_ids().unwrap().is_empty());
        let mut g = goal("01A");
        g.listening_since = Some(9);
        idx.upsert_goal(&g).unwrap();
        idx.upsert_goal(&goal("01B")).unwrap();
        assert_eq!(idx.listening_goal_ids().unwrap(), vec!["01A"]);
        assert_eq!(
            idx.get_goal("01A").unwrap().unwrap().listening_since,
            Some(9)
        );
    }

    #[test]
    fn seen_events_can_be_pruned_by_age() {
        let idx = Index::open_in_memory().unwrap();
        idx.mark_seen("old", 10).unwrap();
        idx.mark_seen("new", 100).unwrap();
        assert_eq!(idx.prune_seen_before(50).unwrap(), 1);
        assert!(!idx.is_seen("old").unwrap());
        assert!(idx.is_seen("new").unwrap());
        assert_eq!(idx.seen_count().unwrap(), 1);
    }

    // added by the coverage pass: index.rs

    // --- the bare lines of the index module ---

    #[test]
    fn a_row_with_neither_goal_nor_run_is_no_home_and_a_session_kind_prints_its_word() {
        assert_eq!(HomeKey::from_columns(None, None), None);
        assert_eq!(
            HomeKey::from_columns(None, Some("r".into())),
            Some(HomeKey::Run("r".into()))
        );
        assert_eq!(SessionKind::Worker.to_string(), "worker");
        assert_eq!(SessionKind::Terminal.to_string(), "terminal");
    }

    /// A cache stamped by another build, or one whose `-wal` sibling cannot
    /// be removed, or one that cannot be made where it is asked for.
    #[test]
    fn a_cache_from_another_build_is_discarded_and_one_that_cannot_be_is_said() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.sqlite");
        let stale = Connection::open(&path).unwrap();
        stale.pragma_update(None, "user_version", 999).unwrap();
        drop(stale);
        let index = Index::open(&path).unwrap();
        assert!(index.fresh, "rebuilt from truth");
        let version: i32 = index
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 0, "stamped only once the rebuild finishes");
        drop(index);
        // The sibling a directory stands in the way of.
        let wal = dir.path().join("index.sqlite-wal");
        std::fs::create_dir(&wal).unwrap();
        let err = Index::open(&path).err().unwrap();
        assert!(
            matches!(&err, StoreError::Io { path, .. } if path.ends_with("index.sqlite-wal")),
            "{err:?}"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let sealed = dir.path().join("sealed");
            std::fs::create_dir(&sealed).unwrap();
            std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o500)).unwrap();
            let err = Index::open(&sealed.join("index.sqlite")).err();
            std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(matches!(err, Some(StoreError::Sqlite(_))), "{err:?}");
        }
    }

    #[test]
    fn workstreams_are_located_by_a_runs_items_and_by_nothing_else_than_the_four_columns() {
        let idx = Index::open_in_memory().unwrap();
        assert!(idx
            .workstream_locators(Some("run_id"), Some("R1"))
            .unwrap()
            .is_empty());
        let err = idx
            .workstream_locators(Some("name"), Some("x"))
            .unwrap_err();
        assert!(matches!(&err, StoreError::Invalid(_)), "{err:?}");
    }

    #[test]
    fn the_sessions_with_a_process_are_listed_and_tags_are_counted_over_every_entity() {
        let idx = Index::open_in_memory().unwrap();
        let session = |id: &str, pid: Option<u32>| SessionRow {
            id: id.into(),
            adapter: "mock".into(),
            kind: SessionKind::Worker,
            work_item: None,
            conversation: None,
            workstream: None,
            agent_id: None,
            transcript_path: None,
            resume_token_json: None,
            status: SessionStatus::Live,
            parked_at: None,
            pid,
            pid_seen_at: pid.map(|_| 5),
            ended_at: None,
        };
        idx.upsert_session(&session("s1", Some(4242))).unwrap();
        idx.upsert_session(&session("s2", None)).unwrap();
        let with = idx.sessions_with_process().unwrap();
        assert_eq!(
            with.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            vec!["s1"]
        );
        assert!(idx.tag_counts(None).unwrap().is_empty());
    }

    #[test]
    fn the_schema_helpers_answer_nothing_for_a_table_or_a_check_that_is_not_there() {
        assert_eq!(table(SCHEMA, "no_such_table"), "");
        assert!(check_values("x IN (", "x IN (").is_empty());
        assert!(check_values("x IN ('a', 'b')", "nowhere").is_empty());
    }

    /// An artifact row the cache holds that names no kind, no scope or no
    /// path this build reads is a read error on the column, never a panic.
    #[test]
    fn an_artifact_row_that_will_not_read_is_an_error_on_its_column() {
        let idx = Index::open_in_memory().unwrap();
        idx.upsert_message(
            "m1", "channel", "c1", "aa", "post", "see", None, None, "[]", None, 1,
        )
        .unwrap();
        idx.conn
            .execute(
                "INSERT INTO artifacts (message_id, ordinal, sha256, name, mime, size, kind, title)
                 VALUES ('m1', 0, 'aa', 'chart.svg', 'image/svg+xml', 1, 'nonsense', 'Chart')",
                [],
            )
            .unwrap();
        let bad_kind = idx.artifacts_for_scope("c1", 10).unwrap_err();
        assert!(matches!(bad_kind, StoreError::Sqlite(_)), "{bad_kind:?}");
        idx.conn
            .execute(
                "UPDATE artifacts SET kind = 'svg', source_scope = 'nowhere', source_id = 'x', source_path = 'a'",
                [],
            )
            .unwrap();
        let bad_scope = idx.artifacts_for_scope("c1", 10).unwrap_err();
        assert!(bad_scope.to_string().contains("nowhere"), "{bad_scope}");
        idx.conn
            .execute(
                "UPDATE artifacts SET source_scope = 'goal', source_path = '/abs'",
                [],
            )
            .unwrap();
        let bad_path = idx.artifacts_for_scope("c1", 10).unwrap_err();
        assert!(bad_path.to_string().contains("/abs"), "{bad_path}");
        idx.conn
            .execute("UPDATE artifacts SET source_path = 'a/b.svg'", [])
            .unwrap();
        let rows = idx.artifacts_for_scope("c1", 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].artifact.source.as_ref().map(|s| s.path.as_str()),
            Some("a/b.svg")
        );
    }
}
