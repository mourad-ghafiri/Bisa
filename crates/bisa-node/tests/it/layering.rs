//! Layering rule 2 (`docs/architecture/07-layering.md`), as a build failure:
//! **the node reads the store; every mutation goes through the engine.**
//!
//! The list of writers is not spelled here — it is derived from the store's
//! own `impl Workspace` blocks at test time, by the verb a method's name
//! starts with, so a new store writer is denied in the node the moment it
//! exists. Every method must be classified as a reader or a writer; one that
//! is neither fails the test until somebody says which it is.
//!
//! This file reads sources; it never writes, deletes or runs anything.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn sources(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
    for entry in fs::read_dir(dir)
        .expect("read a source directory")
        .flatten()
    {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let text = fs::read_to_string(&path).expect("read a source file");
            out.push((path, text));
        }
    }
}

/// A method whose name starts with one of these verbs changes the workspace.
const WRITER_VERBS: &[&str] = &[
    "accept",
    "add",
    "append",
    "apply",
    "attach",
    "claim",
    "clear",
    "create",
    "delete",
    "detach",
    "edit",
    "enqueue",
    "ensure",
    "finish",
    "forget",
    "freeze",
    // `hold_message` / `release_message`: the classifier's hold and its lifting.
    "hold",
    "release",
    "import",
    "ingest",
    "install",
    "mark",
    // `move_signal`: a queued signal settled, held or put back.
    "move",
    "park",
    "post",
    "promote",
    "prune",
    "put",
    "react",
    "rebuild",
    "reconcile",
    "record",
    "remove",
    // `rename_conversation`, `revoke_invite`, `settle_invite`,
    // `start_queued_run`, `unroster_human_everywhere`.
    "rename",
    "requeue",
    "retract",
    "revoke",
    "rotate",
    "save",
    "set",
    "settle",
    "start",
    "transition",
    "unroster",
    "unset",
    "update",
    "write",
];

/// Writers whose first word is not a writing verb.
const IRREGULAR_WRITERS: &[&str] = &[
    "open_dm",
    "recall_store",
    "resolve_review_note",
    // `journal_signal` files the occurrence that began a run on its home.
    "journal_signal",
    "end_session",
    // `end_session_keeping_process` ends the row and keeps its pid for the
    // next boot to end the process.
    "end_session_keeping_process",
    // `end_orphan_runs` ends every run no goal names — the engine's repair at
    // its start, under its lock; a writer by what it does, not by its verb.
    "end_orphan_runs",
    // `admit_claimed` adds the member a claimed invite names.
    "admit_claimed",
    // `drop_held_of` lets a removed person's held messages go.
    "drop_held_of",
];

