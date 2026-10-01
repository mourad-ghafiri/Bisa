# Security

Three features stand between a person's machine and a model, each on a seam that already existed:
the **Redactor** — a key, a token or a value the rules recognise travels as a placeholder, never raw;
the **Tool & Commands Guard** — every command, path and tool a guarded harness is about to run is
judged first; and the **Classifier** — a model reads the redacted call and answers safe or harmful.
Around them stand the `publish` gate an outward git act passes, and the consent a tree-moving git
operation requires. The rules are pure functions in `bisa-security`; the seams are the engine's. A
vulnerability is never an issue: report it privately, as [SECURITY.md](../../../SECURITY.md) says.

## Where it lives

- `crates/bisa-security/src/` — `redact.rs` (rules, placeholders, the vault), `guard.rs` (rules, matchers, the evaluation), `builtin.rs` (the built-in rules and their fixtures), `classify.rs` (what the classifier is asked, how its verdict is read), `net.rs` (outbound hosts), `policy.rs` (the settings layers into one policy).
- `crates/bisa-engine/src/security.rs` — the seams: outbound redaction, inbound placeholders, the restore at an execution point; `crates/bisa-engine/src/inputs.rs` — the one permission funnel; `classifier.rs`, `content.rs` (the content screen) and `ask.rs` (the classifier's bounded session) beside it.
- `crates/bisa-harness/src/proc.rs` — the node's own bearer token kept out of every child's environment.
- `crates/bisa-node/src/security.rs` · `crates/bisa-cli/src/security.rs` — the status and the previews, `bisa security`; `bisa session guard` is the pre-execution hook a harness opened in a terminal runs ([CLI § The guard](../../reference/cli.md#the-guard)).
- `crates/bisa-engine/src/projects.rs` (`pass_publish_gate`) — the `publish` gate a push or a pull request passes.
- `crates/bisa-vcs/src/interactive.rs` · `crates/bisa-node/src/ide/consent.rs` — the tree-moving git verbs, and `HumanConsent`, minted in one place.
- `desktop/src/views/_settings/SecurityPanels.tsx` · `desktop/src/views/_settings/securityRules.mjs` — Settings › Security.

## Read first

- [11 — Security](../../architecture/11-security.md) — the placeholder contract, the seams, the guard's evaluation order, the classifier's contract, outbound hosts, what arrives from outside, the diagnostic log, the invariant.
- [crates/security](../../architecture/crates/security.md) — the modules, the invariants, the extension points, what the crate refuses to do.
- [Projects § The `publish` gate](../../guide/projects.md#the-publish-gate) and [ide/04 § `HumanConsent`](../../architecture/ide/04-git.md#humanconsent).
- [15 § Failing closed at the security points](../../architecture/15-decision-making-agent.md#failing-closed-at-the-security-points) — when the Decision-Making Agent reads for the classifier.

## Rules a change must keep

- No secret the redactor recognises reaches a harness, a journal, a route or a code host from the platform's hand; a placeholder is restored only at an execution point on this machine; the node's bearer token is in no child's environment ([11 § Invariant](../../architecture/11-security.md#invariant)).
- Best effort, not a sandbox: the features judge what a guarded harness asks; a harness without the guard's hook is observed, not vetoed — and the pages say so ([11](../../architecture/11-security.md)).
- `bisa-security` is pure: no I/O, it never runs what it judges, it depends on `bisa-netrules` alone, and it answers *fall through*, not *allow*, when no rule has an opinion ([crates/security § What this crate refuses to do](../../architecture/crates/security.md#what-this-crate-refuses-to-do)).
- The classifier never allows what the rules did not name; every outcome but a clear verdict fails closed to *no verdict*, which goes to the person; its prompt goes out redacted, whoever reads it ([11 § The classifier's contract](../../architecture/11-security.md#the-classifiers-contract)).
- A built-in rule's id is stable — a setting names it to switch it off — and its fixture is synthetic; a destructive shape is spelled only in `builtin.rs`'s own test module, never under a `tests/` directory ([Add a built-in security rule](../recipes.md#17-add-a-built-in-security-rule)).
- An outward act passes the `publish` gate, which never defers to an assignment, so it fails closed to the owner ([Projects § The `publish` gate](../../guide/projects.md#the-publish-gate)).
- A tree-moving git operation requires a `HumanConsent`, minted only from an authenticated request; the engine cannot make one and the MCP intake has no field for one ([02 § The IDE](../../architecture/02-domain-model.md#the-ide), I38).
- The diagnostic log never records a prompt, a message body, a file's text, a token or an environment value ([11 § The diagnostic log](../../architecture/11-security.md#the-diagnostic-log)).

## Testing a change

- `scripts/test lib security` — the crate's unit tests; it has no integration directory, being pure.
- `scripts/test module engine security` · `scripts/test module engine content` · `scripts/test module engine decisions` — the seams against a mock harness, the content screen, the classifier failing closed.
- `scripts/test module node security` — the status, the previews and the transcript route carry no value.
- `scripts/test lib harness proc` — a `printenv` child finds no token.
- `scripts/test module vcs interactive` · `scripts/test module core layering` — every interactive verb takes consent and records its recovery ref first; consent is minted in one place; no test source spells a destructive command.
- `scripts/test desktop views/_settings` and `desktop/src/ui/secretInputModel.test.mjs`.
- The journey `crates/bisa-cli/tests/it/e2e/settings_security_and_the_log.rs` — a secret of the daemon's environment as a placeholder there and back, a call asked and refused, a rule's refusal, the previews — under `scripts/test module cli e2e`.
- Fakes only: a token-shaped string that is not one, a scripted mock, a command matched and never run ([Testing rules § Guard tests](../testing-rules.md#guard-tests)).

## Common changes

- [Add a built-in security rule](../recipes.md#17-add-a-built-in-security-rule) — a person's own rule needs no code: it is a setting.
- A host rule, a detector or a matcher kind: [crates/security § Extension points](../../architecture/crates/security.md#extension-points).
- A `security.*` key: [Add a setting](../recipes.md#2-add-a-setting); a log line: [Log something](../recipes.md#20-log-something).

## Compatibility

- The `security.*` keys and their choices are [settings keys](../../reference/compatibility.md#settings-keys); a built-in's id is a stored word (the `builtins_off` keys name it), so it is never renamed.
- `bisa security`, `bisa session guard`, the security routes and the publish refusals' error codes are [the command line](../../reference/compatibility.md#the-command-line) and [the HTTP API](../../reference/compatibility.md#the-http-api-and-its-events).
- A change grows by addition; one that cannot waits for 1.0.0. Declare its compatibility in the pull request ([Keeping compatibility](../compatibility.md)).

## Review focus

- Every path by which text leaves the machine or reaches a model passes the redactor, and a restore happens only at an execution point ([Security review](../review/security.md)).
- A new place where an agent acts goes through the one permission funnel; a new outward act through the `publish` gate; a new tree-moving verb takes consent.
- No real token, key or destructive command in a test; a setting that loosens a guard states its scope and its default.
- The rule list in [11](../../architecture/11-security.md) and the words of Settings › Security change with a built-in ([Docs and language](../review/docs-and-language.md)); a new dependency's licence ([Licences review](../review/licences.md)).
