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
- `crates/bisa-store/src/usage.rs`, `crates/bisa-store/src/conversation.rs` (`resolve_mentions`), `crates/bisa-store/src/ingest.rs` — who holds a record, who a mention names, what another node's record may change here.
- `crates/bisa-engine/src/directory.rs`, `crates/bisa-engine/src/membership.rs`, `crates/bisa-engine/src/presence.rs`, `crates/bisa-engine/src/ops.rs` (`agent_signer`, `check_assignees`) — the edit door, what a flip announces, the live roster, who signs.
- `crates/bisa-node/src/agents_api.rs`, `crates/bisa-node/src/skills.rs`, `crates/bisa-node/src/mcp.rs`, `crates/bisa-node/src/catalog.rs` — the routes.
- `crates/bisa-cli/src/agents.rs`, `crates/bisa-cli/src/catalog.rs`, `crates/bisa-cli/src/usage.rs`, `crates/bisa-cli/src/skills.rs`, `crates/bisa-cli/src/mcp.rs` — the verbs.
- `crates/bisa-mcp/src/server.rs` — the tools the two core agents alone are granted (`list_staff`, `list_catalog`, `install_catalog_entry`, `assign`).
- `library/core/` — the three core agents' definitions; `library/catalog/agents/`, `library/catalog/teams/`, `library/catalog/skills/` — the catalog.
- `desktop/src/views/Agents.tsx`, `desktop/src/views/Teams.tsx`, `desktop/src/views/rosterModel.mjs`, `desktop/src/views/_work/AgentEditor.tsx`, `desktop/src/views/_work/agentDraftModel.mjs`, `desktop/src/views/_work/teamDraftModel.mjs`, `desktop/src/views/_work/heldAnswerModel.mjs`, `desktop/src/views/_work/AssigneePicker.tsx`, `desktop/src/views/_work/modelPlanModel.mjs`, `desktop/src/views/_work/effortModel.mjs` — the roster, the editor and the pickers.

## Read first

- [Agents and teams guide](../../guide/agents-and-teams.md) — the core agents, definitions, plans and effort, skills, MCP servers, assignment.
- [06 — Agents and teams](../../architecture/06-agents-and-teams.md) — the model, addressing, assignment, context, failover, effort, and what holds each promise.
- [15 — The Decision-Making Agent](../../architecture/15-decision-making-agent.md) — the third core agent, held to one contract.
- [Catalog](../../reference/catalog.md) — every agent, skill and team that ships (generated).

## Rules a change must keep

- Neither the General Agent nor the Workflow Agent can be deleted or disabled, and their names are fixed; a core agent changes only its harness, its plan and its decision-making switch; the Decision-Making Agent's id is reserved (I17).
- The core agents take part in every team and channel, are members of none and take no unassigned work (I19); a disabled agent leaves the addressing directory (I20); an unknown mention is refused (I21).
- A person wakes the General Agent or the Workflow Agent; the General Agent's post wakes each agent it names, once; the Workflow Agent hands off to nobody; nobody wakes itself; nothing wakes the Decision-Making Agent (I22).
- What an agent does — its session, its progress, its note, its result — is signed by the agent or by nobody (`ops::agent_signer`): a key or a record that cannot be read fails the step and refuses the tool call, in words. What the platform says about the work may still be the owner's (`ops::signer_or_owner`).
- An agent's edit names fields (`directory::AgentEdit`, `TeamEdit`) and is applied to the record as it stands; a flip is announced from the record the store answered before and after.
- Removing an agent ends the turns it runs in conversations first (`directory::remove_agent`), after the store agreed nothing points at it.
- Assignment resolves in one union, nearest first, an item's own assignees winning alone, every walk over a parent link carrying a visited set (I23–I25); the engine picks the runner, and no body names it (I29).
- `origin` is recorded at creation and never taken from a caller (I28); an agent signs as itself (I30).
- Nothing is removed while something names it, and the refusal names the holder (I15).
- Effort is one vocabulary — `minimal`, `low`, `medium`, `high`, `xhigh`, `max`, and `auto` where one is asked for — decided by the step, then the model, then the plan, then `agents.effort`, and fitted to what the model takes ([06 § Effort](../../architecture/06-agents-and-teams.md#effort)).
- Every agent the platform ships runs `claude-opus-5-5[1m]` then `claude-sonnet-5-5[1m]` and states no effort; a catalog agent has a fallback model and at most six skills, and every `snake_case` word in its prompt is a real tool.
- An MCP server's `env` and `headers` values are write-once and answered masked.
- The roster has one writer: a create, an edit, an enable, an attach, a detach and a removal of an agent or a team take `roster_writer`, and an edit is a closure over the record as it stands (`update_agent_with`, `update_team_with`) — never a read, a change and a write of the whole record from outside the lock.
- A removal says `StoreEvent::DefinitionRemoved` once, when the record went, and nothing when it was refused; it is asked again after a failure at any step, and a record that will not parse is removed by its id.
- A mention by key gets the check an id gets: the owner's, a member's or an enabled agent's, else refused.
- Whether an assignee's record is there is `Workspace::check_assignee`'s question alone; a record that cannot be read is never said *unknown*.
- A new agent needs a prompt, and no edit blanks one; `state` is no agent's id.

## Testing a change

- `scripts/test lib core agent::`, `scripts/test lib core model_plan::`, `scripts/test lib core effort::` — the domain's rules.
- `scripts/test module store catalog`, `scripts/test module store usage`, `scripts/test module store studio` — the catalog's rules, holders, addressing, removal; `scripts/test lib store agents::`, `scripts/test lib store teams::` — the one writer, the reserved id, a removal stopped midway.
- `scripts/test module engine assign` (who takes the work, who signs, the edit door), `scripts/test module engine conversation` (the wake rules, a removal ending a turn), `scripts/test module engine presence`, `scripts/test module engine core_agent`, `scripts/test module engine models`, `scripts/test module engine mcp_health`, `scripts/test module node effort`.
- `scripts/test desktop views/_work` (the plan, effort, draft and held-answer models), `desktop/src/views/rosterModel.test.mjs`, `desktop/src/scenarios/agentEditor.test.mjs`, `desktop/src/scenarios/agents.test.mjs`, `desktop/src/scenarios/teams.test.mjs`.
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

- Can a caller name the runner, set an origin, or take a core agent's or the Decision-Making Agent's id ([security review](../review/security.md))? What is known to be open is listed, not hidden: [06 § What is open](../../architecture/06-agents-and-teams.md#what-is-open).
- Does a write read the record under the roster's writer — an edit as a closure over the record as it stands — and never from a copy a surface read a while ago?
- Is the rule in the core and the assignment in the one union, not in a route or a picker ([code review](../review/code.md))?
- Do the desktop's mirrors — the effort words, the roster's rules — read the Rust, and does the catalog page regenerate ([docs and language review](../review/docs-and-language.md))?
- Is a record change additive, with defaults ([compatibility review](../review/compatibility.md))?
