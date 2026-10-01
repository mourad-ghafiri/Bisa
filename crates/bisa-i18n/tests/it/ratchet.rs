//! The ratchet: the count of bare sentences per source equals the committed
//! baseline — a rise is a sentence that should have been a `Text`, a fall is
//! a baseline to lower (`just i18n-baseline`). Read-only.

use bisa_i18n::ratchet::{baseline, compare, SCOPES};
use std::collections::BTreeMap;
use std::path::Path;

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

#[test]
fn the_sources_carry_exactly_the_baseline_s_bare_sentences() {
    let root = root();
    let file = root.join("crates/bisa-i18n/tests/ratchet.baseline.json");
    let text = std::fs::read_to_string(&file).unwrap_or_else(|_| {
        panic!(
            "{}: no baseline yet — run `just i18n-baseline`",
            file.display()
        )
    });
    let was: BTreeMap<String, usize> =
        serde_json::from_str(&text).expect("the baseline is a map of counts");
    let now = baseline(&root, SCOPES);
    let (up, down) = compare(&was, &now);
    assert!(
        up.is_empty(),
        "bare sentences appeared — say them as a `text!` instead:\n{}",
        up.join("\n")
    );
    assert!(
        down.is_empty(),
        "bare sentences went — lower the baseline with `just i18n-baseline`:\n{}",
        down.join("\n")
    );
}
