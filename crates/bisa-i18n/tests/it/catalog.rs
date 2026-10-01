//! The catalog held to the sources, and both runtimes to the catalog
//! (17 §Guards). Read-only: this file walks `locales/` and `crates/*/src`.
//!
//! - every `.ftl` under `locales/` parses — one syntax, read by fluent-rs
//!   here and by fluent.js in the desktop;
//! - a shipped language has every namespace the crates own;
//! - the desktop's `AVAILABLE` and this crate's agree;
//! - every `text!("…")` in the crates names a message, with exactly the
//!   arguments the message's placeables use, and every message in a
//!   crate-owned file is said somewhere (or is a derived id — a setting's).

use bisa_i18n::{Namespace, AVAILABLE};
use fluent_syntax::ast;
use fluent_syntax::parser::parse_runtime;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    // A folder the guard cannot walk is a rule it does not hold: said,
    // never passed over.
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("the guard cannot read {}: {e}", dir.display()));
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if p.file_name()
                .is_some_and(|n| n == "target" || n == "node_modules")
            {
                continue;
            }
            walk(&p, ext, out);
        } else if p.extension().is_some_and(|e| e == ext) {
            out.push(p);
        }
    }
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).display().to_string()
}

#[test]
fn every_catalog_file_parses_as_fluent() {
    let root = root();
    let mut files = Vec::new();
    walk(&root.join("locales"), "ftl", &mut files);
    assert!(!files.is_empty(), "locales/ holds the catalog");
    let mut faults = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file).expect("read a catalog file");
        if let Err((_, errors)) = parse_runtime(text.as_str()) {
            for e in errors {
                faults.push(format!("{}: {e:?}", rel(&root, file)));
            }
        }
    }
    assert!(
        faults.is_empty(),
        "a catalog file does not parse:\n{}",
        faults.join("\n")
    );
}

#[test]
fn a_shipped_language_has_every_namespace_the_crates_own() {
    let root = root();
    for tag in AVAILABLE {
        for ns in Namespace::ALL {
            let file = root.join("locales").join(tag).join(ns.file());
            assert!(
                file.is_file(),
                "{}: a shipped language ships whole",
                rel(&root, &file)
            );
            assert!(ns.source(tag).is_some(), "{tag}/{}: compiled in", ns.file());
        }
    }
}

#[test]
fn the_desktop_and_the_crates_agree_on_the_shipped_languages() {
    let root = root();
    let model = fs::read_to_string(root.join("desktop/src/i18n/localeModel.mjs"))
        .expect("desktop/src/i18n/localeModel.mjs");
    let start = model
        .find("export const AVAILABLE = Object.freeze([")
        .expect("the desktop spells AVAILABLE");
    let list = &model[start..];
    let end = list.find("])").expect("a closed list");
    let tags: Vec<&str> = list[..end].split('"').skip(1).step_by(2).collect();
    assert_eq!(
        tags,
        AVAILABLE.to_vec(),
        "one list of languages, spelt twice"
    );
}

/// The variables a pattern's placeables read, recursively.
fn variables_of(pattern: &ast::Pattern<&str>, out: &mut BTreeSet<String>) {
    for element in &pattern.elements {
        if let ast::PatternElement::Placeable { expression } = element {
            expression_variables(expression, out);
        }
    }
}

fn inline_variables(e: &ast::InlineExpression<&str>, out: &mut BTreeSet<String>) {
    match e {
        ast::InlineExpression::VariableReference { id } => {
            out.insert(id.name.to_string());
        }
        ast::InlineExpression::FunctionReference { arguments, .. } => {
            for a in &arguments.positional {
                inline_variables(a, out);
            }
            for a in &arguments.named {
                inline_variables(&a.value, out);
            }
        }
        ast::InlineExpression::TermReference {
            arguments: Some(arguments),
            ..
        } => {
            for a in &arguments.named {
                inline_variables(&a.value, out);
            }
        }
        ast::InlineExpression::Placeable { expression } => expression_variables(expression, out),
        _ => {}
    }
}

