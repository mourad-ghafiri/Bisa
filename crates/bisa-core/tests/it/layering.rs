//! The layering rules, as build failures rather than review comments.
//!
//! `docs/architecture/07-layering.md` names the crate graph, the no-I/O rule
//! for this crate, the no-panic rule for this crate, and the testing rule that
//! no test source may contain a destructive command. Each is a `cargo test`
//! failure here.
//!
//! **This file reads sources and manifests; it never writes, deletes or runs
//! anything.**

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

/// The allowed dependency edges, exactly. A crate not listed here has no
/// internal dependencies.
fn allowed_edges() -> BTreeMap<&'static str, Vec<&'static str>> {
    BTreeMap::from([
        // The host grammar is a leaf the domain validates with: no I/O, no
        // Bisa dependency of its own.
        ("bisa-core", vec!["bisa-netrules"]),
        ("bisa-netrules", vec![]),
        ("bisa-cache", vec![]),
        ("bisa-http", vec![]),
        ("bisa-log", vec![]),
        ("bisa-vcs", vec![]),
        ("bisa-codehost", vec!["bisa-http"]),
        ("bisa-connectors", vec!["bisa-http", "bisa-netrules"]),
        // The Decision-Making Agent's providers: the core's typed question,
        // and the connectors' scrubbed HTTP for a provider reached over a wire.
        ("bisa-decision", vec!["bisa-core", "bisa-connectors"]),
        ("bisa-ssh", vec![]),
        ("bisa-mobile-development", vec![]),
        ("bisa-lsp", vec![]),
        ("bisa-security", vec!["bisa-netrules"]),
        // The catalog and its rendering: the one place a `Text` becomes a
        // sentence in a language; the edges that talk to people depend on it.
        ("bisa-i18n", vec!["bisa-core"]),
        ("bisa-iso", vec!["bisa-vcs"]),
        ("bisa-harness", vec!["bisa-cache", "bisa-core"]),
        (
            "bisa-adapters",
            vec!["bisa-core", "bisa-harness", "bisa-http"],
        ),
        ("bisa-mcp", vec!["bisa-core"]),
        // The MCP client: the core's server configuration, the one HTTP
        // client set for the Streamable HTTP and SSE transports.
        ("bisa-mcp-probe", vec!["bisa-core", "bisa-http"]),
        ("bisa-store", vec!["bisa-core", "bisa-harness"]),
        // The collaboration wire and the two ends that speak it: the guest's
        // replica, and the host's pump over the store.
        ("bisa-collab", vec!["bisa-core"]),
        (
            "bisa-guest",
            vec!["bisa-core", "bisa-collab", "bisa-security"],
        ),
        ("bisa-net", vec!["bisa-core", "bisa-collab", "bisa-store"]),
        (
            "bisa-engine",
            vec![
                "bisa-cache",
                "bisa-log",
                "bisa-core",
                "bisa-store",
                "bisa-harness",
                "bisa-iso",
                "bisa-vcs",
                "bisa-codehost",
                "bisa-connectors",
                "bisa-ssh",
                "bisa-mobile-development",
                "bisa-lsp",
                "bisa-security",
                "bisa-http",
                "bisa-collab",
                "bisa-decision",
                // The MCP health check dials a registered server through the
                // probe crate — the client, never the tool surface.
                "bisa-mcp-probe",
            ],
        ),
        (
            // The node hosts and joins workspaces through the wire crates; the
            // relay pump itself is the CLI's (`bisa node` runs it), not the node's.
            "bisa-node",
            vec![
                "bisa-cache",
                "bisa-core",
                "bisa-i18n",
                "bisa-engine",
                "bisa-store",
                "bisa-harness",
                "bisa-vcs",
                "bisa-collab",
                "bisa-guest",
                "bisa-log",
                "bisa-mcp-probe",
            ],
        ),
        (
            "bisa-cli",
            vec![
                "bisa-core",
                "bisa-i18n",
                "bisa-log",
                "bisa-store",
                "bisa-harness",
                "bisa-adapters",
                "bisa-engine",
                "bisa-vcs",
                "bisa-net",
                "bisa-collab",
                "bisa-guest",
                "bisa-node",
                "bisa-mcp",
                "bisa-mcp-probe",
                "bisa-http",
                "bisa-mobile-development",
            ],
        ),
    ])
}

