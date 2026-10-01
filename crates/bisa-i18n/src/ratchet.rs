//! The ratchet: how many bare sentences a source still carries.
//!
//! While the platform's prose moves into the catalog, this scanner counts
//! the string literals that still read as a sentence to a person — a space
//! and a few letters, starting with a letter or a placeable — in the sources
//! of the crates that talk to people (`crates/bisa-node/src`,
//! `crates/bisa-cli/src`). What it leaves out is what may stay English: a
//! log line (`tracing::…`), a panic or an `expect`, an attribute
//! (`#[error(…)]` is the developer's `Display`; `#[serde]`; clap's derive),
//! a route's documentation (`RouteDoc { … }`), a comment, a test module. The
//! count per file is kept in `tests/ratchet.baseline.json`; the test holds
//! the sources to it exactly, so a file that gained a sentence fails and a
//! file that lost one asks for the baseline to be lowered
//! (`just i18n-baseline`). The end state is an empty baseline.
//!
//! A heuristic, on purpose: it need not understand Rust, only see a sentence
//! where one is. This module reads sources; it never writes.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// The directories the ratchet reads, relative to the workspace root.
pub const SCOPES: &[&str] = &["crates/bisa-node/src", "crates/bisa-cli/src"];

/// A word or a construct that says the literals in its reach are not for a
/// person: the exemption lasts while the brackets it opened stay open, and
/// to the end of its own line.
const EXEMPT_OPENERS: &[&str] = &[
    "tracing::",
    "expect(",
    "panic!(",
    "unreachable!(",
    "todo!(",
    "unimplemented!(",
    "assert!(",
    "assert_eq!(",
    "assert_ne!(",
    "debug_assert",
    "env!(",
    "include_str!(",
    "include_bytes!(",
    "concat!(",
    "RouteDoc {",
    "#[",
    // A line that says so: what a model, another program or a file reads.
    "// for the agent",
    "// for the machine",
    "// for the log",
    "// content, never translated",
    "ApiError::internal(",
    // HTTP's own words.
    "header(",
    "CONTENT_TYPE",
    "CACHE_CONTROL",
    "AUTHORIZATION",
    "TRANSFER_ENCODING",
];

/// The comment markers: on a line of their own they exempt the statement
/// below; at the end of a line, that line (`EXEMPT_OPENERS` has them too).
const MARKERS: &[&str] = &[
    "// for the agent",
    "// for the machine",
    "// for the log",
    "// content, never translated",
];

/// Files under the scopes whose English is not for a person: a wire
/// protocol another program speaks, the reference's section titles a docs
/// generator writes, a generator itself.
const NOT_FOR_A_PERSON: &[&str] = &["/a2a.rs", "/route_docs.rs", "/bin/"];

/// One literal that reads as a sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub line: usize,
    pub literal: String,
}

/// Whether a literal reads as a sentence: a space, at least three letters,
/// and a first character that is a letter or a placeable's brace.
pub fn is_prose(literal: &str) -> bool {
    let t = literal.trim();
    let Some(first) = t.chars().next() else {
        return false;
    };
    if !(first.is_alphabetic() || first == '{') {
        return false;
    }
    if !t.contains(' ') {
        return false;
    }
    // A placeable is one token, whatever it interpolates; `{{` is a brace.
    let bare = strip_placeables(t);
    // Code, a header's grammar, a path: `;`, `=`, `/` between letters, `<`.
    if bare.contains(';') || bare.contains('=') || bare.contains('<') || bare.contains('>') {
        return false;
    }
    let words: Vec<&str> = bare
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| w.chars().count() >= 2)
        .collect();
    // An all-caps token list (`HTTP CSP`) is not a sentence.
    if !words.is_empty() && words.iter().all(|w| w.chars().all(|c| c.is_uppercase())) {
        return false;
    }
    // Two words, or one that begins a sentence.
    if words.len() < 2 && !first.is_uppercase() {
        return false;
    }
    bare.chars().filter(|c| c.is_alphabetic()).count() >= 3
}

/// The literal with every `{…}` placeable taken out and `{{`/`}}` read as braces.
fn strip_placeables(t: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    let chars: Vec<char> = t.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '{' && chars.get(i + 1) == Some(&'{') {
            out.push('{');
            i += 2;
            continue;
        }
        if c == '}' && chars.get(i + 1) == Some(&'}') {
            out.push('}');
            i += 2;
            continue;
        }
        if c == '{' {
            depth += 1;
        } else if c == '}' && depth > 0 {
            depth -= 1;
        } else if depth == 0 {
            out.push(c);
        }
        i += 1;
    }
    out
}

