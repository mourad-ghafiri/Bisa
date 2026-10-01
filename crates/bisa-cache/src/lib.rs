//! A small, reusable in-process cache toolkit.
//!
//! Before this crate, every TTL cache in the workspace hand-rolled the same
//! `Mutex<…(Instant, T)…>` + `elapsed() < ttl` idiom, with no stats and no way
//! to clear it. This crate is the one home for that pattern:
//!
//! - [`TtlCell`] — a single-slot value that expires after a TTL.
//! - [`TtlCache`] — a keyed, optionally size-capped TTL cache.
//! - [`Bounded`] — a keyed record capped by count, the oldest insertion
//!   going first; no TTL, no registry — for what must not grow, not for
//!   what may go stale.
//!
//! Two rules make the whole thing tunable and observable from one place:
//!
//! 1. **The TTL is passed at call time**, never stored, so a caller can hand it
//!    whatever the settings currently resolve to. A TTL of zero means *always
//!    miss* — the clean way to disable a cache without deleting its code.
//! 2. **Every cache registers itself** under a `&'static str` name, so
//!    [`all_stats`] and [`clear_all`] can enumerate and act on all of them with
//!    no per-cache wiring at the call sites — the admin stats/clear surface.
//!    The registry they join is [`Registry::global`]; a [`Registry`] of one's
//!    own (the `in_registry` constructors) is counted and cleared apart.
//!
//! Nothing here does I/O, blocks, or panics on a poisoned lock (a poisoned
//! cache recovers rather than taking the process down); the value a cache holds
//! is the caller's, computed by the caller.

mod bounded;
mod cell;
mod keyed;
mod registry;

pub use bounded::Bounded;
pub use cell::TtlCell;
pub use keyed::TtlCache;
pub use registry::{all_stats, clear_all, CacheHandle, CacheStats, Registry};
