# Decision-making

The Decision-Making Agent is the third core agent of a workspace, and the one that judges. A
**decision point** — which of an agent's models leads, who of a pool takes a work item, whether a tool
call is harmful — asks it a typed question against a state and gets back a **judgement** sure enough to
act on, or the reason the point's own rule runs instead. It holds no conversation, takes no work item,
signs nothing, and is off by default everywhere. In code, the contract and the eleven points are
`bisa-core`'s, the providers that answer are `bisa-decision`, and the engine's `decider.rs` asks,
records and acts.

## Where it lives

- `crates/bisa-core/src/decision.rs` — the contract (requests, answers, certainty), the decision points, which of them fail closed.
- `crates/bisa-decision/src/` — the provider port (`provider.rs`), the calibrated System One wire (`system_one.rs`), a generative model held to the same shape (`prompted.rs`), the three decorators every answer passes (`resilient.rs`), the one factory (`factory.rs`), and `scripted.rs`, the fake every test above the crate answers from.
- `crates/bisa-engine/src/decider.rs` — one judgement end to end: the switch, the redactor, the deadline, the threshold, the record.
- `crates/bisa-node/src/decisions.rs` · `crates/bisa-cli/src/decisions.rs` — `/decisions/status`, `/decisions`, `/decisions/try`, the key routes; `bisa decisions`.
- `crates/bisa-store/src/decision_making_agent.rs` · `library/core/decision-making-agent.toml` — its compiled-in definition, held equal to the registry's defaults.
- `desktop/src/views/_settings/DecisionsPanel.tsx` · `desktop/src/views/_settings/decisionsModel.mjs` — Settings › Decision Settings › Decision Making.

## Read first

- [15 — The Decision-Making Agent](../../architecture/15-decision-making-agent.md) — what it is and is not, the contract, the providers, [the decision points](../../architecture/15-decision-making-agent.md#the-decision-points), the order of one judgement, what is recorded.
- [The Decision-Making Agent](../../guide/decisions.md) — switching it on, choosing who answers, trying it, reading judgements.
- [crates/decision](../../architecture/crates/decision.md) — the providers, the decorators, what the crate refuses to do.
- [11 — Security](../../architecture/11-security.md) — the classifier the three security points stand beside.

## Rules a change must keep

- A judgement never signs a gate: a decision is a person's signed approval alone, and a judgement travels as `JournalPayload::Judgement` and `EnginePayload::Judged`, never as a `Decision` ([15 § Invariant](../../architecture/15-decision-making-agent.md#invariant)).
- Every point keeps its own rule as the fallback: below the threshold, unsure or failed, the caller's own logic runs — the agent is never the only way a point is decided ([15 § The decision points](../../architecture/15-decision-making-agent.md#the-decision-points)).
- The three security points fail closed: only a sure *none* is safe; unsure, failed and off read as no verdict, which goes to the person; `decisions.confidence.security` is never set below 0.5 ([15 § Failing closed at the security points](../../architecture/15-decision-making-agent.md#failing-closed-at-the-security-points)).
- What stays a fixed rule stays one — governance, the guard's rule order, the run machine's conditions and retries, a hard model pin: a judgement there would make a replay non-deterministic ([15 § The decision points](../../architecture/15-decision-making-agent.md#the-decision-points)).
- Off by default, everywhere: `decisions.enabled` is `false` ([15 § Settings](../../architecture/15-decision-making-agent.md#settings)).
- The state and every question pass the redactor before they leave the process; `bisa-decision` reads no setting, keystore or clock of its own and depends on `bisa-core` and `bisa-connectors` alone ([crates/decision](../../architecture/crates/decision.md)).
- A remote provider's API key is never a setting and is never read back: it is kept in this machine's keystore ([15 § Settings](../../architecture/15-decision-making-agent.md#settings)).
- Tests run against fakes: `ScriptedProvider` and a loopback stub, never the real endpoint ([Testing rules § Guard tests](../testing-rules.md#guard-tests)).

## Testing a change

- `scripts/test crate decision` — the factory, the System One wire, the generative reader, the retry budget.
- `scripts/test lib core decision` — the contract round-trips, the points, which of them fail closed.
- `scripts/test module engine decisions` · `scripts/test module node decisions` — the points wired into the engine, the threshold, the record; the HTTP surface, `try` deciding and recording nothing, a key never read back.
- `scripts/test lib store decision_making_agent` — the definition and the registry say the same thing; the reserved id.
- `scripts/test desktop views/_settings`, then from `desktop/`: `node --test --import ./src/i18n/preload.mjs src/scenarios/decisionMaking.test.mjs`.
- The journey `crates/bisa-cli/tests/it/e2e/the_decision_making_agent.rs` — who answers and that it is off, one question tried with nothing recorded, a `judge` step's branch and its `otherwise` — under `scripts/test module cli e2e`.
- One module at a time; `just verify` closes the pass ([Testing rules § Running](../testing-rules.md#running)).

## Common changes

- A `decisions.*` key: [Add a setting](../recipes.md#2-add-a-setting) — the definition's defaults in `library/core/decision-making-agent.toml` follow, and a test holds the two equal.
- A provider: `factory.rs` is the one place a provider kind is matched ([crates/decision § Where things live](../../architecture/crates/decision.md#where-things-live)); its word is a new `decisions.provider` choice.
- A decision point: its word in `crates/bisa-core/src/decision.rs` beside the eleven, the caller's own rule kept as its fallback, the desktop's list (which `desktop/src/views/_settings/decisionsModel.test.mjs` reads from that file), and its row in [15 § The decision points](../../architecture/15-decision-making-agent.md#the-decision-points).
- A surface: [Add an HTTP route](../recipes.md#1-add-an-http-route), [Add a CLI verb](../recipes.md#12-add-a-cli-verb), [Add an MCP tool](../recipes.md#5-add-an-mcp-tool).

## Compatibility

- The `decisions.*` keys and their choices are [settings keys](../../reference/compatibility.md#settings-keys); a point's word is what `decisions.points_off` stores, so it is never renamed.
- The `/decisions` routes, `bisa decisions` and the `decide` tool are [the HTTP API](../../reference/compatibility.md#the-http-api-and-its-events), [the command line](../../reference/compatibility.md#the-command-line) and [MCP tools](../../reference/compatibility.md#mcp-tools); the `judge` step is part of a workflow definition ([Workflow, connector and catalog files](../../reference/compatibility.md#workflow-connector-and-catalog-files)).
- A change grows by addition — a new point, provider or field beside the old ones; what cannot be added waits for 1.0.0. Declare your change's compatibility in the pull request ([Keeping compatibility](../compatibility.md)).

## Review focus

- A judgement standing in for a signature or for one of the fixed rules, or a point that no longer falls back to its own rule ([Code review](../review/code.md)).
- The security points fail closed, nothing reaches a provider unredacted, and a key is never read back ([Security review](../review/security.md)).
- One deadline per judgement, retries included, and a judgement never waits on another ([Performance review](../review/performance.md)).
- *Judgement* for what it answers and *decision* for a person's approval; the agent's full name ([Docs and language](../review/docs-and-language.md)); the contract only grows ([Compatibility review](../review/compatibility.md)).
