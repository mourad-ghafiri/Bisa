# Core

A person using Bisa never sees the core, but it is why a goal's status, a workflow's problems and a
setting's scope read the same in the desktop, in the CLI and over the wire. In code it is
`crates/bisa-core`, the zero-I/O foundation: every domain type and rule, the workflow definition and
its validation, the run machine, the GEP kind registry and the settings registry, each a pure
function with a unit test beside it.

## Where it lives

- `crates/bisa-core/src/run.rs` — the run machine: `WorkflowRun::apply`, the one function that changes a run, and its property tests.
- `crates/bisa-core/src/workflow.rs` — the definition, the eighteen step kinds, `Workflow::validate`, `bind_inputs`, conditions.
- `crates/bisa-core/src/start.rs`, `crates/bisa-core/src/listen.rs`, `crates/bisa-core/src/boundary.rs`, `crates/bisa-core/src/template.rs` — start events, filters and the chain, boundary events, the placeholder grammar.
- `crates/bisa-core/src/goal.rs`, `crates/bisa-core/src/workitem.rs`, `crates/bisa-core/src/workstream.rs` — the goal's status projection and the two smaller machines.
- `crates/bisa-core/src/settings.rs` — the settings registry and the only resolution of a key.
- `crates/bisa-core/src/kind.rs` — the GEP kind numbers and their policy sets.
- `crates/bisa-core/src/error.rs`, `crates/bisa-core/src/error_text.rs` — `CoreError`, and how each refusal is said to a person.
- `crates/bisa-core/tests/it/` — the guards this crate holds for the whole workspace: layering, docs, shapes.

## Read first

- [02 — The domain model](../../architecture/02-domain-model.md) — the vocabulary and the numbered invariants, each with the one place it is enforced.
- [crates/core](../../architecture/crates/core.md) — every module, its invariants and tests, and the extension points table.
- [03 — Workflows](../../architecture/03-workflows.md) — the graph, validation and the run the core implements.
- [07 — Layering § Rule 1](../../architecture/07-layering.md#rule-1--the-domain-lives-in-core) — why every rule lives here and nowhere else.
- [Terminology](../terminology.md) — the words types and variants are named in.

## Rules a change must keep

- Every domain invariant is enforced in exactly one place, and that place is this crate — never a route handler, a SQL constraint or a component ([the five rules](../../architecture/README.md#the-five-rules-everything-else-follows-from), rule 3).
- No I/O and no runtime: the manifest forbids `tokio`, `rusqlite`, `axum`, `reqwest` and `hyper`; time arrives as a parameter. No `unwrap`, `expect` or `panic!` in production code (`crates/bisa-core/tests/it/layering.rs::core_performs_no_io`, `::core_never_panics_in_production_code`).
- A run changes only through `WorkflowRun::apply`, total and byte-identical on `Err` (I1); validation is pure and reports every problem (I2); a goal's status is a projection, never stored (I3) ([02 § The workflow and its run](../../architecture/02-domain-model.md#the-workflow-and-its-run)).
- Parse, don't validate: `Slug`, `StepId`, `RelPath`, `AgentId` are legal by construction; a slug is an allowlist (I10), a branch name is sanitised and never refused (I11).
- Conditions, filters and templates are closed, typed sets with no expression language (I35).
- Every type read from JSON refuses a key nobody knows, or is excused by name with its reason (`crates/bisa-core/tests/it/shapes.rs`).
- A setting is held only at a scope its definition allows (I44).
- A desktop mirror of a core enum reads the Rust source in its test (`desktop/src/views/_workflow/stepKinds.test.mjs`), so a new variant fails there until the mirror follows.
- Keep the crate small and stable: an edit here rechecks every crate downstream ([Faster builds](../testing-rules.md#faster-builds)).

## Testing a change

- A rule gets a unit test beside it ([Where a test lives](../testing-rules.md#where-a-test-lives)): `scripts/test lib core run::` runs one module's, `scripts/test lib core` all of them.
- `scripts/test module core layering`, `scripts/test module core shapes`, `scripts/test module core docs` — the workspace-wide guards.
- `scripts/test crate core` — the whole crate, once its modules are green.
- A step kind or the run machine: the journey `crates/bisa-cli/tests/it/e2e/every_step_kind.rs` (`scripts/test module cli e2e every_step_kind`).
- One module at a time; the whole workspace (`just verify`) is the gate at the end of a pass, never the loop ([Running](../testing-rules.md#running)).

## Common changes

- [Add a step kind](../recipes.md#16-add-a-step-kind)
- [Add a start event or a topic](../recipes.md#3-add-a-start-event-or-a-topic)
- [Add a setting](../recipes.md#2-add-a-setting)
- [Add a GEP kind](../recipes.md#9-add-a-gep-kind)
- [Say something to a person](../recipes.md#26-say-something-to-a-person) — a new refusal's words are a catalog message.
- A condition, a run effect, a placeholder root, a context chip, an id type: [crates/core § Extension points](../../architecture/crates/core.md#extension-points).

## Compatibility

- The core's types are what most of the [public contract](../../reference/compatibility.md) is made of: a stored record's fields ([the workspace on disk](../../reference/compatibility.md#the-workspace-on-disk)), a definition's keys and step kinds ([workflow, connector and catalog files](../../reference/compatibility.md#workflow-connector-and-catalog-files)), the registry ([settings keys](../../reference/compatibility.md#settings-keys)), the kind numbers ([the collaboration wire](../../reference/compatibility.md#the-collaboration-wire)). The crate's Rust API is an internal ([not promised in 0.x](../../reference/compatibility.md#not-promised-in-0x)).
- Inside 0.x a minor or patch release never breaks the public contract, so a change here is additive: a new optional field with a default when absent, a new variant, key or kind number.
- A rename, a retype, a removal or a reused number cannot be made by addition; it waits for 1.0.0, which brings a migration tool ([the first major release](../../reference/compatibility.md#the-first-major-release)).
- Declare the compatibility of your change in the pull request.

## Review focus

- Is the rule in the core and only there, with no copy in the store, the engine, a route or a model ([code review](../review/code.md))?
- Does the crate stay I/O-free and panic-free, and is every new rule a pure function with a unit test?
- Does every new type read from JSON refuse unknown keys, and does every new field have a default ([compatibility review](../review/compatibility.md))?
- Are the exhaustive matches downstream — the engine's interpreter, the desktop mirrors — updated in the same change?
- Are new types and variants named in the vocabulary ([docs and language review](../review/docs-and-language.md))?