/// A method whose name starts with one of these words only reads.
const READER_VERBS: &[&str] = &[
    "activity",
    "connector",
    "default",
    "known",
    "parse",
    "workflow",
    "assignee",
    "assignees",
    // `staff_scope`: the agents and teams a goal's design may name, read.
    "staff",
    "attachment",
    // `change_ledger`, `change_blob`, `change_index_file`: what a conversation's
    // agent changed, read; its writers are `write_change_ledger` and
    // `put_change_blob`.
    "change",
    // `validate_workflow` writes nothing: it is the pure check a route runs
    // on a draft mid-edit; `refuse_channel_delete_if_used` is the answer a
    // delete would give, asked before anything is stopped or removed.
    "validate",
    "refuse",
    "attachments",
    "audience",
    "budget",
    "catalog",
    "channel",
    "channels",
    "checkout",
    // `conversation_event`, `conversation_of_scope`, `conversation_row`:
    // one record, or the record a scope names, read.
    "conversation",
    // The held messages and their reasons, the invitations, the live run,
    // a member and their role, the people, the queue's facts, a transcript.
    "held",
    "invite",
    "invites",
    // `latest_messages`: each scope's newest live post, what a list of
    // channels says was last said.
    "latest",
    "live",
    "member",
    "next",
    "people",
    // `problems`: what the open and the rebuild worked around, read.
    "problems",
    "queued",
    "transcript",
    "decisions",
    "dump",
    "edges",
    "expand",
    "file",
    "gate",
    "get",
    "goal",
    "goals",
    "governance",
    "guards",
    // `has_hook_secret`, `hook_secret`: whether a public hook has its secret,
    // and the secret the public door compares a caller's against.
    "has",
    "hook",
    // `home_of_work_item`: the goal or the run of the workspace an item is
    // filed at, read.
    "home",
    "ids",
    "is",
    "journal",
    // `last_signal_at`: when a listener last heard its event.
    "last",
    "list",
    // `listener_runtime`, `listening`: what a listener's ticker remembers,
    // and what a host listens with.
    "listener",
    "listening",
    "mcp",
    "members",
    "mentioned",
    "mentions",
    "messages",
    // `named_signals_since`: the named signals a re-armed wait replays.
    "named",
    "open",
    "owner",
    "paths",
    // `pending_signals`: a listener's backlog.
    "pending",
    "pet",
    "placement",
    "primary",
    "project",
    "projects",
    "read",
    "recall",
    "resolve",
    "root",
    // `runs_of_listener`: the runs one listener began.
    "runs",
    "scope",
    "search",
    "session",
    "setting",
    "settings",
    "signal",
    "signer",
    "skill",
    "spent",
    "subscribe",
    "tag",
    "tags",
    "team",
    "unread",
    "usage",
    "work",
    // `workspace_runs`: a workflow's runs of the workspace, read.
    "workspace",
    "workstream",
];

/// How a route holds the store: `ws` after `let ws = state.engine.workspace()`,
/// the accessor inline, or the engine's own `inner.ws`. The engine's methods
/// of the same name (`state.engine.start_run`) are exactly what a route
/// *should* call.
const STORE_RECEIVERS: &[&str] = &["ws.", "workspace().", ".ws."];

/// Every `pub fn` declared inside an `impl Workspace` block in the store.
fn workspace_methods() -> BTreeSet<String> {
    let store = Path::new(env!("CARGO_MANIFEST_DIR")).join("../bisa-store/src");
    let mut files = Vec::new();
    sources(&store, &mut files);
    let mut methods = BTreeSet::new();
    for (_, text) in &files {
        let mut depth = 0i32;
        let mut in_impl = false;
        for line in text.lines() {
            let code = line.split("//").next().unwrap_or("");
            let head = code.trim_start();
            if depth == 0
                && (head.starts_with("impl Workspace {") || head.starts_with("impl Workspace<"))
            {
                in_impl = true;
            }
            if in_impl && depth == 1 {
                if let Some(rest) = code.trim_start().strip_prefix("pub fn ") {
                    let name: String = rest
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    methods.insert(name);
                }
            }
            depth += code.matches('{').count() as i32;
            depth -= code.matches('}').count() as i32;
            if depth == 0 {
                in_impl = false;
            }
        }
    }
    methods
}

fn first_word(name: &str) -> &str {
    name.split('_').next().unwrap_or(name)
}

#[test]
fn every_workspace_method_is_classified() {
    let unclassified: Vec<String> = workspace_methods()
        .into_iter()
        .filter(|m| {
            !IRREGULAR_WRITERS.contains(&m.as_str())
                && !WRITER_VERBS.contains(&first_word(m))
                && !READER_VERBS.contains(&first_word(m))
        })
        .collect();
    assert!(
        unclassified.is_empty(),
        "classify these Workspace methods as readers or writers in crates/bisa-node/tests/it/layering.rs:\n{}",
        unclassified.join("\n")
    );
}

