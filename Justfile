# Bisa developer tasks

default:
    @just --list

build:
    cargo build --workspace

# Every suite, with git isolated from this machine (`scripts/test` sets
# GIT_CONFIG_GLOBAL and GIT_CONFIG_NOSYSTEM for the run: a fixture repository
# must never see a developer's `commit.gpgsign` or `core.hooksPath`).
test:
    ./scripts/test all

# The Rust half alone — the workspace and the Tauri shell. `verify` runs this
# and then the desktop once, through its coverage gate, rather than the
# desktop suite twice.
test-rust:
    ./scripts/test rust

# One crate's tests, optionally filtered by name: `just test-crate engine guided`.
test-crate crate filter="":
    ./scripts/test crate {{crate}} {{filter}}

# One crate's unit tests, optionally one module of them: `just test-lib core run::`.
test-lib crate filter="":
    ./scripts/test lib {{crate}} {{filter}}

# One module of a crate's integration binary — `tests/it/<module>.rs` — optionally
# one test of it: `just test-module engine guided`, `just test-module engine projects publish`.
test-module crate module filter="":
    ./scripts/test module {{crate}} {{module}} {{filter}}

# Type-check one crate and its tests — the inner loop. The workspace-wide
# check with every target is `lint`'s, once, before handing over: a crate's
# check is seconds, the workspace's with all targets is minutes.
check crate:
    cargo check -p bisa-{{crate}} --tests

# Where the time goes: an HTML report per compiler invocation, with the
# critical path and the CPU-starved stretches (target/cargo-timings/).
timings:
    cargo check --workspace --tests --timings

# The crates and desktop models touched by uncommitted changes.
test-changed:
    ./scripts/test changed

# cargo-nextest, inside the tree: `target/tools/bin/cargo-nextest`. Every form
# of `scripts/test` prefers it when present — one process pool across binaries,
# a hung test ends at its timeout (`.config/nextest.toml`) — and runs plain
# `cargo test` otherwise. Nothing is written outside the repository.
install-nextest:
    CARGO_TARGET_DIR=target/tools-build cargo install --root target/tools --locked cargo-nextest

# cargo-hakari, inside the tree too: the tool behind `crates/bisa-deps/`, the
# generated crate that pins every third-party dependency to one feature set
# (docs/contributing/testing-rules.md §Faster builds).
install-hakari:
    CARGO_TARGET_DIR=target/tools-build cargo install --root target/tools --locked cargo-hakari

# After a dependency or feature changes anywhere: regenerate the unification crate,
# add it where a new member lacks it, and prove it is consistent. `CARGO` is
# set because the tool is invoked by path rather than as a cargo subcommand.
hakari:
    CARGO="$(command -v cargo)" target/tools/bin/cargo-hakari hakari generate
    CARGO="$(command -v cargo)" target/tools/bin/cargo-hakari hakari manage-deps --yes
    CARGO="$(command -v cargo)" target/tools/bin/cargo-hakari hakari verify

# The unification crate's three checks as a gate (in `verify`): the generated
# Cargo.toml current, every member depending on it, one feature set per
# dependency. Needs the tool under target/tools (`just install-hakari`); says
# so otherwise rather than passing silently — CI installs it and runs the same.
hakari-verify:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -x target/tools/bin/cargo-hakari ]; then
        export CARGO="$(command -v cargo)"
        target/tools/bin/cargo-hakari hakari generate --diff
        target/tools/bin/cargo-hakari hakari manage-deps --dry-run
        target/tools/bin/cargo-hakari hakari verify
    else
        echo "cargo-hakari is not installed under target/tools (just install-hakari); CI runs the three checks" >&2
    fi

