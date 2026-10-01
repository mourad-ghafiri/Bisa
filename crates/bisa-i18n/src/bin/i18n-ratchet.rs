//! The ratchet's baseline: `--write` records how many bare sentences each
//! source under the ratchet's scopes still carries
//! (`crates/bisa-i18n/tests/ratchet.baseline.json`); without it, the
//! report — what rose, what fell — against the committed baseline.
//! `just i18n-baseline` runs the write; the test `tests/it/ratchet.rs` holds
//! the sources to the file exactly. `--list` prints every sentence with its
//! file and line — the work list for emptying a file; `--list --scope
//! <dir>` (repeatable, relative to the workspace root) reads other
//! folders than the ratchet's own — a look at a crate before it is brought
//! under the ratchet.

use bisa_i18n::ratchet::{baseline, compare, listing, SCOPES};
use std::collections::BTreeMap;
use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let file = root.join("crates/bisa-i18n/tests/ratchet.baseline.json");
    let args: Vec<String> = std::env::args().collect();
    let asked: Vec<&str> = args
        .windows(2)
        .filter(|pair| pair[0] == "--scope")
        .map(|pair| pair[1].as_str())
        .collect();
    let scopes: &[&str] = if asked.is_empty() { SCOPES } else { &asked };
    if args.iter().any(|a| a == "--list") {
        for line in listing(&root, scopes) {
            println!("{line}");
        }
        return;
    }
    let now = baseline(&root, SCOPES);
    let write = std::env::args().any(|a| a == "--write");
    if write {
        let json = serde_json::to_string_pretty(&now).expect("a map of counts is JSON");
        std::fs::write(&file, format!("{json}\n")).expect("write the baseline");
        println!(
            "{} files with bare sentences, {} sentences — written to {}",
            now.len(),
            now.values().sum::<usize>(),
            file.display()
        );
        return;
    }
    let was: BTreeMap<String, usize> = std::fs::read_to_string(&file)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let (up, down) = compare(&was, &now);
    for line in &up {
        println!("up    {line}");
    }
    for line in &down {
        println!("down  {line}");
    }
    println!(
        "{} files, {} sentences now; {} rose, {} fell",
        now.len(),
        now.values().sum::<usize>(),
        up.len(),
        down.len()
    );
}
