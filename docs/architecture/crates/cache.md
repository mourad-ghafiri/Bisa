# bisa-cache

The shared in-process cache toolkit: the one home for the TTL-cache pattern — a value, the instant
it was computed, a time-to-live, hit/miss stats and a way to clear it — as a leaf crate, pure `std`
+ `dashmap`, that every I/O-doing crate shares instead of hand-rolling its own.

---

## Where things live

| Module | Owns |
|---|---|
| `lib.rs` | the public surface: `TtlCell`, `TtlCache`, `Bounded`, `CacheHandle`, `CacheStats`, `Registry`, `all_stats`, `clear_all` |
| `bounded.rs` | `Bounded<K, V>` — a keyed record of at most `cap` entries, the oldest insertion evicted first (`insert` returns what went); `get`, `remove`, `retain`, `clear`. No TTL and no registry: it is the shape for a record that must not grow for the process's life (the engine's guard answers and classifier verdicts, the laid-out commit graph per root), never for a value that may go stale |
| `cell.rs` | `TtlCell<T>` — a single-slot value that expires after a call-time TTL (the harness listing, a folder's git status, the mobile toolchain, the presence and model-health snapshots) |
| `keyed.rs` | `TtlCache<K, V>` — a keyed, optionally size-capped TTL cache over `DashMap` (the model lists, git status, the path index, the code host results) |
| `registry.rs` | `Registry` — a set of caches counted and cleared together; `Registry::global()` is the process-wide one every `new` constructor joins, and `all_stats()` / `clear_all()` walk it — the admin surface, with no per-cache wiring. A registry of one's own (`in_registry` constructors) is counted and cleared apart: a test's, or a subsystem's |

---

## The two rules

1. **The TTL is passed at call time, never stored** (`get(ttl, …)`, `get_or_insert(ttl, …)`), so a
   caller hands the cache whatever the settings resolve to. **`ttl == 0` means always-miss** — the
   clean way to disable a cache without deleting its code (`cache.enabled = false` resolves every TTL
   to zero, in `bisa_core::CacheSettings`).
2. **Every cache registers under a `&'static str` name** and carries `AtomicU64` hit/miss counters, so
   `all_stats()` / `clear_all()` serve `GET /cache/stats` and `POST /cache/clear` (and the desktop
   Cache panel) without knowing any cache by name.

---

## Entry points

`TtlCell::new(name)` → `get(ttl)` / `set(v)` / `get_or_insert(ttl, || …)`; `TtlCache::new(name)` or
`with_capacity(name, cap)` → `get(ttl, &k)` / `insert(k, v)` / `get_or_insert(ttl, k, || …)` /
`invalidate(&k)`. `bisa_cache::all_stats()` and `clear_all()` for the management surface.

---

## Invariants held here

| Invariant | Where |
|---|---|
| A fresh value is a hit, an expired one and `ttl == 0` are misses; a failed compute is not stored | `cell.rs`, `keyed.rs` tests |
| The size cap evicts the least recently used — a read keeps an entry alive, an insert alone does not; an entry a miss finds expired is let go, so an unbounded cache holds only what is live and `entries` says how many; a zero TTL is a refusal to read, never an expiry | `keyed.rs` and `cell.rs` tests |
| A `Bounded` evicts the oldest insertion when a new key overflows the cap, a rewrite keeps its place, and `remove`/`retain`/`clear` keep the order and the map in step — the next eviction is never a ghost | `bounded.rs` tests |
| a registry reports every live cache in registration order and prunes dropped ones; `clear` empties its caches, keeps their counters and touches no other registry's; `new` joins the process-wide registry `all_stats` reads | `tests/it/registry.rs` |

## What must not be cached

A cache here is read-only, discardable, and either TTL-bounded or event-invalidated. It never holds a
write, an authoritative-must-be-exact read, a security or consent decision, or anything bound for the
wire — the GEP protocol excludes the index and all derived data ([09](../09-protocol-gep.md)).