# Line coverage for the Rust workspace, when cargo-llvm-cov is installed
# (`cargo install cargo-llvm-cov` and `rustup component add llvm-tools-preview`
# are the developer's to run: both write outside the tree). Without it the
# recipe says so; the desktop half always runs.
coverage:
    #!/usr/bin/env bash
    set -euo pipefail
    export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
    # The tree's own tool first (`target/tools/bin`, where the programme
    # installs it), then one on `PATH`.
    if [ -x target/tools/bin/cargo-llvm-cov ]; then
        target/tools/bin/cargo-llvm-cov llvm-cov --workspace --summary-only
    elif cargo llvm-cov --version >/dev/null 2>&1; then
        cargo llvm-cov --workspace --summary-only
    else
        echo "cargo-llvm-cov is not installed; Rust coverage is judged by the guard-test table in docs/contributing/testing-rules.md" >&2
    fi
    cd desktop && npm run test:coverage

# `--workspace` stops at the root workspace, and `desktop/src-tauri` declares
# its own — so the shell crate was outside the format and lint gate entirely
# and had quietly drifted. A crate nobody lints is a crate where the next
# warning is somebody's afternoon.
#
# The vocabulary check runs first because it needs no toolchain: a reintroduced
# banned term fails in a second rather than after a full build.
lint: lint-terminology
    cd desktop && npm run lint
    cargo clippy --workspace --all-targets -- -D warnings
    cargo fmt --all --check
    cd desktop/src-tauri && cargo clippy --all-targets -- -D warnings
    cd desktop/src-tauri && cargo fmt --all --check

# Fail on banned domain vocabulary, then prove the lint itself still catches
# what it claims to (its fixture suite, the one file allowed to spell the
# words). See docs/contributing/terminology.md
lint-terminology:
    ./scripts/lint-terminology
    ./scripts/lint-terminology-test

# Install the pre-commit hook that runs the vocabulary check on staged files.
# Nothing installs this for you — a repository that writes to your .git/hooks
# without being asked is a repository you cannot trust with the rest.
install-hooks:
    #!/usr/bin/env bash
    set -euo pipefail
    hooks="$(git rev-parse --git-path hooks)"
    ln -sf ../../scripts/pre-commit "$hooks/pre-commit"
    echo "installed: $hooks/pre-commit -> scripts/pre-commit"
    echo "remove it with: unlink \"$hooks/pre-commit\""

fmt:
    cargo fmt --all
    cd desktop/src-tauri && cargo fmt --all

# Build the node at its shipping profile for this machine's architecture
# (throughput: fat LTO, one codegen unit). The macOS bundle builds it for both
# architectures itself (scripts/macos/lib.sh). The binary is named: the
# package has another, the tests' scripted agent, which is never shipped.
dist:
    cargo build --profile dist -p bisa-cli --bin bisa

# Regenerate the desktop's API types from the Rust wire contract.
# The bundle covers everything deriving `schemars::JsonSchema` (node DTOs +
# bisa-core); shapes from store/engine/harness live in types.hand.ts.
gen-types:
    cargo run -q -p bisa-node --bin api-schema > desktop/api-schema.json
    cd desktop && npx --yes json-schema-to-typescript@15 api-schema.json -o src/types.gen.ts

# Fail if the committed types are stale against the current Rust types.
#
# Scratch goes to `target/`, which is gitignored and overwritten each run, so
# nothing in this repository has to delete anything. The previous version
# trapped `rm -rf "$tmp"` on EXIT — safe in practice, and still the only
# destructive command in the tree, which is one more than the rule allows.
check-types:
    #!/usr/bin/env bash
    set -euo pipefail
    # Absolute: the second half runs from `desktop/`, where a relative
    # `target/…` is a folder that does not exist.
    tmp="$PWD/target/check-types"
    mkdir -p "$tmp"
    cargo run -q -p bisa-node --bin api-schema > "$tmp/api-schema.json"
    diff -q "$tmp/api-schema.json" desktop/api-schema.json >/dev/null || {
        echo "desktop/api-schema.json is stale — run \`just gen-types\`" >&2
        exit 1
    }
    cd desktop && npx --yes json-schema-to-typescript@15 "$tmp/api-schema.json" -o "$tmp/types.gen.ts"
    diff -q "$tmp/types.gen.ts" src/types.gen.ts >/dev/null || {
        echo "desktop/src/types.gen.ts is stale — run \`just gen-types\`" >&2
        exit 1
    }

