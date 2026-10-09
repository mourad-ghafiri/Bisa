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

use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let file = root.join("crates/bisa-i18n/tests/ratchet.baseline.json");
    let args: Vec<String> = std::env::args().skip(1).collect();
    print!("{}", bisa_i18n::ratchet::run(&args, &root, &file));
}
