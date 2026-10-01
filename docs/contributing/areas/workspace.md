# Workspace

For a person, the workspace is a directory — `~/.bisa` by default — holding everything Bisa knows:
goals and their runs, workflows, projects, agents, channels, keys and settings, with no account and no
server. In code it is `crates/bisa-store`: the filesystem as truth, the SQLite index as a cache rebuilt
from it, identity and keys, the ingest of what peers send, and the catalog installer. The store writes
records and reads them back; it never creates a working directory, runs `git` or launches a process.

## Where it lives

- `crates/bisa-store/src/workspace.rs` — `Workspace`, the facade every other crate holds; `Workspace::open` bootstraps in one order.
- `crates/bisa-store/src/paths.rs` — the only place a workspace directory name is joined; `resolve_within` keeps a caller's path inside its base.
- `crates/bisa-store/src/snapshots.rs`, `crates/bisa-store/src/journal.rs` — snapshots ordered by revision, append-only journals.
- `crates/bisa-store/src/index.rs` — the schema, `SCHEMA_VERSION` and the rebuild from truth.
- `crates/bisa-store/src/runs.rs` — the one writer of a run (`record_run_event`).
- `crates/bisa-store/src/ingest.rs` — what a peer's facts and snapshots must prove before they land.
- `crates/bisa-store/src/identity.rs`, `crates/bisa-store/src/settings.rs` — keys at mode 0600, the settings scopes.
- `crates/bisa-store/src/catalog.rs` and `library/catalog/` — the bundled catalog and its installer.
- `crates/bisa-store/src/usage.rs` — what holds a record, so nothing is deleted while something points at it.
- `crates/bisa-store/tests/it/` — one integration binary, one module per topic.
- `scripts/reset-dev-workspace` — the one thing that removes a workspace: run by a person, it moves the folder aside and deletes nothing.

## Read first

- [08 — Persistence](../../architecture/08-persistence.md) — truth, cache, the schema, writing truth, bootstrap.
- [Workspace layout](../../reference/workspace-layout.md) — every folder, and which of it is truth, this machine's, or cache.
- [crates/store](../../architecture/crates/store.md) — modules, invariants, readers and writers, extension points.
- [04 — Workspace, Project, Goal](../../architecture/04-workspace-project-goal.md) — why no goal is in any path.
- [09 — GEP § What is deliberately not on the wire](../../architecture/09-protocol-gep.md#what-is-deliberately-not-on-the-wire) — what travels and what stays on this machine.

## Rules a change must keep

- The filesystem is truth; the index is a cache that may be deleted at any moment and is rebuilt from the files ([the five rules](../../architecture/README.md#the-five-rules-everything-else-follows-from), rule 1).
- `crates/bisa-store/src/paths.rs` is the only place a directory name is joined, and anything that becomes a filename is a type, never a checked string ([08 § Where paths come from](../../architecture/08-persistence.md#where-paths-come-from)).
- A caller's path stays inside its base, canonicalised, never prefix-matched (I9); a session runs in the folder of the thing it works on, inside the workspace (I8).
- Truth is written through `write_atomic`, logs through `append_line`, one fsynced line per call ([08 § Writing truth](../../architecture/08-persistence.md#writing-truth)).
- The store writes records; directories, `git` and processes are the engine's (rule 4). Its writers are called by the engine alone — the node only reads ([07 § Rule 2](../../architecture/07-layering.md#rule-2--the-node-reads-the-store-every-write-goes-through-the-engine)).
- A schema change bumps `SCHEMA_VERSION` and the next open rebuilds the index; the index never needs a migration.
- An older on-disk shape is refused by name (`StoreError::Unreadable`), never converted or silently dropped; one unreadable file costs its own row, never the list ([08 § A list tolerates one broken file](../../architecture/08-persistence.md#a-list-tolerates-one-broken-file)).
- A snapshot is replaced only by a higher revision (I5a); a peer's run lands only with a local decision for every approval it passed (I4).
- Nothing is deleted while something points at it, and the refusal names the holders (I15); a secret is created at mode 0600 and never widened (I31).

## Testing a change

- Fixtures are a `tempfile::TempDir` opened with `MemoryKeyStore`: nothing touches the OS keyring or a real workspace, and no fixture is removed by hand ([What a test may never do](../testing-rules.md#what-a-test-may-never-do)).
- `scripts/test module store layout` — no directory name joined outside the paths module; `scripts/test module store schema_doc` — the documented schema and version equal the code.
- `scripts/test module store durability`, `scripts/test module store locking` — rebuilds, torn files, the index lock.
- `scripts/test module store <module>` for the topic you touched; `scripts/test lib store` for the unit tests beside the modules; `scripts/test crate store` once they are green.
- `scripts/test module node layering` — a new `Workspace` method classified as a reader or a writer, and refused in the node if it writes.
- Journeys that read the files back with no daemon: `crates/bisa-cli/tests/it/e2e/from_capture_to_done.rs`, `crates/bisa-cli/tests/it/e2e/crash_in_a_step.rs`.
- One module at a time; the whole workspace (`just verify`) is the gate at the end of a pass ([Running](../testing-rules.md#running)).

## Common changes

- [Add a workspace path](../recipes.md#10-add-a-workspace-path)
- [Change the index schema](../recipes.md#11-change-the-index-schema)
- [Add a GEP kind](../recipes.md#9-add-a-gep-kind) — a record that syncs needs an ingest arm and a path.
- The catalog the store bundles: [Add a catalog workflow template](../recipes.md#15-add-a-catalog-workflow-template), [Add a connector](../recipes.md#21-add-a-connector), [Add a built-in pet](../recipes.md#23-add-a-built-in-pet), [Add a built-in addon](../recipes.md#27-add-a-built-in-addon).
- A truth record kind, a `Workspace` writer, the index lock: [crates/store § Extension points](../../architecture/crates/store.md#extension-points).

## Compatibility

- [The workspace on disk](../../reference/compatibility.md#the-workspace-on-disk) is public: a later 0.x release opens a workspace an earlier 0.x wrote and keeps it as it is. The index, the diagnostic log and this machine's window memories are not promised, so an index change is free — bump the version and the next open rebuilds.
- Inside 0.x a minor or patch release never breaks the public contract: a change may add a file, a folder, or a field with a default when absent.
- Renaming, retyping or removing a field, or moving a folder, cannot be made by addition; it waits for 1.0.0, whose migration tool copies the workspace aside first ([the first major release](../../reference/compatibility.md#the-first-major-release)). Going back to an earlier release is not promised ([not promised in 0.x](../../reference/compatibility.md#not-promised-in-0x)).
- Declare the compatibility of your change in the pull request.

## Review focus

- Every path through the paths module, every write through `write_atomic` or `append_line`, nothing joined by hand ([code review](../review/code.md)).
- A new record is refused by name when unreadable, costs its own row only, and defaults a new field when absent ([compatibility review](../review/compatibility.md)).
- Secrets are created at 0600, kept in the keystore or under `identity/`, and never in a record, a snapshot or the index ([security review](../review/security.md)).
- A schema change has its version bump and its walker; the index lock is never re-entered ([performance review](../review/performance.md)).
- The layout page and the schema block in 08 change in the same pull request ([docs and language review](../review/docs-and-language.md)).
