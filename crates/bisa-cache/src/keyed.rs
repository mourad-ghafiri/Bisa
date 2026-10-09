//! [`TtlCache`] — a keyed TTL cache, optionally size-capped.
//!
//! For the caches keyed by something: per-adapter model lists, per-workstream
//! git status, per-root path index, code host PR/checks by number. A concurrent
//! `DashMap` under the hood so readers of different keys don't contend; an
//! optional `max_entries` cap evicts the oldest entry so a cache keyed by
//! something a person keeps creating cannot grow without bound.

use std::hash::Hash;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;

use crate::registry::{CacheHandle, CacheStats, Registry};

/// One entry: when it was stored (its freshness), when it was last read
/// (its place in the eviction order), and the value.
struct Slot<V> {
    fresh: Instant,
    used: Instant,
    value: V,
}

struct Inner<K, V> {
    name: &'static str,
    map: DashMap<K, Slot<V>>,
    /// 0 means unbounded.
    cap: usize,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl<K, V> CacheHandle for Inner<K, V>
where
    K: Eq + Hash + Send + Sync + 'static,
    V: Send + Sync + 'static,
{
    fn stats(&self) -> CacheStats {
        CacheStats {
            name: self.name,
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            entries: self.map.len(),
        }
    }
    fn clear(&self) {
        self.map.clear();
    }
}

/// A keyed cache with a call-time TTL. `ttl == 0` always misses.
pub struct TtlCache<K, V> {
    inner: Arc<Inner<K, V>>,
}

impl<K, V> Clone for TtlCache<K, V> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<K, V> TtlCache<K, V>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    /// An unbounded cache registered under `name` in the process-wide registry.
    pub fn new(name: &'static str) -> Self {
        Self::in_registry(name, 0, Registry::global())
    }

    /// A cache that holds at most `max_entries` (0 = unbounded), evicting the
    /// oldest when a new key would exceed the cap.
    pub fn with_capacity(name: &'static str, max_entries: usize) -> Self {
        Self::in_registry(name, max_entries, Registry::global())
    }

    /// A cache registered under `name` in a registry of the caller's, holding
    /// at most `max_entries` (0 = unbounded).
    pub fn in_registry(name: &'static str, max_entries: usize, registry: &Registry) -> Self {
        let cap = max_entries;
        let inner = Arc::new(Inner {
            name,
            map: DashMap::new(),
            cap,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        });
        // Coerce to the trait object first, then downgrade: `Arc::downgrade`
        // infers its type parameter from the target, so it cannot unsize the
        // concrete `Arc<Inner>` on its own.
        let as_handle: Arc<dyn CacheHandle> = inner.clone();
        registry.register(Arc::downgrade(&as_handle));
        Self { inner }
    }

