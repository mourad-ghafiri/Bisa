# Agents and teams

For a person, an agent is a definition they pick from the catalog or write — a prompt bound to a
harness, a model plan with an effort, skills and MCP servers — and a team is a named group of agents and
people that work is assigned to. Three core agents ship: the General Agent and the Workflow Agent, which
hold records, and the Decision-Making Agent, which answers typed questions at decision points and has no
prompt or keypair of its own. In code, the types and rules are the core's, the records and the catalog
the store's, and assignment, model failover, effort and sessions the engine's.

## Where it lives

- `crates/bisa-core/src/agent.rs`, `crates/bisa-core/src/team.rs`, `crates/bisa-core/src/skill.rs`, `crates/bisa-core/src/mcp.rs` — `Agent`, `Team`, skills, MCP server records.
- `crates/bisa-core/src/model_plan.rs`, `crates/bisa-core/src/effort.rs` — a plan's order and the effort vocabulary.
- `crates/bisa-store/src/agents.rs`, `crates/bisa-store/src/teams.rs`, `crates/bisa-store/src/skills.rs`, `crates/bisa-store/src/mcp.rs` — the records.
- `crates/bisa-store/src/core_agents.rs`, `crates/bisa-store/src/decision_making_agent.rs` — the core agents.
- `crates/bisa-store/src/catalog.rs`, `crates/bisa-store/src/governance.rs` — the catalog installer; the assignment union.
- `crates/bisa-engine/src/assign.rs`, `crates/bisa-engine/src/staff.rs`, `crates/bisa-engine/src/models.rs`, `crates/bisa-engine/src/effort.rs`, `crates/bisa-engine/src/sessions.rs`, `crates/bisa-engine/src/mcp_health.rs` — who takes the work, failover, effort, sessions, a server's health.
- `crates/bisa-node/src/agents_api.rs`, `crates/bisa-node/src/skills.rs`, `crates/bisa-node/src/mcp.rs`, `crates/bisa-node/src/catalog.rs` — the routes.
- `library/core/` — the three core agents' definitions; `library/catalog/agents/`, `library/catalog/teams/`, `library/catalog/skills/` — the catalog.
- `desktop/src/views/Agents.tsx`, `desktop/src/views/Teams.tsx`, `desktop/src/views/rosterModel.mjs`, `desktop/src/views/_work/modelPlanModel.mjs`, `desktop/src/views/_work/effortModel.mjs` — the roster and the agent editor.

## Read first

- [Agents and teams guide](../../guide/agents-and-teams.md) — the core agents, definitions, plans and effort, skills, MCP servers, assignment.
- [06 — Agents and teams](../../architecture/06-agents-and-teams.md) — the model, addressing, assignment, context, failover, effort, and what holds each promise.
- [15 — The Decision-Making Agent](../../architecture/15-decision-making-agent.md) — the third core agent, held to one contract.
- [Catalog](../../reference/catalog.md) — every agent, skill and team that ships (generated).

## Rules a change must keep

- Neither the General Agent nor the Workflow Agent can be deleted or disabled, and their names are fixed; a core agent changes only its harness, its plan and its decision-making switch; the Decision-Making Agent's id is reserved (I17).
- The core agents take part in every team and channel, are members of none and take no unassigned work (I19); a disabled agent leaves the addressing directory (I20); an unknown mention is refused (I21).
- A person wakes the General Agent or the Workflow Agent; either may wake one other; nobody wakes itself; nothing wakes the Decision-Making Agent (I22).
- Assignment resolves in one union, nearest first, an item's own assignees winning alone, every walk over a parent link carrying a visited set (I23–I25); the engine picks the runner, and no body names it (I29).
- `origin` is recorded at creation and never taken from a caller (I28); an agent signs as itself (I30).
- Nothing is removed while something names it, and the refusal names the holder (I15).
- Effort is one vocabulary — `minimal`, `low`, `medium`, `high`, `xhigh`, `max`, and `auto` where one is asked for — decided by the step, then the model, then the plan, then `agents.effort`, and fitted to what the model takes ([06 § Effort](../../architecture/06-agents-and-teams.md#effort)).
- Every agent the platform ships runs `claude-opus-5-5[1m]` then `claude-sonnet-5-5[1m]` and states no effort; a catalog agent has a fallback model and at most six skills, and every `snake_case` word in its prompt is a real tool.
- An MCP server's `env` and `headers` values are write-once and answered masked.

## Testing a change

- `scripts/test lib core agent::`, `scripts/test lib core model_plan::`, `scripts/test lib core effort::` — the domain's rules.
- `scripts/test module store catalog`, `scripts/test module store usage`, `scripts/test module store studio` — the catalog's rules, holders, addressing.
- `scripts/test module engine assign`, `scripts/test module engine core_agent`, `scripts/test module engine models`, `scripts/test module engine mcp_health`, `scripts/test module node effort`.
- `scripts/test desktop views/_work` (the plan and effort models), `desktop/src/views/rosterModel.test.mjs`, `desktop/src/scenarios/agentEditor.test.mjs`.
- Journeys: `crates/bisa-cli/tests/it/e2e/the_roster_and_its_library.rs`, `crates/bisa-cli/tests/it/e2e/models_and_effort.rs`, `crates/bisa-cli/tests/it/e2e/the_decision_making_agent.rs`.
- One module at a time; the whole workspace (`just verify`) only at the end of a pass ([Running](../testing-rules.md#running)).

## Common changes

- [Add a setting](../recipes.md#2-add-a-setting) — an `agents.*` key.
- [Add an HTTP route](../recipes.md#1-add-an-http-route), [Add a CLI verb](../recipes.md#12-add-a-cli-verb) — the roster's routes and verbs.
- [Add a harness adapter](../recipes.md#8-add-a-harness-adapter) — a harness's effort levels and recommended plans live in its adapter.
- A catalog agent, skill or team is a file under `library/catalog/`; then [Regenerate and verify](../recipes.md#22-regenerate-and-verify) for the catalog page.

## Compatibility

- Agent, team, skill and MCP server records are [the workspace on disk](../../reference/compatibility.md#the-workspace-on-disk); their routes are [the HTTP API and its events](../../reference/compatibility.md#the-http-api-and-its-events); the roster's verbs are [the command line](../../reference/compatibility.md#the-command-line); the `agents.*` keys are [settings keys](../../reference/compatibility.md#settings-keys). The catalog's content may change in any release, and an upgrade never changes an installed copy ([workflow, connector and catalog files](../../reference/compatibility.md#workflow-connector-and-catalog-files)).
- Inside 0.x a minor or patch release never breaks the public contract: add a field with a default, a route, a verb, a key, an effort level.
- An effort word and a core agent's id are values on disk and on the wire: renaming one cannot be made by addition and waits for 1.0.0, which brings a migration tool ([the first major release](../../reference/compatibility.md#the-first-major-release)).
- Declare the compatibility of your change in the pull request.

## Review focus

- Can a caller name the runner, set an origin, or take a core agent's or the Decision-Making Agent's id ([security review](../review/security.md))?
- Is the rule in the core and the assignment in the one union, not in a route or a picker ([code review](../review/code.md))?
- Do the desktop's mirrors — the effort words, the roster's rules — read the Rust, and does the catalog page regenerate ([docs and language review](../review/docs-and-language.md))?
- Is a record change additive, with defaults ([compatibility review](../review/compatibility.md))?
