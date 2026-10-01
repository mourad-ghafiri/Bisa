# bisa-lsp

Language servers, supervised and proxied.
A server is a program the person already has; this crate finds it on the login shell's `PATH`,
starts it once per root and language, frames JSON-RPC with a hand-written `Content-Length` codec,
keeps payloads as `serde_json::Value`, and turns `file://` URIs into root-relative paths on the way
to the editor. The platform never installs a server.

---

## Where things live

| Module | Owns |
|---|---|
| `lib.rs` | the module list; `LspError { Spawn, Protocol, Closed, Timeout, Refused }` |
| `codec.rs` | the incremental `Content-Length` JSON-RPC framing, tested against split and coalesced reads — and bounded: `MAX_HEADER_BYTES` (64 KiB) before a stream is not LSP, `MAX_BODY_BYTES` (16 MiB) refused **before** a byte of the body is buffered, a length that is not one or an addition that would overflow its own error; `DecodeError { NoHeader, MalformedHeaderLine, MissingLength, MalformedLength, BodyTooLarge, NotJson }`, every variant fatal for the server it came from |
| `catalog.rs` | compiled-in server presets probed on the login shell's `PATH` (`available`, one probe a minute per command), plus the user's descriptors from the `lsp.servers` setting; `forget_probes` empties the kept probes — the engine's `lsp::refresh_for` calls it on an `lsp.*` change, so a server installed a moment ago is seen |
| `uri.rs` | `file://` on the server side, root-relative on the editor side, and nothing outside the root either way |
| `server.rs` | `Server` — one supervised stdio process: typed requests with a deadline, a notification channel, `Failed` with a reason; a request still waiting when the server fails or is shut down hears `Closed` at once, so no sender sits in the pending map for the server's life |

---

## Entry points

`catalog::find(language)`; `Server::start(root, spec)`; `server.request(method, params,
deadline)`; `server.notifications()`. The engine's `LspRegistry` (`crates/bisa-engine/src/lsp.rs`)
owns one `Server` per `(root, language)` and contains a crash loop at three starts a minute.

---

## Invariants held here

| Invariant | Held by |
|---|---|
| The codec reassembles a frame split at any byte and separates coalesced frames; a `Content-Length` that is not a number is a malformed length and never a missing header; one over the cap (and `u64::MAX`) is refused before any body is buffered; one at the cap waits for its body | `codec.rs` tests |
| A URI never escapes the root in either direction | `uri.rs` tests |
| A server's output is never parsed as prose: a frame that does not parse is `Failed` with the parser's words as the reason | `tests/it/server.rs` |

---

## Errors

`LspError`, wrapped by the engine and rendered as **400** or **500** by the node depending on whose
fault it is.

---

## Extension points

A preset: an entry in `catalog.rs` naming the binary, its argv and the languages it serves.

---

## Tests

`tests/it/server.rs` against the scripted server built beside them (`tests/scripted_lsp.rs`, the package's `scripted-lsp` binary: the handshake, a hover, a frame that is not one, a crash mid-session); unit tests beside `codec.rs` and `uri.rs`.

---

## What this crate refuses to do

- install a language server;
- embed a parser of its own;
- parse a server's prose.
