# bisa-http

The one place an outbound HTTP client is built: a proxy policy applied to one swappable set of
`reqwest` clients every crate that reaches the network holds by `Arc`, and the environment a child
is handed to obey the same policy — as a leaf crate, like [`bisa-cache`](cache.md), with no
Bisa dependency, so the code host and connector crates stay core-free. The policy is this
crate's own value type; the engine maps the `network.*` settings onto it
([ide/13 — Settings › Capabilities › Network](../ide/13-settings.md#visible-in-the-ui)).

---

## Where things live

| Module | Owns |
|---|---|
| `lib.rs` | the public surface: `HttpPolicy`, `ProxyPolicy`, `ChildEnv`, `Clients`, `HttpError`, `PROXY_ENV_NAMES`, `LOOPBACK_NAMES` |
| `policy.rs` | `ProxyPolicy::{Environment, None, Manual { http, https, no_proxy }}` and `HttpPolicy { proxy, http1_only }` — a `Debug` that masks a proxy URL's password; `no_proxy_value()` (this machine's own names first, then the person's entries); `child_env()` → `ChildEnv { set, remove }` — the same rule `bisa_core::NetworkSettings::child_env` states, held equal by `crates/bisa-engine/src/network.rs`'s test because the crates that hold a `Clients` cannot see the core; `names_a_proxy()` |
| `sse.rs` | `SseFrames` — the platform's one reader of a server-sent event stream (WHATWG § 9.2.6): bytes in through `push`, `SseEvent { event, data }` out through `next_event`, a frame decoded **once** when its blank line has arrived (so a character split across two chunks is one character, never U+FFFD), `\r\n`, `\n` and `\r` all line ends, several `data:` lines joined with `\n`, one leading space stripped, `id:`/`retry:`/comments read past, a frame with no `data:` nothing; `MAX_SSE_FRAME_BYTES` (1 MiB) — past it with no end in sight `SseOverflow`, the buffer cleared, and the caller ends the stream. Three readers were three parsers before this one: the engine's puller of a harness's own events (`interactive.rs`), the CLI's tail of `/events` (`client.rs`), the MCP probe's HTTP+SSE transport (`mcp-probe/sse.rs`) |
| `clients.rs` | `Clients` — `outbound` (the internet: the user agent, a 10 s connect timeout, redirects followed), `strict` (no redirects, for a connector step whose declared host a redirect could leave), `loopback` (built once: `.no_proxy()`, HTTP/1.1 — for a service on this machine), each an `Arc<reqwest::Client>` behind an `RwLock` swapped whole by `apply(&HttpPolicy)`, which builds the new set first and keeps the old on a failure; `new`, `Default` (the contract `reqwest::Client::new()` has), `shared()` (the process's one default set, for a caller handed no policy — a test's stub server, a one-shot command with no workspace open), `policy()`, `loopback_builder()`, `child_env()`; `HttpError` names the cause, never a URL that could carry a login |

---

## The rules

1. **A manual proxy is `Proxy::http` and `Proxy::https` with the bypass as `NoProxy`**, this machine's
   own names — `localhost`, `127.0.0.1`, `::1` — prepended; a login in the URL is applied by reqwest
   as basic auth. `Environment` leaves reqwest's own detection on (`HTTP_PROXY`, `HTTPS_PROXY`,
   `ALL_PROXY`, `NO_PROXY`, either case, read when a client is built — so `apply` re-reads them);
   `None` calls `.no_proxy()`; a manual policy naming no URL is direct and says so.
2. **The loopback client never sees a proxy and never changes**: the harness event puller and a
   loopback service with its own certificate go through it whatever the policy says.
3. **`http1_only` is `ClientBuilder::http1_only`**; the workspace's `reqwest` enables `http2`, so off
   the platform speaks HTTP/2 where a server offers it.

---

## Entry points

`Clients::new(&policy)` / `Clients::shared()`; `clients.outbound()` · `strict()` · `loopback()`;
`clients.apply(&policy)`; `clients.child_env()`. The engine's `network::clients_from(ws)` is the one
path a process makes its set by.

---

## Invariants held here

| Invariant | Where |
|---|---|
| Every policy builds; `apply` swaps the policy; `Debug` never shows a password | `clients.rs::every_policy_builds_and_apply_swaps_the_policy`, `policy.rs::debug_never_shows_a_password` |
| The loopback client is one and the same across a swap | `clients.rs::the_loopback_client_is_one_and_the_same_across_a_swap` |
| A manual policy without a URL is direct | `clients.rs::a_manual_policy_without_a_url_is_direct`, `policy.rs::a_manual_policy_without_a_url_names_no_proxy` |
| An SSE frame is decoded once its blank line has arrived — a multibyte character split across two pushes is one character; CRLF, LF and a lone CR end lines; several `data:` lines join; the `event:` name is read and an empty one is the default; comments, `id:` and `retry:` are read past; a frame over the cap is an overflow that leaves the reader clean for the next frame; a stream split at every byte reassembles | `sse.rs` tests |
| The child environment follows the policy: nothing for `Environment`, the eight names removed for `None`, both cases set and the rest removed for `Manual` | `policy.rs::the_child_environment_follows_the_policy` |

---

## Errors

`HttpError(String)` — a policy that could not become a client, with reqwest's reason; the manual
URL in it is masked. The engine logs it and `GET /network` repeats it as a `problem` until a write
fixes it.

---

## Extension points

| To add… | Touch, in order |
|---|---|
| a client shape (another redirect policy, a timeout profile) | a field of `Set` and its accessor in `clients.rs` → `build` → the crate that needs it |
| a proxy scheme (SOCKS) | reqwest's `socks` feature in the workspace manifest → `ProxyPolicy` → `check_write` in `bisa_core::network_settings` → [ide/13](../ide/13-settings.md) |

---

## Tests

Unit tests beside each module; no network — a client is built, never sent.

---

## What this crate refuses to do

- read a setting or an environment variable itself — the engine maps the settings, reqwest reads
  the environment;
- log or print a proxy's password;
- depend on anything of ours.
