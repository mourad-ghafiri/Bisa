//! The documentation set, held to the code.
//!
//! `docs/` describes the platform as built. These tests are the mechanical part
//! of keeping that true: links resolve, paths named in code spans exist, every
//! crate page names every module of its crate, settings keys quoted in prose are
//! registered, and nothing speaks in the planning voice the set has retired.
//!
//! This file reads sources; it never writes, deletes or runs anything.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read a directory").flatten() {
        let p = entry.path();
        if p.is_dir() {
            if p.file_name()
                .is_some_and(|n| n == "node_modules" || n == "target")
            {
                continue;
            }
            walk(&p, ext, out);
        } else if p.extension().is_some_and(|e| e == ext) {
            out.push(p);
        }
    }
}

/// Every markdown file the set is made of: `docs/**`, and the two READMEs that
/// route into it.
fn docs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(&root.join("docs"), "md", &mut out);
    out.push(root.join("README.md"));
    out.push(root.join("desktop/README.md"));
    out.sort();
    out
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).display().to_string()
}

/// The text with fenced blocks removed — a tree drawn in a block is not a claim
/// about a path.
fn prose(text: &str) -> String {
    let mut out = String::new();
    let mut in_fence = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Inline code spans of the prose.
fn code_spans(text: &str) -> Vec<String> {
    let mut spans = Vec::new();
    for line in prose(text).lines() {
        for piece in line.split('`').skip(1).step_by(2) {
            spans.push(piece.to_string());
        }
    }
    spans
}

/// Relative link targets: `](path)` and `](path#anchor)`, never a URL.
fn links(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (i, _) in text.match_indices("](") {
        let rest = &text[i + 2..];
        let Some(end) = rest.find(')') else { continue };
        let target = &rest[..end];
        if target.starts_with("http://")
            || target.starts_with("https://")
            || target.starts_with("mailto:")
            || target.starts_with('#')
            || target.is_empty()
        {
            continue;
        }
        out.push(target.split('#').next().unwrap_or("").to_string());
    }
    out
}

#[test]
fn every_relative_link_resolves() {
    let root = root();
    let mut broken = Vec::new();
    for path in docs(&root) {
        let text = fs::read_to_string(&path).expect("read doc");
        let dir = path.parent().expect("a doc has a directory");
        for target in links(&text) {
            if !dir.join(&target).exists() {
                broken.push(format!("{}: ]({target})", rel(&root, &path)));
            }
        }
    }
    assert!(
        broken.is_empty(),
        "links to nothing:\n{}",
        broken.join("\n")
    );
}

/// The anchor a heading answers to, as the renderers the docs are read in make
/// it: lowercased, spaces to hyphens, everything that is neither a letter, a
/// digit, a hyphen nor an underscore dropped — `## Workflows (13)` is
/// `workflows-13`.
fn anchor_of(heading: &str) -> String {
    heading
        .trim()
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .collect()
}

/// Every anchor a document offers: its headings, outside fenced code.
fn anchors(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut fenced = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced || !line.starts_with('#') {
            continue;
        }
        out.insert(anchor_of(line.trim_start_matches('#')));
    }
    out
}

/// `](file.md#anchor)` and `](#anchor)`, as `(file or empty, anchor)`.
fn anchored_links(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (i, _) in text.match_indices("](") {
        let rest = &text[i + 2..];
        let Some(end) = rest.find(')') else { continue };
        let target = &rest[..end];
        if target.starts_with("http://") || target.starts_with("https://") {
            continue;
        }
        if let Some((file, anchor)) = target.split_once('#') {
            if !anchor.is_empty() && (file.is_empty() || file.ends_with(".md")) {
                out.push((file.to_string(), anchor.to_string()));
            }
        }
    }
    out
}

/// A link to a heading lands on one. A count in a generated heading —
/// `Workflows (13)` — is part of its anchor, so a template added to the
/// catalog is a link to fix, and this is what says so.
#[test]
fn every_anchor_a_link_names_is_a_heading_of_its_target() {
    let root = root();
    let mut broken = Vec::new();
    for path in docs(&root) {
        let text = fs::read_to_string(&path).expect("read doc");
        let dir = path.parent().expect("a doc has a directory");
        for (file, anchor) in anchored_links(&prose(&text)) {
            let target = if file.is_empty() {
                path.clone()
            } else {
                dir.join(&file)
            };
            let Ok(there) = fs::read_to_string(&target) else {
                continue; // a link to nothing is `every_relative_link_resolves`'s
            };
            if !anchors(&there).contains(&anchor) {
                broken.push(format!("{}: ]({file}#{anchor})", rel(&root, &path)));
            }
        }
    }
    assert!(
        broken.is_empty(),
        "links to a heading that is not there:\n{}",
        broken.join("\n")
    );
}

