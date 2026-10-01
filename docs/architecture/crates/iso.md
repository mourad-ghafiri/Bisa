# bisa-iso

Isolation backends for a work item's checkout, behind one trait with a two-phase probe and an
ordered fallback. A backend gives a session a directory to work in and, when the run ends, a way
to capture what changed. Git worktrees are the common case; a copy is the floor every platform
has.

---

## Where things live

| Module | Owns |
|---|---|
| `lib.rs` | `BackendKind { Apfs, Overlayfs, GitWorktree, Copy }`, `trait IsolationBackend { probe, start, stop, diff }`, `backend(kind)`, `resolve(preferred) -> Resolution` (the ordered candidates), `ProbeResult`, `IsoError` (`is_unavailable` decides whether the next candidate is tried) |
| `worktree.rs` | git-worktree isolation; the git plumbing is `bisa-vcs`'s |
| `copy.rs` | the universal floor: copy the root, capture a `.patch` at the end |
| `diff.rs` | backend-agnostic change capture — `Diff`, `FileChange`, with `similar` for the diffs git cannot produce off disk |
| `stubs.rs` | `Apfs` and `Overlayfs` as unavailable-by-construction backends, so every `BackendKind` is dispatchable on every platform |

---

## Entry points

`resolve(preferred)` → `Resolution { candidates }`; `backend(kind).start(lower, merged)`;
`backend(kind).stop(merged)`; the engine's `open_copy_workstream` walks the candidates until one
starts.

---

## Invariants held here

| Invariant | Held by |
|---|---|
| Every `BackendKind` is dispatchable; an unavailable one says so rather than failing later | `tests/it/backends.rs` |
| A copy's teardown captures its diff before removing anything | `tests/it/backends.rs` |

---

## Errors

`IsoError` — `Unavailable` (try the next candidate) or a real failure with the backend named.

---

## Extension points

A backend: a module implementing `IsolationBackend`, a `BackendKind` variant, a place in
`resolve`'s order, a stub where it cannot exist. `tests/it/backends.rs` walks every kind.

---

The engine tells a backend that is missing (`EngineError::Iso`, the node's 409 — the machine's state)
from one that is there and failed (`EngineError::IsoFailed`, a 500 — the node's fault), so a full
disk is never read as *try something else*. The rule is one function, `EngineError::of_iso`, used
where a copy is made and where its tree is taken away; held by the engine's unit test
`a_missing_backend_is_told_from_one_that_failed` and the node's
`a_missing_isolation_backend_is_a_refusal_and_a_failed_one_is_this_nodes_fault`.

## Tests

`tests/it/backends.rs` on temporary directories.

---

## What this crate refuses to do

- shell out to `git` itself — that is `bisa-vcs`'s;
- delete anything it did not create;
- know about goals or the store.