#[test]
fn the_node_writes_nothing_to_the_store() {
    let writers: Vec<String> = workspace_methods()
        .into_iter()
        .filter(|m| {
            IRREGULAR_WRITERS.contains(&m.as_str()) || WRITER_VERBS.contains(&first_word(m))
        })
        .collect();
    for historic in [
        "create_project",
        "update_project",
        "delete_project",
        "put_workstream",
        "update_workstream",
        "transition_workstream",
        "delete_workstream",
        "record_session",
        "record_run_event",
        "create_run",
        "transition_work_item",
    ] {
        assert!(
            writers.iter().any(|w| w == historic),
            "{historic} is a store writer and must be derived; the parser is broken"
        );
    }

    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&src, &mut files);
    assert!(!files.is_empty(), "the node has sources");

    let mut offences = Vec::new();
    for (path, text) in &files {
        // A call may break across lines (`.workspace()` / `.remove_pet(`), so
        // each line is read together with the one before it.
        let mut previous = String::new();
        // A `#[cfg(test)] mod` is a fixture, not a route: its braces are
        // counted so the whole module is skipped, whatever it calls.
        let mut depth = 0i32;
        let mut test_module_from: Option<i32> = None;
        let mut after_cfg_test = false;
        for (n, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("").trim().to_string();
            if code == "#[cfg(test)]" {
                after_cfg_test = true;
                previous = code;
                continue;
            }
            if after_cfg_test && code.starts_with("mod ") && test_module_from.is_none() {
                test_module_from = Some(depth);
            }
            after_cfg_test = false;
            depth += code.matches('{').count() as i32 - code.matches('}').count() as i32;
            if let Some(from) = test_module_from {
                if depth <= from {
                    test_module_from = None;
                }
                previous = code;
                continue;
            }
            let window = format!("{previous}{code}");
            for writer in &writers {
                for receiver in STORE_RECEIVERS {
                    if window.contains(&format!("{receiver}{writer}("))
                        && code.contains(&format!("{writer}("))
                    {
                        offences.push(format!("{}:{}: {}", path.display(), n + 1, line.trim()));
                    }
                }
            }
            previous = code;
        }
    }
    assert!(
        offences.is_empty(),
        "the node must not call a store writer; route it through the engine:\n{}",
        offences.join("\n")
    );
}

/// How the command line holds the store: `ws` after `let ws = ctx.workspace()?`,
/// the accessor inline, a field of its own.
const CLI_STORE_RECEIVERS: &[&str] = &["ws.", "workspace().", ".ws.", "workspace."];

/// The words that say a function of the command line asked for the node
/// before it wrote: the client it goes through when one runs, or the door
/// of the git setup, which opens on the node first.
const ASKS_THE_NODE: &[&str] = &["node_client(", "Door::open("];

/// Functions of the command line that write the store without asking for
/// the node, each with why that is right — `<file>:<function>`. An excuse
/// nothing needs any more fails, so the list cannot outlive its reasons.
const WRITES_WITH_NO_NODE_ASKED: &[(&str, &str)] = &[
    (
        "main.rs:store_documents",
        "`put_attachment` again: a goal's documents are blobs named by their hash, held \
         before the goal that names them is captured — through the node when one runs",
    ),
    (
        "reindex.rs:reindex",
        "refuses by the engine's lock while a node holds the workspace, then rebuilds \
         with nobody else writing",
    ),
    (
        "studio.rs:post_message",
        "`put_attachment` is a blob named by its hash, written once and atomically — no \
         record, no index row; the message that names it goes through `say`, which asks the node",
    ),
    (
        "workflow.rs:resolve_workflow",
        "installs a template on the way only for `use` and `new`, each of which asked \
         for the node first and found none",
    ),
];

