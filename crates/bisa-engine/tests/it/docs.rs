//! The engine's vocabulary, as the documentation states it.
//!
//! Two lists live in this crate and are quoted across `docs/`: the topics a
//! `platform` start, wait or boundary may name (`events::TOPICS`, one per
//! `EnginePayload` variant) and the variants themselves. A doc that names a
//! topic that does not exist, or a crate page that forgets a variant, fails
//! here rather than misleading the next contributor.
//!
//! This file reads sources; it never writes, deletes or runs anything.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

/// The topics the engine emits — what `EnginePayload::topic` answers.
fn topics() -> BTreeSet<String> {
    let out: BTreeSet<String> = bisa_engine::events::TOPICS
        .iter()
        .map(|t| t.to_string())
        .collect();
    assert!(
        out.len() > 10,
        "found only {} topics; the list is broken",
        out.len()
    );
    out
}

/// Every variant name of `pub enum EnginePayload`.
fn payload_variants() -> BTreeSet<String> {
    let src = read("crates/bisa-engine/src/events.rs");
    let start = src
        .find("pub enum EnginePayload")
        .expect("EnginePayload exists");
    let body = &src[src[start..].find('{').expect("brace") + start + 1..];
    let mut depth = 1usize;
    let mut end = 0usize;
    for (i, c) in body.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    for line in body[..end].lines() {
        let t = line.trim_start();
        if line.starts_with("    ") && !line.starts_with("     ") {
            let name: String = t.chars().take_while(|c| c.is_alphanumeric()).collect();
            if name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                out.insert(name);
            }
        }
    }
    assert!(
        out.len() > 20,
        "found only {} variants; the parser is broken",
        out.len()
    );
    out
}

fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read docs").flatten() {
        let p = entry.path();
        if p.is_dir() {
            markdown_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "md") {
            out.push(p);
        }
    }
}

/// Inline code spans of a markdown text, fenced blocks removed.
fn code_spans(text: &str) -> Vec<String> {
    let mut spans = Vec::new();
    let mut in_fence = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        for piece in line.split('`').skip(1).step_by(2) {
            spans.push(piece.to_string());
        }
    }
    spans
}

#[test]
fn every_topic_named_in_the_docs_exists() {
    let topics = topics();
    let prefixes: BTreeSet<&str> = topics
        .iter()
        .map(|t| t.split('.').next().unwrap_or(""))
        .collect();
    // Settings keys share the `a.b` shape; those are checked by the core crate's
    // docs test, so anything the settings registry knows is not a topic here.
    let settings = read("crates/bisa-core/src/settings.rs");
    // A decision point's id has the same shape too (`model.route`,
    // `goal.adopt`): the core names them, and none of them is a topic.
    let decision_points = read("crates/bisa-core/src/decision.rs");
    // The pages that document events: where a topic is quoted as a topic.
    // Elsewhere `goal.id` is a field and `git.rs` a file.
    let mut files = Vec::new();
    markdown_files(&root().join("docs"), &mut files);
    let mut unknown = Vec::new();
    for path in files {
        let rel = path
            .strip_prefix(root())
            .unwrap_or(&path)
            .display()
            .to_string();
        if !(rel.ends_with("guide/events.md")
            || rel.ends_with("crates/engine.md")
            || rel.ends_with("contributing/recipes.md"))
        {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read doc");
        for span in code_spans(&text) {
            let Some((head, _)) = span.split_once('.') else {
                continue;
            };
            if !prefixes.contains(head)
                || !span
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_' || c == '.')
            {
                continue;
            }
            if span.ends_with('.')
                || span.contains("..")
                || span.rsplit('.').next().is_some_and(|ext| {
                    ["rs", "md", "json", "toml", "jsonl", "mjs", "ts", "ftl"].contains(&ext)
                })
                || settings.contains(&format!("\"{span}\""))
                || decision_points.contains(&format!("=> \"{span}\""))
            {
                continue;
            }
            if !topics.contains(&span) {
                unknown.push(format!("{rel}: `{span}`"));
            }
        }
    }
    assert!(
        unknown.is_empty(),
        "these look like topics and are not in `events::TOPICS`:\n{}",
        unknown.join("\n")
    );
}

#[test]
fn the_engine_page_names_every_event_payload() {
    let page = read("docs/architecture/crates/engine.md");
    let missing: Vec<String> = payload_variants()
        .into_iter()
        .filter(|v| !page.contains(&format!("`{v}`")))
        .collect();
    assert!(
        missing.is_empty(),
        "docs/architecture/crates/engine.md does not name these `EnginePayload` variants:\n{}",
        missing.join("\n")
    );
}

/// The HTTP reference names exactly the requests the editor may proxy —
/// `lsp::ALLOWED_METHODS` — so a client built against the page never meets a
/// 400 for a method the page promised.
#[test]
fn the_lsp_request_route_names_exactly_the_allowed_methods() {
    let page = read("docs/reference/http-api.md");
    let line = page
        .lines()
        .find(|l| l.contains("/ide/lsp/{scope}/{id}/request"))
        .expect("the request route is on the page");
    for method in bisa_engine::lsp::ALLOWED_METHODS {
        assert!(
            line.contains(&format!("`{method}`")),
            "{method} is missing from: {line}"
        );
    }
    for refused in ["rename", "completion", "signatureHelp"] {
        assert!(
            !line.contains(&format!("`textDocument/{refused}`")),
            "{refused} is not proxied and must not be promised: {line}"
        );
    }
}
