//! The layout rule, as a build failure: **`paths.rs` is the only place that
//! knows where anything goes.** No other source in any crate joins a workspace
//! directory name by hand.
//!
//! This file reads sources; it never writes, deletes or runs anything.

use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    // A folder the guard cannot walk is a rule it does not hold: said,
    // never passed over.
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("the guard cannot read {}: {e}", dir.display()));
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if path.is_dir() {
            if name == "target" || name == "node_modules" || name == ".git" || name == "lab" {
                continue;
            }
            rust_sources(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// The directory and file names `paths.rs` owns. A `join("<name>")` of one of
/// these anywhere else is the drift this test exists to catch.
const OWNED_NAMES: &[&str] = &[
    "goals",
    "projects",
    "channels",
    "agents",
    "teams",
    "skills",
    "mcp",
    "notes",
    "drawings",
    "pets",
    "addons",
    "addon.json",
    "events",
    "listeners",
    "listening",
    "workflows",
    "runs",
    "sessions",
    "conversation",
    "attachments",
    "identity",
    "codehost",
    "github",
    "profiles",
    "run",
    "quarantine",
    "ide",
    "logs",
    "layout",
    "graph",
    "state",
    "workstreams",
    "review",
    "tree",
    "scratch",
    "results",
    "documents",
    "named",
    "recall",
    "journal.jsonl",
    "ledger.jsonl",
    "edges.json",
    "queue.jsonl",
    "index.sqlite",
    "members.json",
    "governance.json",
    "seen.jsonl",
    "machine.json",
    "settings.json",
    "project.json",
    "engine.lock",
    "token",
    "node.sock",
];

/// The crates that can drift: the store and everything that depends on it.
/// A crate with no `bisa-store` dependency (`code host`, `harness`, `vcs`)
/// knows nothing about the workspace, and a name it happens to join —
/// `.claude/skills`, a token under `identity/` in its own tests — is not a
/// workspace path.
fn crates_that_know_the_workspace(root: &std::path::Path) -> Vec<String> {
    let mut out = vec!["bisa-store".to_string()];
    for entry in std::fs::read_dir(root.join("crates"))
        .expect("crates/")
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        let manifest = std::fs::read_to_string(entry.path().join("Cargo.toml")).unwrap_or_default();
        let deps = manifest.split("[dependencies]").nth(1).unwrap_or("");
        let deps = deps.split("\n[").next().unwrap_or("");
        if deps.contains("bisa-store") {
            out.push(name);
        }
    }
    out
}

#[test]
fn no_source_outside_paths_rs_joins_a_workspace_directory_name() {
    let root = workspace_root();
    let knowing = crates_that_know_the_workspace(&root);
    let mut sources = Vec::new();
    rust_sources(&root.join("crates"), &mut sources);
    let mut offences = Vec::new();
    for path in sources {
        let rel = path.strip_prefix(&root).unwrap_or(&path).to_path_buf();
        if rel.ends_with("bisa-store/src/paths.rs") {
            continue;
        }
        // Production code of the crates that hold a workspace. A test may
        // build a path by hand to prove something about it.
        let rel_text = rel.display().to_string();
        let crate_name = rel_text.split('/').nth(1).unwrap_or_default();
        if !knowing.iter().any(|c| c == crate_name) || !rel_text.contains("/src/") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read source");
        let text = match text.find("#[cfg(test)]") {
            Some(at) => text[..at].to_string(),
            None => text,
        };
        for (n, line) in text.lines().enumerate() {
            let l = line.trim_start();
            if l.starts_with("//") {
                continue;
            }
            for name in OWNED_NAMES {
                let needle = format!(".join(\"{name}\")");
                if l.contains(&needle) {
                    offences.push(format!("{}:{}: {needle}", rel.display(), n + 1));
                }
            }
        }
    }
    assert!(
        offences.is_empty(),
        "a workspace path is joined by hand outside paths.rs — add an accessor there:\n{}",
        offences.join("\n")
    );
}

// ---------------------------------------------------------------------------
// The reference tree, held to the accessors
// ---------------------------------------------------------------------------

/// The fenced tree of `docs/reference/workspace-layout.md` — the block under
/// `~/.bisa/` — as root-relative paths. A line's first token is a path joined
/// to its parents by indentation (two spaces a level); every other token of
/// the line that carries a slash is a path named in passing — `agents/state/`,
/// `scratch/.tmp` — kept apart, matched only as a tail.
fn reference_tree() -> (Vec<Vec<String>>, Vec<Vec<String>>) {
    let page = std::fs::read_to_string(workspace_root().join("docs/reference/workspace-layout.md"))
        .expect("read docs/reference/workspace-layout.md");
    let block = page
        .split("```")
        .nth(1)
        .expect("the tree is the page's first fenced block");
    let mut structural: Vec<Vec<String>> = Vec::new();
    let mut loose: Vec<Vec<String>> = Vec::new();
    // (depth, the path at that depth) for every ancestor of the current line.
    let mut stack: Vec<(usize, Vec<String>)> = Vec::new();
    for line in block.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with("~/.bisa/") {
            continue;
        }
        let depth = (line.len() - trimmed.len()) / 2;
        while stack.last().is_some_and(|(d, _)| *d >= depth) {
            stack.pop();
        }
        let mut tokens = trimmed.split_whitespace();
        let first = tokens.next().expect("a path");
        let mut path = stack.last().map(|(_, p)| p.clone()).unwrap_or_default();
        path.extend(segments(first));
        structural.push(path.clone());
        stack.push((depth, path));
        for token in tokens {
            let token = token.trim_matches(|c: char| ",;:()[]".contains(c));
            if token.contains('/') && !token.starts_with("..") && !token.contains("](") {
                loose.push(segments(token));
            }
        }
    }
    assert!(
        structural.len() > 60,
        "the parser found {} paths; the tree moved",
        structural.len()
    );
    (structural, loose)
}