/// **One engine holds a workspace** (I40), so the command line writes the
/// store only where no node runs: a verb that changes something asks for
/// the node first and goes through it. A record is rewritten whole, and a
/// write beside the node's would lose one of the two — and nobody listening
/// to the node would hear of it. The store's writers are derived, as above;
/// every function of the command line that calls one must ask for the node
/// before it does, or be excused by name with its reason.
///
/// The check is by function, not by branch: it finds the verb that never
/// asks, which is the fault this guard was written after (thirty of them);
/// that the write then sits on the side where no node answered is the
/// journeys' to show — each runs its verbs while a node runs and reads a
/// refusal for a lock as a failure.
#[test]
fn the_command_line_asks_for_the_node_before_it_writes_the_store() {
    let writers: Vec<String> = workspace_methods()
        .into_iter()
        .filter(|m| {
            IRREGULAR_WRITERS.contains(&m.as_str()) || WRITER_VERBS.contains(&first_word(m))
        })
        .collect();
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../bisa-cli/src");
    let mut files = Vec::new();
    sources(&src, &mut files);
    assert!(
        files.len() > 20,
        "the command line has sources: {}",
        files.len()
    );

    /// The name of the function a line opens, when it opens one at the top
    /// of a file or of an `impl` — a closure or a nested helper is part of
    /// the function it stands in.
    fn opens(line: &str) -> Option<String> {
        let indent = line.len() - line.trim_start().len();
        if indent > 4 {
            return None;
        }
        let mut rest = line.trim_start();
        for word in ["pub(crate) ", "pub(super) ", "pub ", "async "] {
            rest = rest.strip_prefix(word).unwrap_or(rest);
        }
        let name: String = rest
            .strip_prefix("fn ")?
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        (!name.is_empty()).then_some(name)
    }

    let mut offences = Vec::new();
    let mut excused: BTreeSet<String> = BTreeSet::new();
    let mut sites = 0;
    for (path, text) in &files {
        let file = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut previous = String::new();
        // The function the walk is in, and whether it has asked yet.
        let mut function: Option<(String, bool)> = None;
        // A `#[cfg(test)] mod … {` is a fixture: its braces are counted so
        // the whole module is skipped. One declared by name alone
        // (`#[cfg(test)] mod verbs;`) has no body here and skips nothing —
        // read as the end of the file, it once hid everything below it.
        let mut depth = 0i32;
        let mut test_module_from: Option<i32> = None;
        let mut after_cfg_test = false;
        for (n, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            if code.trim() == "#[cfg(test)]" {
                after_cfg_test = true;
                previous = code.trim().to_string();
                continue;
            }
            let opens_test_module = after_cfg_test
                && code.trim_start().starts_with("mod ")
                && code.contains('{')
                && test_module_from.is_none();
            if opens_test_module {
                test_module_from = Some(depth);
            }
            after_cfg_test = false;
            depth += code.matches('{').count() as i32 - code.matches('}').count() as i32;
            if let Some(from) = test_module_from {
                if depth <= from {
                    test_module_from = None;
                }
                previous = code.trim().to_string();
                continue;
            }
            if let Some(name) = opens(code) {
                function = Some((name, false));
            }
            if let Some((_, asked)) = function.as_mut() {
                *asked |= ASKS_THE_NODE.iter().any(|word| code.contains(word));
            }
            let trimmed = code.trim();
            let window = format!("{previous}{trimmed}");
            let writes = writers.iter().any(|writer| {
                trimmed.contains(&format!("{writer}("))
                    && CLI_STORE_RECEIVERS
                        .iter()
                        .any(|receiver| window.contains(&format!("{receiver}{writer}(")))
            });
            if writes {
                sites += 1;
                let (name, asked) = function.clone().unwrap_or_default();
                let place = format!("{file}:{name}");
                if !asked {
                    if WRITES_WITH_NO_NODE_ASKED
                        .iter()
                        .any(|(who, _)| *who == place)
                    {
                        excused.insert(place);
                    } else {
                        offences.push(format!(
                            "{}:{} in `{name}`: {trimmed}",
                            path.display(),
                            n + 1
                        ));
                    }
                }
            }
            previous = trimmed.to_string();
        }
    }
    assert!(
        sites > 20,
        "the walk found {sites} store writes in the command line; it is broken"
    );
    assert!(
        offences.is_empty(),
        "a verb writes the store and never asks for the node — go through the node when one \
         runs (`ctx.node_client()`), or say why not in WRITES_WITH_NO_NODE_ASKED:\n{}",
        offences.join("\n")
    );
    let idle: Vec<&str> = WRITES_WITH_NO_NODE_ASKED
        .iter()
        .map(|(who, _)| *who)
        .filter(|who| !excused.contains(*who))
        .collect();
    assert!(
        idle.is_empty(),
        "excused, and either gone or asking for the node after all: {idle:?}"
    );
    for (who, why) in WRITES_WITH_NO_NODE_ASKED {
        assert!(!why.trim().is_empty(), "{who} is excused with no reason");
    }
}

