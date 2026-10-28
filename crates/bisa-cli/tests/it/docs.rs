//! `docs/reference/cli.md` names every verb of the `bisa` binary at every
//! depth. The `Command` enum in `src/main.rs` is the root; every `command:`
//! field mounts an enum of verbs under one of them, and the walk reads them
//! all — `agent skill add`, `connector account add`, `git profile add`. A verb
//! the help shows must be spelt in the reference under its parent, and a
//! verb the reference spells must be one the parser has; an enum no field
//! mounts is a tree nobody can type. Neither page of the CLI says a word of
//! the feature the events replaced.
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

/// A variant of a subcommand enum: its verb as clap spells it, whether the
/// help hides it, and the enum its `command:` field mounts under it — the
/// module and the enum's name — when it has one.
struct Variant {
    verb: String,
    hidden: bool,
    mounts: Option<(Option<String>, String)>,
}

/// The variants of the enum `named` in `file`, each with what it mounts:
/// `command: workflow::WorkflowCmd` names an enum in `src/workflow.rs`, a
/// bare `command: RefCmd` one in the same file, an `Option<…>` of either the
/// same. An attribute `hide = true` on the variant hides it from the help.
fn variants_of(file: &str, named: &str) -> Vec<Variant> {
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
    let mut out: Vec<Variant> = Vec::new();
    let mut hidden = false;
    // The field under `#[command(subcommand)]` is the mount; a field that
    // happens to be named `command` (`security try <command>`) is not.
    let mut mount_next = false;
    for line in body[..end].lines() {
        let at_variant_depth = line.starts_with("    ") && !line.starts_with("     ");
        let trimmed = line.trim_start();
        if at_variant_depth && trimmed.starts_with("#[") {
            hidden |= trimmed.contains("hide = true");
            continue;
        }
        if trimmed.starts_with("#[command(subcommand)]") {
            mount_next = true;
            continue;
        }
        if at_variant_depth
            && trimmed
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_uppercase())
        {
            let name: String = trimmed
                .chars()
                .take_while(|c| c.is_alphanumeric())
                .collect();
            out.push(Variant {
                verb: kebab(&name),
                hidden,
                mounts: None,
            });
            hidden = false;
            continue;
        }
        if !mount_next {
            continue;
        }
        mount_next = false;
        if let Some(rest) = trimmed.split_once(": ").map(|(_, ty)| ty) {
            let ty = rest
                .trim_end_matches(',')
                .trim_start_matches("Option<")
                .trim_end_matches('>');
            let (module, name) = match ty.rsplit_once("::") {
                Some((m, n)) => (Some(m.to_string()), n.to_string()),
                None => (None, ty.to_string()),
            };
            out.last_mut()
                .expect("a subcommand field belongs to a variant")
                .mounts = Some((module, name));
        }
    }
    out
}

/// A verb with verbs under it: its path from the root (`[]` for `bisa`
/// itself, `["agent", "skill"]` for `bisa agent skill …`) and its variants.
struct Node {
    path: Vec<String>,
    file: String,
    name: String,
    verbs: Vec<Variant>,
}

fn walk(file: &str, name: &str, path: Vec<String>, out: &mut Vec<Node>) {
    let verbs = variants_of(file, &format!("enum {name} "));
    let mounted: Vec<(String, (Option<String>, String))> = verbs
        .iter()
        .filter_map(|v| v.mounts.clone().map(|m| (v.verb.clone(), m)))
        .collect();
    out.push(Node {
        path: path.clone(),
        file: file.to_string(),
        name: name.to_string(),
        verbs,
    });
    for (verb, (module, name)) in mounted {
        let file = match module {
            Some(module) => format!("src/{module}.rs"),
            None => file.to_string(),
        };
        let mut under = path.clone();
        under.push(verb);
        walk(&file, &name, under, out);
    }
}

