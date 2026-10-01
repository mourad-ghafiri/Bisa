# Connectors

A connector lets a workflow step — or an agent — call an outside platform through that platform's
own API: send a mail, post to a channel, search an issue tracker. The **definition** (its base URL,
the hosts it may reach, its auth scheme, its operations) syncs with the workspace; the **account** a
person adds is this machine's alone, its secret fields in the keystore. In code, the definition and
its validation are `bisa-core`'s, everything between a definition and the wire is
`bisa-connectors`, the host grammar three crates share is `bisa-netrules`, and fifteen built-ins are
TOML files in the catalog.

## Where it lives

- `crates/bisa-core/src/connector.rs` — the definition: `Connector`, its operations, auth schemes, body and parameter kinds, and `validate`.
- `crates/bisa-connectors/src/` — the call: parameters bound by kind (`request.rs`), placeholders rendered (`template.rs`), bodies encoded (`body.rs`), one signer per scheme under `auth/`, OAuth2 (`oauth.rs`), hosts (`hosts.rs`), retries and the circuit (`retry.rs`, `breaker.rs`), the outcome; ports for credentials, files, the clock and entropy.
- `crates/bisa-netrules/src/lib.rs` — what a host pattern is, which authorities are loopback, the headers a definition may never set.
- `crates/bisa-security/src/net.rs` — a person's allow and deny lists judged by the same grammar.
- `crates/bisa-engine/src/connectors.rs` — the step and the start that call, `call_spec` and the shape maps.
- `crates/bisa-node/src/connectors.rs` · `crates/bisa-store/src/connectors.rs` · `crates/bisa-cli/src/connector.rs` — the routes, the records and the default account, `bisa connector`.
- `library/catalog/connectors/` — the fifteen built-ins, one TOML file each.
- `desktop/src/views/_settings/connectorsModel.mjs` · `desktop/src/views/_workflow/forms/connectorStepModel.mjs` — the Settings panel's facts and the step form's.

## Read first

- [Connectors](../../guide/connectors.md) — accounts, the built-ins, a custom connector, bodies and signed tokens, polling, [what a call may reach](../../guide/connectors.md#what-a-call-may-reach).
- [03 — Workflows § Connectors](../../architecture/03-workflows.md#connectors) — the model.
- [crates/connectors](../../architecture/crates/connectors.md) — the modules, the invariants, the extension points, what the crate refuses to do.
- [crates/netrules](../../architecture/crates/netrules.md) — the one host grammar.
- [11 — Security § Outbound hosts](../../architecture/11-security.md#outbound-hosts) — deny wins, then the declared hosts, then the allow list.

## Rules a change must keep

- A call reaches only the hosts its definition declares; `http` and `insecure_tls` are for loopback alone; `security.net.deny_hosts` wins over every definition, the OAuth2 consent page and token endpoint included ([guide § What a call may reach](../../guide/connectors.md#what-a-call-may-reach)).
- The credential is applied to the one request that leaves the machine and appears nowhere else: a `Secret` cannot be printed, every error is scrubbed, and an answer that echoes a secret passes the redactor ([11 § Invariant](../../architecture/11-security.md#invariant)).
- The crate follows no redirect, keeps no cookie, runs no shell and touches no file — a token goes back through `Credentials`, a file comes in through `Files` ([crates/connectors § What this crate refuses to do](../../architecture/crates/connectors.md#what-this-crate-refuses-to-do)).
- `bisa-connectors` depends on `bisa-http` and `bisa-netrules` alone; everything else arrives through a trait the engine implements, so a test runs against a loopback stub ([crates/connectors](../../architecture/crates/connectors.md), [07 — Layering](../../architecture/07-layering.md)).
- A built-in describes every operation and parameter, takes its tags from the vocabulary, and names a `check` that has no required parameter and writes nothing ([Add a connector](../recipes.md#21-add-a-connector)).
- A custom definition is validated before it is written and refused by name; it is removable only while no account and no workflow step names it ([Add a connector](../recipes.md#21-add-a-connector)).
- A test reaches no platform, keychain or network beyond a loopback stub, and signing keys are generated as the test runs ([Testing rules § Guard tests](../testing-rules.md#guard-tests)).

## Testing a change

- `scripts/test crate connectors` — `crates/bisa-connectors/tests/it/` (request, auth, oauth, hosts, retry, outcome, error, hardening) and the unit tests.
- `scripts/test crate netrules` — the grammar's cases in `crates/bisa-netrules/tests/it/hosts.rs`.
- `scripts/test module engine connectors` · `scripts/test module node connectors` · `scripts/test module store connectors` · `scripts/test module cli connector`.
- `scripts/test lib store catalog` — `every_connector_is_well_formed` and `the_catalog_ships_fifteen_connectors`.
- `scripts/test desktop views/_settings`, then from `desktop/`: `node --test --import ./src/i18n/preload.mjs src/scenarios/connectors.test.mjs`.
- The journey `crates/bisa-cli/tests/it/e2e/a_platform_reached_through_a_connector.rs` — a definition, an account checked, a run that reads, asks and writes once, a poll, the secret nowhere — under `scripts/test module cli e2e`.
- One module at a time; the whole workspace and `just verify` only at the end ([Testing rules § Running](../testing-rules.md#running)).

## Common changes

- [Add a connector](../recipes.md#21-add-a-connector) — a built-in; a new way a call holds up (a field like `timeout_secs`, `idempotency` or `page`); a new auth scheme, body kind or parameter kind.
- The order across the crate for each of those: [crates/connectors § Extension points](../../architecture/crates/connectors.md#extension-points).
- [Regenerate and verify](../recipes.md#22-regenerate-and-verify) — the catalog page, the wire types, the HTTP reference.
- [Add a dependency](../recipes.md#28-add-a-dependency).

## Compatibility

- A connector definition is one of the files a person writes: a definition a release accepts is accepted, and means the same, by every later release of the line ([Workflow, connector and catalog files](../../reference/compatibility.md#workflow-connector-and-catalog-files)). It grows by an optional key, or by a new scheme, body kind or parameter kind beside the old ones; renaming a key or changing what one means waits for 1.0.0.
- `bisa connector` and the `/connectors` routes, with the status each refusal answers, are [the command line](../../reference/compatibility.md#the-command-line) and [the HTTP API](../../reference/compatibility.md#the-http-api-and-its-events).
- The fifteen built-ins are catalog content, not contract: one may change in a minor, and an installed copy is never touched by an upgrade.
- Declare your change's compatibility in the pull request ([Keeping compatibility](../compatibility.md)).

## Review focus

- No credential is logged, stored, returned or quoted; the host check comes before any request ([Security review](../review/security.md)).
- A new field or kind reaches every layer the recipe names — core validation, the crate, the engine's maps, the desktop's mirror, the guide ([Code review](../review/code.md)).
- Retries and pages stay bounded: a write is not retried on a 500, a paged read stops at its cap, a host's circuit opens ([Performance review](../review/performance.md)).
- A built-in's paragraph in the guide says what to create at the platform and where the credential goes ([Docs and language](../review/docs-and-language.md)); a definition only grows ([Compatibility review](../review/compatibility.md)).