/// **One extractor reads every body** (`Body<T>` in `lib.rs`): a body that
/// does not fit is the documented 400 in the error body's shape on every
/// route. A handler that takes the framework's own `Json` as an argument
/// answers a 422 in prose instead — the four that did were found by a test
/// that read the status; this finds the next one by its signature. `Json` as
/// what a handler *answers* is what it is for.
#[test]
fn every_handler_reads_its_body_through_the_one_extractor() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&src, &mut files);
    assert!(files.len() > 20, "the node has sources: {}", files.len());
    let mut offences = Vec::new();
    for (path, text) in &files {
        // The extractor itself is built on the framework's.
        if path.file_name().is_some_and(|name| name == "lib.rs") {
            continue;
        }
        for (n, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("").trim();
            let argument = code.contains("): Json<")
                || code.contains(": Option<Json<")
                || code.contains(": axum::Json<")
                || code.contains(": Option<axum::Json<");
            if argument {
                offences.push(format!("{}:{}: {code}", path.display(), n + 1));
            }
        }
    }
    assert!(
        offences.is_empty(),
        "read the body through `crate::Body<T>` (or `Option<crate::Body<T>>`):\n{}",
        offences.join("\n")
    );
}

/// **One reader for every query string** (`Query<T>` in `lib.rs`), for the
/// same reason: a cursor that is no number is a 400 in the error body's
/// shape, never the framework's sentence in prose. A module that brings the
/// framework's `Query` into scope, or names it in full, is found here.
/// `tags.rs` reads the pairs of `?tag=` itself and says its own refusal.
#[test]
fn every_handler_reads_its_query_through_the_one_reader() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&src, &mut files);
    let mut offences = Vec::new();
    let mut readers = 0;
    for (path, text) in &files {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if name == "lib.rs" || name == "tags.rs" {
            continue;
        }
        readers += usize::from(text.contains("use crate::Query;"));
        for (n, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("").trim();
            let brought_in = code.starts_with("use axum::extract::")
                && code
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .any(|word| word == "Query");
            if brought_in || code.contains("axum::extract::Query") {
                offences.push(format!("{}:{}: {code}", path.display(), n + 1));
            }
        }
    }
    assert!(readers > 20, "the scan reads the modules: {readers}");
    assert!(
        offences.is_empty(),
        "read the query through `crate::Query<T>`:\n{}",
        offences.join("\n")
    );
}

/// A type as its source declares it: where, under which attributes, and
/// whether it has fields that carry a name.
struct Declared {
    at: String,
    name: String,
    attributes: String,
    named_fields: bool,
}

impl Declared {
    fn is_read(&self) -> bool {
        self.attributes.contains("Deserialize")
    }

    fn refuses_what_it_does_not_know(&self) -> bool {
        self.attributes.contains("deny_unknown_fields")
    }
}

/// Every `struct` and `enum` of `files`. The attributes are the lines above
/// the head, as far up as they are attributes or comments; an enum has named
/// fields when one of its variants does.
fn declared(files: &[(PathBuf, String)]) -> Vec<Declared> {
    let mut all = Vec::new();
    for (path, text) in files {
        let lines: Vec<&str> = text.lines().collect();
        for (n, line) in lines.iter().enumerate() {
            let head = line
                .trim_start()
                .trim_start_matches("pub(crate) ")
                .trim_start_matches("pub ");
            let (is_enum, rest) = match (head.strip_prefix("struct "), head.strip_prefix("enum ")) {
                (Some(rest), _) => (false, rest),
                (_, Some(rest)) => (true, rest),
                _ => continue,
            };
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if name.is_empty() {
                continue;
            }
            let mut above = Vec::new();
            for earlier in lines[..n].iter().rev() {
                let word = earlier.trim();
                // An attribute, a comment, or the inside of an attribute
                // that runs over several lines.
                let of_the_block = word.starts_with("#[")
                    || word.starts_with("//")
                    || word.starts_with(")]")
                    || (word.ends_with(',') && !word.contains(':') && !word.contains('('));
                if !of_the_block {
                    break;
                }
                above.push(word);
            }
            let named_fields = if is_enum {
                let mut depth = 0usize;
                let mut found = false;
                for inner in &lines[n..] {
                    let code = inner.split("//").next().unwrap_or("");
                    // A brace opened inside the enum's own is a variant's.
                    found |= depth == 1 && code.contains('{') && n != 0 && !code.contains("enum ");
                    depth += code.matches('{').count();
                    depth = depth.saturating_sub(code.matches('}').count());
                    if depth == 0 && code.contains('}') {
                        break;
                    }
                }
                found
            } else {
                rest.contains('{')
            };
            all.push(Declared {
                at: format!("{}:{}", path.display(), n + 1),
                name,
                attributes: above.join("\n"),
                named_fields,
            });
        }
    }
    all
}

