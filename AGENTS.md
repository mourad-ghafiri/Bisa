# AGENTS.md — instructions for coding agents

You are changing Bisa, a local-first agentic IDE: a Rust workspace (`crates/`), a Tauri desktop app
(`desktop/`), a catalog (`library/`) and a static website (`website/`). People run it on their own
machines, with their keys and their repositories. Work carefully: read before you write, change the
least that does the job, and prove it with tests.

The person who asked you is responsible for your change and submits it. Every rule in
[CONTRIBUTING.md](CONTRIBUTING.md) applies to you.

## Read first
1. [CONTRIBUTING.md](CONTRIBUTING.md) — the path and what a pull request must carry.
2. The area guide for the code you are changing — [docs/contributing/areas/](docs/contributing/areas/README.md).
3. The architecture pages that guide lists, in full — start at [docs/architecture/README.md](docs/architecture/README.md).
4. The recipe for your kind of change — [docs/contributing/recipes.md](docs/contributing/recipes.md) —
   and follow it to the last file.

## Map
| Path | What |
|---|---|
| `crates/bisa-core` | the domain: types, the run machine, invariants — pure, no I/O |
| `crates/bisa-store` | the workspace on disk: truth files, journals, the index cache |
| `crates/bisa-engine` | everything with an effect: runs, sessions, events, the IDE's operations |
| `crates/bisa-node` | the daemon: HTTP API, auth, the event stream |
| `crates/bisa-cli` | the `bisa` binary: verbs, the node, the MCP server; end-to-end journeys in `tests/it/e2e/` |
| `crates/bisa-adapters`, `crates/bisa-harness` | coding harnesses (Claude Code, Codex, OpenCode, Copilot, Grok, ACP, A2A, custom) |
| `desktop/src` | the UI: facts in `.mjs` models with `.d.mts` and tests; components only draw |
| `desktop/src-tauri` | the native shell |
| `locales/` | every sentence a person reads |
| `library/` | the catalog that ships |
| `docs/` | the documentation — guides, architecture, reference, contributing |

## Commands
```sh
scripts/test module <crate> <module>   # one module of a crate's integration tests — while you work
scripts/test lib <crate>               # a crate's unit tests
scripts/test crate <crate>             # every test of one crate — before you finish
scripts/test desktop <dir>             # the desktop's models and scenarios under src/<dir>
scripts/lint-terminology               # the vocabulary
rustfmt --edition 2021 <files>         # format only the files you touched
just verify                            # the whole gate — once, at the end
```
Run tests one crate or one module at a time; the whole workspace takes most of an hour and is the
last step, not the loop. The forms are in [Testing rules](docs/contributing/testing-rules.md#running).

## Hard rules
- **Nothing destructive, ever** — not in a command you run, not in a test you write: no `rm -rf`, no
  `remove_dir_all` outside a temporary directory, no `git reset --hard`, `git clean`, `--force`, no
  `DROP`. Tests use temporary directories and fakes; a guard test reads every test source for these.
- **Never touch the person's own setup** — their `~/.bisa`, their global git config, their keychain,
  their SSH keys, their shell profile.
- **Never commit a secret**, a key, a token, a personal path or an e-mail address — in code, tests,
  fixtures, docs or screenshots.
- **Never edit a generated file**: `desktop/api-schema.json`, `desktop/src/types.gen.ts`,
  `docs/reference/http-api.md`, `docs/reference/settings-keys.md`, `docs/reference/catalog.md`,
  `docs/reference/keymap.md`, `THIRD-PARTY-NOTICES.md`, the pages under `website/`. Regenerate them
  ([Regenerate and verify](docs/contributing/recipes.md#22-regenerate-and-verify)).
- **Keep compatibility** — inside 0.x nothing that worked may stop working: add, never rename, remove
  or retype ([Keeping compatibility](docs/contributing/compatibility.md)). If your change cannot be
  made by addition, stop and say so.
- **Keep the layering** — a domain rule in `bisa-core`, a mutation through the engine, the node
  writes nothing to the store, the desktop's facts in models
  ([07 — Layering](docs/architecture/07-layering.md)).
- **Words** — one word per concept ([Terminology](docs/contributing/terminology.md)); every sentence a
  person reads is a message of the catalog with an id, never English in code
  ([Say something to a person](docs/contributing/recipes.md#26-say-something-to-a-person)); ending a
  session or process is *terminate*.
- **Errors** — a refusal is a typed error with a message; never a panic or `unwrap` in production
  code of the core; never parse prose; never drop a `Result`.
- **No new dependency** unless the issue agreed it; then [Add a dependency](docs/contributing/recipes.md#28-add-a-dependency).
- **Docs and changelog** move with the code: the pages that describe the change, and *Unreleased* in
  `CHANGELOG.md`.

## When you are done
- Every test you added fails without your change and passes with it.
- The narrow tests, the crate's tests and `just verify` are green.
- Your summary for the pull request says what changed, why, how it was tested, its compatibility
  class, and that a coding agent did the work — the person fills the template with it
  ([the template](.github/pull_request_template.md)).
- If you were unsure about anything, say so plainly instead of guessing.