/// The whole verb tree, from `enum Command` down through every `command:`
/// field: one node per enum that is reached.
fn tree() -> Vec<Node> {
    let mut out = Vec::new();
    walk("src/main.rs", "Command", Vec::new(), &mut out);
    assert!(out.len() > 20, "the walk reached only {} enums", out.len());
    out
}

/// The text with fenced blocks removed — an example in a block is not a claim.
fn prose(text: &str) -> String {
    let mut out = String::new();
    let mut fenced = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if !fenced {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// The inline code spans of `text`, each as its verb words — a leading
/// `bisa` dropped, so `bisa project new` and `project new` read the same,
/// and the arguments dropped, so `git account [--host …] list` reads as the
/// verbs `git account list`.
fn spans_in(text: &str) -> Vec<Vec<String>> {
    text.split('`')
        .skip(1)
        .step_by(2)
        .map(|span| {
            without_groups(span)
                .split_whitespace()
                .filter(|w| !is_argument(w))
                .map(str::to_string)
                .skip_while(|w| w == "bisa")
                .collect()
        })
        .collect()
}

/// The span with its bracketed and angled groups cut out whole —
/// `[--host github\|gitlab\|bitbucket]`, `<statement>` — so a value a flag
/// takes is never read as a verb.
fn without_groups(span: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for c in span.chars() {
        match c {
            '[' | '<' => depth += 1,
            ']' | '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Every code span of the page's prose.
fn spans(doc: &str) -> Vec<Vec<String>> {
    prose(doc).lines().flat_map(spans_in).collect()
}

/// The code spans of the verb column alone: the first cell of every table
/// row that begins with a span. A description cell may name the git
/// program's own `git init`; the verb cell names only the binary's verbs.
fn verb_cells(doc: &str) -> Vec<Vec<String>> {
    prose(doc)
        .lines()
        .filter(|line| line.starts_with("| `"))
        .flat_map(|line| spans_in(line[2..].split(" | ").next().unwrap_or("")))
        .collect()
}

/// The alternatives a word of a span offers: `add\|rm\|list` in a table
/// cell, `add|rm|list` outside one.
fn alternatives(word: &str) -> Vec<&str> {
    word.split("\\|").flat_map(|w| w.split('|')).collect()
}

/// Whether a word of a span is an argument rather than a verb: a value, a
/// flag, an ellipsis, a quoted thing.
fn is_argument(word: &str) -> bool {
    word.is_empty() || word.starts_with(['<', '[', '-', '…', '(', '"', '\''])
}

/// Whether `doc` names the verb at `path`: spelt whole at the start of a
/// code span or after the binary's name, or as one alternative of a pipe
/// list under its parent (`agent skill add\|rm\|list`).
fn names_verb(doc: &str, spans: &[Vec<String>], path: &[String], verb: &str) -> bool {
    let mut spelt = path.to_vec();
    spelt.push(verb.to_string());
    if names(doc, &spelt.join(" ")) {
        return true;
    }
    spans.iter().any(|words| {
        words.len() > path.len()
            && words[..path.len()] == *path
            && alternatives(&words[path.len()]).contains(&verb)
    })
}

/// **Every verb at every depth is in the reference.** The tree is walked from
/// `enum Command` through every `command:` field — `agent skill add`,
/// `connector account add`, `git profile add` included — and each verb the
/// help shows must be spelt in `docs/reference/cli.md` under its parent.
#[test]
fn the_reference_names_every_verb_at_every_depth() {
    let doc = read("../../docs/reference/cli.md");
    let spans = spans(&doc);
    // The walk reaches the deep verbs, and a verb nobody has is not named.
    let tree = tree();
    for deep in [
        ["agent", "skill"],
        ["connector", "account"],
        ["git", "profile"],
    ] {
        assert!(
            tree.iter().any(|n| n.path == deep),
            "the walk did not reach `{}`",
            deep.join(" ")
        );
    }
    assert!(!names_verb(
        &doc,
        &spans,
        &["agent".to_string(), "skill".to_string()],
        "frobnicate"
    ));
    let mut missing = Vec::new();
    for node in tree {
        for verb in node.verbs.iter().filter(|v| !v.hidden) {
            if !names_verb(&doc, &spans, &node.path, &verb.verb) {
                let mut spelt = node.path.clone();
                spelt.push(verb.verb.clone());
                missing.push(spelt.join(" "));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "docs/reference/cli.md does not mention these verbs: {missing:?}"
    );
}

/// **The reference names no verb the binary lacks.** Every span of the verb
/// column that begins with a verb is read down the tree: each word after a
/// verb that has verbs under it must be one of them (or a pipe list of them),
/// until the verb has none. A span under a verb with no enum of its own is
/// not read further: `msg <scope> <text>` says nothing the parser could
/// refuse.
#[test]
fn the_reference_names_no_verb_the_parser_lacks() {
    let doc = read("../../docs/reference/cli.md");
    let tree = tree();
    let node_at = |path: &[String]| tree.iter().find(|n| n.path == path);
    let cells = verb_cells(&doc);
    assert!(
        cells.len() > 100,
        "the verb column holds {} spans",
        cells.len()
    );
    let mut phantom = Vec::new();
    for words in cells {
        let mut path: Vec<String> = Vec::new();
        for word in &words {
            let Some(node) = node_at(&path) else { break };
            let alts = alternatives(word);
            let unknown: Vec<&str> = alts
                .iter()
                .copied()
                .filter(|alt| !node.verbs.iter().any(|v| v.verb == *alt))
                .collect();
            if path.is_empty() && !unknown.is_empty() {
                // Not a verb span at all: a flag's name, a file, a word.
                break;
            }
            if !unknown.is_empty() {
                phantom.push(format!(
                    "`{}`: {unknown:?} under `{}`",
                    words.join(" "),
                    path.join(" ")
                ));
                break;
            }
            if alts.len() != 1 {
                break;
            }
            path.push(alts[0].to_string());
        }
    }
    assert!(
        phantom.is_empty(),
        "docs/reference/cli.md names verbs the parser does not have:\n{}",
        phantom.join("\n")
    );
}

/// **Every subcommand enum is reached.** An enum deriving `Subcommand` that
/// no `command:` field mounts is a verb tree nobody can type — or a mount
/// the walk cannot read.
#[test]
fn every_subcommand_enum_is_mounted_and_walked() {
    let reached: std::collections::BTreeSet<(String, String)> =
        tree().into_iter().map(|n| (n.file, n.name)).collect();
    let mut unreached = Vec::new();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for entry in std::fs::read_dir(&dir).expect("src/").flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let file = format!("src/{}", path.file_name().unwrap().to_string_lossy());
        let src = read(&file);
        let lines: Vec<&str> = src.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if !(line.starts_with("#[derive(") && line.contains("Subcommand")) {
                continue;
            }
            let name = lines[i + 1..]
                .iter()
                .take(3)
                .find_map(|l| {
                    l.trim_start()
                        .trim_start_matches("pub ")
                        .strip_prefix("enum ")
                        .map(|rest| rest.split_whitespace().next().unwrap_or("").to_string())
                })
                .unwrap_or_else(|| panic!("{file}:{}: a derive with no enum under it", i + 1));
            if !reached.contains(&(file.clone(), name.clone())) {
                unreached.push(format!("{file}: {name}"));
            }
        }
    }
    assert!(
        unreached.is_empty(),
        "subcommand enums no `command:` field mounts: {unreached:?}"
    );
}

/// The two flags a test run is asked with, and the one that rotates a hook's
/// secret, are in the reference by name.
#[test]
fn the_reference_names_the_flags_of_a_test_run_and_a_rotation() {
    let doc = read("../../docs/reference/cli.md");
    let missing: Vec<&str> = ["--start", "--data", "--rotate"]
        .into_iter()
        .filter(|flag| !doc.contains(flag))
        .collect();
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
