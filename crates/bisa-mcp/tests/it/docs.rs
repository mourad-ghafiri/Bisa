//! `docs/reference/mcp-tools.md` lists the tools; `src/server.rs` registers
//! them. The two are kept equal here, both directions: a tool that is not in the
//! reference is undocumented, a documented name that is not a tool is a lie an
//! agent will act on.
//!
//! This file reads sources; it never writes, deletes or runs anything.

use std::collections::BTreeSet;
use std::path::Path;

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

/// Every `name = "…"` in the tool router, whatever it is a name of.
fn registered_names() -> BTreeSet<String> {
    let src = read("src/server.rs");
    let mut out = BTreeSet::new();
    for (i, _) in src.match_indices("name = \"") {
        let rest = &src[i + "name = \"".len()..];
        if let Some(end) = rest.find('"') {
            let name = &rest[..end];
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                out.insert(name.to_string());
            }
        }
    }
    assert!(
        out.len() > 20,
        "found only {} names; the parser is broken",
        out.len()
    );
    out
}

/// A lifecycle hook: a name a harness calls at a point of the session's
/// life and keeps off the model's list — the underscore convention.
fn is_hook(name: &str) -> bool {
    name.starts_with('_')
}

/// The tools an agent calls: every registered name that is no hook.
fn registered_tools() -> BTreeSet<String> {
    registered_names()
        .into_iter()
        .filter(|name| !is_hook(name))
        .collect()
}

/// The backticked `snake_case` names in the first column of every table whose
/// header is `| Tool | … |`. Other tables (the shape of a question) list fields.
fn documented_tools() -> BTreeSet<String> {
    let doc = read("../../docs/reference/mcp-tools.md");
    let mut out = BTreeSet::new();
    let mut in_tool_table = false;
    for line in doc.lines() {
        if line.starts_with("| Tool") {
            in_tool_table = true;
            continue;
        }
        if !line.starts_with('|') {
            in_tool_table = false;
            continue;
        }
        if !in_tool_table {
            continue;
        }
        let Some(cell) = line.strip_prefix("| `") else {
            continue;
        };
        let Some(cell) = cell.split('|').next() else {
            continue;
        };
        for piece in cell.split('`').step_by(2) {
            let name = piece.trim();
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
                out.insert(name.to_string());
            }
        }
    }
    out
}

#[test]
fn the_reference_lists_every_tool_and_nothing_else() {
    let registered = registered_tools();
    let documented = documented_tools();
    let undocumented: Vec<&String> = registered.difference(&documented).collect();
    let phantom: Vec<&String> = documented.difference(&registered).collect();
    assert!(
        undocumented.is_empty() && phantom.is_empty(),
        "docs/reference/mcp-tools.md and src/server.rs disagree.\nregistered, not documented: {undocumented:?}\ndocumented, not registered: {phantom:?}"
    );
}

/// A registered name is a tool an agent calls, in `snake_case`, or a hook
/// under the underscore convention — nothing else, or the two guards here
/// would each leave it to the other.
#[test]
fn every_registered_name_is_a_tool_or_a_lifecycle_hook() {
    let odd: Vec<String> = registered_names()
        .into_iter()
        .filter(|name| !is_hook(name) && !name.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
        .collect();
    assert!(
        odd.is_empty(),
        "neither a snake_case tool nor an underscore hook: {odd:?}"
    );
}

/// A hook is in no tool table — an agent never calls it — and the reference
/// still says each one by name, and names none the router does not register:
/// a name on the wire that no page explains is what a harness author trips on.
#[test]
fn the_reference_names_every_lifecycle_hook_and_no_other() {
    let doc = read("../../docs/reference/mcp-tools.md");
    let registered: BTreeSet<String> = registered_names()
        .into_iter()
        .filter(|name| is_hook(name))
        .collect();
    assert!(
        !registered.is_empty(),
        "the router registers no hook any more: say so here and on the page"
    );
    // Every backticked word that begins with an underscore and a capital.
    let named: BTreeSet<String> = doc
        .split('`')
        .skip(1)
        .step_by(2)
        .filter(|word| {
            let mut chars = word.chars();
            chars.next() == Some('_')
                && chars.next().is_some_and(|c| c.is_ascii_uppercase())
                && word.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
        .map(str::to_string)
        .collect();
    assert_eq!(
        named, registered,
        "the hooks docs/reference/mcp-tools.md names, against the ones src/server.rs registers"
    );
    for hook in &registered {
        assert!(
            !documented_tools().contains(hook),
            "{hook} is a hook, in no tool table"
        );
    }
}
