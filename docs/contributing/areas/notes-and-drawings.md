# Notes and drawings

Notes and drawings float over whatever screen a person is on. A **note** is a Markdown scratchpad; a
**drawing** is a picture on a canvas (Excalidraw). Each is about one of six things — the workspace, a
project, a goal, a workflow, a channel or this node — and each set is a git repository the person
commits and pushes from the panel's strip. A note stays on this machine unless the person pushes; a
drawing is a record of the workspace (GEP kind 33401) that travels to every member, and agents read
and draw into it with seven tools while the person watches.

## Where it lives

- `crates/bisa-core/src/note.rs` · `crates/bisa-core/src/draw.rs` · `crates/bisa-core/src/owner_scope.rs` — a note; a scene's bounds, reading and erasure, and `DRAW_TOOLS`; what a record hangs off.
- `crates/bisa-store/src/notes.rs` · `crates/bisa-store/src/drawings.rs` — the files, a drawing's snapshot and its `.excalidraw` export, the compare-and-swap on a scene's hash.
- `crates/bisa-engine/src/notes.rs` · `crates/bisa-engine/src/drawings.rs` — the ops, who may draw (`Access`), the requests parked for the desktop.
- `crates/bisa-engine/src/folder_git.rs` — both repositories, on git's safe tier alone.
- `crates/bisa-node/src/notes.rs` · `crates/bisa-node/src/drawings.rs` — the routes, the repositories' routes under `/notes/git` and `/drawings/git`.
- `crates/bisa-mcp/src/server.rs` — `note_read`, `note_append` and the drawing tools on an agent's menu; `library/catalog/skills/drawing.toml` — the Drawing skill every catalog agent carries.
- `desktop/src/notes/` · `desktop/src/draw/` — the two overlays; `desktop/src/ui/excalidraw.ts` — the one door to Excalidraw; `desktop/src/shell/repo/RepoStrip.tsx` — the strip both panels share.

## Read first

- [The desktop § Notes](../../guide/the-desktop.md#notes) and [§ Draw](../../guide/the-desktop.md#draw) — both panels as a person uses them, the repository strip.
- [19 — Drawings](../../architecture/19-drawings.md) — the record and why it has a kind, the canvas, [the agents' tools](../../architecture/19-drawings.md#the-agents-tools), who may draw, [the repository](../../architecture/19-drawings.md#the-repository), the invariants.
- [13 — Conversations](../../architecture/13-conversations.md) — the drawer beside a note or a drawing is the one conversation surface.
- [MCP tools](../../reference/mcp-tools.md) — `note_read`, `note_append` and the seven drawing tools.

## Rules a change must keep

- A drawing is a signed snapshot that travels; its `.excalidraw` file is an export rewritten by every write and every ingest, never the truth ([19 § Invariants](../../architecture/19-drawings.md#invariants)).
- A scene is vector only, under 768 KiB and 4000 elements; an image is refused by name wherever it is written ([19 § Invariants](../../architecture/19-drawings.md#invariants)).
- A scene change is compare-and-swap on the store's hash: the desktop hands back the hash it read, never one it computes, and a stale save is a 409 with the current scene. A listing never carries a scene ([19 § Invariants](../../architecture/19-drawings.md#invariants)).
- Who may draw is checked before anything is read or parked; the General Agent and the Workflow Agent always may ([19 § Who may draw](../../architecture/19-drawings.md#who-may-draw)).
- What needs a canvas is parked for the desktop; with no desktop heard from, *nobody home* is said at once — never a wait ([19 § What the desktop performs](../../architecture/19-drawings.md#what-the-desktop-performs)).
- An agent's one write to a note is to append when asked ([The desktop § Notes](../../guide/the-desktop.md#notes)); a person's stale edit is refused with what is there (`crates/bisa-cli/tests/it/e2e/addons_drawings_and_notes.rs`).
- Both repositories run through `folder_git.rs` on the safe tier: add, commit, push, fetch. No Publish gate stands here — the folders belong to no project and no goal — so the only hand that pushes is the person's, on a button; the drawings' repository offers no pull and never commits `state/` ([19 § The repository](../../architecture/19-drawings.md#the-repository)).
- Leaving a dirty note or drawing asks *Save*, *Don't save* or *Cancel* — the *hold* ([Terminology](../terminology.md)); a *drawing* is never called a diagram, which is Mermaid's word.

## Testing a change

- `scripts/test lib core draw` — the bounds, the reading, the erasure, the skeleton.
- `scripts/test module store notes` · `scripts/test module store drawings` — every scope, the snapshot and the file, the conflict, the rebuild, the cascades.
- `scripts/test module engine notes` · `scripts/test module engine drawings` · `scripts/test module engine folder_git` — the ops over a socket, a request parked and answered, nobody home, both repositories.
- `scripts/test module node drawings` · `scripts/test module node notes_git` — the routes, the 409's shape, a repository without a pull.
- `scripts/test module mcp docs` — the tools and the reference held equal.
- `scripts/test desktop notes` · `scripts/test desktop draw` · `scripts/test desktop shell/repo`, then from `desktop/`: `node --test --import ./src/i18n/preload.mjs src/scenarios/draw.test.mjs src/scenarios/notesGit.test.mjs`.
- The journey `crates/bisa-cli/tests/it/e2e/addons_drawings_and_notes.rs` — a drawing read, filed and erased with no window open, a note added to and never over, the notes' repository committed — under `scripts/test module cli e2e`.

## Common changes

- A drawing tool, or any tool the desktop performs: [Add an MCP tool](../recipes.md#5-add-an-mcp-tool) — it parks on a desk in the engine, and `crates/bisa-engine/src/drawings.rs` is the model.
- A `draw.*` key: [Add a setting](../recipes.md#2-add-a-setting).
- A path on disk: [Add a workspace path](../recipes.md#10-add-a-workspace-path); a change to the record's kind: [Add a GEP kind](../recipes.md#9-add-a-gep-kind).
- The panels: [Add a desktop model](../recipes.md#7-add-a-desktop-model) and [Say something to a person](../recipes.md#26-say-something-to-a-person).

## Compatibility

- Notes and drawings are part of [the workspace on disk](../../reference/compatibility.md#the-workspace-on-disk): a later 0.x opens them as an earlier one wrote them.
- The drawing's record is kind 33401 on [the collaboration wire](../../reference/compatibility.md#the-collaboration-wire); a kind's number is never reused.
- `note_read`, `note_append` and the drawing tools are [MCP tools](../../reference/compatibility.md#mcp-tools) — their names and inputs are promised; the `draw.*` keys are [settings keys](../../reference/compatibility.md#settings-keys); the `/notes` and `/drawings` routes are [the HTTP API](../../reference/compatibility.md#the-http-api-and-its-events).
- A change grows by addition: a new optional field on a record, a new tool, a new optional input. Declare its compatibility in the pull request ([Keeping compatibility](../compatibility.md)).

## Review focus

- The truth and its export are never confused: a write goes to the record, and the file is drawn from it ([Code review](../review/code.md)).
- Every scene write carries the hash it read, and a conflict is said in words with what is there.
- Who may draw is asked before any read or park; nothing pushes on its own; a scene stays within its bounds ([Security review](../review/security.md)).
- A listing reads no scene, and the canvas is loaded lazily ([Performance review](../review/performance.md)).
- The words: drawing, scene, canvas, template, hold ([Docs and language](../review/docs-and-language.md)).
