# Notes and drawings

Notes and drawings float over whatever screen a person is on. A **note** is a Markdown scratchpad; a
**drawing** is a picture on a canvas (Excalidraw). Each is about one of six things — the workspace, a
project, a goal, a workflow, a channel or this node — and each set is a git repository the person
commits and pushes from the panel's strip. Both stay on this machine unless the person pushes: a
note has no kind at all, and a drawing is a signed record of the workspace (GEP kind 33401) that no
transport sends today ([19 § What travels today](../../architecture/19-drawings.md#what-travels-today)).
Agents read and write a note with three tools, and read and draw into a drawing with seven while
the person watches.

## Where it lives

- `crates/bisa-core/src/note.rs` · `crates/bisa-core/src/draw.rs` · `crates/bisa-core/src/owner_scope.rs` — a note; a scene's bounds, reading and erasure, and `DRAW_TOOLS`; what a record hangs off.
- `crates/bisa-store/src/notes.rs` · `crates/bisa-store/src/drawings.rs` — the files, a drawing's snapshot and its `.excalidraw` export, the compare-and-swap on a scene's hash.
- `crates/bisa-engine/src/notes.rs` · `crates/bisa-engine/src/drawings.rs` — the ops, who may draw (`Access`), the requests parked for the desktop.
- `crates/bisa-engine/src/folder_git.rs` — both repositories, on git's safe tier alone.
- `crates/bisa-node/src/notes.rs` · `crates/bisa-node/src/drawings.rs` — the routes, the repositories' routes under `/notes/git` and `/drawings/git`.
- `crates/bisa-mcp/src/server.rs` — `note_read`, `note_append`, `note_write` and the drawing tools on an agent's menu; `crates/bisa-mcp/src/client.rs` — a canvas request on a connection of its own, sent once; `library/catalog/skills/drawing.toml` — the Drawing skill every catalog agent carries.
- `desktop/src/notes/` · `desktop/src/draw/` — the two overlays; `desktop/src/ui/excalidraw.ts` — the one door to Excalidraw; `desktop/src/shell/repo/RepoStrip.tsx` — the strip both panels share.

## Read first

- [The desktop § Notes](../../guide/the-desktop.md#notes) and [§ Draw](../../guide/the-desktop.md#draw) — both panels as a person uses them, the repository strip.
- [19 — Drawings](../../architecture/19-drawings.md) — the record and why it has a kind, the canvas, [the agents' tools](../../architecture/19-drawings.md#the-agents-tools), who may draw, [the repository](../../architecture/19-drawings.md#the-repository), the invariants.
- [13 — Conversations](../../architecture/13-conversations.md) — the drawer beside a note or a drawing is the one conversation surface.
- [MCP tools](../../reference/mcp-tools.md) — `note_read`, `note_append`, `note_write` and the seven drawing tools.

## Rules a change must keep

- A drawing is a signed snapshot; its `.excalidraw` file is an export rewritten by every write and every ingest, never the truth — a save whose file cannot be written has still saved ([19 § Invariants](../../architecture/19-drawings.md#invariants)).
- **A deletion is a write.** A note's, a drawing's and a whole scope's go under the one writer's lock, so a write that had already read the record cannot put it back; and each goes by where the **index** says the record is, never by reading it — a note whose file will not read and a drawing whose snapshot is torn are deleted like any other (`delete_note`, `delete_drawing`).
- **A check added later is a write's check.** What a file or a snapshot already holds still reads, lists and deletes; a write validates the field it writes (`Note::validate_title` / `validate_body`, `Scene::validate_for_write`). A check on the reading path makes records vanish at the next rebuild.
- A note's title is one trimmed line of at most 200 characters, the same in the answer, the index and the file (`Note::clean_title`).
- A scene is vector only, under 768 KiB and 4000 elements; an image is refused by name wherever it is written ([19 § Invariants](../../architecture/19-drawings.md#invariants)).
- A scene change is compare-and-swap on the store's hash: the desktop hands back the hash it read, never one it computes, and a stale save is a 409 with the current scene. A listing never carries a scene ([19 § Invariants](../../architecture/19-drawings.md#invariants)).
- Who may draw is checked before anything is read or parked; the General Agent and the Workflow Agent always may ([19 § Who may draw](../../architecture/19-drawings.md#who-may-draw)).
- What needs a canvas is parked for the desktop; with no desktop heard from, *nobody home* is said at once — never a wait ([19 § What the desktop performs](../../architecture/19-drawings.md#what-the-desktop-performs)).
- An agent writes into a note only when asked, and in two bounded ways: `note_append` adds under its name and changes nothing; `note_write` rewrites the body at the hash `note_read` answered and is refused with the current hash when the note moved since — never emptying a note, never over a body holding a secret the platform redacted ([The desktop § Notes](../../guide/the-desktop.md#notes)); a person's stale edit is refused with what is there (`crates/bisa-cli/tests/it/e2e/addons_drawings_and_notes.rs`). A note made or deleted, an agent's write, and a note a pull moved are announced with the body's hash and, for an agent's, the agent (`note_changed`). **A person's edit is not** — the editor holds the answer, and hearing its own save once made it refetch and replace the text being typed, until the app fell over (`a_note_write_is_silent_and_an_agent_append_is_not`) — unless its caller asks (`announce` on `PATCH /notes/{id}`), which the command line does, being nobody's open editor. So the editor sends only what moved: a title nobody touched is never sent, or it would undo a rename no frame told it of (`notesModel.savePatch`). The person's unsaved text is the desktop's to keep — an append lands under it live, a rewrite over it is a conflict the person settles, a rewrite over a clean editor is taken with *Restore my version* offered, and a draft restored from a closed window never autosaves over what an agent wrote since (`notesModel.adoptIncoming`, `restoredDraft`).
- A harness handed no MCP server is told in its frame that it has no platform tools for the note or the drawing, so it says so instead of describing a change it never made (`framing::origin_frame_for`, `HarnessCaps::MCP_SERVERS`).
- Notes and drawings are written under one writer's lock each (`notes_writes`, `drawings_writes`); a drawing save that changes nothing writes nothing; a canvas is live from its first change, an offscreen save moves no open canvas's record, a late answer after the engine's wait and a drawing landing from the owner's other machine are announced all the same ([19 § What the desktop performs](../../architecture/19-drawings.md#what-the-desktop-performs)).
- **A pull moves files under the store**: after one that moved, the notes are resynced — indexed, the rows of files that left cleared, each change announced (`Workspace::resync_notes`, `notes::adopt_moved`). The notes repository leaves out `node/` — this machine's own notes — and a write a crash cut short; an exclude only keeps a file from being *added*, so one already tracked travels until it is untracked by hand.
- **What the canvas saves is the scene it last reported**, never one read back from the canvas's API — which reads as empty once the canvas has unmounted. A 409 leaves the canvas unsaved; a save the node refuses is not sent again until the canvas changes ([19 § The canvas](../../architecture/19-drawings.md#the-canvas)).
- **Every drawing request is answered, and lasts as long as its asker**: no open-ended wait on the desktop, a request withdrawn when its session hangs up, a canvas request sent once, the reply's facts read from the store, the policy of the project the drawing is filed under ([19 § What the desktop performs](../../architecture/19-drawings.md#what-the-desktop-performs)).
- A record this build cannot read answers 404 with the code `record_unreadable`; a client says *unreadable*, never *gone*, and still offers the delete.
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
- `note_read`, `note_append`, `note_write` and the drawing tools are [MCP tools](../../reference/compatibility.md#mcp-tools) — their names and inputs are promised; the `draw.*` keys are [settings keys](../../reference/compatibility.md#settings-keys); the `/notes` and `/drawings` routes are [the HTTP API](../../reference/compatibility.md#the-http-api-and-its-events).
- A change grows by addition: a new optional field on a record, a new tool, a new optional input. Declare its compatibility in the pull request ([Keeping compatibility](../compatibility.md)).

## What is open (notes)

Drawings' open items are in [19 § What is open](../../architecture/19-drawings.md#what-is-open).

- **An agent reaches any note whose id it learns.** `note_read`, `note_append` and `note_write` take a `note` id with no check that the session's conversation, goal or project may reach it; `get_goal` hands out a goal's note ids. Left by decision (2026-11): scoping it changes what agents can do today.
- **`bisa note show` prints a note's body raw**, so a body an agent wrote can carry terminal escape sequences to the person's terminal.
- **A note's preview loads remote images** (`https:` in the window's `img-src`): an image an agent writes into a note is fetched when the note is read.
- **Only the app's own Pull resyncs.** A `git pull` in a terminal moves files the store hears nothing of; `bisa workspace reindex` (with the node stopped) brings them in. A file changed by hand under an unchanged `updated_at` is indexed and announced by nobody.
- **A machine note an earlier commit tracks keeps travelling**, and shows under *Node* on the machines that pull it. Untracking it for the person would delete it from their other machines at their next pull.
- **`bisa note edit --body` without `--base-hash` writes over the body as it stands** — said, with what it replaced, but not refused: refusing would break a script that calls it today.
- **A note skipped by a rebuild has no row** — a file that will not read is named as a workspace problem and listed by nothing until it is put right or removed by hand.
- **The dock's count reads every note's body** (`GET /notes` has no count); focus is not moved on opening a note, going back or closing the panel.

## Review focus

- The truth and its export are never confused: a write goes to the record, and the file is drawn from it ([Code review](../review/code.md)).
- Every scene write carries the hash it read, and a conflict is said in words with what is there.
- Who may draw is asked before any read or park; nothing pushes on its own; a scene stays within its bounds ([Security review](../review/security.md)).
- A listing reads no scene, and the canvas is loaded lazily ([Performance review](../review/performance.md)).
- The words: drawing, scene, canvas, template, hold ([Docs and language](../review/docs-and-language.md)).
