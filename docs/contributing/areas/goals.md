# Goals

For a person, a goal is the durable object they come back to: a stated want, the workflow that says how
it becomes real, the runs that carry it, and a signed journal of everything that happened. Its **mode**
— auto, guided or manual — says who designs its workflow and who adopts, starts and repairs it; in auto
and guided the Workflow Agent designs. In code, `Goal` is the core's, its status a projection; its truth
is the store's under `goals/<id>/`; its life — capture, design, adoption, runs, repair, closing,
retiring — is the engine's.

## Where it lives

- `crates/bisa-core/src/goal.rs` — `Goal`, `GoalMode`, `Goal::status`, the holder ladder, budgets.
- `crates/bisa-store/src/runs.rs` — a goal's runs: created, queued behind the live one, started from the queue, closed.
- `crates/bisa-engine/src/ops.rs` — a goal's verbs: propose, adopt, start, stop, restart, decide.
- `crates/bisa-engine/src/guided.rs` — the Workflow Agent's design phases and the directives it wakes with.
- `crates/bisa-engine/src/intake.rs` — the ops a session's tools reach, `ask_human` among them.
- `crates/bisa-engine/src/gates.rs`, `crates/bisa-engine/src/recovery.rs`, `crates/bisa-engine/src/retire.rs` — a gate decided once, a restart's walk, retiring.
- `crates/bisa-node/src/goals.rs`, `crates/bisa-node/src/runs.rs`, `crates/bisa-node/src/inbox.rs` — the routes, and the asks a person answers in the Inbox.
- `library/core/workflow-agent.toml` — the Workflow Agent's definition and prompt.
- `desktop/src/views/_goal/`, `desktop/src/views/_goals/` — the goal page and the Goals screen.

## Read first

- [Goals guide](../../guide/goals.md) — status, modes, runs, questions, budgets, as a person meets them.
- [03 — Workflows § The goal, and its status](../../architecture/03-workflows.md#the-goal-and-its-status) — the projection and who holds the ball.
- [10 — Runtime flows § A goal, from capture to done](../../architecture/10-runtime-flows.md#a-goal-from-capture-to-done) — the path through the layers, file by file.
- [crates/engine](../../architecture/crates/engine.md) — the ops, the guided wakes, the design ask, where a change goes.
- [06 — Agents and teams § The core agents](../../architecture/06-agents-and-teams.md#the-core-agents) — the Workflow Agent's place.

## Rules a change must keep

- A goal stores no status: it is a projection of whether it is closed, whether it listens and its current run (I3).
- A goal has at most one live run; every other is queued in order and starts only when nothing is live; every writer of a run takes the one `run_writes` lock (I3a).
- An agent proposes and a person adopts: no tool adopts a workflow, decides a gate or starts a run, and the engine records no gate decision from an agent's op ([crates/engine § What this crate refuses to do](../../architecture/crates/engine.md#what-this-crate-refuses-to-do)).
- Auto skips only the platform's own gates — the adoption, the amendment, the start — never a person's; it stops adopting alone past `goals.auto.repair_limit` or for a design that cannot start unattended, and says why (I51). A manual goal wakes nobody, and a proposal on it is the person's draft.
- The design question is answerable as asked, journaled under its stable subject and rebuilt after a restart; a proposal or a start withdraws it (I48).
- Free text and *not sure* are valid answers to every question; an unknown option is refused (I26, I27).
- A restart loses no acknowledged fact and launches no second agent for a step whose session died (I53).
- Retiring refuses first, stops the work next, and never widens on its own (I50); a goal never enters a path, a URL or an id ([04](../../architecture/04-workspace-project-goal.md#what-this-design-refuses-to-do)).
- The words: a goal has a **status** and a **mode**; a run is **stopped**, **restarted**, **withdrawn**; a goal's conversation is its **thread** ([Terminology](../terminology.md)).

## Testing a change

- `scripts/test module engine guided` — the three modes, the design, adoption, repair; `scripts/test module engine workflow_agent` — proposals, amendments, where the Workflow Agent may act.
- `scripts/test module engine engine`, `scripts/test module engine recovery`, `scripts/test module engine retire` — gates and answers, a restart's walk, retiring.
- `scripts/test module node node`, `scripts/test module node runs` — the lifecycle over the socket.
- `scripts/test desktop views/_goal`, and one scenario with `cd desktop && node --test --import ./src/i18n/preload.mjs src/scenarios/goals.test.mjs`.
- Journeys: `crates/bisa-cli/tests/it/e2e/goals.rs` (each mode, a queued run, stop, restart, close), `crates/bisa-cli/tests/it/e2e/from_capture_to_done.rs`, `crates/bisa-cli/tests/it/e2e/crash_in_a_step.rs` — `scripts/test module cli e2e goals` runs one.
- A restart in a test is a second start over the same directory with its keys, the crash staged as the files a dead process leaves ([A restart in a test](../testing-rules.md#a-restart-in-a-test)).
- One module at a time; the whole workspace (`just verify`) only at the end of a pass ([Running](../testing-rules.md#running)).

## Common changes

- [Add an HTTP route](../recipes.md#1-add-an-http-route) — a new verb on a goal calls an engine function.
- [Add a CLI verb](../recipes.md#12-add-a-cli-verb)
- [Add an engine event](../recipes.md#4-add-an-engine-event) — a new fact of a goal's life.
- [Add a setting](../recipes.md#2-add-a-setting) — a `goals.*` or `budget.*` key.
- [Add an MCP tool](../recipes.md#5-add-an-mcp-tool) — a tool the Workflow Agent designs with.
- [Say something to a person](../recipes.md#26-say-something-to-a-person)

## Compatibility

- A goal reaches the [public contract](../../reference/compatibility.md) through its snapshot and journal ([the workspace on disk](../../reference/compatibility.md#the-workspace-on-disk)), the goal and run routes and their events ([the HTTP API and its events](../../reference/compatibility.md#the-http-api-and-its-events)), the goal verbs and their `--json` ([the command line](../../reference/compatibility.md#the-command-line)), the tools a design session holds ([MCP tools](../../reference/compatibility.md#mcp-tools)) and the `goals.*` keys ([settings keys](../../reference/compatibility.md#settings-keys)).
- Inside 0.x a minor or patch release never breaks the public contract: a change adds a field with a default, a route, a verb, a flag or an event.
- A status word, a mode or a cancel cause is a value on the wire: renaming or removing one cannot be made by addition and waits for 1.0.0, which brings a migration tool ([the first major release](../../reference/compatibility.md#the-first-major-release)).
- Declare the compatibility of your change in the pull request.

## Review focus

- Does a person's gate stay a person's — no path where auto or an agent decides it ([security review](../review/security.md))?
- Is a run changed only through `WorkflowRun::apply` and the store's one writer, under the run lock ([code review](../review/code.md))?
- Is there a recovery test for anything a restart must pick up, and does nothing launch twice?
- Are new goal fields defaulted ([compatibility review](../review/compatibility.md)) and new words in the vocabulary ([docs and language review](../review/docs-and-language.md))?
