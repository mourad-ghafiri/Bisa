# MCP

For a person, MCP is how the agents Bisa launches reach the platform — read a goal or a run, ask a
person, post a message, propose a workflow — and how their own MCP servers are attached to an agent and
checked. In code, `crates/bisa-mcp` is the MCP server every harness session is handed (`bisa mcp`): it
forwards each tool call over the engine's intake socket, and the engine decides. `crates/bisa-mcp-probe`
is the client that dials a server a person registered and reports what answered, in either protocol era.

## Where it lives

- `crates/bisa-mcp/src/server.rs` — the tool manifest, the routers by scope, the instructions a session reads first.
- `crates/bisa-mcp/src/client.rs` — the JSONL client for the engine's intake socket.
- `crates/bisa-mcp/src/lib.rs` — `run_stdio` and `Scope`: a work item, a goal, a conversation.
- `crates/bisa-engine/src/intake.rs` — the `Op` behind every tool, and who may call it.
- `crates/bisa-mcp-probe/src/rmcp_probe.rs`, `crates/bisa-mcp-probe/src/sse.rs`, `crates/bisa-mcp-probe/src/sanitize.rs` — the probe over stdio, Streamable HTTP and HTTP+SSE, and what an error may carry.
- `crates/bisa-engine/src/mcp_health.rs` — the probe's port and its rules; `crates/bisa-core/src/mcp.rs` — a server's record and its masked secrets.
- `crates/bisa-adapters/src/mcp_inject.rs` — how each harness is handed its servers.
- `docs/reference/mcp-tools.md` — every tool, by scope.

## Read first

- [crates/mcp](../../architecture/crates/mcp.md) — the tools by tier, and what the crate refuses to do.
- [MCP tools](../../reference/mcp-tools.md) — what each scope's session can call.
- [crates/mcp-probe](../../architecture/crates/mcp-probe.md) — the probe, its eras and its bounds.
- [06 — Agents and teams](../../architecture/06-agents-and-teams.md) — sessions, the core agents' tool sets, MCP servers on an agent.
- [11 — Security § The placeholder contract](../../architecture/11-security.md#the-placeholder-contract) — what both halves of a tool call pass through.

## Rules a change must keep

- The engine decides; this crate forwards. A core-only tool is re-checked per op by the engine, because the socket is a path on disk.
- A refusal is an MCP error, never a result that looks like an answer ([07 — Layering § Errors](../../architecture/07-layering.md#errors)).
- No tool removes or edits a definition, adopts a workflow, decides a gate or starts a run: an agent proposes, a person adopts.
- Both halves of every tool call are redacted by the engine where they enter, and a placeholder an agent hands back is never restored (I49).
- The reference equals the manifest both ways; every registered name is a `snake_case` tool or an underscore hook; a catalog prompt names only real tools.
- This crate never names `HumanConsent` or the consented git tier.
- A probe never calls a tool, never keeps a connection, never runs without a budget, and never carries a header, an environment value or a URL's userinfo in an error.
- An installed server's secrets are write-once: answered masked over the wire and in `--json`, and a mask sent back keeps the stored value.

## Testing a change

- `scripts/test module mcp intake` — the tools driven through the socket against a real engine.
- `scripts/test module mcp docs` — the reference against the manifest, the hooks beside it.
- `scripts/test module engine intake_scope`, `scripts/test module engine core_agent` — every op answers each scope in words; core-only ops refuse every other session.
- `scripts/test crate mcp-probe` — a scripted server and two loopback stubs, never a real server or the network; `scripts/test module engine mcp_health`.
- `scripts/test module store catalog` — a prompt names only real tools.
- Journeys: `crates/bisa-cli/tests/it/e2e/the_platforms_tools.rs` (the menu each scope is promised, every tool called with nothing in its hands), `crates/bisa-cli/tests/it/e2e/the_roster_and_its_library.rs` (a server registered and dialled).
- One module at a time; the whole workspace (`just verify`) only at the end of a pass ([Running](../testing-rules.md#running)).

## Common changes

- [Add an MCP tool](../recipes.md#5-add-an-mcp-tool)
- A tool the desktop performs parks its request in the engine and is answered over a node route — [Add an HTTP route](../recipes.md#1-add-an-http-route).
- A transport: a variant of `McpServerConfig` and its arm in the probe ([crates/mcp-probe § Extension points](../../architecture/crates/mcp-probe.md#extension-points)).
- [Say something to a person](../recipes.md#26-say-something-to-a-person) — a refusal's sentence.

## Compatibility

- [MCP tools](../../reference/compatibility.md#mcp-tools) are public: every tool name and the inputs it accepts, in each scope; a tool's description is not promised. A server's registry entry is a record of [the workspace on disk](../../reference/compatibility.md#the-workspace-on-disk), and its routes are [the HTTP API and its events](../../reference/compatibility.md#the-http-api-and-its-events).
- Inside 0.x a minor or patch release never breaks the public contract: add tools and optional inputs.
- Renaming a tool, removing an input, making one required or moving a tool out of a scope cannot be made by addition; it waits for 1.0.0, which brings a migration tool ([the first major release](../../reference/compatibility.md#the-first-major-release)).
- Declare the compatibility of your change in the pull request.

## Review focus

- Is the decision in the engine, and is every refusal an error ([code review](../review/code.md))?
- Can the tool reach beyond its scope — a goal from a run in the workspace, another agent's tools, a consented git verb — or carry a secret out ([security review](../review/security.md))?
- Is every answer bounded — a probe under its budget, a list capped ([performance review](../review/performance.md))?
- Do the reference row and the catalog's prompts change with the tool ([docs and language review](../review/docs-and-language.md))?