/// The `[dependencies]` section's internal crate names, from a manifest.
fn internal_deps(manifest: &str) -> Vec<String> {
    let mut in_deps = false;
    let mut out = Vec::new();
    for line in manifest.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_deps = l == "[dependencies]";
            continue;
        }
        if !in_deps || l.is_empty() || l.starts_with('#') {
            continue;
        }
        let name = l.split(['=', ' ']).next().unwrap_or("").trim();
        // `bisa-deps` is the dependency-unification crate every member
        // names so each third-party dependency is built with one feature set
        // (docs/architecture/crates/deps.md). It holds no code and is not an
        // edge of the architecture; the graph below is about who may call whom.
        if name.starts_with("bisa-") && name != "bisa-deps" {
            out.push(name.to_string());
        }
    }
    out.sort();
    out
}

#[test]
fn the_crate_graph_has_exactly_the_documented_edges() {
    let root = workspace_root();
    let allowed = allowed_edges();
    let mut seen = Vec::new();
    for entry in fs::read_dir(root.join("crates")).expect("crates/") {
        let dir = entry.expect("entry").path();
        let manifest_path = dir.join("Cargo.toml");
        if !manifest_path.exists() {
            continue;
        }
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        if name == "bisa-deps" {
            // The unification crate names every member and is named by every
            // member; it is tooling, not an edge (`internal_deps` skips it too).
            continue;
        }
        let manifest = fs::read_to_string(&manifest_path).expect("manifest");
        let deps = internal_deps(&manifest);
        let mut want: Vec<String> = allowed
            .get(name.as_str())
            .unwrap_or_else(|| panic!("{name} is not in the documented crate graph"))
            .iter()
            .map(|s| s.to_string())
            .collect();
        want.sort();
        assert_eq!(
            deps, want,
            "{name}: dependency edges differ from docs/architecture/07"
        );
        seen.push(name);
    }
    for name in allowed.keys() {
        assert!(
            seen.contains(&name.to_string()),
            "{name} is documented but missing"
        );
    }
}

#[test]
fn core_performs_no_io() {
    let manifest = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        .expect("manifest");
    for forbidden in [
        "tokio",
        "rusqlite",
        "axum",
        "reqwest",
        "hyper",
        "sqlx",
        "async-std",
    ] {
        for line in manifest.lines() {
            let l = line.trim();
            let name = l.split(['=', ' ']).next().unwrap_or("");
            assert_ne!(name, forbidden, "bisa-core must not depend on {forbidden}");
        }
    }
}

/// Production code — everything before `#[cfg(test)]` in each file.
/// Every `.rs` under `dir`, recursively, cut at its `#[cfg(test)]` module.
fn production_sources(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).expect("src/") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            out.extend(production_sources(&path));
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let text = fs::read_to_string(&path).expect("read source");
        let prod = match text.find("#[cfg(test)]") {
            Some(at) => text[..at].to_string(),
            None => text,
        };
        out.push((path, prod));
    }
    out
}

#[test]
fn core_never_panics_in_production_code() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offences = Vec::new();
    for (path, text) in production_sources(&src) {
        for (n, line) in text.lines().enumerate() {
            let l = line.trim_start();
            if l.starts_with("//") {
                continue;
            }
            for needle in [
                ".unwrap()",
                ".expect(",
                "panic!(",
                "unreachable!(",
                "todo!(",
                "unimplemented!(",
            ] {
                if l.contains(needle) {
                    offences.push(format!(
                        "{}:{}: {}",
                        path.file_name().unwrap().to_string_lossy(),
                        n + 1,
                        l
                    ));
                }
            }
        }
    }
    assert!(
        offences.is_empty(),
        "bisa-core production code must not panic:\n{}",
        offences.join("\n")
    );
}

/// Every test source in the repository: `crates/*/tests/**/*.rs`, `#[cfg(test)]`
/// blocks are covered by the same rule but live in production files, so the
/// grep here is over whole files under `tests/` and every `*.test.mjs`.
fn test_sources(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        // A folder the guard cannot walk is a rule it does not hold: said,
        // never passed over.
        let entries = fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("the guard cannot read {}: {e}", dir.display()));
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if path.is_dir() {
                if name == "node_modules" || name == "target" || name == "lab" || name == ".git" {
                    continue;
                }
                walk(&path, out);
            } else if (path.to_string_lossy().contains("/tests/")
                && path.extension().and_then(|e| e.to_str()) == Some("rs"))
                || name.ends_with(".test.mjs")
            {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(&root.join("crates"), &mut out);
    walk(&root.join("desktop/src"), &mut out);
    walk(&root.join("desktop/src-tauri"), &mut out);
    // The scripts' own model tests (`scripts/licences`, `scripts/release`).
    walk(&root.join("scripts"), &mut out);
    out
}

