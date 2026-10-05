# bisa-harness

The abstraction every coding harness is driven through: two traits, one event model, a three-tier
catalog, skill delivery, and the subprocess plumbing adapters share. It knows nothing about goals.
A harness is somebody else's program; this crate is the shape the engine expects it to have.

---

## Where things live

| Module | Owns |
|---|---|
| `lib.rs` | the module list and the contract in prose: snapshots are authoritative and progress advisory; errors are events, never panics; `Unavailable` walks the harness fallback chain; `ModelUnavailable` walks the model plan instead of failing the item |
| `error_text.rs` | `Localize` for `HarnessError` and `CatalogError` (`error-harness-…`) |
| `traits.rs` | `HarnessAdapter { id, display_name, caps, probe, models, efforts (the levels it takes for a model, lowest first — synchronous, the adapter's own table, spawning nothing; empty is nothing sent), recommended_plan and recommended_judge (what the setup gate's fixes write; `None` is *the first listed*), usage (the account's limits from the harness's own source, or the honest sentence — `USAGE_REPORTING` says which), interactive, interactive_reporting, translate_report, launch, attach }`, `HarnessSession { snapshot, phase, prompt, steer, follow_up, abort, answer (an `InputRequested`, requires `INPUT_REQUESTS`; the default refuses), subscribe, resume_token, dispose }`, `EventBroadcaster` — the fan-out every session implementation shares: events are advisory and a late subscriber misses what came before it, **but for one — the session's first terminal end**, kept and handed first to whoever subscribes after it, once (a session may be over before anybody listens: an agent refusing its model in the handshake) |
| `types.rs` | `SessionSpec` (`model` and `effort` — the effort already resolved and fitted by the engine, [06 § Effort](../06-agents-and-teams.md#effort); `mcp_servers: Vec<McpMount>` — every server with its provenance, and only a `Platform` mount's tools are pre-allowed by an adapter; `env` and `env_remove`: what the harness process is given and what it must not inherit — the engine\'s `network::session_env`), `ProbeResult`, `InteractiveLaunch`, `ReportingPlan` + `ReportingContext` + `LaunchFile` + `PullSource` (how one interactive session reports: files, arguments, a notification to intercept, a stream to pull; the context's `guard` argv and `guard_timeout_secs` are the pre-execution guard hook a recipe may add, `None` when this machine does not guard terminal sessions), `ResumeToken` (`model?` and `effort?` — what the session ran with, so a revival runs it again; `ResumeToken::of(adapter, native_id, spec)`), `Phase`, `SessionCost`, `SessionSnapshot`, `Attachment`, `PromptInput`, `Steer`, `SkillPayload`, `ModelInfo` (`efforts` — always said, empty when the model takes none), and the model-plan trio, `Effort` and `EffortChoice` re-exported from the core |
| `event.rs` | `SessionEvent`, `LifecycleEvent` (with `InputRequested { request }` and `InputResolved { id }`), `Outcome`, `ProgressEvent` (turns, tools, text — `TextDelta`, a piece of the reply — and thinking — `ThinkingDelta`, a piece of the model's reasoning as it streams, never an instruction and never the reply ([13 § The reply streams](../13-conversations.md#the-reply-streams)) — cost, sub-agents, and `ModelChanged { model } — the harness now runs on this model, said at start and on every switch of its own; the one way a roster row's model moves after the launch that registered it), `InputRequest { id, kind, parent }`, `InputKind { Permission { tool_name, tier, args_summary, input }, Question, Auth }` — `input` is the tool's raw input as the harness carries it, what the guard reads a command and its paths from, `Null` when the harness does not say — `InputAnswer { Allow { input }, Deny, Text }` — an `Allow` may hand back a rewritten input, a redacted placeholder restored, for a harness with `INPUT_REWRITE`; `InputAnswer::ALLOW` runs the call as asked — `SubagentId`, `ProgressEvent` (with `SubagentStarted`, `SubagentEnded` and `Nested { parent, event }` — a sub-agent's progress, wrapped so a consumer of the parent's stream never mistakes it for the agent's own) ; `LifecycleEvent::ProcessStarted { pid }` is emitted by every subprocess adapter after each spawn — the engine records it on the session row, which is what lets a restart terminate the one child a dead node left writing |
| `error.rs` | `HarnessError` — surfaced upstream as a session outcome, never a panic |
| `usage.rs` | what a harness's **account** has left — `UsageState { Report { report } · Unsupported { reason } · NotSignedIn { reason } · Off · Failed { reason } }`, `UsageReport { harness, read_at, source: endpoint · app_server · cli, account { plan, login }, windows, extras }`, `UsageWindow { id, label, scope, used_percent, resets_at }` (a model's week is labelled with the model's own word — *Fable* — and scoped to it); `window_label` (*5h*, *Daily*, *Weekly*, *Monthly* — the one vocabulary every adapter labels through), `tightest`, `model_word`, `iso_to_unix`; `UsageState::unsupported(name)` is the trait's default answer in the harness's own words, `cacheable()` says a failure is asked again. *Usage* is the account's limits; *cost* (`SessionCost`) is what one session spent. A report never carries a credential |
| `catalog.rs` | three tiers: `BUILTIN_IDS` (`claude-code`, `codex`, `pi`, `omp`, `opencode`, `copilot`, `grok`, `acp`, `custom`), `PRESET_HARNESSES` (`preset:goose`, `preset:cursor-agent` — probed on `PATH`, never installed, never allowed to shadow an adapter), and `CustomHarnessSpec` (no install commands, a sanitised environment, `RESERVED_ENV_PREFIX = "BISA_"` forbidden); `HarnessCatalog`, `HarnessListing` (with `tool_guard` and `input_rewrite`, read off a compiled-in adapter's caps and `false` for a preset or custom row — whether the Tool & Commands Guard can stop a call in it), `HarnessTier`, `probe_command`; `list_cached` — the probed listing held for `cache.harness_listing.ttl_ms`, except one in which a probe outstayed its budget: answered, the row unavailable and saying so, and not kept, so the next ask probes again (a cold `--version` at a first launch is not a harness that is absent) — a preset names its own way to continue its latest session (`PresetHarness::resume_args`: Goose `session --resume`, Cursor `--continue`), the tool's own words; `HarnessListing.install` — the official install hint of a compiled-in harness (`install::install_hint`), the URL also in `install_hint`; `installed_cached(id, ttl)` — whether the last probe found it, from the warm listing alone, for a synchronous reader |
| `install.rs` | how a person installs what the platform needs, in the official documentation's words and never run by the platform ([16 — The setup gate](../16-setup-gate.md)): `InstallHint { url, commands: [PlatformCommand { platform, command }], verify?, sign_in? }`, `Platform::{MacOs, Linux, Windows}`, `install_hint(id)` for `claude-code` · `codex` · `opencode` · `copilot` · `grok`, `git_hint()`, `commands_for(platform)` — every URL and line read from git-scm.com/install, code.claude.com, the openai/codex README, opencode.ai/docs, the github/copilot-cli README and docs.x.ai/build; Copilot CLI's sign-in line says a `COPILOT_GITHUB_TOKEN`, `GH_TOKEN` or `GITHUB_TOKEN` in the environment is used before `copilot login` |
| `skills.rs` | how a skill reaches a session: `claude-code` gets `<cwd>/.claude/skills/<id>/SKILL.md`, every other harness gets a markdown appendix; a pre-existing file with different content is never overwritten — the user's copy wins and the appendix carries the skill instead; `SkillInjection`, `as_appendix`, `materialize`, `safe_id` |
| `proc.rs` | `ProcSpec`, `ProcHandle`, `Line` — the one subprocess launcher adapters share; `SCRUBBED_ENV` (`BISA_API_TOKEN`) is removed from every child's environment before `spec.env` is added and `spec.env_remove` — the proxy names the `network.proxy.mode` setting says the harness must not inherit — is removed after it, so no harness — and nothing a harness runs — inherits the node's own authority (a unit test spawns `printenv` for the name and reads nothing back) |
| `mock.rs` (feature `mock`) | `MockAdapter`, `MockSession`, `IntakeScript`, `DeadModel`, `ModelFailure` — a scriptable harness for every engine test, including model walls in the four shapes a real harness takes (`DeadModel`: refused by `launch`, a terminal wall with nothing before it, a wall after real work, and `BeforeListening` — over before `launch` returns, the first prompt taken all the same); `MockAdapter::prompts()` is what every session was prompted with, so a test can read the directive an agent received, and `follow_ups()` what a warm session was told on the steer path; `input_request` parks a turn on a scripted request until `answer` (`answered()` records it); `subagent_script` emits one nested sub-agent; `listed_models`, `efforts` and `model_efforts` are the models and the levels it answers, `recommended_plan` and `recommended_judge` what it recommends, `launched_efforts()` the effort each launch carried, `attached()` every token a revival was given and `closes()` how each session was closed from outside (`Close::Aborted`, `Close::Disposed`) — what lets a test tell a harness that was told to stop from a roster row that only says so |

---

## Entry points

`HarnessCatalog::new()` and `register(adapter)`; `catalog.listing()`; `adapter.launch(spec)` →
`Box<dyn HarnessSession>`; `session.subscribe()` → a stream of `SessionEvent`.

---

## Invariants held here

| Invariant | Held by |
|---|---|
| A preset never shadows a built-in adapter | `tests/it/harness.rs` |
| A custom harness cannot set an `BISA_*` variable or run an install command | `tests/it/harness.rs` |
| A session's terminal end is never missed: a subscriber that comes after it hears it, once; one that was there hears it once, from the session; what is kept is the first terminal end, and a turn's end that is not the session's is kept for nobody | `traits.rs` — `a_subscriber_that_comes_after_the_end_still_hears_it_once`, `a_subscriber_that_was_there_hears_the_end_once`, `what_is_kept_is_the_first_terminal_end_and_no_turns_end` |
| A child's output is read to its end whatever it holds (`proc::next_line`): a byte that is no UTF-8 is replaced and the stream goes on — `lines()` stopped there for good, leaving the child blocked on a full pipe with nobody saying why — one line is held to `MAX_LINE_BYTES` (16 MiB), its rest read and let go and the cut said once; a last line needs no newline; stderr is drained the same way; a read that fails is a `warn` naming the program | `src/proc.rs` unit tests |
| A skill file a person edited is never overwritten | `skills.rs` tests |
| This crate never names `HumanConsent` or the consented git tier | `crates/bisa-core/tests/it/layering.rs` |

---

## Errors

`HarnessError`: a launch that failed, a probe that found nothing, a session that died. Each reaches
the engine as an `Outcome` on the session, where `Unavailable` moves to the next harness candidate
and `ModelUnavailable` to the next model.

---

## Extension points

| To add… | Touch, in order | The gate that catches a miss |
|---|---|---|
| a built-in harness | `BUILTIN_IDS` in `catalog.rs` → the adapter in [`bisa-adapters`](adapters.md) (its own wire, or its facts over the one ACP door when it speaks that protocol) → `register_all` → a Cargo feature → its `InstallHint` in `install.rs`, from its official page → its `usage()`: a reader in `adapters/src/usage/` over a fixture, or the trait's default sentence | `tests/it/harness.rs`; `install.rs` tests (a line for every platform, a way to check it landed) |
| a preset | `PRESET_HARNESSES` — a command to probe on `PATH`, nothing installed | `tests/it/harness.rs` |
| a capability flag | `HarnessCaps` in `crates/bisa-core/src/caps.rs` → the adapters that have it | — |
| a session event | `event.rs` → every adapter that emits it → `EnginePayload::Session` carries it unchanged | — |

---

## Tests

`tests/it/harness.rs`; unit tests beside `skills.rs` and `catalog.rs`; `mock.rs` is what the engine's
tests script.

---

## What this crate refuses to do

- know about goals, projects or the store;
- install a harness;
- read a harness's prose — that is the adapter boundary's job, once;
- hold or print a harness's credential — a usage reader in the adapters crate holds one for its single request and drops it; what crosses this crate is percentages, labels and reset times.
