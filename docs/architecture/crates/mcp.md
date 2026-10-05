# bisa-mcp

The MCP server a harness session is handed, and the intake client behind it. A session speaks MCP
over stdio to `bisa mcp`; every tool call becomes one JSONL request to the engine's intake
socket, and the engine — not this crate — decides whether the caller may do what it asks. A refusal
comes back as an MCP **error**, never as a successful-looking result: a model that cannot tell a
refusal from an answer is the failure *never parse prose* exists to remove.

---

## Where things live

| Module | Owns |
|---|---|
| `lib.rs` | `run_stdio(socket_path, scope)` and `Scope { WorkItem, Goal { goal, agent }, Conversation { scope, agent } }` — the three kinds of session (a conversation about a note or a drawing is a conversation); the re-exports `CORE_AGENT_IDS`, `WORKFLOW_AGENT_ID`, `is_core_agent`, `Proposed` |
| `error_text.rs` | `Localize for IntakeError` (`error-mcp-…`) |
| `client.rs` | the async JSONL client for the engine's intake socket: one JSON object per line, LF-terminated, one reply line per request, in order; `Scope::{WorkItem, Goal { goal, agent }, Conversation { scope, agent, goal }}` — a conversation's `goal` is the one the engine launched the turn knowing, sent on every request (the scope id rides as the candidate without one); the ops `get_run` (a work item's run, of either kind), `propose_workflow`, `amend_workflow`, `list_staff(agent, goal?)`, `list_workflow_templates`, `get_workflow`, `validate_workflow(agent, workflow, goal?)` (`goal` on the wire only when given — the roster that goal's design staffs from; without it, the workspace's), `save_workflow(scope, revision, definition)` (the conversation's workflow, named by the engine from the scope; `Saved { workflow, revision }`), `emit_signal(scope, name, payload, signal_scope)` (a named signal raised through the engine's one emit door; `Raised { signal, listeners }` — the record, and the signals written for the listeners that heard it), `capture_goal(agent, StandingGoal { statement, title })` (work that recurs or waits for something, captured as a standing goal; the goal's id), `decide` (`{state, questions}` to the Decision-Making Agent, answers `{sure, response}`) among the rest; `Proposed { workflow, revision, gate }` |
| `server.rs` | a reply the engine could not read (`bisa_core::browser::PLATFORM_FAULT`) is an internal error, never *Not done*, so an agent reports it and carries on; every other refusal — the engine's or a result's — wears *Not done*; the tool manifest — every `#[tool(name = …)]` — and the instructions a session reads first, one paragraph per kind of agent (a worker orients with `get_run`, and with `get_goal` when the run is a goal's); a worker's run read once and kept (`run_home`), so a message with no `scope` goes to the goal's thread on a goal's run and to `general` in a run of the workspace, which has no goal (`default_thread`); `PostMessageParams` with `attachments` and `artifacts: Vec<ArtifactParam { path, title? }>` ([12 — Artifacts](../12-artifacts.md)); `DecideParams { state, questions }` for the `decide` tool ([15 — The Decision-Making Agent](../15-decision-making-agent.md)); the routers `worker_router` (a work item's: `get_run`, `yield_result`, `report_progress`), `goal_router`, `core_shared_router` (`CORE_SHARED`: `workspace_overview`, `list_staff`, `list_catalog`), `platform_router`, `workflow_agent_router`; `CORE_AGENT_IDS` (= `AgentId::CORE`, the two core agents that hold a record — the Decision-Making Agent runs no session and is granted no tools), `CORE_AGENT_ID`, `WORKFLOW_AGENT_ID`, `is_core_agent`; `session_goal` (the goal a session designs for, if any — `Scope::Goal`, or a conversation's goal — which `list_staff` and `validate_workflow` pass on, and `shaping_goal` requires); the renderers `render_workflow_list`, `render_workflow`, `render_problems` (problems grouped by step) |

### The tools, by tier

| Tier | Tools |
|---|---|
| every session | `get_goal` (refused in a run of the workspace, which has no goal — *orient with get_run*), `ask_human`, `await_human`, `ask_human_and_wait`, `add_note`, `spawn_sub_goal`, `emit_signal` (`{name, payload?, scope?}` — a named signal, dotted lowercase words like `report.ready`, that a workflow's `signal` start begins on and a `wait` or a boundary holds for; the reply says how many listeners heard it), `post_message`, `recall_store`, `recall_get`, `recall_list`, `note_read` (the body under its `hash`), `note_append`, `note_write` (the whole body at the `base_hash` the read answered — refused with the current hash when the note moved since, a tool error the model reads as *not done*; never an empty body, never over a body holding a secret the platform redacted), `review_notes_list`, `review_note_resolve`, `list_connectors` and `call_connector` (the connectors installed here, and a read through one of their read operations — a write is refused; the roster is every session's to read, a worker's too: who asks is named as recall names it, the scope's agent or the work item the engine resolves one from, and never required), `mobile_development_status`, `mobile_development_devices`, `mobile_development_boot` and `mobile_development_screenshot` (ide/19 — on every menu, answered only where mobile development is on), `decide` (typed questions to the Decision-Making Agent — a noul, a choice or a score — read calibrated: `{sure, response}`, refused in a sentence unless the workspace switch or the session's own agent's `decision_making` switch is on; [15 — The Decision-Making Agent](../15-decision-making-agent.md)), `create_project` (attaches to the session's goal, when it has one, and records where the project was born — the goal, or the very step, with no goal in a run of the workspace; the origin is the engine's to derive, never a parameter), and the embedded browser (ide/18) — twenty-one tools, `bisa_core::browser::BROWSER_TOOLS`: `browser_open`, `browser_tabs`, `browser_snapshot`, `browser_read`, `browser_find`, `browser_click`, `browser_type`, `browser_fill`, `browser_press`, `browser_select`, `browser_hover`, `browser_scroll`, `browser_wait`, `browser_back`, `browser_forward`, `browser_reload`, `browser_console`, `browser_eval`, `browser_screenshot`, `browser_close` — each one `browser` op the engine checks against the workspace's `browser.*` policy (the tools are a menu; the engine is the boundary), parks for the desktop and answers with what the page said (`browser_words`: the tab and a move, the text, a count, a wait, a scroll, a value, the console, the dialogs the page raised), a screenshot's path, or that no desktop is there  — and `browser_serve`, the one `browser_serve` op: the folder of the session's checkout served by the node's server (`ServedPage`), refused in a sentence when the session stands in no checkout or the engine has no server to lend, `drawing_list`, `drawing_read`, `drawing_create`, `drawing_draw`, `drawing_mermaid`, `drawing_erase`, `drawing_snapshot` (19 — Drawings: the first four answered by the engine, the last three performed by the desktop's canvas); beside them the router registers `_Stop`, which is no tool an agent calls: a lifecycle hook a harness that follows the underscore convention invokes when the agent is about to stop and keeps off the model's list — it takes nothing, reaches no intake op and answers an empty result, *no objection* |
| work-item sessions | `get_run` (the run the item belongs to — its workflow, inputs, every step's state, its own work items, the journal, its budget and spend, and `goal`, null for a run of the workspace), `yield_result`, `report_progress` |
| a conversation's turn in a checkout | `pr_reviews_list`, `pr_review_submit`, `pr_thread_reply`, `pr_thread_resolve` (the PR of the checkout the conversation runs in — a workstream's, or a project's primary; off a checkout they refuse; the session's `agent` rides on the submit and on a reply, and the engine signs the first line with it) |
| goal sessions (designing), and the Workflow Agent's turn in a goal's thread — `Scope::Conversation { goal: Some(_) }` | `revise_statement`, `propose_workflow`, `amend_workflow` |
| both core agents | `workspace_overview`, `list_staff` (in a goal's design session — `Scope::Goal`, or a conversation in a goal's thread — the goal's own roster: the agents and teams it names, else the whole enabled staff), `list_catalog` |
| the General Agent only | `install_catalog_entry`, `assign`, `capture_goal` (`{statement, title?}` — recurring or awaited work as a standing goal in the workspace's default mode, whose workflow the Workflow Agent designs with the start event the statement names) |
| the Workflow Agent only | `list_workflow_templates`, `get_workflow`, `validate_workflow` (staffing checked against the goal's roster in a goal's session, the workspace's elsewhere), `save_workflow` (the one write to a library workflow — the engine allows it only from the conversation about that workflow, at the revision read) |

There is no removal tool, no self-edit tool, and no tool that adopts a workflow, decides a gate or
starts a run — an agent proposes, a person adopts. The reference is
[`docs/reference/mcp-tools.md`](../../reference/mcp-tools.md); `tests/it/docs.rs` keeps its tool tables equal to
the manifest's tools, both directions — and holds the hooks beside them: every registered name is a
`snake_case` tool or an underscore hook, `_Stop` is in no tool table, and the reference names each hook the
router registers and no other (`the_reference_names_every_lifecycle_hook_and_no_other`).

---

## Entry points

`run_stdio(socket, scope)` from `bisa mcp`; `client::Client` for anything else that needs the
intake socket.

---

## Invariants held here

| Invariant | Held by |
|---|---|
| The reference lists every registered tool and nothing else | `tests/it/docs.rs` |
| A refusal is an MCP error | `tests/it/intake.rs` |
| Both halves of every tool call are redacted by the engine, not here: the request before any op reads it (a secret an agent writes into a message, a note, a result or a proposal is stored as a placeholder), the reply before it is written back — and a placeholder an agent hands back is never restored | `crates/bisa-engine/tests/it/security.rs` (`an_agents_message_and_note_through_the_socket_are_stored_as_placeholders`, `get_goal_answers_over_the_socket_with_the_statement_redacted`) |
| Each core agent gets its own tools and not the other's; the goal router carries the designing set; the Workflow Agent's goal-thread turn holds it and no other agent's does; a conversation launched with a goal sends it on the wire; the core ids are the two cores | `server.rs` unit tests (`each_core_agent_gets_its_own_tools_and_not_the_others`, `goal_router_has_the_designing_set`, `a_goal_thread_turn_of_the_workflow_agent_holds_the_designing_set`, `a_goal_thread_turn_of_another_agent_gets_no_designing_tool`, `a_conversation_with_a_goal_sends_that_goal_on_the_wire`, `core_agent_ids_are_the_cores`) |
| The core-only tools are refused to any other caller — by the engine, re-checked per op, because the socket is a path on disk; the General Agent cannot propose a workflow | `tests/it/intake.rs` (`the_general_agent_cannot_propose_a_workflow`), `crates/bisa-engine/tests/it/core_agent.rs` |
| A validation failure reaches the Workflow Agent as problems grouped by step, and writes nothing | `tests/it/intake.rs::workflow_agent_texts_render_problems_by_step` |
| A worker of a run of the workspace orients with `get_run`, is refused `get_goal`, and a message it sends with no scope lands in `general` | `tests/it/intake.rs::a_worker_of_a_workspace_run_orients_with_get_run_and_speaks_in_general` |
| Every post through the tool says who speaks — the scope's agent (`--agent`), else the work item the engine resolves one from — so it is signed as that agent and may publish from where it works; recall speaks as the same | `tests/it/intake.rs::a_post_through_the_tool_says_who_speaks`, the fake intake's own check on every post; the journey `crates/bisa-cli/tests/it/e2e/a_channel_and_what_is_said_in_it.rs` |
| This crate never names `HumanConsent` or the consented git tier | `crates/bisa-core/tests/it/layering.rs::no_agent_path_reaches_interactive` |

---

## Errors

A transport failure is an MCP error naming the socket; an engine refusal is an MCP error carrying
the engine's sentence — for a workflow, every problem it found.

---

## Extension points

| To add… | Touch, in order | The gate that catches a miss |
|---|---|---|
| a tool | the `Op` and its arm in `crates/bisa-engine/src/intake.rs` → the `#[tool]` in `server.rs` under its router → a row in [`mcp-tools.md`](../../reference/mcp-tools.md) → the catalog prompts may name it | `tests/it/docs.rs`; `crates/bisa-store/tests/it/catalog.rs` (a prompt may only name a real tool) |

---

## Tests

`tests/it/intake.rs` boots a real engine and drives the tools through the socket, with fake ops for the
Workflow Agent's set (`workflow_agent_scope()`, `good_workflow()`, `broken_workflow()`);
`tests/it/docs.rs` holds the tools reference equal to the tools `server.rs` registers, both ways.

---

## What this crate refuses to do

- decide anything: it forwards, the engine decides;
- offer a tool that removes or edits a definition, adopts a workflow or starts a run;
- flatten a refusal into a result.

`ask_human` and `ask_human_and_wait` ask an **answer** by default: a question the person types an answer to, with `options` when it is enumerable; `expects="decision"` is for a bare approve/decline with nothing to type.
