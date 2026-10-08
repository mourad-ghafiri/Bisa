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

/// The ratchet's own command, in its report form: `i18n-ratchet` with no
/// flag reads the committed baseline and the sources and says what rose and
/// what fell — nothing, while the test above holds. The command runs here so
/// its report is held by a test and not only by the recipe that calls it.
#[test]
fn the_ratchet_command_reports_nothing_rose_and_nothing_fell() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_i18n-ratchet"))
        .current_dir(root())
        .output()
        .expect("run i18n-ratchet");
    assert!(
        out.status.success(),
        "i18n-ratchet exited {:?}: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("the report is text");
    let last = stdout.lines().last().unwrap_or_default();
    assert!(
        last.ends_with("0 rose, 0 fell"),
        "the report's last line counts what moved: {stdout}"
    );
    assert!(
        !stdout
            .lines()
            .any(|l| l.starts_with("up ") || l.starts_with("down ")),
        "a sentence rose or fell against the baseline:\n{stdout}"
    );
}
