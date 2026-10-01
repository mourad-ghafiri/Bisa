//! A stored record, and every part of one, refuses a key nobody knows — by
//! name, never read with the field dropped in silence
//! ([08](../../../../docs/architecture/08-persistence.md), *there is no older
//! shape to read*). Two tests: a table of shapes, each read whole and refused
//! with one key too many, and a guard over the sources that holds every type
//! the crate reads from JSON to the rule, or names why it is excused.
//!
//! **This file reads sources; it never writes, deletes or runs anything.**

use bisa_core::*;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

/// Read `whole` as `T`, then the same document with one key nobody knows
/// added where `stray` says — at the top, or inside the named object —
/// and expect the refusal to name the key.
fn reads_whole_and_refuses_a_stray<T: DeserializeOwned + std::fmt::Debug>(
    what: &str,
    whole: Value,
    stray: &[&str],
) {
    serde_json::from_value::<T>(whole.clone())
        .unwrap_or_else(|e| panic!("{what}: the whole shape reads: {e}\n{whole}"));
    let mut spoilt = whole.clone();
    let mut at = &mut spoilt;
    for key in stray {
        at = match key.parse::<usize>() {
            Ok(i) => at.get_mut(i),
            Err(_) => at.get_mut(*key),
        }
        .unwrap_or_else(|| panic!("{what}: no `{key}` to spoil in {whole}"));
    }
    at.as_object_mut()
        .unwrap_or_else(|| panic!("{what}: not an object at {stray:?}"))
        .insert("zzz_nobody_knows".into(), json!(1));
    let err = serde_json::from_value::<T>(spoilt.clone())
        .err()
        .unwrap_or_else(|| panic!("{what}: a key nobody knows was read: {spoilt}"));
    assert!(
        err.to_string().contains("zzz_nobody_knows"),
        "{what}: the refusal names the key: {err}"
    );
}

const ULID: &str = "01J000000000000000000000AA";
const HEX: &str = "d23e0c6a603a38d530beb516e3ebc23abaef219e241cb739b37f74cbe05bf7df";