fn expression_variables(e: &ast::Expression<&str>, out: &mut BTreeSet<String>) {
    match e {
        ast::Expression::Inline(inline) => inline_variables(inline, out),
        ast::Expression::Select { selector, variants } => {
            inline_variables(selector, out);
            for v in variants {
                variables_of(&v.value, out);
            }
        }
    }
}

/// Every message of the crate-owned English files: id → the variables it reads
/// (its value's and its attributes').
fn messages() -> BTreeMap<String, BTreeSet<String>> {
    let mut out = BTreeMap::new();
    for ns in Namespace::ALL {
        let source = ns.source("en").expect("English ships every namespace");
        let resource = match parse_runtime(source) {
            Ok(r) => r,
            Err((r, _)) => r,
        };
        for entry in &resource.body {
            if let ast::Entry::Message(m) = entry {
                let mut vars = BTreeSet::new();
                if let Some(v) = &m.value {
                    variables_of(v, &mut vars);
                }
                for a in &m.attributes {
                    variables_of(&a.value, &mut vars);
                }
                out.insert(m.id.name.to_string(), vars);
            }
        }
    }
    out
}

/// One `text!(…)` or `Text::new(…)` in a source: where, the id, the argument names.
struct Call {
    at: String,
    id: String,
    args: BTreeSet<String>,
}

/// The text between the opening paren at `open` and its match.
fn balanced(text: &str, open: usize) -> Option<&str> {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut i = open;
    while i < bytes.len() {
        let c = bytes[i];
        if in_str {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == b'"' {
                in_str = false;
            }
        } else if c == b'"' {
            in_str = true;
        } else if c == b'(' || c == b'[' || c == b'{' {
            depth += 1;
        } else if c == b')' || c == b']' || c == b'}' {
            depth -= 1;
            if depth == 0 {
                return Some(&text[open + 1..i]);
            }
        }
        i += 1;
    }
    None
}

/// The top-level, comma-separated pieces of an argument list.
fn pieces(inner: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut start = 0;
    let bytes = inner.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if in_str {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == b'"' {
                in_str = false;
            }
        } else {
            match c {
                b'"' => in_str = true,
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                b',' if depth == 0 => {
                    out.push(inner[start..i].trim());
                    start = i + 1;
                }
                _ => {}
            }
        }
        i += 1;
    }
    let last = inner[start..].trim();
    if !last.is_empty() {
        out.push(last);
    }
    out
}

fn calls_in(root: &Path) -> Vec<Call> {
    let mut files = Vec::new();
    walk(&root.join("crates"), "rs", &mut files);
    let mut calls = Vec::new();
    for file in files {
        // The domain's own `text.rs` and this crate's tests spell the macro
        // to define and to prove it, not to say anything.
        let path = rel(root, &file);
        if path.ends_with("bisa-core/src/text.rs") || path.starts_with("crates/bisa-i18n/") {
            continue;
        }
        let source = fs::read_to_string(&file).expect("read a source");
        for needle in ["text!(", "Text::new("] {
            let mut from = 0;
            while let Some(found) = source[from..].find(needle) {
                let open = from + found + needle.len() - 1;
                let Some(inner) = balanced(&source, open) else {
                    break;
                };
                let parts = pieces(inner);
                let line = source[..open].matches('\n').count() + 1;
                if let Some(first) = parts.first() {
                    if first.starts_with('"') && first.ends_with('"') && first.len() >= 2 {
                        let id = first[1..first.len() - 1].to_string();
                        let args = parts[1..]
                            .iter()
                            .filter_map(|p| p.split('=').next())
                            .map(|n| n.trim().to_string())
                            .filter(|n| !n.is_empty())
                            .collect();
                        calls.push(Call {
                            at: format!("{path}:{line}"),
                            id,
                            args,
                        });
                    }
                }
                from = open + 1;
            }
        }
    }
    calls
}

/// One registry entry as the source spells it: the key, and a `Choice`'s values.
struct Registered {
    key: String,
    choices: Vec<String>,
}

