# Migrations — preparing a major release

Inside a major, nothing breaks ([Compatibility](../reference/compatibility.md)). A change that cannot be
made by addition waits for the next major — 1.0.0 is the first — and that release carries a migration
for every incompatible change it makes. This page is how that is prepared, so that a person's
workspace crosses the boundary whole.

## From a proposal to the major

1. **Propose.** A change that would break the contract starts as a Discussion in *Ideas*, then an
   issue labelled `compat: breaking` and set to the next major's milestone. It says what breaks, why
   no additive way exists, and who is affected.
2. **Decide.** A maintainer accepts it (`status: accepted`) or closes it with the additive way it
   missed. Nothing is merged for a major before it is accepted.
3. **Collect.** Accepted breaking changes are developed on the branch of the major, never on `main`,
   while 0.x releases go on from `main`. Each merged breaking change adds its entry to the migration
   guide and its step to the migration tool in the same pull request.
4. **Release candidates.** The major is published first as release candidates (`1.0.0-rc.1`, …),
   marked as pre-releases, for people to migrate copies of real workspaces.
5. **Release.** The major is released with the tool, the guide and the changelog's *Removed* and
   *Changed* entries; the last 0.x minor keeps receiving security fixes for the time
   [SECURITY.md](../../SECURITY.md) states.

## What the migration tool must do

| Requirement | Why |
|---|---|
| **Copies the workspace aside first**, under a name that says the version and the time, and never deletes it | the copy is the way back; nothing the person owned is ever lost |
| **Dry run** — says every change it would make, file by file, and makes none | a person sees the effect before it happens |
| **Refuses a workspace it does not recognise**, in words, and touches nothing | a half-migrated workspace is worse than none |
| **Idempotent and resumable** — run twice, it does nothing the second time; stopped half-way, it finishes on the next run | power cuts and closed laptops happen |
| **Atomic per file** — writes through the store's atomic write, the old file intact until the new one is in place | [08 — Persistence](../architecture/08-persistence.md) |
| **Verifies** — opens the migrated workspace with the new release and reads every record before it reports success | a migration that passes and a workspace that does not open is the failure this prevents |
| **Says what it did** — a report of every change, kept beside the copy | support, and the person's own trust |
| **Runs from the command line and from the app** — the app offers it at the first open of an older workspace and never runs it unasked | a person decides when their data changes |

## What the migration guide must say

- Every incompatible change, in the person's words: what changes, who is affected, what to do.
- How to back up, migrate, verify and go back.
- What collaborating people do: every node of a shared workspace migrates, and when.
- What an integration must change — the HTTP API, the CLI, MCP, the addon API — with before and after.

## How a migration is tested

- A **fixture workspace** written by the last 0.x release, committed with the tests (no private key in
  it), migrated by the tool in a temporary directory, then opened and read whole.
- Every step on its own: a fixture holding exactly the shape it changes, before and after.
- The dry run produces the same list the real run applies.
- A run stopped at every step, then resumed, ends in the same workspace.
- Tests follow [the testing rules](testing-rules.md): nothing destructive, nothing outside a
  temporary directory.

## Known work for 1.0.0

Recorded here so the first major starts from the facts:

- A **format version** on the workspace — absent means a 0.x workspace — so a release can tell which
  migration a workspace needs.
- A **protocol version** on the collaboration wire (the `join` control message and the events), so
  nodes of different releases recognise each other ([14 — Collaboration](../architecture/14-collaboration.md)).
- Records read strictly, and a synced record that does not decode is marked seen and dropped: mixed
  releases in one shared workspace and going back to an earlier release are not promised in 0.x for
  this reason ([Compatibility](../reference/compatibility.md#not-promised-in-0x)).
- The error a person meets when a record cannot be read names a developer script the app does not
  ship; its advice changes with the migration tool.