/// The records the store writes and the parts they carry — one document
/// each, in the shape on disk — read whole and refused with a key too many:
/// at the record's top, and inside a tagged part, where an internally tagged
/// enum is held to it as a struct is.
#[test]
fn every_stored_record_and_its_parts_refuse_a_key_nobody_knows_by_name() {
    reads_whole_and_refuses_a_stray::<Skill>(
        "a skill",
        json!({"id": "sk", "name": "Skill", "description": "when", "markdown": "# s",
               "origin": "local", "created_at": 1}),
        &[],
    );
    reads_whole_and_refuses_a_stray::<Skill>(
        "a skill's origin",
        json!({"id": "sk", "name": "Skill", "description": "when", "markdown": "# s",
               "origin": {"catalog": {"slug": "s"}}, "created_at": 1}),
        &["origin", "catalog"],
    );
    reads_whole_and_refuses_a_stray::<Archived>("an archive mark", json!({"at": 1}), &[]);
    reads_whole_and_refuses_a_stray::<ManualPeer>(
        "a manual peer",
        json!({"member_pubkey": HEX, "node_id": "n"}),
        &[],
    );
    reads_whole_and_refuses_a_stray::<Home>("a home", json!({"home": "run", "run": ULID}), &[]);
    reads_whole_and_refuses_a_stray::<RunScope>(
        "a run's scope",
        json!({"scope": "workspace", "budget": {"max_tokens": 5}}),
        &["budget"],
    );
    reads_whole_and_refuses_a_stray::<RunScope>(
        "a goal's run's scope",
        json!({"scope": "goal", "goal": ULID}),
        &[],
    );
    reads_whole_and_refuses_a_stray::<ClosureReason>(
        "a closure's reason",
        json!({"reason": "superseded", "by": ULID}),
        &[],
    );
    reads_whole_and_refuses_a_stray::<Closure>(
        "a closure — its reason flattened beside `at`",
        json!({"reason": "abandoned", "rationale": "no", "at": 3}),
        &[],
    );
    reads_whole_and_refuses_a_stray::<WorkstreamKind>(
        "a workstream's kind",
        json!({"kind": "worktree", "branch": "b", "base": "main"}),
        &[],
    );
    reads_whole_and_refuses_a_stray::<AuthScheme>(
        "a connector's auth scheme",
        json!({"scheme": "api_key", "place": {"in": "header", "name": "X-Key"}}),
        &["place"],
    );
    reads_whole_and_refuses_a_stray::<Budget>("a budget", json!({"max_tokens": 1}), &[]);
    reads_whole_and_refuses_a_stray::<ProjectOrigin>(
        "a project's origin in a step — a variant that flattens a strict part",
        json!({"origin": "step", "goal": ULID, "run": ULID, "step": "build", "workflow": ULID}),
        &[],
    );
    // A struct that flattens an untagged choice — read by hand, since a
    // derived `flatten` drops a stray key in silence.
    reads_whole_and_refuses_a_stray::<OperationBody>(
        "a multipart part",
        json!({"kind": "multipart", "parts": [{"name": "doc", "file": "attachment", "filename": "a.pdf"}]}),
        &["parts", "0"],
    );
    // A newtype variant of an internally tagged enum: the tag is the enum's,
    // the rest the struct's — and a stray is still refused.
    reads_whole_and_refuses_a_stray::<MessageBody>(
        "a membership message",
        json!({"body": "membership", "channel": "general", "member": {"agent": "dev"},
               "change": "joined", "cause": "enabled", "at": 1}),
        &[],
    );
    reads_whole_and_refuses_a_stray::<ModelPlan>(
        "a model plan",
        json!({"models": [{"model": "m"}]}),
        &["models", "0"],
    );
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

/// The types the rule does not hold, each with its reason. A type here that
/// is strict all the same, or gone, fails the guard: an excuse nobody needs
/// is a lie about the code.
const EXCUSED: &[(&str, &str)] = &[
    // Another program's file: the keys it writes beyond what is read are its own.
    (
        "Scene",
        "an Excalidraw file, read by the fields the platform draws",
    ),
    ("SceneAppState", "an Excalidraw file's view state"),
    (
        "Pet",
        "a pet pack's manifest, in the pack's format (`frame` and more are left alone)",
    ),
    ("PetAnimation", "a pet pack's manifest"),
    ("PetFlavour", "a pet pack's manifest"),
    // The node's own output, read back by the command line and the desktop.
    (
        "Text",
        "a message id and its arguments — a sentence, not a record",
    ),
    (
        "Resolved",
        "one resolved setting, the settings routes' answer",
    ),
    (
        "ActivityFact",
        "a row of the activity index typed back, its payload verbatim",
    ),
    ("ActivitySource", "a row of the activity index typed back"),
    // A peer's wire: the GEP is read by the fields this node knows
    // (09 — Protocol), so a fact from a node one step ahead is a fact still.
    ("JournalEvent", "a journal fact off the wire"),
    ("JournalPayload", "a journal fact's payload"),
    ("StepFact", "a step's fact"),
    ("RunFact", "a run's fact"),
    // Read only inside a step, whose hand reader holds their keys first:
    // `Step`'s (a start's `on`, a wait's `until`) and `Boundary`'s (`on`) call
    // `refuse_unknown` before the derived shape reads — see `workflow.rs`
    // (`Step`), `boundary.rs` (`Boundary::refuse_unknown`).
    (
        "StartOn",
        "a start's event, held by `Step`'s reader (`StartOn::refuse_unknown`)",
    ),
    (
        "WaitFor",
        "a wait's event, held by `Step`'s reader (`WaitFor::refuse_unknown`)",
    ),
    (
        "BoundaryOn",
        "a boundary's event, held by `Boundary`'s reader (`BoundaryOn::refuse_unknown`)",
    ),
];

/// Every `pub struct` and `pub enum` that derives `Deserialize` carries
/// `deny_unknown_fields`, unless nothing about it can take a stray key — a
/// unit-only enum, a newtype, a tuple, a `transparent` wrapper — or it is
/// untagged (a choice between shapes, each strict by itself), or it is
/// excused above with its reason. **A derived reading that flattens a struct
/// is a hole**: serde hands the flattened struct every key its siblings did
/// not take and lets it drop what it does not know, whatever the struct's
/// own rule says (`McpServerView`, `Part` and `ProjectOrigin::Step` were
/// found so) — such a type is read by hand (`refuse_unknown_keys`), and a
/// derived one fails here. A flattened *enum* is read whole and refuses.
#[test]
fn every_type_the_core_reads_from_json_refuses_a_key_nobody_knows() {
    let src = workspace_root().join("crates/bisa-core/src");
    let all: Vec<(PathBuf, String)> = sources(&src);
    let structs: std::collections::BTreeSet<String> = all
        .iter()
        .flat_map(|(_, text)| derived_types(text))
        .filter(|item| !item.is_enum)
        .map(|item| item.name)
        .collect();
    let mut loose = Vec::new();
    let mut seen_excused = Vec::new();
    for (path, text) in &all {
        for item in derived_types(text) {
            if !item.attrs.contains("Deserialize") {
                continue;
            }
            if let Some((name, _)) = EXCUSED.iter().find(|(n, _)| *n == item.name) {
                seen_excused.push(*name);
                assert!(
                    !item.attrs.contains("deny_unknown_fields"),
                    "{name} is strict: its excuse is a lie"
                );
                continue;
            }
            if let Some(field) = item.flattened_struct(&structs) {
                loose.push(format!(
                    "{}: {} derives its reading and flattens the struct `{field}` — a stray key drops there in silence; read it by hand",
                    path.display(),
                    item.name
                ));
                continue;
            }
            if item.attrs.contains("deny_unknown_fields")
                || item.attrs.contains("untagged")
                || item.attrs.contains("transparent")
                || item.body.contains("flatten")
                || item.cannot_take_a_key()
            {
                continue;
            }
            loose.push(format!("{}: {}", path.display(), item.name));
        }
    }
    assert!(
        loose.is_empty(),
        "types read from JSON that take a key nobody knows (add `deny_unknown_fields`, or an excuse with its reason):\n{}",
        loose.join("\n")
    );
    for (name, _) in EXCUSED {
        assert!(
            seen_excused.contains(name),
            "{name} is excused and does not exist: take the excuse out"
        );
    }
}

fn sources(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let entries = fs::read_dir(&d).unwrap_or_else(|e| panic!("{}: {e}", d.display()));
        for entry in entries {
            let p = entry.expect("a directory entry").path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|e| e == "rs") {
                let text =
                    fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
                out.push((p, text));
            }
        }
    }
    out.sort();
    out
}

