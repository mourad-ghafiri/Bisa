//! The lock rule, as a build failure: **nothing under an `idx()` guard locks
//! the index again.** The index lock is a plain mutex, so a method that holds
//! the guard and calls one that locks — or a loop that iterates a locked read
//! while its body reads through the workspace — hangs the thread forever. Two
//! shapes are refused here by reading the sources:
//!
//! 1. a `*_in(&self, idx: &Index, …)` method — the projection a caller runs
//!    under its own transaction — never calls `self.idx()` or a workspace
//!    reader that would (`self.get_*`);
//! 2. a `for … in self.idx().…` loop, whose guard lives for the whole loop.
//!
//! A debug build also turns a re-entry into a panic naming the rule
//! (`Workspace::idx`), so a test that hits one fails in milliseconds rather
//! than at the runner's timeout. This file reads sources; it never writes,
//! deletes or runs anything.

use std::path::{Path, PathBuf};

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    // A folder the guard cannot walk is a rule it does not hold: said,
    // never passed over.
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("the guard cannot read {}: {e}", dir.display()));
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The code of every source, comments stripped line by line.
fn sources() -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    rust_sources(&src_dir(), &mut files);
    files.sort();
    files
        .into_iter()
        .map(|p| {
            let text = std::fs::read_to_string(&p).expect("source");
            let code = text
                .lines()
                .map(|l| l.split("//").next().unwrap_or(""))
                .collect::<Vec<_>>()
                .join("\n");
            (p, code)
        })
        .collect()
}

/// The body of every `fn <name>_in(` that takes an `&Index`, by brace depth.
fn in_method_bodies(code: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut search = 0;
    while let Some(at) = code[search..].find("fn ") {
        let start = search + at;
        search = start + 3;
        let rest = &code[start..];
        let Some(paren) = rest.find('(') else {
            continue;
        };
        let name = rest[3..paren].trim();
        if !name.ends_with("_in") {
            continue;
        }
        let Some(open) = rest.find('{') else { continue };
        let signature = &rest[..open];
        if !signature.contains("Index") {
            continue;
        }
        let mut depth = 0i32;
        let mut end = None;
        for (i, c) in rest[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + i);
                        break;
                    }
                }
                _ => {}
            }
        }
        if let Some(end) = end {
            out.push((name.to_string(), rest[open..=end].to_string()));
        }
    }
    out
}

#[test]
fn a_method_that_takes_the_index_never_locks_it_again() {
    let mut seen = 0;
    let mut offences = Vec::new();
    for (path, code) in sources() {
        for (name, body) in in_method_bodies(&code) {
            seen += 1;
            if body.contains("self.idx()") {
                offences.push(format!("{}: {name} calls self.idx()", path.display()));
            }
            // A workspace reader locks the index itself; under a guard it is
            // the row on the `Index` that is wanted.
            for reader in ["self.get_run(", "self.get_goal(", "self.get_workflow("] {
                if body.contains(reader) {
                    offences.push(format!(
                        "{}: {name} reads through the workspace ({reader}…) while holding the index",
                        path.display()
                    ));
                }
            }
        }
    }
    assert!(seen >= 3, "the `*_in` projections were found ({seen})");
    assert!(
        offences.is_empty(),
        "a method under the index guard must use the `&Index` it was handed:\n{}",
        offences.join("\n")
    );
}

#[test]
fn no_loop_iterates_a_locked_read() {
    let mut offences = Vec::new();
    for (path, code) in sources() {
        let flat: String = code.split_whitespace().collect::<Vec<_>>().join(" ");
        for (n, window) in flat.match_indices("in self.idx()") {
            // `for x in self.idx().rows()? { … }` keeps the guard for the body.
            let before = &flat[n.saturating_sub(40)..n];
            if before.contains("for ") {
                offences.push(format!(
                    "{}: a `for` loop over `self.idx()` — bind the rows first ({window})",
                    path.display()
                ));
            }
        }
        // `match self.idx().row()? { … }`, `if let … = self.idx().row()? { … }`
        // and `while let …` keep the guard through every arm and the body the
        // same way: the statement the read sits in must be a plain `let`.
        for (n, _) in flat.match_indices("self.idx()") {
            let statement = flat[..n].rsplit([';', '{', '}']).next().unwrap_or("");
            if statement.contains("match ")
                || statement.contains("if let ")
                || statement.contains("while let ")
            {
                offences.push(format!(
                    "{}: `{}self.idx()…` holds the guard through its arms — bind the read to a `let` first",
                    path.display(),
                    statement.trim_start()
                ));
            }
        }
    }
    assert!(
        offences.is_empty(),
        "a loop must not hold the index guard while its body runs:\n{}",
        offences.join("\n")
    );
}