#[test]
fn a_headings_anchor_is_what_a_renderer_makes_of_it() {
    assert_eq!(anchor_of(" Workflows (13)"), "workflows-13");
    assert_eq!(
        anchor_of("The run funnel — start, stop"),
        "the-run-funnel--start-stop"
    );
    assert_eq!(anchor_of("`goals.*`"), "goals");
    let text = "# One\n```sh\n# not a heading\n```\n## Two words\n";
    assert_eq!(
        anchors(text).into_iter().collect::<Vec<_>>(),
        vec!["one".to_string(), "two-words".to_string()]
    );
    assert_eq!(
        anchored_links("see [a](x.md#two-words), [b](#one), [c](https://e.test/#no) and [d](y.md)"),
        vec![
            ("x.md".to_string(), "two-words".to_string()),
            (String::new(), "one".to_string())
        ]
    );
}

/// The crates a shorthand like `store/paths.rs` may stand for.
const CRATES: &[&str] = &[
    "core", "store", "engine", "node", "cli", "vcs", "iso", "codehost", "lsp", "harness",
    "adapters", "mcp", "net", "i18n",
];

/// A code span that names a file or directory in this repository, or `None`
/// when it is something else — a type, a command, a URL, a wire string.
fn named_path(span: &str) -> Option<PathBuf> {
    // `store/paths.rs::resolve_within` names the file before the `::`.
    let span = span.split("::").next().unwrap_or(span).trim();
    if span.contains(' ') || span.contains('<') || span.contains('*') || span.contains('{') {
        return None;
    }
    let bare = span.trim_end_matches('/');
    for prefix in [
        "crates/",
        "desktop/src/",
        "desktop/src-tauri/",
        "scripts/",
        "docs/",
        "library/",
    ] {
        if bare.starts_with(prefix) {
            return Some(PathBuf::from(bare));
        }
    }
    if bare.ends_with(".rs") {
        let (crate_name, rest) = bare.split_once('/')?;
        if CRATES.contains(&crate_name) {
            let dir = if rest.starts_with("tests/") {
                ""
            } else {
                "src/"
            };
            return Some(PathBuf::from(format!(
                "crates/bisa-{crate_name}/{dir}{rest}"
            )));
        }
    }
    None
}

#[test]
fn every_path_named_in_a_code_span_exists() {
    let root = root();
    let mut missing = Vec::new();
    for path in docs(&root) {
        let r = rel(&root, &path);
        let text = fs::read_to_string(&path).expect("read doc");
        for span in code_spans(&text) {
            if let Some(named) = named_path(&span) {
                if !root.join(&named).exists() {
                    missing.push(format!("{r}: `{span}`"));
                }
            }
        }
    }
    // Doc comments in the crates point at docs too; a renamed page must not
    // leave a `docs/...` mention behind.
    let mut sources = Vec::new();
    walk(&root.join("crates"), "rs", &mut sources);
    for path in sources {
        let text = fs::read_to_string(&path).expect("read source");
        for line in text.lines() {
            let t = line.trim_start();
            let Some(comment) = t.strip_prefix("///").or_else(|| t.strip_prefix("//!")) else {
                continue;
            };
            for span in comment.split('`').skip(1).step_by(2) {
                if span.starts_with("docs/")
                    && !span.contains('*')
                    && !root.join(span.trim_end_matches('/')).exists()
                {
                    missing.push(format!("{}: `{span}`", rel(&root, &path)));
                }
            }
        }
    }
    missing.sort();
    missing.dedup();
    assert!(
        missing.is_empty(),
        "paths named in docs that do not exist:\n{}",
        missing.join("\n")
    );
}

