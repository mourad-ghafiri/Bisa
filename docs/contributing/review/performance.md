# Performance review

For any change on a path with a budget ([14 — Performance](../../architecture/ide/14-performance.md)),
or one that adds a poller, a cache, a lock, a stream, an index query, or work done per event, per
frame or per keystroke. The rules are in [Performance](../performance.md).

## Before and after
- [ ] The pull request gives numbers for the path it changes, before and after — the bench
  (`cargo bench -p <crate> --bench <name>`) or a measurement described well enough to repeat.
- [ ] Nothing that was linear became quadratic: loops over a run's steps, a channel's messages, a
  workspace's files, a log's commits are checked for nested scans.
- [ ] A suite that got slower says why; a slow suite is a bug ([Testing rules](../testing-rules.md)).

## The node and the engine
- [ ] Locks are taken through the sanctioned helper and never held across an await or a slow call.
- [ ] A cache has a bound, a named setting for its lifetime, and is invalidated by every write it
  shadows; nothing is cached that must be exact, a write, or a security or consent decision.
- [ ] Index access goes through the existing queries or adds one with the index it needs; nothing
  rebuilds the index outside its rebuild.
- [ ] Frames are sent per change, never per token; a stream that can lag says so rather than buffering
  without bound.

## The desktop
- [ ] A new poller or cosmetic clock is gated on visibility; a hidden window does no work.
- [ ] A store commits a new snapshot only when its payload changed; a list re-renders the row that
  changed, not the list.
- [ ] A heavy module loads with a dynamic `import()`; the bundle does not grow without a reason.

## The binary
- [ ] The shipped profile is unchanged, or the change to it is argued (it keeps unwinding; it never
  aborts on panic).

## The verdict
- [ ] The reviewer states the cost the change adds and why it is acceptable, or requests a cheaper way.
