//! [`Bounded`] — a keyed map that holds at most `cap` entries, the oldest
//! insertion going when a new key arrives.
//!
//! Not a cache: nothing here expires, and a read never misses because of
//! time. It is the shape for a *record* that must not grow for the
//! process's life — a person's answers to a guard's questions, a
//! classifier's verdicts by subject, a laid-out commit graph per root —
//! where the right bound is a count, and the thing to let go of is the
//! entry that has been there longest. One type, so no module hand-rolls a
//! `HashMap` beside a `VecDeque` and gets the eviction subtly wrong (the
//! engine once evicted "the oldest" by taking whatever key a `HashMap`
//! iterated first).
//!
//! Single-threaded by design: the owner wraps it in the lock it already
//! holds. It joins no registry — there is no TTL to report and clearing a
//! record is the owner's decision, never a cache-wide sweep's.

use std::borrow::Borrow;
use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

/// A map of at most `cap` entries; the oldest inserted goes first.
#[derive(Debug)]
pub struct Bounded<K, V> {
    map: HashMap<K, V>,
    order: VecDeque<K>,
    cap: usize,
}

impl<K, V> Bounded<K, V>
where
    K: Hash + Eq + Clone,
{
    /// Room for `cap` entries; `cap == 0` holds nothing (every insert is
    /// evicted at once), which is a configuration mistake worth being loud
    /// about in a debug build.
    pub fn new(cap: usize) -> Self {
        debug_assert!(cap > 0, "a Bounded of zero holds nothing");
        Self {
            map: HashMap::with_capacity(cap.min(1024)),
            order: VecDeque::with_capacity(cap.min(1024)),
            cap,
        }
    }

    /// The bound this map was made with.
    pub fn capacity(&self) -> usize {
        self.cap
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Keys are looked up as a `HashMap`'s are: a `String` key answers to a
    /// `&str`.
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.map.contains_key(key)
    }

    pub fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.map.get(key)
    }

    /// Store a value. A key already present keeps its place in the order
    /// and takes the new value; a new key that overflows the bound evicts
    /// the oldest entry, which is returned so the owner can say so.
    pub fn insert(&mut self, key: K, value: V) -> Option<(K, V)> {
        if self.map.insert(key.clone(), value).is_some() {
            return None;
        }
        self.order.push_back(key);
        if self.map.len() > self.cap {
            if let Some(old) = self.order.pop_front() {
                let value = self.map.remove(&old)?;
                return Some((old, value));
            } // LCOV_EXCL_LINE: the order holds every key the map holds, so a map past its bound has an oldest to pop
        }
        None
    }

    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let value = self.map.remove(key)?;
        self.order.retain(|k| k.borrow() != key);
        Some(value)
    }

    /// Keep the entries `keep` answers `true` for, in their order.
    pub fn retain(&mut self, mut keep: impl FnMut(&K, &V) -> bool) {
        self.map.retain(|k, v| keep(k, v));
        let map = &self.map;
        self.order.retain(|k| map.contains_key(k));
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_oldest_insertion_goes_first_and_a_rewrite_keeps_its_place() {
        let mut b = Bounded::new(2);
        assert!(b.insert("a", 1).is_none());
        assert!(b.insert("b", 2).is_none());
        // Rewriting `a` neither grows the map nor moves `a` to the back.
        assert!(b.insert("a", 10).is_none());
        assert_eq!(b.get(&"a"), Some(&10));
        // A third key evicts `a`, the oldest insertion, and says which.
        assert_eq!(b.insert("c", 3), Some(("a", 10)));
        assert_eq!(b.len(), 2);
        assert!(!b.contains_key(&"a"));
        assert!(b.contains_key(&"b") && b.contains_key(&"c"));
        assert_eq!(b.capacity(), 2);
    }

    #[test]
    fn a_string_key_answers_to_a_str() {
        let mut b: Bounded<String, u8> = Bounded::new(2);
        b.insert("root".to_string(), 1);
        assert_eq!(b.get("root"), Some(&1));
        assert!(b.contains_key("root"));
        assert_eq!(b.remove("root"), Some(1));
        assert!(b.is_empty());
    }

    #[test]
    fn remove_retain_and_clear_keep_the_order_and_the_map_in_step() {
        let mut b = Bounded::new(3);
        for (k, v) in [("x", 1), ("y", 2), ("z", 3)] {
            b.insert(k, v);
        }
        assert_eq!(b.remove(&"y"), Some(2));
        assert_eq!(b.remove(&"y"), None);
        // With `y` gone there is room: `w` evicts nothing.
        assert!(b.insert("w", 4).is_none());
        // Over the bound again: `x` is the oldest left.
        assert_eq!(b.insert("v", 5), Some(("x", 1)));
        b.retain(|_, v| *v != 4);
        assert_eq!(b.len(), 2);
        assert!(!b.contains_key(&"w"));
        // The order forgot `w` too: the next eviction is `z`, not a ghost.
        b.insert("u", 6);
        assert_eq!(b.insert("t", 7), Some(("z", 3)));
        b.clear();
        assert!(b.is_empty());
        assert!(b.insert("s", 8).is_none());
        assert_eq!(b.len(), 1);
    }
}
