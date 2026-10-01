# Performance — the rules that keep a weeks-long session cheap

The platform stays open on a laptop for weeks, so two costs matter more than raw speed on any one
action: **CPU throughput** in the node, and **idle draw** in the desktop — what the app burns when
nobody is looking at it. This page is the working rules.

---

## The node is built for speed

The shipped node is `just dist` — the `bisa` CLI at the `dist` profile: `opt-level = 3`,
`lto = "fat"`, `codegen-units = 1`, `strip = "debuginfo"` — symbols stay, so a backtrace names its
frames — and it unwinds, never `panic = "abort"`. Do not lower `opt-level` for
size; the node is CPU-bound and long-lived, so throughput wins. The Tauri shell's release profile
(`desktop/src-tauri`, its own workspace — it does not inherit the root profiles) matches it, and
`just desktop-bundle` builds both.

## A hidden window does no read-only work

There is one answer to "should a background timer run right now": `shell/visibility.ts`
(`isHidden` / `isAwake` / `onVisibilityChange` / `useVisible`).

**Every new poller or cosmetic clock in the desktop must gate on it, and run once on becoming
visible.** A 5 s scan, a 15 s poll, a per-second counter — all of it pauses when the window is hidden
and resumes with one immediate tick so the focused experience is unchanged. For a cosmetic clock,
reach for `shell/clock.ts` (`useClock(periodMs)`): one shared, visibility-gated timer per cadence
rather than one `setInterval` per component.

**Never gate on visibility:** the SSE bus (event-driven, cheap, keeps state live for the moment you
look back) and autosave (a hidden window must still save). Those keep running unseen.

## A store re-renders only when its data moves

A `useSyncExternalStore` store hands React one snapshot object; a fresh reference re-renders every
reader. So commit a new snapshot **only when the payload differs** — `shell/snapshotEqual.mjs`
(`sameJsonList`) for a list of wire DTOs. A duplicate stream frame or an identical poll must keep the
old reference. Keep fast-ticking, per-item text (a live "open 12s" counter) in a small leaf that
ticks itself, so it never invalidates the list or model that renders it.

## Bound anything that accumulates for the session

A cache keyed by something a person keeps creating (roots visited, files opened) grows without bound
over weeks. Cap it — `shell/lru.mjs` is the shared LRU; the per-root path index uses it. A dropped
entry is refetched on next use, exactly as a first use is.

## Defer parse-time at route entry

Large modules are parsed when their route is entered even before they are used. Load them behind a
dynamic `import()` on first real use — Monaco (`ui/monaco.ts` `loadMonaco`) loads on the first editor
mount, not at Workbench entry; the bundled fonts load when a non-system dial is chosen. `manualChunks`
is low value here: the app is loaded from disk, so parse-time, not download, is the cost.

## The log is one write per line

The diagnostic log (`bisa-log`) writes each line synchronously — no worker thread, no buffer,
nothing lost when a process aborts — and the file costs nothing at the default, errors only. At
`debug` it is one system call per event, under the presence fold's own cost; a site that would log
per token logs per turn, or at `trace`. The flight recorder keeps the last 256 events at `debug` and
above in memory whatever the file's level — one visit of the fields into bounded strings and a ring
insert per event, no system call — so a crash report has the lines before it; `trace` never reaches
the ring.

## The store index opens local-first

`Index::open` sets WAL plus `synchronous = NORMAL`, `busy_timeout`, `mmap_size`, `cache_size` and
`temp_store = MEMORY`. `NORMAL` trades a last-transaction rollback on power loss for far fewer fsyncs;
WAL still guarantees no corruption. Standard for a local developer tool.

## Locks

A `std::sync::Mutex` is taken with `Locked::locked` (`bisa_core::sync`), never `lock().unwrap()` or
`lock().expect(..)`. The platform contains panics at its boundaries — an executor, a walk, a route —
so a poisoned lock is the trace of a fault already reported, and its value is a plain value the next
reader can use; a second panic at every later `lock()` would turn one contained fault into a cascade
through every task sharing the state. `crates/bisa-core/tests/it/layering.rs` refuses a panicking
lock in any crate's shipped code.

## Caching

Reach for the shared toolkit, never a hand-rolled `Mutex<…Instant…>`. `bisa-cache`
gives you `TtlCell<T>` (one value) and `TtlCache<K, V>` (keyed, optionally capped) — and, for a
record that must not grow but must not expire either (a person's answers on a goal, a verdict by
subject, a layout per root), `Bounded<K, V>`, capped by count with the oldest insertion going first,
behind the owner's own lock. The rules:

- **Pass the TTL at call time**, from a `cache.*` setting — add a `def!` in `bisa-core`'s
  `settings.rs`, read it through `CacheSettings`, and hand it to `get`/`get_or_insert`. `ttl == 0`
  disables the cache (and `cache.enabled = false` zeroes them all), so a suspected staleness bug is
  one switch away from being ruled out.
- **Name the cache** in its constructor. That registers it, so it shows up in `GET /cache/stats` and
  the desktop Cache panel and is emptied by `POST /cache/clear` — no extra wiring.
- **Invalidate on a mutation** when correctness needs it (the path index on `FileChanged`, the code host
  caches on merge/review/open); rely on the TTL only for data that tolerates brief staleness.
- **Never cache** a write, an authoritative-must-be-exact read, a security/consent decision, or
  anything bound for the wire. Every cache is read-only and discardable.

Cache where the I/O is — `harness`, `engine`, `node` — never in `core` (no I/O, no panics, WASM) and
never as the node's authoritative state. `index.sqlite` stays the canonical rebuildable cache; its
hot lookups use rusqlite's `prepare_cached`.

## GPU

Already engaged wherever a Tauri webview can use it — compositing, scrolling, Monaco's canvas, and the
terminal's WebGL renderer pool with a DOM fallback. There is no hidden flag and no untapped lever; do
not add Rust-side GPU compute for these UI surfaces. Protect the paths that exist (transform/opacity
motion, the WebGL pool) rather than adding new ones.

---

## How to measure

- **Idle draw:** hide the window for 60 s and watch it in Activity Monitor — the pollers should stop,
  so CPU should fall to near zero.
- **Node throughput:** `hyperfine` / `time` an A/B of a representative request (`/inbox`, IDE search)
  built at `opt-level = "z"` vs `3`.
- **Terminal-open latency:** before/after the harness-listing cache, opening several terminals in a
  burst.
- **Re-render fan-out:** the React DevTools Profiler — an idle rail should not re-render every few
  seconds, and a store tick with unchanged data should render nothing.