/// One `pub struct` or `pub enum` with the attribute lines above it and the
/// text of its braces (empty for a newtype or a unit struct).
struct Item {
    name: String,
    is_enum: bool,
    attrs: String,
    body: String,
}

impl Item {
    /// The name of a struct this item flattens into itself, when it does:
    /// the type of the field under each `#[serde(flatten)]`, looked up among
    /// the crate's structs (an enum flattened is read whole and refuses).
    fn flattened_struct(&self, structs: &std::collections::BTreeSet<String>) -> Option<String> {
        let lines: Vec<&str> = self.body.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if !line.contains("serde(flatten)") {
                continue;
            }
            let field = lines.get(i + 1)?.trim();
            // `pub source: PartSource,` · `step: StepRef,`
            let ty = field
                .split_once(':')?
                .1
                .trim()
                .trim_end_matches(',')
                .rsplit("::")
                .next()?
                .trim();
            if structs.contains(ty) {
                return Some(ty.to_string());
            }
        }
        None
    }

    /// Nothing about the shape takes a key: a struct with no braces (a
    /// newtype, a unit), or an enum whose every variant is a word or a tuple.
    fn cannot_take_a_key(&self) -> bool {
        if self.body.is_empty() {
            return true;
        }
        if !self.is_enum {
            return false;
        }
        // The variants alone: inside the enum's own braces, comments out.
        let inner = &self.body[1..self.body.len() - 1];
        let code: String = inner
            .lines()
            .map(|l| l.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        !code.contains('{')
    }
}

/// Every `pub struct` / `pub enum` in one source, with what stands above it.
fn derived_types(text: &str) -> Vec<Item> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let (is_enum, rest) = if let Some(r) = trimmed.strip_prefix("pub struct ") {
            (false, r)
        } else if let Some(r) = trimmed.strip_prefix("pub enum ") {
            (true, r)
        } else {
            continue;
        };
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        // The attribute lines above, up to the first line that is neither.
        let mut attrs = Vec::new();
        let mut j = i;
        while j > 0 {
            j -= 1;
            let above = lines[j].trim_start();
            if above.starts_with("#[") || above.starts_with("///") || above.starts_with("//") {
                if above.starts_with("#[") {
                    attrs.push(above);
                }
            } else {
                break;
            }
        }
        // The braces, when the item has any before its `;`.
        let after = lines[i..].join("\n");
        let head_end = after.find(['{', ';']).unwrap_or(after.len());
        let body =
            if after[..head_end].contains('{') || after.as_bytes().get(head_end) == Some(&b'{') {
                let open = head_end;
                let mut depth = 0usize;
                let mut end = open;
                for (k, ch) in after[open..].char_indices() {
                    match ch {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                end = open + k + 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                after[open..end].to_string()
            } else {
                String::new()
            };
        out.push(Item {
            name,
            is_enum,
            attrs: attrs.join("\n"),
            body,
        });
    }
    out
}