# Render the HTTP reference from the route tables beside each `routes()`.
gen-api-docs:
    mkdir -p docs/reference
    cargo run -q -p bisa-node --bin api-docs > docs/reference/http-api.md

# Render the settings reference from the registry in bisa-core.
gen-settings-docs:
    mkdir -p docs/reference
    cargo run -q -p bisa-node --bin settings-docs > docs/reference/settings-keys.md

# Fail if the committed settings reference is stale.
check-settings-docs:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p target/check-api-docs
    cargo run -q -p bisa-node --bin settings-docs > target/check-api-docs/settings-keys.md
    diff -q target/check-api-docs/settings-keys.md docs/reference/settings-keys.md >/dev/null || {
        echo "docs/reference/settings-keys.md is stale — run \`just gen-settings-docs\`" >&2
        exit 1
    }

# Render the catalog reference from the bundled TOML sources.
gen-catalog-docs:
    mkdir -p docs/reference
    cargo run -q -p bisa-node --bin catalog-docs > docs/reference/catalog.md

# Fail if the committed catalog reference is stale.
check-catalog-docs:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p target/check-api-docs
    cargo run -q -p bisa-node --bin catalog-docs > target/check-api-docs/catalog.md
    diff -q target/check-api-docs/catalog.md docs/reference/catalog.md >/dev/null || {
        echo "docs/reference/catalog.md is stale — run \`just gen-catalog-docs\`" >&2
        exit 1
    }

# Fail if the committed reference is stale against the route tables.
check-api-docs:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p target/check-api-docs
    cargo run -q -p bisa-node --bin api-docs > target/check-api-docs/http-api.md
    diff -q target/check-api-docs/http-api.md docs/reference/http-api.md >/dev/null || {
        echo "docs/reference/http-api.md is stale — run \`just gen-api-docs\`" >&2
        exit 1
    }

# The performance budgets (ide/14) that have a bench. Sizes the synthetic
# fixture with SEARCH_BENCH_FILES. Criterion reports; nothing gates on the numbers.
bench-search:
    cargo bench -p bisa-engine --bench search

# Commit-graph layout over a synthesised 100k-commit log (ide/05, ide/14).
# GRAPH_BENCH_COMMITS sizes it.
bench-graph:
    cargo bench -p bisa-engine --bench graph

