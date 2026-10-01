# bisa-mcp-probe

An installed MCP server, dialed to see what answers.
`bisa-mcp` is the *server* every session is handed; this crate is the *client* that checks a server a
person registered — over its own transport, in whichever protocol era it speaks — and reports who
answered, the negotiated revision, its capabilities and tools, or how far the conversation got. It
never calls a tool and never keeps a connection.

---

## Where things live

| Module | Owns |
|---|---|
| `lib.rs` | `McpProbe` (the port), `McpProbeReport { ok, transport, era, protocol_version, server, capabilities, instructions, tools, tool_count, resource_count, prompt_count, elapsed_ms, stage, error }`, `McpProbeStage::{Spawn, Initialize, Ping, Tools, Done}`, `McpEra::{Handshake, Discover}`, the bounds (`DEFAULT_BUDGET` 10 s, `MAX_BUDGET` 30 s, `MAX_TOOLS`, `MAX_INSTRUCTIONS_CHARS`, `MAX_STDERR_BYTES`), `budget_of` |
| `rmcp_probe.rs` | `RmcpProbe` — `rmcp`'s client over stdio (`TokioChildProcess`, the child's stderr kept as a bounded tail for a spawn failure) and Streamable HTTP (`StreamableHttpClientTransport` with the entry's headers), `ClientLifecycleMode::Auto` (discover first, `initialize` as the fallback), then `ping` in the handshake era, `tools/list` and the counts of what is advertised, then goodbye |
| `sse.rs` | the 2024-11-05 HTTP+SSE transport, ours: GET the stream, the `endpoint` event, POST each message, the answers read off the stream — through `bisa_http::SseFrames`, the platform's one SSE reader (decoded once per complete frame, capped), `resolve_endpoint` |
| `sanitize.rs` | what an error may carry out: every header and environment value replaced, a URL's userinfo removed |
| `fake.rs` | `FakeProbe` (`--features fake`): canned reports, and every transport it was asked about |

---

## Entry points

`RmcpProbe.probe(&config, budget)`; `FakeProbe::answering(reports)`. The engine's `McpHealth`
(`crates/bisa-engine/src/mcp_health.rs`) owns the port and the rules around it; the CLI's `bisa mcp
probe` links this crate directly when no node runs.

---

## Invariants held here

| Invariant | Held by |
|---|---|
| The negotiated revision is the server's word, never ours: a 2024-11-05 server reads as 2024-11-05 | `tests/it/probe.rs` |
| A budget bounds the whole conversation, and a report names the stage it ran out in | `tests/it/probe.rs` |
| An error never carries a header or environment value, nor a URL's userinfo | `sanitize.rs` tests, `tests/it/probe.rs` |
| A probe never calls a tool | `tests/it/probe.rs` (the stub records every method) |
| The stream is read by the platform's one SSE reader, so an event split at any byte reassembles and coalesced ones separate | `bisa-http`'s `sse.rs` tests |

---

## Errors

None typed: a probe always answers a report — `ok: false` with a `stage` and a sanitised sentence
is the answer for a server that would not.

---

## Extension points

A transport: a variant of `bisa_core::McpServerConfig`, an arm in `rmcp_probe::run`.

---

## Tests

`tests/it/probe.rs` against the scripted server built beside them (`tests/scripted_mcp.rs`, the
package's `scripted-mcp` binary — the discover era, the handshake era at two revisions, a hang, an
exit, stderr noise) and two loopback stubs (Streamable HTTP with headers asserted received; the
two-endpoint HTTP+SSE stream); unit tests beside `sanitize.rs`, `sse.rs` and the report's words.

---

## What this crate refuses to do

- call a tool the server offers;
- keep a connection open after the report;
- carry a secret in an error;
- run without a budget.
