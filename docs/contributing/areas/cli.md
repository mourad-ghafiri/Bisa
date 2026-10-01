# The command line

For a person, `bisa` is the command line: every verb a terminal or a script needs, `--json` for
machines, and the same workspace the desktop shows. In code, `crates/bisa-cli` builds the `bisa`
multicall binary in three personalities — the client, the daemon (`bisa node`) and the MCP server a
harness session is handed (`bisa mcp`). It holds no domain rule: it parses arguments, routes each
command to a running node or embeds an engine for one shot, and prints with one output discipline. Its
integration binary also holds the journeys — the platform end to end through the real binary.

## Where it lives

- `crates/bisa-cli/src/main.rs` — `enum Command`, the verbs.
- One module per family — `crates/bisa-cli/src/workflow.rs`, `crates/bisa-cli/src/projects.rs`, `crates/bisa-cli/src/agents.rs`, `crates/bisa-cli/src/step.rs` and the rest.
- `crates/bisa-cli/src/client.rs` — the node's client, and the way to it when a daemon holds the lock.
- `crates/bisa-cli/src/output.rs` — human lines and `--json`.
- `crates/bisa-cli/src/verbs.rs` — every leaf of the parser, and the guard that each is run by a test.
- `crates/bisa-cli/src/localize.rs` — the help in a person's language; every line lives in `locales/en/cli.ftl`.
- `crates/bisa-cli/src/session.rs` — the reporter a terminal's hooks run.
- `crates/bisa-cli/tests/it/cli.rs`, `crates/bisa-cli/tests/it/docs.rs`, `crates/bisa-cli/tests/it/connector.rs` — the verbs over the built binary.
- `crates/bisa-cli/tests/it/e2e/` — the journeys; `crates/bisa-cli/tests/it/e2e/sealed.rs` the sealed workspace; `crates/bisa-cli/tests/scripted_agent.rs` the scripted agent.
- `docs/reference/cli.md` — every verb.

## Read first

- [crates/cli](../../architecture/crates/cli.md) — the personalities, the invariants, the journeys and what each proves.
- [CLI reference](../../reference/cli.md) — every verb.
- [Testing rules § Where a test lives](../testing-rules.md#where-a-test-lives) — the journeys, the sealed workspace, the scripted agent, the daemon.
- [Operating](../../guide/operating.md) — the daemon, a crash, the log, as a person runs them.

## Rules a change must keep

- No domain rule and no store write of its own. A command routes through a running node when one holds the lock and embeds an engine otherwise — never both (I40); an embedded engine starts no run from an event.
- The command line writes the store only where no node runs: a function that calls a store writer asks for the node first, or is excused by name with its reason (`crates/bisa-node/tests/it/layering.rs`).
- Exit 0 when the verb did what it said, 1 when it ran and failed, 2 when the line could not be read; under `--json` a failure writes nothing on stdout, and its words go to stderr.
- Every verb is named in the reference and run by a test that names it (`crates/bisa-cli/tests/it/docs.rs`, `crates/bisa-cli/src/verbs.rs`).
- Every line is a catalog message said by its id, its help mirrored as `cli-cmd-<path>` and `cli-arg-<path>-<id>`; the help in another language keeps every argument's order.
- No token on the command line — it is read from stdin; never `--force` or `--no-verify` to git; never a secret printed twice.
- A journey asserts what a feature promises a person, never how the code keeps it. It runs sealed — no token, proxy, real harness or code host CLI in reach, no push beyond a local bare repository — and no script spells a destructive command, even as data.

## Testing a change

- `scripts/test module cli cli` — every verb over the real binary, the exit codes, routing through a running daemon.
- `scripts/test module cli docs` — the reference names every verb.
- `scripts/test lib cli verbs`, `scripts/test lib cli localize` — every verb run by a test; the help in a language.
- `scripts/test module cli e2e` — the journeys, one nextest group run two at a time; `scripts/test module cli e2e goals` runs one.
- A refusal in a journey goes through `Sealed::refused`, the one way to ask for one; a daemon a test starts is ended when its value is dropped, and one that cannot bind fails the test in its own words.
- One module at a time; the whole workspace (`just verify`) only at the end of a pass ([Running](../testing-rules.md#running)).

## Common changes

- [Add a CLI verb](../recipes.md#12-add-a-cli-verb)
- [Say something to a person](../recipes.md#26-say-something-to-a-person)
- [Add an engine event](../recipes.md#4-add-an-engine-event) — its line in `crates/bisa-cli/src/activity.rs`.
- [Add a step kind](../recipes.md#16-add-a-step-kind) — a `step` verb when a person completes it.
- [A harness that speaks ACP](../recipes.md#a-harness-that-speaks-acp) — the scripted agent placed under a harness's own program name.
- A journey: one file under `crates/bisa-cli/tests/it/e2e/`, named after what it proves and declared in `crates/bisa-cli/tests/it/e2e/mod.rs`.

## Compatibility

- [The command line](../../reference/compatibility.md#the-command-line) is public: every verb and flag of `bisa`, the exit statuses, and under `--json` the output's shape and the rule that a failure prints nothing on stdout. The human-readable output — its wording and layout — is not promised.
- Inside 0.x a minor or patch release never breaks the public contract: add verbs, flags and `--json` fields.
- Renaming or removing a verb or a flag, or changing an exit status, cannot be made by addition; it waits for 1.0.0, which brings a migration tool ([the first major release](../../reference/compatibility.md#the-first-major-release)).
- Declare the compatibility of your change in the pull request.

## Review focus

- Does the verb hold a rule or write the store itself, instead of going through the node and the engine ([code review](../review/code.md))?
- Are the exit codes and the `--json` shape held, so a script can rely on them ([compatibility review](../review/compatibility.md))?
- Does a secret reach argv, the output twice, or a journey's environment ([security review](../review/security.md))?
- Is every new line a catalog message, the help mirrored and the reference updated ([docs and language review](../review/docs-and-language.md))?
- Does a journey stay sealed and quick — a poll with a deadline, never a sleep ([performance review](../review/performance.md))?