    /// The value for `key` if younger than `ttl`, else `None` (a recorded
    /// miss). A hit marks the entry most recently used; an entry the miss
    /// finds expired is let go, so an unbounded cache holds only what is
    /// live and `entries` says how many.
    pub fn get(&self, ttl: Duration, key: &K) -> Option<V> {
        if ttl.is_zero() {
            self.inner.misses.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        let now = Instant::now();
        let hit = {
            let mut entry = self.inner.map.get_mut(key);
            match entry.as_deref_mut() {
                Some(slot) if now.duration_since(slot.fresh) < ttl => {
                    slot.used = now;
                    Some(slot.value.clone())
                }
                _ => None,
            }
        };
        match hit {
            Some(value) => {
                self.inner.hits.fetch_add(1, Ordering::Relaxed);
                Some(value)
            }
            None => {
                self.inner
                    .map
                    .remove_if(key, |_, slot| now.duration_since(slot.fresh) >= ttl);
                self.inner.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// Store `value` under `key`, stamped now, evicting the least recently
    /// used if capped.
    pub fn insert(&self, key: K, value: V) {
        let now = Instant::now();
        self.inner.map.insert(
            key,
            Slot {
                fresh: now,
                used: now,
                value,
            },
        );
        self.enforce_cap();
    }

    /// The fresh value for `key`, or compute-store-return. `ttl == 0` computes
    /// every time and stores nothing.
    pub fn get_or_insert(&self, ttl: Duration, key: K, compute: impl FnOnce() -> V) -> V {
        if let Some(v) = self.get(ttl, &key) {
            return v;
        }
        let value = compute();
        if !ttl.is_zero() {
            self.insert(key, value.clone());
        }
        value
    }

    /// Drop one key (e.g. a workstream whose status just changed).
    pub fn invalidate(&self, key: &K) {
        self.inner.map.remove(key);
    }

    /// Empty the cache. Counters are left standing.
    pub fn clear(&self) {
        self.inner.map.clear();
    }

    /// This cache's counters and live entry count.
    pub fn stats(&self) -> CacheStats {
        self.inner.stats()
    }

    /// Evict the least recently used until at or under the cap — a read
    /// keeps an entry alive, an insert alone does not. A no-op when unbounded.
    fn enforce_cap(&self) {
        let cap = self.inner.cap;
        if cap == 0 {
            return;
        }
        while self.inner.map.len() > cap {
            let oldest = self
                .inner
                .map
                .iter()
                .min_by_key(|e| e.value().used)
                .map(|e| e.key().clone());
            match oldest {
                Some(k) => {
                    self.inner.map.remove(&k);
                }
                None => break, // LCOV_EXCL_LINE: a map past its cap holds an entry, so the least recently used is always found
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_is_a_hit_while_fresh_and_a_miss_once_expired() {
        let cache: TtlCache<String, i32> = TtlCache::new("test.keyed.fresh");
        assert_eq!(cache.get(Duration::from_secs(60), &"a".into()), None);
        cache.insert("a".into(), 1);
        assert_eq!(cache.get(Duration::from_secs(60), &"a".into()), Some(1));
        assert_eq!(
            cache.get(Duration::ZERO, &"a".into()),
            None,
            "ttl zero never serves"
        );
    }

    #[test]
    fn invalidate_drops_one_key() {
        let cache: TtlCache<String, i32> = TtlCache::new("test.keyed.invalidate");
        cache.insert("a".into(), 1);
        cache.insert("b".into(), 2);
        cache.invalidate(&"a".into());
        assert_eq!(cache.get(Duration::from_secs(60), &"a".into()), None);
        assert_eq!(cache.get(Duration::from_secs(60), &"b".into()), Some(2));
    }

    #[test]
    fn the_cap_evicts_the_least_recently_used_and_a_read_keeps_an_entry_alive() {
        let cache: TtlCache<i32, i32> = TtlCache::with_capacity("test.keyed.cap", 2);
        cache.insert(1, 1);
        std::thread::sleep(Duration::from_millis(2));
        cache.insert(2, 2);
        std::thread::sleep(Duration::from_millis(2));
        // Reading 1 makes 2 the one least recently used.
        assert_eq!(cache.get(Duration::from_secs(60), &1), Some(1));
        std::thread::sleep(Duration::from_millis(2));
        cache.insert(3, 3);
        assert_eq!(
            cache.get(Duration::from_secs(60), &2),
            None,
            "the least recently used went"
        );
        assert_eq!(
            cache.get(Duration::from_secs(60), &1),
            Some(1),
            "the one just read stayed"
        );
        assert_eq!(cache.get(Duration::from_secs(60), &3), Some(3));
        assert_eq!(cache.stats().entries, 2, "never over the cap");
        // Without a read in between, the oldest insert goes first.
        let plain: TtlCache<i32, i32> = TtlCache::with_capacity("test.keyed.cap.plain", 2);
        plain.insert(1, 1);
        std::thread::sleep(Duration::from_millis(2));
        plain.insert(2, 2);
        std::thread::sleep(Duration::from_millis(2));
        plain.insert(3, 3);
        assert_eq!(plain.get(Duration::from_secs(60), &1), None);
        assert_eq!(plain.stats().entries, 2);
    }

    #[test]
    fn an_expired_entry_is_let_go_on_the_miss_that_finds_it_and_a_fresh_one_stays() {
        let cache: TtlCache<&str, i32> = TtlCache::new("test.keyed.expired");
        cache.insert("old", 1);
        cache.insert("new", 2);
        assert_eq!(cache.stats().entries, 2);
        std::thread::sleep(Duration::from_millis(5));
        // A miss on a short TTL lets the expired entry go — the map does not grow with every key ever seen.
        assert_eq!(cache.get(Duration::from_millis(1), &"old"), None);
        assert_eq!(
            cache.stats().entries,
            1,
            "the expired entry left on its miss"
        );
        assert_eq!(
            cache.get(Duration::from_secs(60), &"new"),
            Some(2),
            "a fresh one is untouched"
        );
        assert_eq!(
            cache.get(Duration::from_secs(60), &"missing"),
            None,
            "a key never stored is a plain miss"
        );
        assert_eq!(cache.stats().entries, 1);
        let s = cache.stats();
        assert_eq!((s.hits, s.misses), (1, 2));
    }

    #[test]
    fn get_or_insert_computes_once_then_serves() {
        let cache: TtlCache<&str, i32> = TtlCache::new("test.keyed.once");
        let mut calls = 0;
        let make = |calls: &mut i32| {
            *calls += 1;
            10
        };
        assert_eq!(
            cache.get_or_insert(Duration::from_secs(60), "k", || make(&mut calls)),
            10
        );
        assert_eq!(
            cache.get_or_insert(Duration::from_secs(60), "k", || make(&mut calls)),
            10
        );
        assert_eq!(calls, 1);
    }

    // added by the coverage pass: b6-keyed.rs
    #[test]
    fn a_clone_shares_the_map_and_clear_empties_it_but_keeps_the_counters() {
        let cache: TtlCache<&str, i32> = TtlCache::new("test.keyed.clone");
        let twin = cache.clone();
        cache.insert("k", 1);
        assert_eq!(twin.get(Duration::from_secs(60), &"k"), Some(1));
        cache.clear();
        assert_eq!(twin.get(Duration::from_secs(60), &"k"), None);
        let s = cache.stats();
        assert_eq!((s.entries, s.hits, s.misses), (0, 1, 1));
    }
}
