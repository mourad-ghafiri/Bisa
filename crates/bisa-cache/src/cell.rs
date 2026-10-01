//! [`TtlCell`] — a single value that expires after a TTL.
//!
//! For the caches that hold one thing: the harness listing, a sorted presence
//! snapshot. Cloneable and cheap to pass around — a clone shares the one slot,
//! so a `static` cell and a struct field behave the same.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::registry::{CacheHandle, CacheStats, Registry};

struct Inner<T> {
    name: &'static str,
    slot: Mutex<Option<(Instant, T)>>,
    hits: AtomicU64,
    misses: AtomicU64,
}

/// Recover a poisoned lock rather than panic — a cache must not take the
/// process down.
fn lock<T>(m: &Mutex<Option<(Instant, T)>>) -> MutexGuard<'_, Option<(Instant, T)>> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl<T: Clone + Send + Sync + 'static> CacheHandle for Inner<T> {
    fn stats(&self) -> CacheStats {
        CacheStats {
            name: self.name,
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            entries: usize::from(lock(&self.slot).is_some()),
        }
    }
    fn clear(&self) {
        *lock(&self.slot) = None;
    }
}

/// A single cached value with a call-time TTL. `ttl == 0` always misses.
pub struct TtlCell<T> {
    inner: Arc<Inner<T>>,
}

impl<T> Clone for TtlCell<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T: Clone + Send + Sync + 'static> TtlCell<T> {
    /// A new cell registered under `name` (e.g. `harness.listing`) in the
    /// process-wide registry.
    pub fn new(name: &'static str) -> Self {
        Self::in_registry(name, Registry::global())
    }

    /// A new cell registered under `name` in a registry of the caller's.
    pub fn in_registry(name: &'static str, registry: &Registry) -> Self {
        let inner = Arc::new(Inner {
            name,
            slot: Mutex::new(None),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        });
        // Coerce to the trait object first, then downgrade (see `keyed.rs`).
        let as_handle: Arc<dyn CacheHandle> = inner.clone();
        registry.register(Arc::downgrade(&as_handle));
        Self { inner }
    }

    /// The value if it is younger than `ttl`, else `None` (and a recorded miss).
    /// For an async caller that computes the miss itself, then calls [`set`](Self::set).
    pub fn get(&self, ttl: Duration) -> Option<T> {
        if ttl.is_zero() {
            self.inner.misses.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        let mut guard = lock(&self.inner.slot);
        match guard.as_ref() {
            Some((at, v)) if at.elapsed() < ttl => {
                self.inner.hits.fetch_add(1, Ordering::Relaxed);
                Some(v.clone())
            }
            _ => {
                // An expired value is not a live entry: let it go on the miss
                // that finds it, so `entries` says what is held.
                *guard = None;
                self.inner.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// Store a freshly computed value, stamped now.
    pub fn set(&self, value: T) {
        *lock(&self.inner.slot) = Some((Instant::now(), value));
    }

    /// The fresh value, or compute-store-return. `ttl == 0` computes every time
    /// and stores nothing (the disabled state).
    pub fn get_or_insert(&self, ttl: Duration, compute: impl FnOnce() -> T) -> T {
        if let Some(v) = self.get(ttl) {
            return v;
        }
        let value = compute();
        if !ttl.is_zero() {
            self.set(value.clone());
        }
        value
    }

    /// Like [`get_or_insert`](Self::get_or_insert) for a fallible computation; a
    /// failed compute is neither stored nor counted as a stored value.
    pub fn get_or_try_insert<E>(
        &self,
        ttl: Duration,
        compute: impl FnOnce() -> Result<T, E>,
    ) -> Result<T, E> {
        if let Some(v) = self.get(ttl) {
            return Ok(v);
        }
        let value = compute()?;
        if !ttl.is_zero() {
            self.set(value.clone());
        }
        Ok(value)
    }

    /// Forget the value. Counters are left standing.
    pub fn clear(&self) {
        self.inner.clear();
    }

    /// This cell's counters and whether it currently holds a value.
    pub fn stats(&self) -> CacheStats {
        self.inner.stats()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_value_is_a_hit_and_an_expired_one_is_a_miss() {
        let cell = TtlCell::new("test.cell.fresh");
        assert_eq!(cell.get(Duration::from_secs(60)), None, "empty is a miss");
        cell.set(7);
        assert_eq!(
            cell.get(Duration::from_secs(60)),
            Some(7),
            "stored is a hit"
        );
        // Zero elapsed budget expires immediately — and a zero TTL never touches the slot.
        assert_eq!(cell.get(Duration::ZERO), None, "ttl zero never serves");
        assert_eq!(
            cell.stats().entries,
            1,
            "a zero TTL is a refusal to read, not an expiry"
        );
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(
            cell.get(Duration::from_millis(1)),
            None,
            "expired is a miss"
        );
        assert_eq!(
            cell.stats().entries,
            0,
            "and the expired value left on that miss"
        );
        cell.set(8);
        assert_eq!(
            cell.get(Duration::from_secs(60)),
            Some(8),
            "a fresh set serves again"
        );
    }

    #[test]
    fn ttl_zero_computes_every_time_and_stores_nothing() {
        let cell = TtlCell::new("test.cell.zero");
        let mut n = 0;
        let a = cell.get_or_insert(Duration::ZERO, || {
            n += 1;
            n
        });
        let b = cell.get_or_insert(Duration::ZERO, || {
            n += 1;
            n
        });
        assert_eq!((a, b), (1, 2), "each call recomputes");
        assert_eq!(cell.stats().entries, 0, "nothing was stored");
    }

    #[test]
    fn get_or_insert_computes_once_then_serves() {
        let cell = TtlCell::new("test.cell.once");
        let mut calls = 0;
        let make = |calls: &mut i32| {
            *calls += 1;
            *calls
        };
        assert_eq!(
            cell.get_or_insert(Duration::from_secs(60), || make(&mut calls)),
            1
        );
        assert_eq!(
            cell.get_or_insert(Duration::from_secs(60), || make(&mut calls)),
            1
        );
        assert_eq!(calls, 1, "second read did not recompute");
        let s = cell.stats();
        assert_eq!((s.hits, s.misses, s.entries), (1, 1, 1));
    }

    #[test]
    fn a_failed_compute_is_not_stored() {
        let cell = TtlCell::new("test.cell.err");
        let r: Result<i32, &str> = cell.get_or_try_insert(Duration::from_secs(60), || Err("boom"));
        assert_eq!(r, Err("boom"));
        assert_eq!(cell.stats().entries, 0);
    }

    #[test]
    fn clear_forgets_the_value_but_keeps_counters() {
        let cell = TtlCell::new("test.cell.clear");
        cell.set(1);
        let _ = cell.get(Duration::from_secs(60));
        cell.clear();
        assert_eq!(cell.get(Duration::from_secs(60)), None);
        assert!(cell.stats().hits >= 1, "counters survive a clear");
    }
}
