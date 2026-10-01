//! The one place an outbound HTTP client is built.
//!
//! Before this crate, six independent `reqwest::Client::builder()` sites each
//! chose their own options and none knew a proxy; a proxy set in the node's
//! environment reached the loopback puller too, and a bad proxy would have
//! been swallowed by `unwrap_or_default`. This crate is the one home for that
//! decision:
//!
//! - [`HttpPolicy`] — where the proxy comes from and whether HTTP/1.1 is the
//!   only version spoken; a value, with a `Debug` that never shows a password.
//! - [`Clients`] — one swappable set of clients every crate holds by `Arc`:
//!   `outbound` for the internet, `strict` for a call that must not follow a
//!   redirect, `loopback` for a service on this machine that no proxy may sit
//!   in front of. `apply` builds the new set first and keeps the old on a
//!   failure, so a bad policy is an error the caller reads, never a client
//!   that quietly does something else.
//! - [`ChildEnv`] — the same policy as the environment a child process reads
//!   it from, so the platform and the tools it runs agree.
//! - [`SseFrames`] — the one reader of a server-sent event stream: bytes in,
//!   events out, decoded once per complete frame and capped, for the three
//!   places the platform tails one (a harness's own events, the node's
//!   `/events`, an MCP server's HTTP+SSE stream).
//!
//! Nothing here reads a setting: the engine maps `network.*` onto a policy.

mod clients;
mod policy;
mod sse;

pub use clients::{Clients, HttpError};
pub use policy::{ChildEnv, HttpPolicy, ProxyPolicy, LOOPBACK_NAMES, PROXY_ENV_NAMES};
pub use sse::{SseEvent, SseFrames, SseOverflow, MAX_SSE_FRAME_BYTES};