/// No test may execute a destructive command. Fixtures live in `tempfile`
/// directories and are released by `Drop`; git tests use a local bare
/// repository as `origin`; deletion is asserted through typed errors and
/// recovery refs, never performed against a fixture.
#[test]
fn no_test_source_contains_a_destructive_command() {
    let root = workspace_root();
    let banned: &[(&str, &str)] = &[
        // Spawning `rm`. The CLI verb `rm` (`bisa agent rm`) is a
        // different thing and is not matched: the needle is the program name.
        ("new(\"rm\")", "spawning rm"),
        ("arg(\"rm\")", "spawning rm"),
        ("\"rm \"", "spawning rm"),
        ("rm -rf", "rm -rf"),
        ("rm -fr", "rm -fr"),
        ("DROP TABLE", "DROP TABLE"),
        ("DROP DATABASE", "DROP DATABASE"),
        ("git clean", "git clean"),
        (
            "--force",
            "a force push (use --force-with-lease only in production code, never in a test)",
        ),
        (
            "remove_dir_all",
            "remove_dir_all — let TempDir::drop do the work",
        ),
        ("reset-dev-workspace", "the human-only reset script"),
    ];
    let this_file = root.join(file!());
    let mut offences = Vec::new();
    for path in test_sources(&root) {
        // The table above is the rule, not an offence.
        if path == this_file {
            continue;
        }
        let text = fs::read_to_string(&path).expect("read test source");
        for (n, line) in text.lines().enumerate() {
            let l = line.trim_start();
            // The rule's own statement in a comment is not an offence.
            if l.starts_with("//") || l.starts_with("///") || l.starts_with("*") {
                continue;
            }
            // `--force-with-lease` is the production spelling; a test may
            // assert it. A bare `--force` may not appear.
            let l = l.replace("--force-with-lease", "");
            for (needle, what) in banned {
                if l.contains(needle) {
                    offences.push(format!(
                        "{}:{}: {what}",
                        path.strip_prefix(&root).unwrap_or(&path).display(),
                        n + 1
                    ));
                }
            }
        }
    }
    assert!(
        offences.is_empty(),
        "a test source contains a destructive command:\n{}",
        offences.join("\n")
    );
}

#[test]
fn no_ci_file_references_the_reset_script() {
    let root = workspace_root();
    for rel in [".github/workflows/verify.yml", "Justfile"] {
        let path = root.join(rel);
        if !path.exists() {
            continue;
        }
        let text = fs::read_to_string(&path).expect("read");
        for (n, line) in text.lines().enumerate() {
            let l = line.trim_start();
            if l.starts_with('#') {
                continue;
            }
            assert!(
                !l.contains("reset-dev-workspace"),
                "{rel}:{}: the reset script is human-invoked only",
                n + 1
            );
        }
    }
}

// ---------------------------------------------------------------------------
// The gate's own files: a gate that cannot run a step, or runs it on the
// wrong thing, passes by saying nothing.
// ---------------------------------------------------------------------------

/// The characters a bracket class of a `sed` pattern admits: `a-z-` is the
/// letters and the hyphen.
fn class_admits(class: &str, c: char) -> bool {
    let chars: Vec<char> = class.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if i + 2 < chars.len() && chars[i + 1] == '-' {
            if (chars[i]..=chars[i + 2]).contains(&c) {
                return true;
            }
            i += 3;
        } else {
            if chars[i] == c {
                return true;
            }
            i += 1;
        }
    }
    false
}

/// `scripts/test changed` reads a changed path's crate off its first folder.
/// The pattern once stopped at the first hyphen, so a change under
/// `crates/bisa-mobile-development/` asked cargo for a crate that does not
/// exist, and one under `bisa-mcp-probe` ran `bisa-mcp`'s tests; then it
/// admitted no digit, and `bisa-i18n` was `bisa-i`.
#[test]
fn the_test_runner_names_every_crate_by_its_whole_name() {
    let root = workspace_root();
    let script = fs::read_to_string(root.join("scripts/test")).expect("scripts/test");
    let at = script
        .find("crates/\\(bisa-[")
        .expect("scripts/test reads a crate's name off a changed path");
    let rest = &script[at + "crates/\\(bisa-[".len()..];
    let class = &rest[..rest.find(']').expect("the pattern's class ends")];
    let mut crates = 0;
    for entry in fs::read_dir(root.join("crates"))
        .expect("the crates folder")
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(suffix) = name.strip_prefix("bisa-") else {
            continue;
        };
        crates += 1;
        let cut: String = suffix
            .chars()
            .take_while(|c| class_admits(class, *c))
            .collect();
        assert_eq!(
            cut, suffix,
            "scripts/test would name `{name}` as `bisa-{cut}`: its pattern admits only [{class}]"
        );
    }
    assert!(
        crates >= 20,
        "the reader found {crates} crates; it is broken"
    );
    assert!(class_admits("a-z-", 'm') && class_admits("a-z-", '-'));
    assert!(!class_admits("a-z", '-') && !class_admits("a-z-", '_'));
    assert!(class_admits("a-z0-9-", '8') && !class_admits("a-z-", '8'));
}