fn bracket_delta(c: char) -> i32 {
    match c {
        '(' | '[' | '{' => 1,
        ')' | ']' | '}' => -1,
        _ => 0,
    }
}

/// The sentences of one Rust source, in order.
pub fn prose_literals(source: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    // The depth an exemption began at, while one is open.
    let mut exempt_from: Option<i32> = None;
    let mut raw: Option<(usize, String, usize)> = None; // (hashes, buffer, line) of a raw string under way
    let mut plain: Option<(String, usize)> = None; // a "…" string under way (may span lines)
                                                   // A marker on a comment line of its own — `// for the log: …` — exempts the statement below it.
    let mut marked_next = false;
    // A `#[cfg(test)]` was read and its item has not begun yet; then the
    // item under way: the depth it began at, and whether its body opened.
    // It ends where that body closes, or — with no body — at its `;`.
    let mut test_item_next = false;
    let mut test_item: Option<(i32, bool)> = None;
    for (index, line) in source.lines().enumerate() {
        let number = index + 1;
        let trimmed = line.trim_start();
        if raw.is_none() && plain.is_none() {
            // A file that says at its head it is test code is a fixture whole.
            if trimmed.starts_with("#![cfg(test)]") {
                break;
            }
            // `#[cfg(test)]` exempts the item it stands on, and that item
            // alone: a module with its body, a function, a `use`, a module
            // declared by name (`mod verbs;`, whose file says so itself).
            // Read as the end of the file — which is only where a file's
            // tests usually stand — it hid every sentence below it.
            if trimmed.starts_with("#[cfg(test)]") {
                test_item_next = true;
                continue;
            }
        }
        let mut exempt_line = false;
        if raw.is_none() && plain.is_none() {
            if trimmed.starts_with("//") {
                if MARKERS.iter().any(|m| trimmed.starts_with(m)) {
                    marked_next = true;
                }
                continue;
            }
            // The item begins on the first line under `#[cfg(test)]` that is
            // no further attribute of it.
            if test_item_next && !trimmed.starts_with("#[") {
                test_item_next = false;
                test_item = Some((depth, false));
            }
            if marked_next {
                marked_next = false;
                exempt_line = true;
                if exempt_from.is_none() {
                    exempt_from = Some(depth);
                }
            }
            let opener_at = EXEMPT_OPENERS.iter().filter_map(|o| line.find(o)).min();
            if let Some(at) = opener_at {
                exempt_line = true;
                if exempt_from.is_none() {
                    let before = &line[..at];
                    let d: i32 = before.chars().map(bracket_delta).sum();
                    exempt_from = Some(depth + d);
                }
            }
        }
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        // The test item under way ends on this line: exempt to its end.
        let mut test_item_over = false;
        while i < chars.len() {
            let c = chars[i];
            if let Some((hashes, buf, at)) = raw.as_mut() {
                // Inside r#"…"#: ends at `"` followed by `hashes` hashes.
                if c == '"'
                    && chars[i + 1..]
                        .iter()
                        .take(*hashes)
                        .filter(|h| **h == '#')
                        .count()
                        == *hashes
                {
                    let (literal, started) = (buf.clone(), *at);
                    let exempt = exempt_from.is_some() || exempt_line || test_item.is_some();
                    if !exempt && is_prose(&literal) {
                        out.push(Finding {
                            line: started,
                            literal,
                        });
                    }
                    i += 1 + *hashes;
                    raw = None;
                    continue;
                }
                buf.push(c);
                i += 1;
                continue;
            }
            if let Some((buf, at)) = plain.as_mut() {
                if c == '\\' {
                    if let Some(next) = chars.get(i + 1) {
                        buf.push(*next);
                    }
                    i += 2;
                    continue;
                }
                if c == '"' {
                    let (literal, started) = (buf.clone(), *at);
                    let exempt = exempt_from.is_some() || exempt_line || test_item.is_some();
                    if !exempt && is_prose(&literal) {
                        out.push(Finding {
                            line: started,
                            literal,
                        });
                    }
                    plain = None;
                    i += 1;
                    continue;
                }
                buf.push(c);
                i += 1;
                continue;
            }
            // Outside any string.
            if c == '/' && chars.get(i + 1) == Some(&'/') {
                break;
            }
            if c == '\'' {
                // A char literal, `'"'` or `'\''`: skip it whole.
                if chars.get(i + 1) == Some(&'\\') {
                    i += 4;
                } else {
                    i += 3;
                }
                continue;
            }
            if c == 'r' && (i == 0 || !chars[i - 1].is_alphanumeric() && chars[i - 1] != '_') {
                let mut j = i + 1;
                let mut hashes = 0;
                while chars.get(j) == Some(&'#') {
                    hashes += 1;
                    j += 1;
                }
                if chars.get(j) == Some(&'"') {
                    raw = Some((hashes, String::new(), number));
                    i = j + 1;
                    continue;
                }
            }
            if c == '"' {
                plain = Some((String::new(), number));
                i += 1;
                continue;
            }
            depth += bracket_delta(c);
            if let Some(from) = exempt_from {
                if depth <= from && !exempt_line {
                    exempt_from = None;
                }
            }
            if let Some((from, opened)) = test_item.as_mut() {
                if c == '{' && depth == *from + 1 {
                    *opened = true;
                }
                let closed = c == '}' && *opened && depth == *from;
                let declared = c == ';' && !*opened && depth == *from;
                test_item_over |= closed || declared;
            }
            i += 1;
        }
        if test_item_over {
            test_item = None;
        }
        if let Some(from) = exempt_from {
            // An exemption that opened nothing ends with its line.
            if depth <= from {
                exempt_from = None;
            }
        }
        if plain.is_some() {
            if let Some((buf, _)) = plain.as_mut() {
                buf.push('\n');
            }
        }
        if let Some((_, buf, _)) = raw.as_mut() {
            buf.push('\n');
        }
    }
    out
}

fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
}

/// The count of sentences per source file under `scopes` (paths relative to
/// `root`), files with none left out — the shape of the baseline.
pub fn baseline(root: &Path, scopes: &[&str]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for scope in scopes {
        let mut files = Vec::new();
        walk(&root.join(scope), &mut files);
        for file in files {
            if not_for_a_person(&file) {
                continue;
            }
            let Ok(source) = fs::read_to_string(&file) else {
                continue;
            };
            let n = prose_literals(&source).len();
            if n > 0 {
                let rel = file
                    .strip_prefix(root)
                    .unwrap_or(&file)
                    .to_string_lossy()
                    .replace('\\', "/");
                counts.insert(rel, n);
            }
        }
    }
    counts
}

/// Every bare sentence under the scopes, as `path:line: literal` — the list
/// behind the counts, for the pass that empties a file (`i18n-ratchet --list`).
pub fn listing(root: &Path, scopes: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for scope in scopes {
        let mut files = Vec::new();
        walk(&root.join(scope), &mut files);
        for file in files {
            if not_for_a_person(&file) {
                continue;
            }
            let Ok(source) = fs::read_to_string(&file) else {
                continue;
            };
            let rel = file
                .strip_prefix(root)
                .unwrap_or(&file)
                .to_string_lossy()
                .replace('\\', "/");
            for f in prose_literals(&source) {
                out.push(format!("{rel}:{}: {}", f.line, f.literal));
            }
        }
    }
    out
}

/// Whether a file's English is not for a person (`NOT_FOR_A_PERSON`).
fn not_for_a_person(file: &Path) -> bool {
    let s = file.to_string_lossy().replace('\\', "/");
    NOT_FOR_A_PERSON.iter().any(|n| s.contains(n))
}

