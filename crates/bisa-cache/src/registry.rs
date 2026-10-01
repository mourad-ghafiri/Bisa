//! The registry every cache joins on construction, and the two things it
//! exists to serve: stats for the admin surface, and clearing everything at once.
//!
//! Caches register a [`Weak`] handle, so a cache that is dropped simply falls
//! out of the registry the next time it is walked — a static cache lives for the
//! program, a field cache lives with its owner, and neither has to deregister.
//!
//! There is one process-wide registry, [`Registry::global`], which every
//! `new` constructor joins and which [`all_stats`] and [`clear_all`] read. A
//! [`Registry`] of one's own — [`Registry::new`], joined through the
//! `in_registry` constructors — is for a test, or a subsystem that wants its
//! caches counted apart: clearing it clears nothing else.

use std::sync::{Mutex, MutexGuard, Weak};

/// One cache's counters, as read for the admin surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheStats {
    /// The stable name the cache registered under (e.g. `harness.listing`).
    pub name: &'static str,
    /// Reads served from the cache without recomputing.
    pub hits: u64,
    /// Reads that had to compute (absent or expired).
    pub misses: u64,
    /// Live entries held right now (0 or 1 for a cell).
    pub entries: usize,
}

/// What the registry can ask of any cache, regardless of its key/value types.
pub trait CacheHandle: Send + Sync {
    fn stats(&self) -> CacheStats;
    fn clear(&self);
}

/// A set of caches that are counted and cleared together.
#[derive(Default)]
pub struct Registry {
    caches: Mutex<Vec<Weak<dyn CacheHandle>>>,
}

static GLOBAL: Registry = Registry::new();

impl Registry {
    /// An empty registry of its own.
    pub const fn new() -> Self {
        Self {
            caches: Mutex::new(Vec::new()),
        }
    }

    /// The process-wide registry: what every `new` constructor joins.
    pub fn global() -> &'static Registry {
        &GLOBAL
    }

    /// Recover a poisoned lock rather than panic: a cache is best-effort, and a
    /// panic while holding it must never cascade into the whole registry.
    fn lock(&self) -> MutexGuard<'_, Vec<Weak<dyn CacheHandle>>> {
        self.caches.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Join the registry. Called once per cache, from its constructor.
    pub(crate) fn register(&self, handle: Weak<dyn CacheHandle>) {
        self.lock().push(handle);
    }

    /// Every live cache's stats, in registration order, pruning any that have
    /// been dropped since the last walk.
    pub fn stats(&self) -> Vec<CacheStats> {
        let mut guard = self.lock();
        guard.retain(|w| w.strong_count() > 0);
        guard
            .iter()
            .filter_map(|w| w.upgrade())
            .map(|c| c.stats())
            .collect()
    }

    /// Clear every live cache. Stats counters are left standing — clearing
    /// empties entries, it does not reset the record of how the cache has been
    /// performing.
    pub fn clear(&self) {
        let mut guard = self.lock();
        guard.retain(|w| w.strong_count() > 0);
        for cache in guard.iter().filter_map(|w| w.upgrade()) {
            cache.clear();
        }
    }
}

/// Every live cache's stats in the process-wide registry.
pub fn all_stats() -> Vec<CacheStats> {
    Registry::global().stats()
}

/// Clear every live cache in the process-wide registry.
pub fn clear_all() {
    Registry::global().clear()
}
