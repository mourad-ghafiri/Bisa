//! `docs/architecture/08-persistence.md` states the index schema and its version.
//! This keeps the statement equal to the code: the doc's `sql` block holds the
//! same `CREATE` statements as [`bisa_store::SCHEMA`], and its `rust` block
//! quotes the current `SCHEMA_VERSION`.
//!
//! This file reads sources; it never writes, deletes or runs anything.

use std::collections::BTreeSet;
use std::path::Path;

fn doc() -> String {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/architecture/08-persistence.md");
    std::fs::read_to_string(&path).expect("read docs/architecture/08-persistence.md")
}

/// The bodies of every fenced block opened with ```<lang>.
fn fenced(text: &str, lang: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        match &mut current {
            Some(body) => {
                if line.trim_end() == "```" {
                    out.push(std::mem::take(body));
                    current = None;
                } else {
                    body.push_str(line);
                    body.push('\n');
                }
            }
            None => {
                if line.trim_end() == format!("```{lang}") {
                    current = Some(String::new());
                }
            }
        }
    }
    out
}

/// Every `CREATE …` statement, comments stripped and whitespace collapsed, so
/// the comparison is about what the schema says and not how it is laid out.
fn statements(sql: &str) -> BTreeSet<String> {
    let stripped: String = sql
        .lines()
        .map(|l| l.split("--").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    stripped
        .split(';')
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|s| s.starts_with("CREATE"))
        .collect()
}

#[test]
fn the_documented_schema_version_is_the_codes() {
    let needle = format!(
        "const SCHEMA_VERSION: i32 = {};",
        bisa_store::SCHEMA_VERSION
    );
    let blocks = fenced(&doc(), "rust");
    assert!(
        blocks.iter().any(|b| b.contains(&needle)),
        "docs/architecture/08-persistence.md must quote `{needle}`"
    );
}

#[test]
fn the_documented_schema_is_the_codes() {
    let blocks = fenced(&doc(), "sql");
    let documented: BTreeSet<String> = blocks.iter().flat_map(|b| statements(b)).collect();
    let code = statements(bisa_store::SCHEMA);
    assert!(!code.is_empty(), "the schema has CREATE statements");
    let missing: Vec<&String> = code.difference(&documented).collect();
    let extra: Vec<&String> = documented.difference(&code).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "docs/architecture/08-persistence.md and bisa-store's SCHEMA differ.\nin the code and not the doc:\n{}\nin the doc and not the code:\n{}",
        missing.iter().map(|s| format!("  {s}")).collect::<Vec<_>>().join("\n"),
        extra.iter().map(|s| format!("  {s}")).collect::<Vec<_>>().join("\n"),
    );
}