fn segments(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Whether a segment of the tree names a segment of a real path: a literal
/// must be the same word; a placeholder (`<GoalId>`, `*`) names anything, and
/// a mixed one (`33400-<GoalId>.json`, `<host>-<step>.json`) holds the real
/// segment to its literal pieces, in order, at the ends.
fn segment_names(doc: &str, real: &str) -> bool {
    if !doc.contains('<') && !doc.contains('*') {
        return doc == real;
    }
    let mut pieces = Vec::new();
    let mut rest = doc;
    while !rest.is_empty() {
        if let Some(open) = rest.find(['<', '*']) {
            pieces.push(&rest[..open]);
            let after = &rest[open..];
            rest = if after.starts_with('<') {
                &after[after.find('>').map_or(after.len(), |c| c + 1)..]
            } else {
                &after[1..]
            };
        } else {
            pieces.push(rest);
            rest = "";
        }
    }
    // A placeholder at the end names the tail: the last piece is empty.
    if doc.ends_with('>') || doc.ends_with('*') {
        pieces.push("");
    }
    let (first, last) = (pieces[0], *pieces.last().expect("a piece"));
    if !real.starts_with(first) || !real.ends_with(last) || real.len() < first.len() + last.len() {
        return false;
    }
    let middle = if pieces.len() > 2 {
        &pieces[1..pieces.len() - 1]
    } else {
        &[][..]
    };
    let mut at = first.len();
    for piece in middle {
        match real[at..].find(piece) {
            Some(found) => at += found + piece.len(),
            None => return false,
        }
    }
    true
}

fn path_names(doc: &[String], real: &[String]) -> bool {
    doc.len() == real.len() && doc.iter().zip(real).all(|(d, r)| segment_names(d, r))
}

/// Every path `paths.rs` can build, named by its accessor, with sample ids.
fn accessor_paths() -> Vec<(&'static str, PathBuf)> {
    use bisa_core::{
        AccountId, AddonId, AgentId, ConnectorId, ConversationId, DrawingId, GoalId, Home,
        ListenerHost, ListenerKey, McpId, NoteId, RunId, Sha256, SkillId, Slug, TeamId, WorkItemId,
        WorkflowId, WorkstreamId,
    };
    let p = bisa_store::Paths::new("/ws");
    let ulid = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    let goal: GoalId = ulid.parse().unwrap();
    let run: RunId = ulid.parse().unwrap();
    let item: WorkItemId = ulid.parse().unwrap();
    let wf: WorkflowId = ulid.parse().unwrap();
    let conversation: ConversationId = ulid.parse().unwrap();
    let workstream: WorkstreamId = ulid.parse().unwrap();
    let note: NoteId = ulid.parse().unwrap();
    let drawing: DrawingId = ulid.parse().unwrap();
    let account: AccountId = ulid.parse().unwrap();
    let agent: AgentId = "sample".parse().unwrap();
    let team: TeamId = "sample".parse().unwrap();
    let skill: SkillId = "sample".parse().unwrap();
    let mcp: McpId = "sample".parse().unwrap();
    let connector: ConnectorId = "sample".parse().unwrap();
    let addon: AddonId = "sample".parse().unwrap();
    let slug = Slug::new("shelf").unwrap();
    let sha: Sha256 = "ab".repeat(32).parse().unwrap();
    let key = ListenerKey {
        host: ListenerHost::Goal { goal },
        step: "ticket".parse().unwrap(),
    };
    let goal_home = p.home(&Home::Goal { goal });
    let run_home = p.home(&Home::Run { run });
    let g = p.goal(goal);
    let pr = p.project(&slug);
    let a = p.agent(&agent);
    vec![
        ("goals_dir", p.goals_dir()),
        ("projects_dir", p.projects_dir()),
        ("agents_dir", p.agents_dir()),
        ("teams_dir", p.teams_dir()),
        ("skills_dir", p.skills_dir()),
        ("mcp_dir", p.mcp_dir()),
        ("connectors_dir", p.connectors_dir()),
        ("connector_file", p.connector_file(&connector)),
        (
            "connector_accounts_dir",
            p.connector_accounts_dir(&connector),
        ),
        (
            "connector_account_file",
            p.connector_account_file(&connector, account),
        ),
        ("notes_dir", p.notes_dir()),
        ("drawings_dir", p.drawings_dir()),
        ("pets_dir", p.pets_dir()),
        ("addons_dir", p.addons_dir()),
        ("addon_dir", p.addon_dir(&addon)),
        ("addon_record", p.addon_record(&addon)),
        ("addon_files_dir", p.addon_files_dir(&addon)),
        ("events_dir", p.events_dir()),
        ("workflows_dir", p.workflows_dir()),
        ("workspace_runs_dir", p.workspace_runs_dir()),
        ("listening_dir", p.listening_dir()),
        ("listening_file", p.listening_file(wf)),
        ("sessions_dir", p.sessions_dir()),
        ("conversation_dir", p.conversation_dir()),
        ("attachments_dir", p.attachments_dir()),
        ("attachments_named_dir", p.attachments_named_dir()),
        ("identity_dir", p.identity_dir()),
        ("codehost_tokens_root", p.codehost_tokens_root()),
        ("git_profiles_dir", p.git_profiles_dir()),
        ("run_dir", p.run_dir()),
        ("ide_dir", p.ide_dir()),
        ("logs_dir", p.logs_dir()),
        ("index_db", p.index_db()),
        ("members_file", p.members_file()),
        ("governance_file", p.governance_file()),
        ("invites_file", p.invites_file()),
        ("held_file", p.held_file()),
        ("seen_file", p.seen_file()),
        ("signal_queue", p.signal_queue()),
        ("listeners_dir", p.listeners_dir()),
        ("listener_runtime_file", p.listener_runtime_file(&key)),
        ("listener_scratch_dir", p.listener_scratch_dir(&key)),
        ("machine_settings", p.machine_settings()),
        ("workspace_settings", p.workspace_settings()),
        ("token_file", p.token_file()),
        ("engine_lock", p.engine_lock()),
        ("node_socket", p.node_socket()),
        ("quarantine_dir", p.quarantine_dir()),
        ("terminals_dir", p.terminals_dir()),
        (
            "ide_layout",
            p.ide_layout(bisa_core::FileScope::Workstream, ulid),
        ),
        ("changes_dir", p.changes_dir(conversation)),
        ("change_ledger", p.change_ledger(conversation)),
        ("change_blob", p.change_blob(conversation, &sha)),
        ("change_blobs_dir", p.change_blobs_dir(conversation)),
        ("change_index", p.change_index(conversation)),
        ("agent_file", p.agent_file(&agent)),
        ("team_file", p.team_file(&team)),
        ("skill_file", p.skill_file(&skill)),
        ("mcp_file", p.mcp_file(&mcp)),
        ("pet_dir", p.pet_dir("moonrice").unwrap()),
        ("pet_manifest", p.pet_manifest("moonrice").unwrap()),
        (
            "session_meta_file",
            p.session_meta_file("claude-code", ulid).unwrap(),
        ),
        ("conversation_log", p.conversation_log("general").unwrap()),
        ("attachment", p.attachment(&"ab".repeat(32)).unwrap()),
        (
            "attachment_named",
            p.attachment_named(&"ab".repeat(32), "brief.pdf").unwrap(),
        ),
        ("library_workflow_snapshot", p.library_workflow_snapshot(wf)),
        (
            "state_dir(goal)",
            p.state_dir(&bisa_store::Paths::ns_goal(goal)),
        ),
        (
            "state_dir(run)",
            p.state_dir(&bisa_store::Paths::ns_run(run)),
        ),
        (
            "state_dir(channels)",
            p.state_dir(bisa_store::Paths::NS_CHANNELS),
        ),
        (
            "state_dir(connectors)",
            p.state_dir(bisa_store::Paths::NS_CONNECTORS),
        ),
        (
            "state_dir(conversations)",
            p.state_dir(bisa_store::Paths::NS_CONVERSATIONS),
        ),
        (
            "state_dir(projects)",
            p.state_dir(bisa_store::Paths::NS_PROJECTS),
        ),
        (
            "state_dir(workflows)",
            p.state_dir(bisa_store::Paths::NS_WORKFLOWS),
        ),
        (
            "state_dir(agents)",
            p.state_dir(bisa_store::Paths::NS_AGENTS),
        ),
        (
            "state_dir(addons)",
            p.state_dir(bisa_store::Paths::NS_ADDONS),
        ),
        (
            "state_dir(drawings)",
            p.state_dir(bisa_store::Paths::NS_DRAWINGS),
        ),
        ("home(goal).journal", goal_home.journal()),
        ("home(goal).state", goal_home.state()),
        ("home(goal).run_snapshot", goal_home.run_snapshot(run)),
        ("home(goal).ledger", goal_home.ledger()),
        ("home(goal).results", goal_home.results()),
        ("home(goal).result", goal_home.result(item)),
        ("home(goal).scratch", goal_home.scratch()),
        ("home(goal).tmp", goal_home.tmp()),
        ("home(run).dir", run_home.dir().to_path_buf()),
        ("home(run).journal", run_home.journal()),
        ("home(run).run_snapshot", run_home.run_snapshot(run)),
        ("home(run).ledger", run_home.ledger()),
        ("home(run).result", run_home.result(item)),
        ("home(run).scratch", run_home.scratch()),
        ("goal.workflow_snapshot", g.workflow_snapshot(wf)),
        ("goal.edges", g.edges()),
        ("goal.documents", g.documents()),
        ("goal.document", g.document("brief.pdf")),
        ("project.record", pr.record()),
        ("project.settings", pr.settings()),
        ("project.tree", pr.tree()),
        ("project.review", pr.review()),
        ("project.review_note", pr.review_note(note)),
        ("project.workstreams", pr.workstreams()),
        (
            "project.workstream_record",
            pr.workstream_record(workstream),
        ),
        ("project.workstream_dir", pr.workstream_dir(workstream)),
        ("agent.dir", a.dir().to_path_buf()),
        ("agent.scratch", a.scratch()),
        ("agent.recall_index", a.recall_index()),
        (
            "drawing_file_in",
            bisa_store::Paths::drawing_file_in(&p.drawings_dir().join("workspace"), drawing),
        ),
        (
            "note_file_in",
            bisa_store::Paths::note_file_in(&p.notes_dir().join("workspace"), note),
        ),
    ]
}

fn relative(path: &Path) -> Vec<String> {
    segments(
        path.strip_prefix("/ws")
            .unwrap_or_else(|_| panic!("{} is not under the root", path.display()))
            .to_str()
            .expect("utf-8"),
    )
}

/// **Every path the store builds is in the reference tree.** An accessor
/// whose path the tree does not draw — as a line of its own, as the folder
/// a drawn path sits in, or named in passing on a line — is a layout the
/// page does not tell.
#[test]
fn the_reference_tree_names_every_path_paths_rs_joins() {
    let (structural, loose) = reference_tree();
    let mut missing = Vec::new();
    for (name, path) in accessor_paths() {
        let real = relative(&path);
        // Drawn whole, or the folder of something drawn under it.
        let drawn = structural
            .iter()
            .chain(&loose)
            .any(|doc| doc.len() >= real.len() && path_names(&doc[..real.len()], &real));
        // Named in passing, as the tail of the real path.
        let named = loose
            .iter()
            .any(|doc| doc.len() <= real.len() && path_names(doc, &real[real.len() - doc.len()..]));
        if !drawn && !named {
            missing.push(format!("{name}: {}", real.join("/")));
        }
    }
    assert!(
        missing.is_empty(),
        "docs/reference/workspace-layout.md does not draw these paths:\n{}",
        missing.join("\n")
    );
}

/// Paths the tree draws that no accessor builds, because another crate owns
/// them — each with the source that joins it, so a fourth cannot slip in.
const DRAWN_ELSEWHERE: &[(&str, &str)] = &[
    (
        "harnesses/*.json",
        "crates/bisa-adapters: the custom harness descriptors",
    ),
    (
        "hosts/<host-pubkey>",
        "crates/bisa-guest: a joined workspace's replica",
    ),
    (
        "net_published.jsonl",
        "crates/bisa-net/src/host.rs: the host's pump ledger",
    ),
    (
        "run/desktop-node.json",
        "desktop/src-tauri/src/sidecar.rs: the desktop's node",
    ),
    (
        "run/interactive/<session>",
        "crates/bisa-engine: a terminal harness's reporter files",
    ),
    (
        "logs/<process>/<process>.<period>.jsonl",
        "crates/bisa-log: one folder per process",
    ),
    (
        "logs/crashes/<process>.<stamp>.<pid>.json",
        "crates/bisa-log: the crash reports",
    ),
    (
        "logs/runs/<process>.<pid>.json",
        "crates/bisa-log: a run's marker",
    ),
];

/// **Every path the tree draws is one the store builds** — itself, or a file
/// under a directory it builds (the file names the store's modules format
/// under `state/`, `codehost/`, `git/`) — or one another crate owns, named
/// with its source in `DRAWN_ELSEWHERE`. A path the tree draws that nobody
/// joins is a layout that moved.
#[test]
fn every_path_the_reference_tree_draws_is_built_by_somebody() {
    let (structural, _) = reference_tree();
    let built: Vec<Vec<String>> = accessor_paths()
        .into_iter()
        .map(|(_, path)| relative(&path))
        .collect();
    let elsewhere: Vec<Vec<String>> = DRAWN_ELSEWHERE
        .iter()
        .map(|(path, _)| segments(path))
        .collect();
    let mut unknown = Vec::new();
    for doc in &structural {
        let known = (1..=doc.len()).rev().any(|k| {
            built.iter().any(|real| path_names(&doc[..k], real))
                || elsewhere.iter().any(|owned| owned == &doc[..k])
        });
        if !known {
            unknown.push(doc.join("/"));
        }
    }
    assert!(
        unknown.is_empty(),
        "the tree draws paths nobody joins — moved, or an owner missing from DRAWN_ELSEWHERE:\n{}",
        unknown.join("\n")
    );
    // The segment matcher, at its edges.
    assert!(segment_names(
        "33400-<GoalId>.json",
        "33400-01ARZ3NDEKTSV4RRFFQ69G5FAV.json"
    ));
    assert!(!segment_names(
        "33400-<GoalId>.json",
        "33413-01ARZ3NDEKTSV4RRFFQ69G5FAV.json"
    ));
    assert!(segment_names(
        "<host>-<step>.json",
        "goal-01ARZ-ticket.json"
    ));
    assert!(!segment_names("<host>-<step>.json", "goal01ARZticket.json"));
    assert!(segment_names("*.json", "custom.json") && !segment_names("*.json", "custom.toml"));
    assert!(segment_names("<2 hex>", "ab") && !segment_names("tree", "trees"));
}