/// The settings registry's entries, read from `def!(…)` invocations.
fn settings_registry(root: &Path) -> Vec<Registered> {
    let registry = fs::read_to_string(root.join("crates/bisa-core/src/settings.rs"))
        .expect("the settings registry");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(found) = registry[from..].find("def!(") {
        let open = from + found + "def!".len();
        let Some(inner) = balanced(&registry, open) else {
            break;
        };
        let parts = pieces(inner);
        if let Some(first) = parts.first() {
            let key = first.trim_matches('"').to_string();
            let choices = parts
                .get(1)
                .filter(|k| k.contains("Choice("))
                .map(|k| {
                    k.split('"')
                        .skip(1)
                        .step_by(2)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            out.push(Registered { key, choices });
        }
        from = open + 1;
    }
    out
}

/// The git config schema's entries (`bisa-vcs/src/config_schema.rs`).
fn git_config_schema(root: &Path) -> Vec<Registered> {
    let schema = fs::read_to_string(root.join("crates/bisa-vcs/src/config_schema.rs"))
        .expect("the git config schema");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(found) = schema[from..].find("ConfigKeyDef {") {
        let open = from + found + "ConfigKeyDef ".len();
        let Some(inner) = balanced(&schema, open) else {
            break;
        };
        from = open + 1;
        let Some(k) = inner.find("key: \"") else {
            continue;
        };
        let key = inner[k + 6..].split('"').next().unwrap_or("").to_string();
        let choices = inner
            .find("Choice(&[")
            .map(|c| {
                inner[c..]
                    .split(']')
                    .next()
                    .unwrap_or("")
                    .split('"')
                    .skip(1)
                    .step_by(2)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        out.push(Registered { key, choices });
    }
    out
}

fn message_id(prefix: &str, key: &str) -> String {
    format!("{prefix}-{}", key.replace('.', "-"))
}

/// Ids the sources produce by a rule rather than a literal: a setting's
/// message is `setting-<key>`, a git config key's `git-config-<key>`.
fn derived_ids(root: &Path) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = settings_registry(root)
        .iter()
        .map(|r| message_id("setting", &r.key))
        .collect();
    out.extend(
        git_config_schema(root)
            .iter()
            .map(|r| message_id("git-config", &r.key)),
    );
    out
}

/// The attributes of every message in the crate-owned English files.
fn attributes() -> BTreeMap<String, BTreeSet<String>> {
    let mut out = BTreeMap::new();
    for ns in Namespace::ALL {
        let source = ns.source("en").expect("English ships every namespace");
        let resource = match parse_runtime(source) {
            Ok(r) => r,
            Err((r, _)) => r,
        };
        for entry in &resource.body {
            if let ast::Entry::Message(m) = entry {
                out.insert(
                    m.id.name.to_string(),
                    m.attributes.iter().map(|a| a.id.name.to_string()).collect(),
                );
            }
        }
    }
    out
}

#[test]
fn every_setting_and_every_git_config_key_has_its_words_whole() {
    let root = root();
    let attrs = attributes();
    let mut faults = Vec::new();
    let registry = settings_registry(&root);
    assert!(registry.len() > 100, "the registry was read");
    for (prefix, entries, sentence) in [
        ("setting", registry, "help"),
        ("git-config", git_config_schema(&root), "hint"),
    ] {
        assert!(!entries.is_empty(), "{prefix}: entries were read");
        for r in entries {
            let id = message_id(prefix, &r.key);
            let Some(present) = attrs.get(&id) else {
                faults.push(format!(
                    "{}: no message `{id}` in locales/en/settings.ftl",
                    r.key
                ));
                continue;
            };
            if prefix == "git-config" && !present.contains(sentence) {
                faults.push(format!("{id}: no `.{sentence}`"));
            }
            for value in &r.choices {
                if !present.contains(&format!("choice-{value}")) {
                    faults.push(format!(
                        "{id}: no `.choice-{value}` — a choice needs its word"
                    ));
                }
            }
            for attr in present {
                if let Some(value) = attr.strip_prefix("choice-") {
                    if !r.choices.iter().any(|c| c == value) {
                        faults.push(format!(
                            "{id}: `.{attr}` names a value the key does not take"
                        ));
                    }
                } else if attr != sentence && attr != "help" {
                    faults.push(format!("{id}: `.{attr}` is not an attribute a setting has"));
                }
            }
        }
    }
    assert!(
        faults.is_empty(),
        "a registry and the catalog disagree:\n{}",
        faults.join("\n")
    );
}

#[test]
fn every_text_names_a_message_with_its_arguments_and_every_message_is_said() {
    let root = root();
    let messages = messages();
    let calls = calls_in(&root);
    let mut faults = Vec::new();
    let mut said: BTreeSet<String> = BTreeSet::new();
    for call in &calls {
        said.insert(call.id.clone());
        match messages.get(&call.id) {
            None => faults.push(format!(
                "{}: no message `{}` in locales/en",
                call.at, call.id
            )),
            Some(vars) => {
                if &call.args != vars {
                    faults.push(format!(
                        "{}: `{}` is said with {:?} and the message reads {:?}",
                        call.at, call.id, call.args, vars
                    ));
                }
            }
        }
    }
    let derived = derived_ids(&root);
    for id in messages.keys() {
        // The help's words are held to the commands by the CLI's own test
        // (`crates/bisa-cli/src/localize.rs`), not by a `text!` in a source.
        if id.starts_with("cli-cmd-") || id.starts_with("cli-arg-") {
            continue;
        }
        // A display field's translation is read by its entry's id, never by a `text!`.
        if id.starts_with("catalog-") {
            continue;
        }
        if !said.contains(id) && !derived.contains(id) {
            faults.push(format!("locales/en: `{id}` is a message nothing says"));
        }
    }
    assert!(
        faults.is_empty(),
        "the catalog and the sources disagree:\n{}",
        faults.join("\n")
    );
}

/// The content seam: a message in `catalog.ftl` names a shipped entry —
/// `catalog-<kind>-<slug>` for a file under `library/catalog/<kind>s/`,
/// `catalog-pet-<id>` for a package under `library/pets/` — and its
/// attributes are the display fields that kind has. English ships none; a
/// language that does is held to the rule.
#[test]
fn a_content_message_names_a_shipped_entry_and_its_display_fields() {
    let root = root();
    let mut faults = Vec::new();
    for tag in AVAILABLE {
        let source = Namespace::Content
            .source(tag)
            .expect("a shipped language ships the content file");
        let resource = match parse_runtime(source) {
            Ok(r) => r,
            Err((r, _)) => r,
        };
        for entry in &resource.body {
            let ast::Entry::Message(m) = entry else {
                continue;
            };
            let id = m.id.name;
            let Some(rest) = id.strip_prefix("catalog-") else {
                faults.push(format!("{tag}: `{id}` is not a `catalog-<kind>-<slug>` id"));
                continue;
            };
            let Some((kind, slug)) = rest.split_once('-') else {
                faults.push(format!("{tag}: `{id}` names no kind and slug"));
                continue;
            };
            let allowed: &[&str] = match kind {
                "pet" => &["description", "tagline"],
                "agent" | "skill" | "team" | "channel" | "connector" | "workflow" | "addon" => {
                    &["description"]
                }
                _ => {
                    faults.push(format!(
                        "{tag}: `{id}` names a kind the catalog has not: {kind}"
                    ));
                    continue;
                }
            };
            let shipped = if kind == "pet" {
                root.join("library")
                    .join("pets")
                    .join(slug)
                    .join("pet.json")
            } else if kind == "addon" {
                root.join("library")
                    .join("addons")
                    .join(slug)
                    .join("addon.json")
            } else {
                root.join("library")
                    .join("catalog")
                    .join(format!("{kind}s"))
                    .join(format!("{slug}.toml"))
            };
            if !shipped.is_file() {
                faults.push(format!(
                    "{tag}: `{id}` names an entry the library does not ship ({})",
                    rel(&root, &shipped)
                ));
            }
            for attr in &m.attributes {
                if !allowed.contains(&attr.id.name) {
                    faults.push(format!(
                        "{tag}: `{id}.{}` is not a display field of a {kind}",
                        attr.id.name
                    ));
                }
            }
        }
    }
    assert!(
        faults.is_empty(),
        "the content file breaks its rule:\n{}",
        faults.join("\n")
    );
}
