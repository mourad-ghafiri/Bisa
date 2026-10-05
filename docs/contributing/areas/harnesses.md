# Harnesses

For a person, a harness is a coding agent program they already have — Claude Code, Codex CLI, OpenCode,
GitHub Copilot CLI, Grok Build, Gemini CLI, pi, Oh My Pi, anything that speaks ACP, an A2A agent or a custom binary
— which Bisa drives rather than shipping its own, in the background for a step or a conversation, or in
a terminal tab. In code, `crates/bisa-harness` is the abstraction — two traits, one event model, a
three-tier catalog, skill delivery, the shared subprocess plumbing — and `crates/bisa-adapters` holds one
adapter per harness, the only place a harness's own wording is ever read.

## Where it lives

- `crates/bisa-harness/src/traits.rs` — `HarnessAdapter` and `HarnessSession`.
- `crates/bisa-harness/src/catalog.rs` — `BUILTIN_IDS`, the presets, custom harnesses.
- `crates/bisa-harness/src/event.rs`, `crates/bisa-harness/src/types.rs` — `SessionEvent`, `SessionSpec`.
- `crates/bisa-harness/src/install.rs` — install hints in the official documentation's words, shown and never run.
- `crates/bisa-harness/src/proc.rs` — the one subprocess launcher; the node's token scrubbed from every child.
- `crates/bisa-harness/src/skills.rs` — how a skill reaches a session; `crates/bisa-harness/src/mock.rs` — `MockAdapter`, the harness every engine test scripts.
- `crates/bisa-adapters/src/` — one file per harness; `crates/bisa-adapters/src/acp.rs`, the one ACP door; `crates/bisa-adapters/src/util.rs`, probes and the dead-model patterns.
- `crates/bisa-adapters/src/hooks/` — a harness in a terminal reporting through its own hooks; `crates/bisa-adapters/src/usage/` — what a harness account has left.
- `crates/bisa-engine/src/readiness.rs`, `crates/bisa-engine/src/models.rs`, `crates/bisa-engine/src/presence.rs`, `crates/bisa-engine/src/interactive.rs` — the setup gate, model failover, the roster, a session in a terminal.
- `desktop/src/ui/harnessMarkModel.mjs` — the mark every built-in harness wears.

## Read first

- [06 — Agents and teams](../../architecture/06-agents-and-teams.md) — sessions and presence, model plans and failover, effort.
- [16 — The setup gate](../../architecture/16-setup-gate.md) — what a machine needs, and the rule that nothing is installed for the person.
- [ide/06 — Terminals § Reporting](../../architecture/ide/06-terminals.md#reporting--a-harness-in-a-terminal-is-a-roster-session) — a harness in a terminal as a roster session.
- [crates/harness](../../architecture/crates/harness.md), [crates/adapters](../../architecture/crates/adapters.md) — modules, invariants, extension points.
- [Recipe 8 — Add a harness adapter](../recipes.md#8-add-a-harness-adapter) — the files in order.

## Rules a change must keep

- A harness's prose is read once, at the adapter boundary: a line that means *this model is unavailable* becomes `ModelUnavailable` there, from error channels only, by conjunctions of literal fragments ([07 — Layering § Errors](../../architecture/07-layering.md#errors)).
- Snapshots are authoritative and progress advisory; errors are events, never panics; `Unavailable` walks the harness fallback chain, `ModelUnavailable` the model plan.
- The platform never installs, updates or signs in to anything and never runs an install line; every line of a hint comes from the harness's official page (I62).
- No harness child inherits the node's token, scrubbed in `crates/bisa-harness/src/proc.rs`; a custom harness may not set a `BISA_` variable or run an install command; a preset never shadows a built-in.
- An ACP harness writes no frame of its own — the protocol is `crates/bisa-adapters/src/acp.rs`, the harness is its facts — names no model or effort on its command line, and never passes a word that allows a tool unasked.
- `HarnessCaps::TOOL_GUARD` is declared only when the adapter stops before a tool runs and obeys the engine's `Deny`.
- Effort levels come from the harness's own documentation, cited with the date read; a level a model does not take is never sent; the harness's own word for it stays in its adapter — above it, *effort*.
- In a terminal, reporting files go under the session's own run folder, never into the harness's configuration or a project.
- Never a real harness in a test: `MockAdapter` in the engine, a stub peer in the adapters, the scripted agent in the journeys.

## Testing a change

- `scripts/test crate harness` — the catalog, the process plumbing, the line reader under property tests.
- `scripts/test crate adapters` — every adapter against its scripted peer; `scripts/test lib adapters` for the unit tests beside each wire; `FEATURES=a2a scripts/test crate adapters a2a` for A2A.
- `scripts/test module engine models`, `scripts/test module engine readiness`, `scripts/test module engine presence`, `scripts/test module engine interactive`.
- `desktop/src/ui/harnessMarkModel.test.mjs` — every built-in id wears a mark.
- Journeys: `crates/bisa-cli/tests/it/e2e/copilot_grok_and_gemini.rs`, `crates/bisa-cli/tests/it/e2e/a_harness_in_a_terminal.rs`, `crates/bisa-cli/tests/it/e2e/models_and_effort.rs`, `crates/bisa-cli/tests/it/e2e/the_setup_gate.rs`; the scripted agent goes under a harness's program name with `Sealed::install_agent_as`.
- One module at a time; the whole workspace (`just verify`) only at the end of a pass ([Running](../testing-rules.md#running)).

## Common changes

- [Add a harness adapter](../recipes.md#8-add-a-harness-adapter), and [a harness that speaks ACP](../recipes.md#a-harness-that-speaks-acp).
- [Add a setting](../recipes.md#2-add-a-setting) — `harness.*`, `terminal.*` and `agents.*` keys.
- [Add a dependency](../recipes.md#28-add-a-dependency) — a protocol crate is a licence the platform ships.
- [Say something to a person](../recipes.md#26-say-something-to-a-person) — a refusal's or a hint's sentence.

## Compatibility

- A harness id is written into agent records and settings (`terminal.default_harness`, `decisions.harness.id`), so it belongs to [the workspace on disk](../../reference/compatibility.md#the-workspace-on-disk) and [settings keys](../../reference/compatibility.md#settings-keys); the harness listing and usage routes are [the HTTP API and its events](../../reference/compatibility.md#the-http-api-and-its-events). The traits and each adapter's wire handling are internals ([not promised in 0.x](../../reference/compatibility.md#not-promised-in-0x)): a harness that changes its own CLI is met in its adapter, not in the contract.
- Inside 0.x a minor or patch release never breaks the public contract: a new harness id may be added.
- Renaming or removing an existing id cannot be made by addition; it waits for 1.0.0, which brings a migration tool ([the first major release](../../reference/compatibility.md#the-first-major-release)).
- Declare the compatibility of your change in the pull request.

## Review focus

- Is the harness's wording read only in its adapter, once and conservatively ([code review](../review/code.md))?
- Does a launch allow a tool unasked, leak the node's token or an environment value, or write into the person's configuration ([security review](../review/security.md))?
- Is every fact — an install line, a level, a model — from the harness's official documentation and cited with the date ([docs and language review](../review/docs-and-language.md))?
- Are line readers bounded and polls lengthened, nothing spinning ([performance review](../review/performance.md))?
- Is a new protocol dependency's licence one the platform may ship ([licences review](../review/licences.md))?
