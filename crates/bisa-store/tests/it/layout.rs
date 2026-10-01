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
