//! The four generator binaries beside the node (`src/bin/`) answer exactly
//! the pages and the schema committed in the tree: `docs/reference/http-api.md`,
//! `docs/reference/settings-keys.md`, `docs/reference/catalog.md` and
//! `desktop/api-schema.json`. The `check-*` recipes and CI's `types` job diff
//! the same outputs; this is the same fact held by a test, so the binaries
//! run under the test gate — and under the coverage meter — rather than only
//! from a recipe.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

/// The binary run from the workspace root with no arguments: its stdout,
/// asserted to be exactly the committed file's bytes.
fn generates(binary: &str, committed: &str) {
    let root = root();
    let out = Command::new(binary)
        .current_dir(&root)
        .output()
        .unwrap_or_else(|e| panic!("{binary}: {e}"));
    assert!(
        out.status.success(),
        "{binary} exited {:?}: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let want = std::fs::read(root.join(committed)).unwrap_or_else(|e| panic!("{committed}: {e}"));
    assert!(
        out.stdout == want,
        "{committed} is stale against {binary} — regenerate it (docs/contributing/release.md § The generated files)"
    );
}

/// The committed page is what `cargo run -p bisa-node --bin api-docs`
/// renders — the node alone, so without the `a2a` feature. A workspace
/// build unifies the CLI's features into the node, and then the same binary
/// renders the same page with one more section, the A2A routes, among the
/// route tables: held here under either feature set, so `cargo test
/// --workspace` and `cargo test -p bisa-node` read the same fact.
#[test]
fn the_http_reference_is_what_api_docs_renders() {
    if !cfg!(feature = "a2a") {
        generates(env!("CARGO_BIN_EXE_api-docs"), "docs/reference/http-api.md");
        return;
    }
    let root = root();
    let out = Command::new(env!("CARGO_BIN_EXE_api-docs"))
        .current_dir(&root)
        .output()
        .expect("run api-docs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let page = std::fs::read_to_string(root.join("docs/reference/http-api.md"))
        .expect("docs/reference/http-api.md");
    let rendered = String::from_utf8(out.stdout).expect("the page is text");
    // The one section the feature adds, from its heading to the next.
    let heading = "## Agent-to-agent (feature `a2a`)\n";
    let at = rendered.find(heading).unwrap_or_else(|| {
        panic!("under the a2a feature the page carries the A2A section: {rendered}")
    });
    let after = rendered[at + heading.len()..]
        .find("\n## ")
        .map(|i| at + heading.len() + i + 1)
        .expect("a section follows the A2A routes");
    let section = &rendered[at..after];
    assert!(
        section.contains("| `POST /a2a` |")
            && section.contains("| `GET /.well-known/agent-card.json` |"),
        "the A2A section lists its two routes: {section}"
    );
    let without = format!("{}{}", &rendered[..at], &rendered[after..]);
    assert!(
        without == page,
        "docs/reference/http-api.md is stale against api-docs — regenerate it (docs/contributing/release.md § The generated files)"
    );
}

#[test]
fn the_settings_reference_is_what_settings_docs_renders() {
    generates(
        env!("CARGO_BIN_EXE_settings-docs"),
        "docs/reference/settings-keys.md",
    );
}

#[test]
fn the_catalog_reference_is_what_catalog_docs_renders() {
    generates(
        env!("CARGO_BIN_EXE_catalog-docs"),
        "docs/reference/catalog.md",
    );
}

#[test]
fn the_desktops_schema_bundle_is_what_api_schema_emits() {
    generates(env!("CARGO_BIN_EXE_api-schema"), "desktop/api-schema.json");
}