# Recovery refs (ide/04) are written by every consented git operation and
# removed by nobody but a person. This is that person's tool: it lists what
# would go, and deletes only with `--yes`. It never touches a ref younger than
# --older-than days, and never anything outside refs/bisa/safety/.
#   just prune-recovery-refs repo=/path/to/repo older_than=30 yes=--yes
prune-recovery-refs repo older_than="30" yes="":
    #!/usr/bin/env bash
    set -euo pipefail
    cutoff=$(( $(date +%s) - {{older_than}} * 86400 ))
    git -C "{{repo}}" for-each-ref --format='%(refname)' refs/bisa/safety/ | while read -r ref; do
        stamp=${ref#refs/bisa/safety/}; stamp=${stamp%%-*}
        if [[ "$stamp" =~ ^[0-9]+$ ]] && (( stamp < cutoff )); then
            if [[ "{{yes}}" == "--yes" ]]; then
                git -C "{{repo}}" update-ref -d "$ref" && echo "pruned $ref"
            else
                echo "would prune $ref (pass yes=--yes)"
            fi
        fi
    done

# The licence gate (Part II): no copyleft in either dependency tree. The Rust
# half needs cargo-deny (`cargo install cargo-deny`); CI always runs it, and
# locally the recipe says so rather than passing silently. The Node gate's own
# model (SPDX branches, copyleft) is proven first — a gate without its tests
# is a gate that can quietly stop gating.
licence-gate:
    #!/usr/bin/env bash
    set -euo pipefail
    # The tree's own tool first (`target/tools/bin`), then one on `PATH`.
    if [ -x target/tools/bin/cargo-deny ]; then
        target/tools/bin/cargo-deny check licenses
        target/tools/bin/cargo-deny --manifest-path desktop/src-tauri/Cargo.toml check licenses
    elif command -v cargo-deny >/dev/null 2>&1; then
        cargo deny check licenses
        cargo deny --manifest-path desktop/src-tauri/Cargo.toml check licenses
    else
        echo "cargo-deny is not installed (cargo install cargo-deny); CI runs it on both workspaces — the Node gate below judges the same graphs against deny.toml" >&2
    fi
    node --test scripts/licences/licencesModel.test.mjs
    node scripts/licences/check-licences.mjs

# Render THIRD-PARTY-NOTICES.md: NOTICES.md (hand-written) followed by every
# Rust crate and npm package the shipped binaries carry, with their licence
# texts and notices (docs/contributing/release.md §Licence).
gen-notices:
    node scripts/licences/gen-third-party-notices.mjs

# Fail if the committed notices are stale against the dependency graphs.
check-notices:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p target/check-api-docs
    node scripts/licences/gen-third-party-notices.mjs --stdout > target/check-api-docs/THIRD-PARTY-NOTICES.md
    diff -q target/check-api-docs/THIRD-PARTY-NOTICES.md THIRD-PARTY-NOTICES.md >/dev/null || {
        echo "THIRD-PARTY-NOTICES.md is stale — run \`just gen-notices\`" >&2
        exit 1
    }

# Render the keymap reference from the desktop's keymap model (ide/15).
gen-keymap-docs:
    node scripts/gen-keymap-docs.mjs

# Fail if the committed keymap reference is stale.
check-keymap-docs:
    node --import ./desktop/src/i18n/preload.mjs scripts/gen-keymap-docs.mjs --check

# Every mermaid block under docs/ parses with the Mermaid the desktop renders with.
check-mermaid:
    node scripts/check-mermaid.mjs docs

# Typecheck + bundle the desktop app
desktop-build:
    cd desktop && npm run build

# The platform's OS icon: `tauri icon` over the one mark, `logo/logo.svg`,
# into `desktop/src-tauri/icons/` — the set `bundle.icon` names. The same
# file is what the app shows (`ui/PlatformMark.tsx`), so a redraw is a new
# file at that path and this. Its sibling `tray-icon` is the menu bar's mark.
app-icon:
    cd desktop && npx tauri icon ../logo/logo.svg

# The menu bar's mark: `logo/tray-mark.svg` — the split B and its rail in one
# ink, no squircle — rasterised once to `desktop/src-tauri/icons/tray/36x36.png`
# (18 pt at 2×, the height every status item is drawn at). The shell ships
# the PNG (`tray/glyph.rs`) and tints it to the menu bar's ink at run time.
tray-icon:
    cd desktop && npx tauri icon ../logo/tray-mark.svg -o src-tauri/icons/tray -p 36

# Build the shippable desktop shell (release, ADR-0059): the node at the `dist`
# profile (speed) and the Tauri shell at its release profile (fat LTO, one
# codegen unit). Reproducible rather than hand-built.
desktop-bundle: dist
    cd desktop && npm run tauri build

# The translation ratchet's baselines: how many bare sentences each source
# under desktop/src and the node's and CLI's crates still carries, per file
# (17 §Guards). The two guards hold the sources to these files exactly, so a
# sentence moved into the catalog lowers a count here — run this, commit both.
i18n-baseline:
    cd desktop && node src/i18n/ratchet.mjs --write
    cargo run -q -p bisa-i18n --bin i18n-ratchet -- --write

# The desktop application for this Mac at dist/Bisa.app: universal (Apple
# Silicon and Intel), the node inside the bundle, signed ad hoc — it opens
# here and nowhere else (docs/contributing/release.md §The app for this Mac).
bundle-macos:
    ./scripts/bundle-macos.sh

# Open dist/Bisa.app, building it first when it is not there.
start-macos:
    ./scripts/start-macos.sh

# The release for macOS under release/: the same universal bundle signed with
# your Developer ID Application identity (APPLE_SIGNING_IDENTITY), notarized
# through the keychain profile in BISA_NOTARY_PROFILE, stapled, shipped as a
# disk image with its SHA-256 (docs/contributing/release.md §The release).
# Refuses to start without the two variables, a clean tree and a changelog
# section for the version.
release-macos:
    ./scripts/release-macos.sh

# A rehearsal of the release: built, signed and imaged, the hash printed;
# nothing submitted to Apple, nothing under release/. The tree may be dirty.
release-macos-rehearsal:
    ./scripts/release-macos.sh --skip-notarize --allow-dirty

# Publish the release under release/ on GitHub as a draft, with the
# changelog's section as its notes: `just publish-release --dry-run` prints
# the three commands that would change something; `--publish` publishes at
# once (docs/contributing/release.md §Publishing).
publish-release *ARGS:
    ./scripts/publish-release.sh {{ARGS}}

# Move the platform to a new version: every manifest that repeats it and the
# changelog cut from Unreleased; a dry run without `write=--write`
# (docs/contributing/release.md §The version and the changelog).
set-version VERSION write="":
    node scripts/release/set-version.mjs {{VERSION}} {{write}}

# The release tooling's own gate, in `verify`: the model's tests, and every
# release script parsed by bash.
release-check:
    node --test scripts/release/releaseModel.test.mjs
    bash -n scripts/macos/lib.sh scripts/bundle-macos.sh scripts/release-macos.sh scripts/publish-release.sh scripts/start-macos.sh

# The website (website/README.md): build it from scripts/website/, then serve
# website/ on http://localhost:4321 until you stop it.
website: website-gen
    python3 -m http.server 4321 --bind 127.0.0.1 --directory website

# Build the website's pages, the README's screenshot block and the copied
# brand files from scripts/website/ and the repository's own facts — after
# making any new screenshot small for the web, where this machine has cwebp.
website-gen:
    node scripts/website/web-screenshots.mjs --if-available
    node scripts/website/build.mjs

# Make the screenshots small for the web: each .png or .jpg in
# website/screenshots/ becomes a .webp at most 2000 px wide, by cwebp; the
# original is moved aside under target/website-screenshot-originals/. Then the
# README's animated slideshow is made again if its screens changed (img2webp).
website-shots:
    node scripts/website/web-screenshots.mjs

# The website's gate, in `verify`: its rules, its rail, and the committed
# pages held to what scripts/website/ builds.
website-check:
    node --test scripts/website/websiteModel.test.mjs scripts/website/railModel.test.mjs scripts/website/site.test.mjs

# Render the social card a shared link shows, with this machine's Chrome.
website-card:
    node scripts/website/card.mjs

# The desktop's own tests. Chiefly the theme contract: a theme that forgets a
# role does not look wrong, it looks *absent* — the variable resolves to
# nothing and the control it painted disappears while staying focusable and
# clickable. Nobody files that bug, because nobody can see the thing they
# would be filing it about, which is why this runs in `verify` rather than by
# hand.
desktop-test:
    cd desktop && npm test

# The desktop's tests under one directory: `just desktop-test-dir views/_goals`.
desktop-test-dir dir:
    ./scripts/test desktop {{dir}}

# The desktop's model tests with line coverage, failing under the threshold
# the script sets. Node's own coverage; nothing to install.
desktop-coverage:
    cd desktop && npm run test:coverage

# Run the full verification suite. The desktop suite runs once, under its
# coverage gate (`desktop-coverage`); `test-rust` is the workspace and the shell.
verify: build test-rust lint check-types check-api-docs check-settings-docs check-catalog-docs check-keymap-docs check-mermaid licence-gate check-notices release-check website-check hakari-verify desktop-coverage desktop-build
