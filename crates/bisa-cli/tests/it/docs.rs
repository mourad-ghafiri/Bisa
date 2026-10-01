//! `docs/reference/cli.md` names every verb of the `bisa` binary. The
//! `Command` enum in `src/main.rs` is the list; a verb added there without a
//! sentence in the reference fails here. The verbs of listening, signals and
//! test runs live one level down — under `workflow` and `signal` — and are
//! held to the reference by name, each checked against the parser that has
//! it; and neither page of the CLI says a word of the feature they replaced.
//!
//! This file reads sources; it never writes, deletes or runs anything.

use std::path::Path;

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

/// `AgentContext` → `agent-context`, the way clap spells a variant.
fn kebab(variant: &str) -> String {
    let mut out = String::new();
    for (i, c) in variant.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            out.push('-');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// The variants of the enum `named` in `file`, as clap spells them.
fn verbs_of(file: &str, named: &str) -> Vec<String> {
    let src = read(file);
    let start = src
        .find(named)
        .unwrap_or_else(|| panic!("{named} in {file}"));
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
    let mut out = Vec::new();
    for line in body[..end].lines() {
        if line.starts_with("    ") && !line.starts_with("     ") {
            let name: String = line
                .trim_start()
                .chars()
                .take_while(|c| c.is_alphanumeric())
                .collect();
            if name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                out.push(kebab(&name));
            }
        }
    }
    out
}

fn verbs() -> Vec<String> {
    let out = verbs_of("src/main.rs", "enum Command");
    assert!(
        out.len() > 30,
        "found only {} verbs; the parser is broken",
        out.len()
    );
    out
}

/// Whether `doc` names `verb`. The reference spells a verb at the start of a
/// code span — `new <statement>`, `project new <slug>` — or after the
/// binary's name.
fn names(doc: &str, verb: &str) -> bool {
    doc.contains(&format!("bisa {verb}"))
        || doc.contains(&format!("`{verb}`"))
        || doc.contains(&format!("`{verb} "))
}

#[test]
fn the_reference_names_every_verb() {
    let doc = read("../../docs/reference/cli.md");
    let missing: Vec<String> = verbs().into_iter().filter(|v| !names(&doc, v)).collect();
    assert!(
        missing.is_empty(),
        "docs/reference/cli.md does not mention these verbs: {missing:?}"
    );
}

/// The verbs of listening, signals and test runs: a workflow is turned on
/// and off, its listeners and its hooks' secrets are shown, a run is begun
/// at a start; a signal is raised, listed and let through. Each is a verb of
/// the parser, and each is in the reference — with the two flags a test run
/// is asked with.
#[test]
fn the_reference_names_the_verbs_of_listening() {
    let doc = read("../../docs/reference/cli.md");
    let workflow = verbs_of("src/workflow.rs", "enum WorkflowCmd");
    let signal = verbs_of("src/signals.rs", "enum SignalCmd");
    assert_eq!(
        signal,
        ["emit", "list", "release"],
        "every verb of `signal` is held to the reference"
    );
    let mut missing = Vec::new();
    for (noun, verbs, parsed) in [
        (
            "workflow",
            &["on", "off", "listeners", "hook-secret", "run"][..],
            &workflow,
        ),
        ("signal", &["emit", "list", "release"][..], &signal),
    ] {
        for verb in verbs {
            assert!(
                parsed.iter().any(|v| v == verb),
                "`{noun} {verb}` is no verb of the parser: {parsed:?}"
            );
            let spelt = format!("{noun} {verb}");
            if !names(&doc, &spelt) {
                missing.push(spelt);
            }
        }
    }
    for flag in ["--start", "--data", "--rotate"] {
        if !doc.contains(flag) {
            missing.push(flag.to_string());
        }
    }
    assert!(
        missing.is_empty(),
        "docs/reference/cli.md does not mention these: {missing:?}"
    );
}

/// The feature the events replaced left no word on either page of the CLI.
#[test]
fn the_pages_say_nothing_of_the_retired_feature() {
    // Spelt in two halves, so this source does not carry the word either.
    let retired = ["trig", "ger"].concat();
    for page in [
        "../../docs/reference/cli.md",
        "../../docs/architecture/crates/cli.md",
    ] {
        let doc = read(page);
        let left: Vec<(usize, &str)> = doc
            .lines()
            .enumerate()
            .filter(|(_, line)| line.to_lowercase().contains(&retired))
            .map(|(n, line)| (n + 1, line))
            .collect();
        assert!(left.is_empty(), "{page} still says it: {left:?}");
    }
}

/// The architecture page lists the verbs by hand; the list is held to the
/// parser both ways, so it names no verb that is gone and misses none.
#[test]
fn the_architecture_page_lists_exactly_the_verbs() {
    let page = read("../../docs/architecture/crates/cli.md");
    let line = page
        .lines()
        .find(|l| l.contains("`enum Command`"))
        .expect("the main.rs row");
    let listed: std::collections::BTreeSet<String> = line
        .split("the verbs (")
        .nth(1)
        .and_then(|rest| rest.split(')').next())
        .expect("a parenthesised list")
        .split(',')
        .map(|v| v.trim().trim_matches('`').to_string())
        .filter(|v| !v.is_empty())
        .collect();
    let real: std::collections::BTreeSet<String> = verbs().into_iter().collect();
    let phantom: Vec<&String> = listed.difference(&real).collect();
    let missing: Vec<&String> = real.difference(&listed).collect();
    assert!(
        phantom.is_empty(),
        "verbs the page names and the parser has not: {phantom:?}"
    );
    assert!(
        missing.is_empty(),
        "verbs the parser has and the page omits: {missing:?}"
    );
}