/// `scripts/test lib <crate>` runs a crate's unit tests, and `--lib` refuses
/// a package with no library: the command line is binaries alone, so the
/// form the pages name for its unit tests (`scripts/test lib cli localize`)
/// could not run. The runner asks for the binaries' tests where there is no
/// library — and a crate with neither has no unit tests to ask for.
#[test]
fn the_test_runner_runs_the_unit_tests_of_a_crate_that_is_binaries_alone() {
    let root = workspace_root();
    let script = fs::read_to_string(root.join("scripts/test")).expect("scripts/test");
    let mut without_a_library = Vec::new();
    for entry in fs::read_dir(root.join("crates"))
        .expect("the crates folder")
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("bisa-") {
            continue;
        }
        let src = entry.path().join("src");
        if !src.join("lib.rs").is_file() {
            assert!(
                src.join("main.rs").is_file(),
                "{name} has neither a library nor a binary"
            );
            without_a_library.push(name);
        }
    }
    assert_eq!(
        without_a_library,
        vec!["bisa-cli"],
        "the crates that are binaries alone"
    );
    let arm = script
        .split("\n    lib)")
        .nth(1)
        .and_then(|rest| rest.split(";;").next())
        .expect("scripts/test has a `lib` form");
    assert!(
        arm.contains("src/lib.rs") && arm.contains("--lib") && arm.contains("--bins"),
        "the `lib` form asks for a library's tests whatever the crate is:{arm}"
    );
}

/// The steps of the gate that need a tool find it where the tree installs
/// it, and the one that changes directory reads its scratch by a path that
/// is still right afterwards.
/// **One version, one licence** (`docs/contributing/release.md` § One
/// version): every crate of the workspace inherits the workspace's version,
/// edition and licence — none spells its own, so a release that bumps the
/// one line bumps them all, the generated unification crate among them (its
/// package header is ours; the tool owns only the table between its markers).
#[test]
fn every_crate_wears_the_workspaces_version_edition_and_licence() {
    let crates = workspace_root().join("crates");
    let mut read = 0;
    let mut offences = Vec::new();
    for entry in fs::read_dir(&crates)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", crates.display()))
        .flatten()
    {
        let manifest = entry.path().join("Cargo.toml");
        let Ok(text) = fs::read_to_string(&manifest) else {
            continue;
        };
        read += 1;
        for field in ["version", "edition", "license"] {
            let inherited = text
                .lines()
                .any(|line| line.trim() == format!("{field}.workspace = true"));
            if !inherited {
                offences.push(format!(
                    "{}: `{field}` is not the workspace's",
                    entry.file_name().to_string_lossy()
                ));
            }
        }
    }
    assert!(read > 20, "the walk found {read} crates; it is broken");
    assert!(
        offences.is_empty(),
        "inherit it (`<field>.workspace = true`):\n{}",
        offences.join("\n")
    );
}

#[test]
fn the_gate_can_run_the_steps_it_names() {
    let root = workspace_root();
    let just = fs::read_to_string(root.join("Justfile")).expect("the Justfile");
    // A recipe: its header line, then every line that is blank or indented.
    let recipe = |name: &str| -> String {
        let mut lines = just.lines().skip_while(|l| *l != format!("{name}:"));
        let header = lines.next().unwrap_or_else(|| panic!("no recipe {name}"));
        let body: Vec<&str> = lines
            .take_while(|l| l.is_empty() || l.starts_with(' ') || l.starts_with('\t'))
            .collect();
        format!("{header}\n{}", body.join("\n"))
    };
    let licences = recipe("licence-gate");
    assert!(
        licences.contains("target/tools/bin/cargo-deny check licenses"),
        "the licence gate never finds the tree's own cargo-deny:\n{licences}"
    );
    let coverage = recipe("coverage");
    assert!(
        coverage.contains("target/tools/bin/cargo-llvm-cov"),
        "the coverage recipe never finds the tree's own cargo-llvm-cov:\n{coverage}"
    );
    let types = recipe("check-types");
    assert!(
        types.contains("tmp=\"$PWD/target/check-types\""),
        "check-types changes into desktop/ and must read its scratch by an absolute path:\n{types}"
    );

    // CI: the keymap check loads the desktop's catalog, so the desktop's
    // packages are installed before it.
    let ci = fs::read_to_string(root.join(".github/workflows/verify.yml")).expect("the CI file");
    let job = &ci[ci.find("  types:").expect("the generated-types job")..];
    let keymap = job
        .find("gen-keymap-docs.mjs --check")
        .expect("the keymap check");
    let install = job.find("npm ci").expect("the desktop's packages");
    assert!(
        install < keymap,
        "CI checks the keymap page before the desktop's packages are installed"
    );
}

