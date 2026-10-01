# Setup

The source is https://github.com/mourad-ghafiri/Bisa; the website is https://bisa.dev; the licence is MIT (`LICENSE`).

## The tools

| Tool | Version | For |
|---|---|---|
| Rust | stable, as `rust-toolchain.toml` pins it, with `rustfmt` and `clippy` | every crate |
| Node.js and npm | 22 (what CI runs) | the desktop, the website, the release and licence scripts |
| Python | 3 | the vocabulary lint (`scripts/lint-terminology`) |
| git | 2.36 or later | projects, and every git test |
| `just` | any recent | the task runner below |
| Tauri CLI | `cargo install tauri-cli` | building and running the desktop app |
| A coding harness | Claude Code, Codex CLI, OpenCode, GitHub Copilot CLI, Grok Build, … | running Bisa for real; the tests use a scripted agent and need none |
| macOS with Xcode's command-line tools | — | building the desktop app as it ships |

Optional, installed inside the tree by their recipes: cargo-nextest (`just install-nextest`),
cargo-hakari, cargo-deny, cargo-llvm-cov. Then read [How to contribute](how-to-contribute.md) and your
area's guide ([areas](areas/README.md)).

## The commands

```sh
cargo build --workspace                 # every crate
cargo test --workspace                  # every test — nothing here touches your ~/.bisa
cd desktop && npm install && npm test   # the desktop's pure models and the theme contract
```

`just` is the task runner:

| Target | Does |
|---|---|
| `just build` · `just test` · `just fmt` | the obvious; `test` also runs `cargo test` in `desktop/src-tauri` and then the desktop's `npm test` |
| `just test-rust` | the workspace and `desktop/src-tauri` alone — what `verify` runs before the desktop's one coverage run |
| `just lint` | `just lint-terminology`, then the desktop's `npm run lint`, then `clippy -D warnings` and `fmt --check` for the workspace and `desktop/src-tauri` |
| `just lint-terminology` | the banned-word scan and the lint's own fixture suite (`scripts/lint-terminology-test`) — no toolchain needed |
| `just gen-types` · `just check-types` | the desktop's generated wire types, and a staleness check |
| `just gen-api-docs` · `just check-api-docs` | `docs/reference/http-api.md` from the route tables, and a staleness check |
| `just gen-settings-docs` · `just check-settings-docs` | `docs/reference/settings-keys.md` from the settings registry, and a staleness check |
| `just gen-catalog-docs` · `just check-catalog-docs` | `docs/reference/catalog.md` from `library/catalog/`, and a staleness check |
| `just gen-keymap-docs` · `just check-keymap-docs` | `docs/reference/keymap.md` from the desktop's keymap model, and a staleness check |
| `just check-mermaid` | every Mermaid block under `docs/` parses with the desktop's Mermaid |
| `just licence-gate` | `cargo deny` over `deny.toml` on both Cargo workspaces (when installed), then `scripts/licences/check-licences.mjs` — the same graphs judged against the same list, so the gate runs without it ([Release § Licence](release.md#licence)) |
| `just gen-notices` · `just check-notices` | `THIRD-PARTY-NOTICES.md` from `NOTICES.md` and the dependency graphs, and a staleness check |
| `just hakari` · `just hakari-verify` | regenerate the unification crate (`crates/bisa-deps`) after a dependency change; the three checks — the manifest current, every member but `bisa-log` depending on it, one feature set per dependency — as a gate that says so when the tool is not under `target/tools` |
| `just bench-search` · `just bench-graph` | the two engine benches, by hand |
| `just prune-recovery-refs repo=<path> older_than=30 yes=--yes` | prune `refs/bisa/safety/*` — human-only; nothing is pruned without `yes=--yes` |
| `just desktop-build` · `just desktop-test` | `npm run build` (not `tsc` alone — module resolution) and `npm test` |
| `just app-icon` | the platform's OS icon — `tauri icon` over `logo/logo.svg` into `desktop/src-tauri/icons/` |
| `just dist` | the node at its shipping profile for this machine |
| `just bundle-macos` · `just start-macos` | the macOS application for this Mac: universal, the node inside, signed ad hoc, at `dist/Bisa.app` — and opened ([Release § The app for this Mac](release.md#the-app-for-this-mac)) |
| `just release-macos` · `just release-macos-rehearsal` | the release under `release/`: the same bundle signed with `APPLE_SIGNING_IDENTITY`, notarized through `BISA_NOTARY_PROFILE`, stapled, shipped as a disk image with its SHA-256; the rehearsal submits nothing ([Release § The release](release.md#the-release)) |
| `just publish-release` | the release on GitHub as a draft, with the changelog's section as its notes — `--dry-run` prints what would run, `--publish` publishes ([Release § Publishing](release.md#publishing)) |
| `just set-version 0.2.0 write=--write` | every manifest's version and the changelog cut from Unreleased; a dry run without `write=` ([Release § The version and the changelog](release.md#the-version-and-the-changelog)) |
| `just release-check` | the release tooling's model tests and a parse of every release script — in `verify` |
| `just website` · `just website-check` · `just website-shots` · `just website-card` | the website: build it from `scripts/website/` and serve `website/` on `http://localhost:4321`; its tests and the staleness check — in `verify`; the screenshots made WebP for the web with `cwebp`, the originals moved aside under `target/`; the social card, rendered with this machine's Chrome ([the website's README](../../website/README.md)) |
| `just install-hooks` | symlink `scripts/pre-commit` into your `.git/hooks` — nothing installs it for you |
| `just verify` | the whole gate — see [Release](release.md) |
| `just` (no target) | lists the targets |

The desktop in development: build the CLI (`cargo build -p bisa-cli --bin bisa`), then `cd desktop &&
BISA_BIN=../target/debug/bisa cargo tauri dev`. UI only against a running node:
`BISA_API_BASE=http://127.0.0.1:4477 npm run dev`. See [`desktop/README.md`](../../desktop/README.md).

A scratch workspace: `bisa --data-dir /tmp/ws init`. `scripts/reset-dev-workspace` moves a
development workspace aside — it is human-invoked, and a test asserts no CI or test file names it.

## Where keys live

The workspace's own keys — the owner identity, agent keys, a public hook's secret, a stored code host token —
are `0600` files under `~/.bisa/identity/` (a `0700` directory) **by default**. The OS keyring is
opt-in: `BISA_KEYSTORE=keyring` moves them there. The default is the file store because a
command-line tool asking the macOS Keychain to hold an entry makes the OS ask for your login
password, a prompt that reads as "this app wants my passwords" — it never did; the entry is the
workspace's key and nothing else is read — and a tool should not need to explain that. Nothing in
the platform calls a keyring API unless that variable says so.