/// Types the node reads that are no body of a route of its own, each with
/// the reason it takes what it does not know.
const READ_AND_NO_BODY: &[(&str, &str)] = &[
    (
        "PulseMessage",
        "a row of the activity index typed back: the store's shape",
    ),
    (
        "SendParams",
        "A2A's `message/send`: another protocol's message",
    ),
    ("IncomingMessage", "A2A's message: another protocol's"),
    (
        "TaskIdParams",
        "A2A's `tasks/get` and `tasks/cancel`: another protocol's",
    ),
];

/// **A body refuses a key nobody knows.** `POST /goals` with `mode` misspelt
/// captured an auto goal; a supersession sent under the journal's keys closed
/// a goal as abandoned. Every type a handler reads through `Body<T>` — where
/// ever it is declared — and every type the node declares to read carries
/// `deny_unknown_fields`, so a key that is not the route's is the documented
/// 400, by name.
///
/// A query string is not held to it: it is one string with several readers —
/// the token, the tags, the handler's own keys — and none of them can tell a
/// key nobody knows from a key that is another's. What a body *holds* of the
/// core's own records keeps the core's rule for what is stored.
#[test]
fn every_body_refuses_a_key_nobody_knows() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crates folder")
        .to_path_buf();
    let mut of_the_node = Vec::new();
    sources(&crates.join("bisa-node/src"), &mut of_the_node);
    let mut of_the_rest = Vec::new();
    for entry in fs::read_dir(&crates).expect("read the crates").flatten() {
        let src = entry.path().join("src");
        if src.is_dir() && entry.file_name() != "bisa-node" {
            sources(&src, &mut of_the_rest);
        }
    }
    let node = declared(&of_the_node);
    let rest = declared(&of_the_rest);

    // What is read as a body, and what is read as a query, by name.
    let mut bodies = BTreeSet::new();
    let mut queries = BTreeSet::new();
    for (_, text) in &of_the_node {
        for (marker, into) in [("Body<", &mut bodies), ("Query<", &mut queries)] {
            for after in text.split(marker).skip(1) {
                let named: String = after
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
                    .collect();
                let name = named.rsplit("::").next().unwrap_or_default().to_string();
                // `T` is the extractor's own parameter; a `Value` has no keys
                // of its own to know.
                if !name.is_empty() && name != "T" && name != "Value" {
                    into.insert(name);
                }
            }
        }
    }
    assert!(
        bodies.len() > 100,
        "the scan reads the bodies: {}",
        bodies.len()
    );
    assert!(queries.len() > 30, "and the queries: {}", queries.len());

    let mut offences = Vec::new();
    for body in &bodies {
        let mut found: Vec<&Declared> = node
            .iter()
            .filter(|d| &d.name == body && d.is_read())
            .collect();
        if found.is_empty() {
            found = rest
                .iter()
                .filter(|d| &d.name == body && d.is_read())
                .collect();
        }
        assert!(
            !found.is_empty(),
            "`{body}` is read as a body and declared nowhere"
        );
        for declared in found {
            if declared.named_fields && !declared.refuses_what_it_does_not_know() {
                offences.push(format!("{}: {} (a body)", declared.at, declared.name));
            }
        }
    }
    for declared in &node {
        let excused = queries.contains(&declared.name)
            || declared.name.ends_with("Query")
            || READ_AND_NO_BODY
                .iter()
                .any(|(name, _)| *name == declared.name);
        if declared.is_read()
            && declared.named_fields
            && !excused
            && !declared.refuses_what_it_does_not_know()
        {
            offences.push(format!("{}: {}", declared.at, declared.name));
        }
    }
    offences.sort();
    offences.dedup();
    assert!(
        offences.is_empty(),
        "add `#[serde(deny_unknown_fields)]` — or, for what is no body, a row of \
         `READ_AND_NO_BODY` with its reason:\n{}",
        offences.join("\n")
    );
    for (name, why) in READ_AND_NO_BODY {
        assert!(
            node.iter().any(|d| d.name == *name && d.is_read()),
            "`{name}` is excused ({why}) and no longer read: take its row out"
        );
    }
}