#[test]
fn every_crate_page_names_every_module_of_its_crate() {
    let root = root();
    let mut missing = Vec::new();
    for entry in fs::read_dir(root.join("crates"))
        .expect("crates/")
        .flatten()
    {
        let crate_dir = entry.path();
        let name = crate_dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        // A crate is a directory named `bisa-*`; a Finder's `.DS_Store`
        // or a stray file in `crates/` is not one and has no page to own.
        let Some(short) = name.strip_prefix("bisa-") else {
            continue;
        };
        if !crate_dir.is_dir() {
            continue;
        }
        let short = short.to_string();
        let page = root.join(format!("docs/architecture/crates/{short}.md"));
        let text = fs::read_to_string(&page).unwrap_or_else(|_| {
            panic!("docs/architecture/crates/{short}.md: every crate has a page")
        });
        let mut modules = Vec::new();
        walk(&crate_dir.join("src"), "rs", &mut modules);
        for module in modules {
            let r = module
                .strip_prefix(crate_dir.join("src"))
                .unwrap()
                .display()
                .to_string();
            if !text.contains(&format!("`{r}`"))
                && !text.contains(&format!("`src/{r}`"))
                && !text.contains(&format!("`crates/{name}/src/{r}`"))
            {
                missing.push(format!("docs/architecture/crates/{short}.md: `{r}`"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "a crate page must name every source file of its crate as a code span:\n{}",
        missing.join("\n")
    );
}

/// A span that names a file, not a key or a topic: `git.rs`, `settings.json`.
fn looks_like_a_file(span: &str) -> bool {
    [
        ".rs", ".md", ".ts", ".tsx", ".mts", ".mjs", ".json", ".jsonl", ".toml", ".sqlite",
        ".sock", ".lock", ".css", ".yml", ".key", ".patch", ".txt", ".rows", ".git", ".html",
        ".svg", ".png",
    ]
    .iter()
    .any(|ext| span.ends_with(ext))
}

/// Every key the settings registry declares, read from its source: the
/// first quoted word after each `def!(`, whether rustfmt left it on the same
/// line or broke it onto the next.
fn settings_keys(root: &Path) -> BTreeSet<String> {
    let src =
        fs::read_to_string(root.join("crates/bisa-core/src/settings.rs")).expect("settings.rs");
    let mut out = BTreeSet::new();
    for piece in src.split("def!(").skip(1) {
        let Some(open) = piece.find('"') else {
            continue;
        };
        let rest = &piece[open + 1..];
        let Some(close) = rest.find('"') else {
            continue;
        };
        let key = &rest[..close];
        if key.contains('.') && rest[close + 1..].trim_start().starts_with(',') {
            out.insert(key.to_string());
        }
    }
    assert!(
        out.len() > 30,
        "found only {} settings keys; the parser is broken",
        out.len()
    );
    out
}

#[test]
fn every_settings_key_named_in_prose_is_registered() {
    let root = root();
    let keys = settings_keys(&root);
    let groups: BTreeSet<String> = keys
        .iter()
        .map(|k| k.split('.').next().unwrap_or("").to_string())
        .collect();
    // The engine's bus topics share the `a.b` shape (`gate.opened`), which a
    // `platform` start event names; the engine's docs test owns those
    // (`EnginePayload::topic`), so a topic prefix is not a settings group here.
    let topics =
        fs::read_to_string(root.join("crates/bisa-engine/src/events.rs")).expect("events.rs");
    // So do the decision points (`security.tool`, `browser.headless`): the
    // domain declares each as its wire word, and a word it declares is not a
    // setting somebody forgot to register.
    let points =
        fs::read_to_string(root.join("crates/bisa-core/src/decision.rs")).expect("decision.rs");
    // And the addon bridge's methods and events (`network.fetch`,
    // `system.load`): the bridge registry declares each as its wire word
    // (18 — Addons), quoted like the topics are.
    let bridge = fs::read_to_string(root.join("desktop/src/addons/addonBridgeModel.mjs"))
        .expect("addonBridgeModel.mjs");
    let mut unknown = Vec::new();
    for path in docs(&root) {
        let r = rel(&root, &path);
        if r.ends_with("reference/settings-keys.md") {
            continue;
        }
        let text = fs::read_to_string(&path).expect("read doc");
        for span in code_spans(&text) {
            let Some((head, _)) = span.split_once('.') else {
                continue;
            };
            if !groups.contains(head)
                || !span
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_' || c == '.')
                || span.ends_with('.')
                || span.ends_with(".*")
                || looks_like_a_file(&span)
            {
                continue;
            }
            let quoted = format!("\"{span}\"");
            if keys.contains(&span)
                || topics.contains(&quoted)
                || points.contains(&quoted)
                || bridge.contains(&quoted)
            {
                continue;
            }
            unknown.push(format!("{r}: `{span}`"));
        }
    }
    assert!(
        unknown.is_empty(),
        "these look like settings keys and are not in the registry:\n{}",
        unknown.join("\n")
    );
}

/// The planning voice the set has retired: a phase token, or a phrase that
/// describes a plan rather than a platform.
fn planning_voice(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    for phrase in [
        "the brief",
        "reference implementation",
        "gap matrix",
        "this document adds",
        "not yet built",
        "will ship",
        "run ahead of code",
    ] {
        if lower.contains(phrase) {
            return Some(phrase.to_string());
        }
    }
    for token in line.split(|c: char| !c.is_ascii_alphanumeric()) {
        let mut chars = token.chars();
        let Some(first) = chars.next() else { continue };
        let rest: String = chars.collect();
        // `F2` is a key; a lettered `F3c` is a phase. `E2`, `W3`, `M13` are phases.
        let is_phase = match first {
            'F' => {
                rest.len() == 2
                    && rest.starts_with(|c: char| ('0'..='6').contains(&c))
                    && rest.ends_with(|c: char| ('a'..='d').contains(&c))
            }
            'E' | 'W' => rest.len() == 1 && rest.chars().all(|c| ('0'..='5').contains(&c)),
            'M' => (1..=2).contains(&rest.len()) && rest.chars().all(|c| c.is_ascii_digit()),
            _ => false,
        };
        if is_phase {
            return Some(token.to_string());
        }
    }
    None
}

#[test]
fn nothing_speaks_in_the_planning_voice() {
    let root = root();
    let mut hits = Vec::new();
    for path in docs(&root) {
        let r = rel(&root, &path);
        let text = fs::read_to_string(&path).expect("read doc");
        for (n, line) in prose(&text).lines().enumerate() {
            if let Some(what) = planning_voice(line) {
                hits.push(format!("{r}:{}: {what}", n + 1));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "the documentation describes the platform as built; these lines describe a plan:\n{}",
        hits.join("\n")
    );
}

// ---------------------------------------------------------------------------
// The scenario page names only what exists
// ---------------------------------------------------------------------------

/// The slugs under one catalog folder: the file names.
fn catalog_slugs(root: &Path, kind: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for entry in fs::read_dir(root.join("library/catalog").join(kind))
        .expect("a catalog folder")
        .flatten()
    {
        let p = entry.path();
        if p.extension().is_some_and(|e| e == "toml") {
            out.insert(p.file_stem().unwrap().to_string_lossy().into_owned());
        }
    }
    assert!(
        !out.is_empty(),
        "the {kind} folder is empty; the reader is broken"
    );
    out
}

/// Every `connector.operation` the catalog ships, read from the TOML.
fn connector_operations(root: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for slug in catalog_slugs(root, "connectors") {
        let text = fs::read_to_string(
            root.join("library/catalog/connectors")
                .join(format!("{slug}.toml")),
        )
        .expect("a connector definition");
        let mut in_operation = false;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with("[[") {
                in_operation = line == "[[connector.operations]]";
                continue;
            }
            if in_operation {
                if let Some(id) = line
                    .strip_prefix("id = \"")
                    .and_then(|s| s.strip_suffix('"'))
                {
                    out.insert(format!("{slug}.{id}"));
                }
            }
        }
    }
    assert!(
        out.len() > 30,
        "found only {} operations; the reader is broken",
        out.len()
    );
    out
}

/// The words a `match` on an enum answers with — `Kind::Variant { .. } => "word"`
/// — read from a source file between two markers.
fn wire_words(root: &Path, file: &str, from: &str, until: &str) -> BTreeSet<String> {
    let src = fs::read_to_string(root.join(file)).expect(file);
    let start = src
        .find(from)
        .unwrap_or_else(|| panic!("{file} has no {from:?}"));
    let body = &src[start..];
    let end = body
        .find(until)
        .unwrap_or_else(|| panic!("{file} has no {until:?} after {from:?}"));
    let mut out = BTreeSet::new();
    for line in body[..end].lines() {
        if let Some(rest) = line.split("=> \"").nth(1) {
            if let Some(word) = rest.split('"').next() {
                out.insert(word.to_string());
            }
        }
    }
    assert!(
        out.len() > 5,
        "found only {} words in {file}; the reader is broken",
        out.len()
    );
    out
}

/// Every tool the MCP server declares — `name = "…"` on a tool attribute.
fn mcp_tools(root: &Path) -> BTreeSet<String> {
    let src = fs::read_to_string(root.join("crates/bisa-mcp/src/server.rs")).expect("server.rs");
    let mut out = BTreeSet::new();
    // `name = "…"` stands alone on a line or inside `#[tool(name = "…", …)]`.
    for line in src.lines() {
        for rest in line.split("name = \"").skip(1) {
            if let Some(name) = rest.split('"').next() {
                out.insert(name.to_string());
            }
        }
    }
    for tool in bisa_core::browser::BROWSER_TOOLS {
        out.insert((*tool).to_string());
    }
    assert!(
        out.len() > 40,
        "found only {} tools; the reader is broken",
        out.len()
    );
    out
}

/// The scenario page is a coverage check, so every name it puts in a code span
/// is a thing that exists: a catalog slug, a `connector.operation`, a step
/// kind, a start event, a tool, a setting key, an auth scheme's or a body's
/// word, or a parameter kind. A scenario that names a part nobody shipped
/// fails here by name — the page cannot rot quietly as the catalog changes.
#[test]
fn the_scenario_page_names_only_things_that_exist() {
    let root = root();
    let text = fs::read_to_string(root.join("docs/guide/real-world-scenarios.md"))
        .expect("the scenario page");
    let mut known: BTreeSet<String> = BTreeSet::new();
    for kind in [
        "agents",
        "teams",
        "skills",
        "workflows",
        "connectors",
        "channels",
    ] {
        known.extend(catalog_slugs(&root, kind));
    }
    known.extend(connector_operations(&root));
    known.extend(wire_words(
        &root,
        "crates/bisa-core/src/workflow.rs",
        "impl StepKind {",
        "\n    }\n",
    ));
    known.extend(wire_words(
        &root,
        "crates/bisa-core/src/start.rs",
        "impl StartOn {",
        "\n    }\n",
    ));
    known.extend(mcp_tools(&root));
    known.extend(settings_keys(&root));
    // The connector vocabulary the page may name: schemes, body kinds, kinds.
    for word in [
        "none",
        "api_key",
        "bearer",
        "basic",
        "oauth2",
        "jwt",
        "json",
        "form",
        "multipart",
        "raw",
        "text",
        "number",
        "bool",
        "file",
    ] {
        known.insert(word.to_string());
    }
    let mut unknown = Vec::new();
    for span in code_spans(&text) {
        if span == "connector.operation" || span.starts_with("crates/") || span.starts_with("docs/")
        {
            continue;
        }
        if !known.contains(&span) {
            unknown.push(span);
        }
    }
    unknown.sort();
    unknown.dedup();
    assert!(
        unknown.is_empty(),
        "docs/guide/real-world-scenarios.md names things that do not exist:\n{}",
        unknown.join("\n")
    );
    let rows = text.lines().filter(|l| {
        let l = l.trim_start_matches("| ");
        l.starts_with(|c: char| c.is_ascii_uppercase())
            && l.chars().nth(1).is_some_and(|c| c.is_ascii_digit())
    });
    assert_eq!(rows.count(), 100, "the page holds a hundred scenarios");
}

/// The coverage page (`docs/contributing/coverage.md`) has a row for every
/// feature the status page calls implemented, and no row for a feature the
/// status page does not name. "Implemented" means covered by tests; this is
/// where that clause is kept from decaying.
#[test]
fn every_implemented_feature_has_a_coverage_row() {
    let root = root();
    let status = fs::read_to_string(root.join("docs/feature-status.md")).expect("feature status");
    let coverage =
        fs::read_to_string(root.join("docs/contributing/coverage.md")).expect("coverage page");
    let implemented = feature_cells(&status, "## Implemented", "## Partial");
    let covered = feature_cells(
        &coverage,
        "## The crates",
        "## The guards that hold the rest",
    );
    let missing: Vec<&String> = implemented
        .iter()
        .filter(|f| !covered.contains(f))
        .collect();
    let extra: Vec<&String> = covered
        .iter()
        .filter(|f| !implemented.contains(f))
        .collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "coverage rows missing for {missing:?}; rows for features the status page does not call implemented: {extra:?}"
    );
}

/// The first cell of every body row of the tables between two headings — a
/// feature's name, a parenthetical aside dropped.
fn feature_cells(text: &str, from: &str, to: &str) -> Vec<String> {
    let start = text.find(from).expect("the section starts");
    let end = text[start..]
        .find(to)
        .map(|i| start + i)
        .unwrap_or(text.len());
    text[start..end]
        .lines()
        .filter(|l| l.starts_with("| ") && !l.starts_with("|---"))
        .map(|l| {
            l.trim_start_matches('|')
                .split('|')
                .next()
                .unwrap_or("")
                .trim()
        })
        .filter(|cell| !matches!(*cell, "Crate" | "Area" | "Feature"))
        .map(|cell| cell.split(" (").next().unwrap_or(cell).trim().to_string())
        .collect()
}
