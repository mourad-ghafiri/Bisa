# Workflows

For a person, a workflow is a small graph of steps — eighteen kinds in four families: events, gateways,
loops and tasks — drawn in the designer, picked from a template or proposed by the Workflow Agent. It
runs on a goal, one run at a time, or on its own as a run in the workspace, begun by hand or by an
event it listens for. In code, the definition, its validation and the run machine are the core's; the
store keeps definitions, runs, who listens and the durable signal queue; the engine interprets every
run effect, arms waits and boundary events, and dispatches signals.

## Where it lives

- `crates/bisa-core/src/workflow.rs` — `StepKind`, the definition's closed vocabulary, `Workflow::validate`, conditions.
- `crates/bisa-core/src/run.rs` — `WorkflowRun::apply`, entries, effects, the settle loop.
- `crates/bisa-core/src/start.rs`, `crates/bisa-core/src/listen.rs`, `crates/bisa-core/src/boundary.rs`, `crates/bisa-core/src/signal.rs`, `crates/bisa-core/src/template.rs` — start events, filters and the chain, boundary events, signals, placeholders.
- `crates/bisa-store/src/workflows.rs`, `crates/bisa-store/src/runs.rs`, `crates/bisa-store/src/listening.rs`, `crates/bisa-store/src/signals.rs` — the library and a goal's designs, runs, who listens, the queue.
- `crates/bisa-engine/src/effects.rs`, `crates/bisa-engine/src/waits.rs`, `crates/bisa-engine/src/listen/` — the interpreter, waits and boundaries, listening and dispatch.
- `crates/bisa-node/src/workflows.rs`, `crates/bisa-node/src/runs.rs`, `crates/bisa-node/src/listening.rs`, `crates/bisa-node/src/hooks.rs` — the routes and the public hook.
- `library/catalog/workflows/` — the catalog's templates.
- `desktop/src/views/_workflow/` — the designer, the library, the run view; `desktop/src/views/_workflow/stepKinds.mjs` mirrors the Rust enums.

## Read first

- [Workflows guide](../../guide/workflows.md) — the kinds, templates, the designer, the TOML shape, running.
- [Events and gateways](../../guide/events.md) — start events, listening, catches, boundary events, gateways, hooks.
- [03 — Workflows](../../architecture/03-workflows.md) — the graph, validation, the run, events, the designer's promises.
- [10 — Runtime flows § An event starting a run](../../architecture/10-runtime-flows.md#an-event-starting-a-run) — from an occurrence to a run.
- [Catalog](../../reference/catalog.md) — the templates that ship (generated).

## Rules a change must keep

- A run changes only through `WorkflowRun::apply` — locally through `record_run_event` alone, from a peer only through ingest's admission (I1, I4; [03 § One entry point](../../architecture/03-workflows.md#one-entry-point-for-local-writes-and-synced-snapshots)).
- The eighteen kinds are a closed set: a nineteenth touches every layer that names them, in the recipe's order.
- Validation is pure and exhaustive, and every `ProblemKind` has a test that earns it (I2).
- Conditions, filters and templates are closed typed sets matched exactly — no expression language (I35).
- No step reads the event: a start's mapping is its one reader (I5); a run in the workspace reads no goal (I3b).
- An event never acts: it is written down as a durable signal and dispatched under the same gates, budgets and caps as work a person started; one signal makes one run; a chain is bounded by `events.chain_depth` and `events.fires_per_minute` (I32–I34).
- A host listens only once a person turned it on, and what it listens with is apart from its definition (I59); what comes from outside is redacted and held for the content screen before it begins anything (I61).
- A boundary event sits only on a step whose work can be stopped, and fires only for the visit it was armed for (I60).
- A connector write is gated by an `approval` or `human` step upstream, or the step says it runs unattended.
- A workflow is held by the goals, `spawn` steps and `run` starts that name it, and is not deleted while held (I15).
- Every catalog template begins at a `start`, runs in the workspace, validates against the catalog's agents and has a journey that installs and runs it.

## Testing a change

- `scripts/test lib core run::`, `scripts/test lib core workflow::` — the run machine's property tests and validation's unit tests.
- `scripts/test module engine workflow`, `scripts/test module engine events`, `scripts/test module engine boundaries`, `scripts/test module engine gateways`, `scripts/test module engine waits`, `scripts/test module engine workspace_runs`.
- `scripts/test module store listening`, `scripts/test module store workspace_runs`, `scripts/test module store catalog`; `scripts/test module node events`, `scripts/test module node runs`.
- `scripts/test desktop views/_workflow` — the designer's models; `desktop/src/views/_workflow/stepKinds.test.mjs` reads the Rust enums.
- Journeys: `crates/bisa-cli/tests/it/e2e/every_step_kind.rs`, `crates/bisa-cli/tests/it/e2e/events_and_gateways.rs`, `crates/bisa-cli/tests/it/e2e/runs_in_the_workspace.rs`, `crates/bisa-cli/tests/it/e2e/templates_installed_and_run.rs` (`every_template_of_the_catalog_has_its_journey`).
- Engine tests drive time by hand — listening off, the ticker and the queue driven by the test, the clock an argument — never a naked sleep.
- One module at a time; the whole workspace (`just verify`) only at the end of a pass ([Running](../testing-rules.md#running)).

## Common changes

- [Add a step kind](../recipes.md#16-add-a-step-kind)
- [Add a start event or a topic](../recipes.md#3-add-a-start-event-or-a-topic)
- [Add an engine event](../recipes.md#4-add-an-engine-event)
- [Add a catalog workflow template](../recipes.md#15-add-a-catalog-workflow-template)
- [Add a connector](../recipes.md#21-add-a-connector)
- [Add a desktop model](../recipes.md#7-add-a-desktop-model) — a designer fact belongs in a tested model.

## Compatibility

- Definitions are public ([workflow, connector and catalog files](../../reference/compatibility.md#workflow-connector-and-catalog-files)): a definition a release accepts is accepted, and means the same, by every later release of the line. An earlier release refuses, by name, a definition that uses something added later — the stated limit.
- Inside 0.x a minor or patch release never breaks the public contract: add optional keys, step kinds, start events, conditions, topics and engine events ([the HTTP API and its events](../../reference/compatibility.md#the-http-api-and-its-events)); a topic's word is never renamed. The catalog's content may change in any release; an upgrade never changes an installed copy.
- A change that cannot be made by addition waits for 1.0.0, which brings a migration tool ([the first major release](../../reference/compatibility.md#the-first-major-release)).
- Declare the compatibility of your change in the pull request.

## Review focus

- Is a new rule in the core's validation with a `ProblemKind` and its test, not in the engine or the designer ([code review](../review/code.md))?
- Are the exhaustive matches updated together — the run machine, the interpreter, the desktop's step kinds — and the schema's version bumped when its step kind check changed?
- Does anything from outside act before it is a durable, deduplicated, screened signal ([security review](../review/security.md))?
- Are loops and chains bounded, and tests free of wall-clock waits ([performance review](../review/performance.md))?
- Does every existing definition still validate and mean the same ([compatibility review](../review/compatibility.md))?