/// Bodies no screen sends, each with why it carries no schema.
const BODIES_WITH_NO_SCHEMA: &[(&str, &str)] = &[(
    "ReportBody",
    "what a hook inside a terminal reports, in the harness's own events: the \
     command line builds it, and the harness's events carry no schema",
)];

/// **Every body is in the schema the desktop is held to.** The types the
/// desktop sends are generated from the bundle (`src/bin/api-schema.rs`); a
/// body that is not in it is one the desktop writes by hand, and a key it
/// misspells is found by a person at the window. Twelve were not — three
/// mirrored by hand, nine sent as they were typed.
#[test]
fn every_body_is_in_the_schema_the_desktop_is_held_to() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&src, &mut files);
    let bundle = fs::read_to_string(src.join("bin/api-schema.rs")).expect("the schema's bundle");
    let bundled: BTreeSet<String> = bundle
        .split("add!(")
        .skip(1)
        .filter_map(|after| {
            let named: String = after
                .trim_start()
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
                .collect();
            named.rsplit("::").next().map(str::to_string)
        })
        .collect();
    assert!(
        bundled.len() > 300,
        "the bundle was read: {}",
        bundled.len()
    );
    let mut missing = BTreeSet::new();
    let mut bodies = 0;
    for (_, text) in &files {
        for after in text.split("Body<").skip(1) {
            let named: String = after
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
                .collect();
            let name = named.rsplit("::").next().unwrap_or_default();
            if name.is_empty() || name == "T" || name == "Value" {
                continue;
            }
            bodies += 1;
            let excused = BODIES_WITH_NO_SCHEMA.iter().any(|(body, _)| *body == name);
            if !excused && !bundled.contains(name) {
                missing.insert(name.to_string());
            }
        }
    }
    assert!(bodies > 100, "the scan reads the bodies: {bodies}");
    assert!(
        missing.is_empty(),
        "derive `JsonSchema` and add each to `src/bin/api-schema.rs` — or, for a body no \
         screen sends, a row of `BODIES_WITH_NO_SCHEMA` with its reason: {missing:?}"
    );
    for (body, why) in BODIES_WITH_NO_SCHEMA {
        assert!(
            !bundled.contains(*body),
            "`{body}` is excused ({why}) and in the bundle: take its row out"
        );
    }
}

/// **The token is written before any door is opened.** A client takes the
/// socket for the node being there and reads the token at once; with the
/// socket bound first, one that came in the moment between found a node and
/// nothing to ask it with — seen once in a whole suite's run, on a loaded
/// machine. The order is one function's, so it is held where it is written.
#[test]
fn the_node_writes_its_token_before_it_binds_a_door() {
    let lib = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"))
        .expect("the node's lib.rs");
    let serve = lib
        .split("pub async fn serve(")
        .nth(1)
        .expect("the node serves");
    let at = |what: &str| {
        serve
            .find(what)
            .unwrap_or_else(|| panic!("`serve` no longer calls {what}"))
    };
    let token = at("auth::ensure_token(");
    for door in ["bind_socket(", "TcpListener::bind("] {
        assert!(
            token < at(door),
            "`serve` calls {door} before the token is written"
        );
    }
}