/// What changed between the baseline and now: files that rose, files that fell.
pub fn compare(
    baseline: &BTreeMap<String, usize>,
    now: &BTreeMap<String, usize>,
) -> (Vec<String>, Vec<String>) {
    let mut up = Vec::new();
    let mut down = Vec::new();
    for (file, n) in now {
        let was = baseline.get(file).copied().unwrap_or(0);
        if *n > was {
            up.push(format!("{file}: {was} → {n}"));
        } else if *n < was {
            down.push(format!("{file}: {was} → {n}"));
        }
    }
    for (file, was) in baseline {
        if !now.contains_key(file) {
            down.push(format!("{file}: {was} → 0"));
        }
    }
    (up, down)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn literals(src: &str) -> Vec<String> {
        prose_literals(src).into_iter().map(|f| f.literal).collect()
    }

    #[test]
    fn a_sentence_has_a_space_and_letters_and_starts_like_one() {
        assert!(is_prose("the goal was not found"));
        assert!(is_prose("{name} is not a goal"));
        assert!(!is_prose("goal_not_found"));
        assert!(!is_prose("application/json"));
        assert!(!is_prose("/goals/{id}"));
        assert!(
            !is_prose("a b"),
            "two letters are an abbreviation, not a sentence"
        );
        assert!(!is_prose("--flag value"));
    }

    #[test]
    fn literals_are_found_and_a_log_line_a_panic_and_an_attribute_are_not() {
        let src = r##"
fn f() -> ApiError {
    tracing::warn!(
        target: "bisa_node",
        "the quit request did not reach the webview: {e}"
    );
    let x = map.get("k").expect("the map has the key we put");
    ApiError::new(StatusCode::NOT_FOUND, "no goal by that id")
}
#[error("a developer sentence for the log")]
struct E;
const R: &[RouteDoc] = &[RouteDoc {
    method: "GET",
    path: "/x",
    summary: "Every pet the platform ships, first.",
}];
"##;
        assert_eq!(literals(src), vec!["no goal by that id".to_string()]);
    }

    #[test]
    fn a_raw_string_a_comment_and_the_test_module_are_read_right() {
        let src = "let a = r#\"a raw sentence with \"quotes\"\"#;\n// a comment \"with a sentence\"\nlet b = \"a plain one\"; // and \"another\"\nlet c = '\"';\nlet d = \"not\";\n#[cfg(test)]\nmod tests { let e = \"a test sentence\"; }\n";
        assert_eq!(
            literals(src),
            vec![
                "a raw sentence with \"quotes\"".to_string(),
                "a plain one".to_string()
            ]
        );
    }

    /// `#[cfg(test)]` exempts the item it stands on and nothing after it:
    /// a module declared by name, a function with its body, a module under
    /// a second attribute — and the sentences below each are still read. A
    /// file that says at its head it is test code is a fixture whole.
    #[test]
    fn a_test_item_is_exempt_and_what_stands_below_it_is_still_read() {
        let src = "#[cfg(test)]\nmod verbs;\nlet a = \"a sentence below a declared module\";\n#[cfg(test)]\npub fn for_a_test(\n    x: u8,\n) -> String {\n    \"a test sentence\".to_string()\n}\nlet b = \"a sentence below a test function\";\n#[cfg(test)]\n#[allow(dead_code)]\nmod tests {\n    fn f() { let c = \"another test sentence\"; }\n}\nlet d = \"a sentence below a test module\";\n";
        assert_eq!(
            literals(src),
            vec![
                "a sentence below a declared module".to_string(),
                "a sentence below a test function".to_string(),
                "a sentence below a test module".to_string(),
            ]
        );
        let fixture = "//! A guard, compiled under test alone.\n#![cfg(test)]\nconst WHY: &str = \"a road a test takes\";\n";
        assert!(literals(fixture).is_empty());
    }

    #[test]
    fn a_marker_exempts_its_line_or_the_statement_below_and_a_header_is_not_prose() {
        let src = "let a = json!({\"error\": \"rate limit exceeded\"}); // for the machine\n// for the log: a startup fault\nlet b = Error::other(\"the token file is empty\");\nlet c = \"a sentence for a person\";\n";
        assert_eq!(literals(src), vec!["a sentence for a person".to_string()]);
        assert!(!is_prose("text/html; charset=utf-8"));
        assert!(
            !is_prose("{mark} {:<26} {:<20}"),
            "a table row of placeables"
        );
        assert!(!is_prose("HTTP CSP"), "an all-caps token list");
        assert!(is_prose("{scope}✓ result accepted"));
    }

    #[test]
    fn the_comparison_names_what_rose_and_what_fell() {
        let was = BTreeMap::from([("a.rs".to_string(), 2), ("b.rs".to_string(), 1)]);
        let now = BTreeMap::from([("a.rs".to_string(), 3), ("c.rs".to_string(), 1)]);
        let (up, down) = compare(&was, &now);
        assert_eq!(up, vec!["a.rs: 2 → 3", "c.rs: 0 → 1"]);
        assert_eq!(down, vec!["b.rs: 1 → 0"]);
    }
}