// ---------------------------------------------------------------------------
// The consented git tier (ide/04): three shape tests that make
// "no agent can reach a tree-moving verb" a build failure, not a promise.
// ---------------------------------------------------------------------------

/// Every `.rs` under `crates/*/src` — shipped code, never tests or benches.
fn production_sources_workspace(root: &Path) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    for crate_dir in fs::read_dir(root.join("crates"))
        .expect("crates/")
        .flatten()
    {
        let src = crate_dir.path().join("src");
        if src.is_dir() {
            out.extend(production_sources(&src));
        }
    }
    assert!(out.len() > 50, "found only {} sources", out.len());
    out
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).display().to_string()
}

/// A `std::sync::Mutex` is taken with `Locked::locked` (`bisa_core::sync`)
/// or with `unwrap_or_else(|e| e.into_inner())`, never with `lock().unwrap()`
/// or `lock().expect(..)`: a lock poisoned by a panic the platform already
/// contained must not turn every later reader into a second panic.
#[test]
fn no_production_code_panics_on_a_poisoned_lock() {
    let root = workspace_root();
    let mut offences = Vec::new();
    for (path, text) in production_sources_workspace(&root) {
        for (n, line) in text.lines().enumerate() {
            let l = line.trim_start();
            if l.starts_with("//") {
                continue;
            }
            let squeezed: String = l.split_whitespace().collect();
            if squeezed.contains(".lock().unwrap()") || squeezed.contains(".lock().expect(") {
                offences.push(format!("{}:{}: {l}", rel(&root, &path), n + 1));
            }
        }
    }
    assert!(
        offences.is_empty(),
        "a std mutex is taken with `.locked()` (bisa_core::sync::Locked), never a panicking lock:\n{}",
        offences.join("\n")
    );
}

#[test]
fn consent_is_minted_in_one_place() {
    let root = workspace_root();
    let mut sites = Vec::new();
    for (path, text) in production_sources_workspace(&root) {
        // The definition itself, in the vcs crate, is not a call.
        if text.contains("HumanConsent::mint(") {
            sites.push(rel(&root, &path));
        }
    }
    assert_eq!(
        sites,
        vec!["crates/bisa-node/src/ide/consent.rs".to_string()],
        "HumanConsent::mint( may be called from the node's consent module and nowhere else"
    );
}

#[test]
fn interactive_tier_has_two_named_engine_callers() {
    let root = workspace_root();
    let mut callers = Vec::new();
    for (path, text) in production_sources_workspace(&root) {
        let r = rel(&root, &path);
        if r.starts_with("crates/bisa-vcs/") {
            continue;
        }
        if text.contains("vcs::interactive") {
            callers.push(r);
        }
    }
    callers.sort();
    // The IDE's consented verbs, and the folder repositories' pull — two
    // doors, both the engine's, both minting consent from a person's own request.
    assert_eq!(
        callers,
        vec![
            "crates/bisa-engine/src/folder_git.rs".to_string(),
            "crates/bisa-engine/src/ide/interactive.rs".to_string(),
        ],
        "bisa_vcs::interactive may be named by the engine's folder_git and ide::interactive, and nowhere else"
    );
}

#[test]
fn no_agent_path_reaches_interactive() {
    let root = workspace_root();
    let agent_facing = ["bisa-mcp", "bisa-harness", "bisa-adapters"];
    let mut offences = Vec::new();
    for (path, text) in production_sources_workspace(&root) {
        let r = rel(&root, &path);
        if !agent_facing
            .iter()
            .any(|c| r.starts_with(&format!("crates/{c}/")))
        {
            continue;
        }
        for needle in ["HumanConsent", "vcs::interactive", "ide::interactive"] {
            if text.contains(needle) {
                offences.push(format!("{r}: names {needle}"));
            }
        }
    }
    assert!(
        offences.is_empty(),
        "an agent-facing crate reaches the consented tier:\n{}",
        offences.join("\n")
    );
}
