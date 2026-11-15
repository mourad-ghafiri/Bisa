# 19 — Drawings

A **drawing** is a picture on a canvas: boxes, arrows, text, sticky notes, frames — an architecture,
a flow, a board, a map of who talks to whom. It lives beside Notes and works the way a note does:
attached to the workspace, a goal, a project, a workflow, a channel or this node; listed by those
tabs in a panel that floats over whatever screen you are on; committed from that panel to a git
repository of its own. Unlike a note, a drawing is a **record of the workspace** (GEP kind
**33401**) — signed and addressable, so it *can* travel — and the platform's agents can draw into
it with tools of their own, while the person watches. Today a drawing leaves the machine one way:
its git repository ([What travels today](#what-travels-today)). This page is the model, the canvas,
the tools, who may draw, where it all lives, and what is open.

---

## The words

| Term | Means | Does **not** mean |
|---|---|---|
| **Drawing** | the record: a title, what it is attached to (an `OwnerScope`), `pinned`, its timestamps and its **scene** — GEP kind 33401 in the `drawings/` namespace | a diagram type, a file, an image |
| **Scene** | Excalidraw's element JSON as the canvas wrote it, and the little of the canvas's state a drawing keeps — the ground colour and whether the grid shows (`SceneAppState`); vector only, under 768 KiB and 4000 elements | the whole `appState` (the selection, the zoom, the open menu are one window's) |
| **Canvas** | the surface the desktop draws a scene on: Excalidraw, embedded, with the platform's shape libraries and templates | a screen of its own; a tab |
| **Template** | a skeleton a new drawing starts from — *Empty*, a system architecture, a flowchart, swimlanes, a mind map, a kanban board, a user journey, an entity relationship, a retrospective board, a wireframe, a timeline | a catalog item; something an agent installs |
| **Shape library** | the platform's own items in the canvas's library sidebar — software-engineering elements (a service, a database, a queue, a cache, a client, an API gateway, a load balancer, an external system, an actor), stickers (sticky notes, callouts, badges, a checkmark, a warning), drawing elements (a title banner, a section frame, a legend, a note card) | Excalidraw's public library site, which the desktop never opens |
| **Skeleton** | what an agent hands `drawing_draw`: elements as a person would describe them — a type, a place, a label, a binding by id — which the canvas fills out (sizes to fit, arrow points, bindings) | a full scene |
| **Request** | one drawing tool call the engine parked for the desktop's canvas to perform — a skeleton to lay out, a Mermaid text to draw, a snapshot to render | a read, a listing, a new drawing or an erasure, which the engine answers itself |
| **The repository** | `<data_dir>/drawings/` as a git repository the person commits and pushes from the panel; it holds the `.excalidraw` **files**, the export of every record | the truth (that is the snapshot) |

---

## The record, and why it has a kind

Notes are denied a kind by [09](09-protocol-gep.md)'s one question — a scratchpad on one machine is
not a fact a second node could act on. A drawing passes that test: a picture of the system is read
the same on every node, a teammate draws into it, and an agent on another member's machine can read
it and add to it. So a drawing is an **addressable snapshot** like an addon's record or a channel:
`drawings/state/33401-<id>.json`, signed by the owner at the next revision (`SnapshotStore::put`)
and put on the store's bus as a `ConversationSnapshot` with the workspace as its audience.

### What travels today

**Nothing sends a drawing.** The host's pump publishes a channel's snapshot and nothing else of the
kind — *goals, journals, snapshots of anything but a channel, and read markers stay on this node*
(`bisa-net/src/host.rs`) — so a drawing's snapshot is written, signed and announced locally, and no
transport carries it. Its way off the machine is the **drawings repository**: the `.excalidraw`
files, committed and pushed by hand, opened anywhere Excalidraw does. They do not come back as
records: the repository offers no pull, and a file is an export the next write redraws.

The **receiving door exists and is held shut to everyone but the owner's own key**
(`Workspace::ingest_remote_event`): a hosted member writes as a human only, and a drawing is not a
person's act; an agent of another node never writes here. What the owner's key sends — the same
person's other machine, the day a transport carries it — is read **before** it is stored
(`ingest_drawing_snapshot`): it must decode as a drawing, be filed under its own id (`d` is the
id), and be a scene a save here would accept (`Drawing::validate_for_write`), or nothing of it is
kept. Landed, it is indexed, drawn into its file and announced (`RemoteDrawingArrived`) — announced
only when it is listed here: one about a goal this workspace does not hold is kept as its snapshot
alone. Two machines that both edited one drawing would meet as last-writer-wins on
`(revision, created_at)`; that is [open](#what-is-open) until something sends.

The number is **reissued**. `33401` was a plan's once and was retired as a hole; it comes back for
the drawing by the same rule that gave `33407` to the addon record: no workspace this version opens
can hold an event of the old kind, because the index carries no ladder and an older shape is refused
at open — the rule before 0.1.0; from that first public release no number is reissued
([Compatibility](../reference/compatibility.md#the-collaboration-wire)). Two numbers were reissued; four stay holes (`3405`, `3409`, `33406`, `33410`); the next free
addressable number is still `33416` ([reference/gep.md](../reference/gep.md)).

**Vector only.** A record crosses the wire whole, and one frame of the sync carries a mebibyte with
the envelope around it. So a scene is bounded at `MAX_SCENE_BYTES` (768 KiB) and `MAX_DRAWING_ELEMENTS`
(4000), every element is one of eight vector types (`rectangle`, `ellipse`, `diamond`, `arrow`,
`line`, `freedraw`, `text`, `frame`), and an `image`, an `embeddable`, an `iframe` or a `magicframe`
is **refused by name** — on the canvas (the image tool is off, and no web embed validates) and at
the store, whoever writes. The core checks only what it promises (`bisa_core::draw`): the type, a
string `id` and numeric `x`/`y` on every element, texts under 4 KiB; Excalidraw owns the rest of the
element's shape.

**A write is checked as a write** (`Scene::validate_for_write`), on top of that: no two elements
under one id; `x`, `y`, `width` and `height` finite and within 10⁹ of the origin; and a **link** a
person may follow — `http`, `https`, `mailto`, the app's own scheme, or relative (a path, a
`#fragment`, a `?query`), at most 2048 bytes, with no control character. `javascript:`, `data:`,
`file:` and every other scheme are refused, with the element named. The two checks are apart on
purpose: a scene stored before a check existed still **reads**, lists, renames, pins and deletes —
it is its next save that is refused. A skeleton an agent hands `drawing_draw` is held to the same
numbers and links before the canvas draws it, and so is a drawing from another machine before it
is stored.

---

## Where it lives

| What | Where | Notes |
|---|---|---|
| The record | `drawings/state/33401-<DrawingId>.json` | the truth: a signed, addressable event; `revision` is the authority |
| The file | `drawings/workspace/<id>.excalidraw`, `drawings/goals/<GoalId>/<id>.excalidraw`, `drawings/projects/<slug>/…`, `drawings/workflows/<WorkflowId>/…`, `drawings/channels/<ChannelId>/…`, `drawings/node/…` | the **export**, rewritten by every write and every ingest: the standard `{type: "excalidraw", version, source, elements, appState, files: {}}` — it opens anywhere Excalidraw does — plus a `bisa` block (`id`, `title`, `scope`, `scope_id`, `updated_at`) Excalidraw's reader ignores, so the title is in the diff and in the commit message |
| The repository | `drawings/` | one git repository, made by the first drawing (`folder_git::FolderGit::ensure`); its own `.git/info/exclude` names `state/`, so the snapshots are never committed and no tracked file is written for it |
| The index | the `drawings` table | the locator and every column a list draws — `title`, `pinned`, `hash`, `element_count`, the timestamps — so a listing never reads a scene; the snapshot is the truth a rebuild reads back (`reindex_drawings`), and the files are drawn again with it |

The layout is the notes' (`Paths::scoped_dir`), and the two share one type for what a record hangs
off — `OwnerScope`, six choices — one filter for a listing (`OwnerFilter`), and one conflict
(`StoreError::EditConflict { what, current, current_hash }`: a note's body as a string, a drawing's
scene as an object). Deleting a goal, a project, a workflow or a channel takes its drawings with it
— the files, the snapshots and the rows — so the repository's next commit records them leaving with
what they were about.

**The snapshot is the truth and the file its export.** A write signs the snapshot, makes the row
match, puts the change on the bus and draws the file, in that order — nothing is announced that a
reader could not then find — and a file that cannot be written is said in the log and drawn again
by the next write or rebuild: the save has happened, and answering it as failed would leave its
caller holding a stale hash. A deletion is a write like any other, under the one writer's lock: a
save that had already read the drawing cannot sign it again after it went. It goes by where the
**row** says the drawing is, so a drawing whose snapshot will not read — which the list still
shows — is removed like any other.

---

## The canvas

The desktop embeds **Excalidraw** (`@excalidraw/excalidraw`, MIT) behind one door, `ui/excalidraw.ts`,
loaded lazily so the chunk is never in the boot path. Its fonts are copied from the package into
`desktop/public/excalidraw/fonts/` — a generated folder, ignored by git — at `npm run dev` and
`npm run build` and served from the app's own
origin (`window.EXCALIDRAW_ASSET_PATH`): the canvas asks no CDN for anything. The image tool is off
and no web embed validates (`validateEmbeddable={false}`); *Open*, *Save to*, the export-to-a-link
dialog and the external library site are not offered — *Save as image* is, in the menu; the theme
follows the app's and the language the app's locale.

What the canvas saves is the scene: the elements that are drawn (a deleted element the canvas keeps
for its undo is dropped) and the two facts of `SceneAppState`. Saving is **compare-and-swap** on the
scene's hash, the store's alone: the desktop hands back the `hash` it read with the drawing, never
one it computes, and a stale one is a 409: saving **freezes**, the bar says somebody drew meanwhile,
and the person chooses — *Take theirs* adopts the current scene (a drawing has no textual merge; the
strokes since the last save are the price, said in words), or *Keep mine as a new drawing* files
what is on the canvas as a drawing of its own, beside the one it lost to. A frozen canvas is
**unsaved**: leaving it asks.

**What is saved is what the canvas last reported** (`onChange`'s scene, kept in a ref), never what
the canvas's API answers at the moment of the save: the save that flushes on the way out runs after
the canvas has unmounted, when its API reads as an empty scene — and an empty scene at the right
hash is a save the store takes. Nothing is armed once the editor is gone.

A save the node **refuses for what the scene holds** — too large, an element or a link the store
does not keep — is said in the node's sentence and not sent again until the canvas changes; one the
node **could not take** is tried again at growing waits, up to half a minute
(`autosaveModel.failureKind` · `rearms` · `retryDelay`). A drawing **deleted elsewhere** while open
(`drawing_changed` with `removed`, or a save's 404) says so, is saved to no more, and keeps its
canvas to be saved as a new drawing. A save that changes nothing — the same scene at the right
hash — writes nothing and answers the record as it stands, so a canvas saving what it just adopted
moves no hash anyone has to follow. The canvas is **live from its first change**: Excalidraw hands
its API over before the scene loads and reports nothing while loading, so the first `onChange` is
the load and the live record (`liveScene.ts`) is made from it, at the hash the drawing was read at —
never from the empty canvas the API arrived on, which read every drawing as moved and autosaved its
old elements. The record is the open canvas's: a save through another canvas — the bridge's offscreen
one, a mount that closed while its flush was in the air — moves nothing of it, and a close forgets
its own record alone. A `drawing_changed` frame for the open drawing whose `hash`
is not the one the canvas last saved means somebody else drew: the canvas reloads when it is clean,
and freezes the same way when it is not; the frames reach the editor through the overlay, the one
subscriber (`heard`), so a frame heard between the detail's read and the editor's mount is judged
on the mount, and a reload a canvas cannot perform yet — not loaded — is **owed** to its first change
rather than dropped. A frame heard **while a save is in the air** waits for that
save's answer — the bus and the PATCH race, and the frame may be the save's own echo outrunning it
— and is judged then: the echo is nothing — the bridge's too, since it saves through the same record
(`atStore`) — any other hash is somebody else's scene
(`autosaveModel.reloadDecision` · `frameHeard` · `heardAfterSave`). When the bus comes back, the
list is read again and the open drawing's row — its hash — is judged as a frame would be: a clean
canvas adopts, a dirty one freezes; no read ever swaps the canvas under the pen — a reload a frame
asked for replaces the canvas only if no stroke landed while its read was out
(`autosaveModel.adoptDecision`). Beside the autosave, **Save** in the header (and the
keymap's `save`, ⌘S) saves now; the editor **holds** the drawing for the panel's leave guard
(`shell/documentGuard.ts`, [ide/03](ide/03-files-and-editing.md)): leaving with unsaved strokes —
Back, the panel's ×, the chord, the dock, another drawing opened over it — asks *Save · Don't save ·
Cancel* through the one `UnsavedDialog`, and the same hold is a dirty source of `editorRegistry`, so
quitting counts and saves it like a document. *Delete* asks first (`ConfirmDialog`, the plain verb)
and names what goes with the drawing: its snapshot and its conversations. The list offers the same
*Delete* on every row (a trash that shows on hover or focus, beside the row and never inside it), asked
in the same words, so a drawing goes without being opened; one already gone simply leaves the list.

The **panel** is the notes overlay's twin — a pane that floats over whatever screen is open, at
`z-40` under every dialog, over a live browser tab too (the browser layer cuts around it, ide/18), with the same seven tabs, search, *New* and a repository strip at the
foot of its list; its **dock** is a floating button the footer's Draw switch shows or hides — wearing
how many drawings there are (`drawingCount.ts`, a live count over `GET /drawings`: read while the dock
shows, again on `drawing_changed` and on a project, goal or workflow gone with its drawings, and when
the bus comes back, whether or not the panel is open, and kept across a hide and a show; Settings
turns the number off, never the count) — and **Mod+Alt+D** (`toggle_draw`) opens and closes the panel. *New* is a dialog (`NewDrawingDialog.tsx`):
a gallery of the templates as tiles — each a **preview** drawn from its own skeleton (`templates/preview.mjs`,
no canvas loaded) and a **blurb** on when to reach for it — a title the template proposes and a hand
may change (`newDrawingModel.mjs`), and *Where* only when the tab offers several places; *Create*
makes it and opens it. A drawing
open in the panel has a header bar — back, its title, its status, **Save**, *Ask an agent* (the agent
glyph), **maximize**, delete and the panel's × (which closes the panel and keeps the drawing open in it,
as the list's × does) — and the
canvas; maximized, the panel fills exactly the content column (everything but the header, the footer
and the sidebar), a box `App.tsx` publishes from its content column's resize observer — the one frame
`shell/maximizedPanel.ts` gives the notes overlay too, and one header button, `shell/MaximizeToggle.tsx` —
and Escape restores it once the canvas has nothing of its own to cancel. The Ask drawer is the
**conversation drawer** every document has (`views/_studio/ConversationDrawer.tsx`, the notes
overlay's too): a conversation whose origin is the drawing (`ConversationOrigin::Drawing { id }`;
[13](13-conversations.md)) — the same timeline, streaming, mentions, addressee pill and *Stop* as
every conversation, started in one click and untitled — the one surface every owner paints
(`ConversationSurface`; [13](13-conversations.md)) — in a column resized by its left edge and
remembered (`bisa.draw.askwidth`), its pick kept per drawing (`bisa.conversations.pick`) so the drawer
reopened lands on the conversation left there — with the drawing chosen for every drawing tool the agents call
in it. Deleting the drawing takes its conversations with it.

---

## The agents' tools

Every session holds seven tools (`bisa_core::draw::DRAW_TOOLS`), the one list the MCP server, the
Drawing skill and [reference/mcp-tools.md](../reference/mcp-tools.md) are checked against, and every
session reads one sentence about them (`DRAW_NOTE`) beside the browser's in its framing and the MCP
server's instructions. How to compose a picture a person can read is the **Drawing** skill's
(`library/catalog/skills/drawing.toml`), which every catalog agent carries beside Embedded Browser
— the two platform skills, so a catalog agent's cap is six.

| Tool | Answered by | What |
|---|---|---|
| `drawing_list` | the engine, from the store | the drawings of a scope, or every drawing — id, title, where filed, element count; never a scene |
| `drawing_read` | the engine | the reading (`Scene::describe`): one line per drawn element — id, type, text, place, size — an arrow with its two ends, a frame with its name, a label on its box; cut at 16 KiB |
| `drawing_create` | the engine | a new empty drawing with a title, filed under the named scope, else where the conversation stands — its goal, project or workflow — else the goal the session serves, else — for a session of a run of the workspace, which serves no goal — the workflow that run runs, else the workspace |
| `drawing_draw` | **the desktop** | skeleton elements laid out by the canvas (`convertToExcalidrawElements`, the agent's ids kept), appended or, with `replace`, drawn alone |
| `drawing_mermaid` | **the desktop** | a Mermaid text laid out into real shapes and arrows (`@excalidraw/mermaid-to-excalidraw`): a flowchart, a sequence, class, entity-relationship or state diagram. Every other kind converts to one image of the rendered diagram, which a drawing never keeps — refused in a sentence that names the kinds that draw |
| `drawing_erase` | the engine | elements removed by id; a label goes with its box, an arrow that ended on a removed box is left loose at that end, a child of a removed frame stands free (`Scene::erase`) |
| `drawing_snapshot` | **the desktop** | a PNG at `draw.snapshot.width`, uploaded as an attachment; the engine answers the path of a named copy the agent reads with its own tools |

A call that names no drawing means **the conversation's** — the engine reads the conversation's
origin and chooses it, server-side, so the MCP client's scope did not change and the agent leaves
`drawing` out. Outside a conversation about a drawing, a call naming none is refused with the two
tools that find or make one.

### What the desktop performs

The engine has no canvas. A skeleton, a Mermaid text and a snapshot need one — text is measured with
the canvas's fonts, an arrow's points are computed by the canvas, a PNG is rendered by it — so those
three are **parked** on the engine's drawing desk (`parked::Desk<PendingDrawRequest>`, the same
mechanism the browser bridge of [ide/18](ide/18-browser-and-servers.md) parks on) and put on the bus
as `drawing_request`. The desktop hears the frame (and reads `GET /drawings/requests` when it opens
and every twenty seconds while open, which is how the engine knows a desktop is home), performs the
request in the **live canvas when the drawing is open** — the person watches the agent's shapes land
and the view scrolls to them — or in an **offscreen canvas** otherwise (chosen as late as it can be,
after the shapes are laid out, so a drawing the person opens meanwhile is drawn where they watch),
saves through `PATCH /drawings/{id}` with the hash it read — the open canvas's record when it drew
there, two writers saving through one canvas, else the hash of the detail it read
(`drawRequestModel.saveBase`) — and answers `POST /drawings/requests/{id}`. The waiting tool call
returns; the engine announces `drawing_changed` with the new hash — a snapshot, which changes
nothing, announces nothing. A 409 on that save means somebody drew meanwhile
(`drawRequestModel.conflictStep`). **Offscreen**, the store's **scene** is re-read, the agent's
shapes merged into that scene, drawn again and saved at its hash; a second 409 is answered as a
refusal that tells the agent to read and draw again. On the **open canvas** the bridge gives way at
once: its shapes are taken back off, the agent is told the drawing moved, and the canvas is left to
its own editor — nothing a person drew is replaced by an agent's retry. Any save that does not land
takes the agent's shapes back off the open canvas, so what a person sees is what was saved or what
they drew. An offscreen save moves no open canvas's record: a drawing opened mid-way hears the frame
as somebody else's and adopts it. The offscreen canvas is loaded with the drawing's own ground and
grid, and let go of after two idle minutes.

**Every request is answered, and a request lasts as long as its asker.** On the desktop
(`drawBridge.ts`): work that throws is a refusal; work that outlasts fifty seconds is answered as
too slow, with what to do next, while the queue still waits for it to end — two requests on the one
offscreen canvas would save into each other's drawings; no wait is open-ended (a window hidden to
the tray paints no frame, so the layout wait ends on a clock too, and a canvas that never mounts
fails its request); an answer the node did not take is sent again; a request read off the list
after the engine stopped waiting for it is not drawn at all. In the engine: a request is on the desk
only while its call waits (`Desk::hold`) — a session that **hangs up**, its agent stopped, withdraws
it (`intake::unless_hung_up`) — and the MCP client sends a canvas request **once**, on a connection
of its own (`IntakeClient::call_apart`), so an engine that went away mid-request never has it drawn
twice and the session's other tools are not held behind it. **What the canvas answers is its word
on whether it drew and on the picture it took**; the drawing, its title, its hash and its element
count in the reply are read from the store (`drawings::canvas_result`), and a refusal is cut to
4 KiB.

No desktop heard from within 45 s means **nobody home**, said at once and before anything is parked;
a desktop that was home and did not answer within 60 s is **silent**, a different sentence with a
different thing to do about it: read the drawing first — the canvas may have saved after the wait —
then try once more. A save the desktop finished after that wait is still **announced**
(`drawings::answer`): its answer finds nothing waiting and is a 404 to the desktop, but a result that
says the canvas drew re-reads the drawing and emits `drawing_changed` all the same, so the canvas
and the list learn what the store holds. A drawing landing through the receiving door is announced
the same way (the store's `RemoteDrawingArrived`, relayed by `drawings::spawn_listener`). A deleted
drawing is announced with `removed`, which a change never carries. Both refusals are one
sentence the skill tells the agent to say to the person once and stop at.

### Who may draw

Three settings, read per op and per project where a project says so:

| Key | Values | Scope |
|---|---|---|
| `draw.enabled` | on · off — the machine's switch; off, the panel, its dock and the footer's switch are not drawn, no request is served, and the tools say so | machine |
| `draw.agents` | `everyone` (default) · `assigned` — agents carrying the Drawing skill; the General Agent and the Workflow Agent always · `nobody` | workspace · project |
| `draw.snapshot.width` | 320–4096 px, 1280 by default — the picture `drawing_snapshot` renders | machine · workspace |

The refusal is a sentence naming the switch or the policy (`OFF`, `NOBODY_MAY`, `NOT_ASSIGNED`) and
comes before a list is read or a request is parked, as the browser's does. **The project that
speaks is the one the drawing is filed under** (`drawings::policy_project`): the drawing named, else
the conversation's drawing, else the project a new drawing or a listing names as its scope. Only a
call that names no project answers to the project its conversation stands in — so a project that
lets no agent draw is not drawn into from a conversation about another.

---

## The repository

`drawings/` is a git repository the person commits and pushes from the panel's strip, made lazily
by the first drawing and kept by `folder_git::FolderGit` — the one module both the notes' and the
drawings' repositories run through (`Folder::Notes` · `Folder::Drawings`), over `inner.git()` and the
safe tier alone: `add --all` and `commit`, `push` with the upstream set, `fetch`, the remote's URL,
the local identity. **No Publish gate** — the folder belongs to no project and no goal, and the only
hand that pushes it is the person's, on a button. The strip is one component too (`shell/repo/RepoStrip`),
handed the routes under `/notes/git…` or `/drawings/git…`.

The drawings' repository offers **no pull**. The files are the records' export, rewritten by the
next write, so a pull that changed a file would be undone the moment the record moved — and would
bring no record with it. The route is not mounted, and `FolderGit::pull` refuses with `PullNotOffered` for the folder.
Notes keep their consented fast-forward pull, since a note has no other way to arrive.

---

## Invariants

| Invariant | Held by |
|---|---|
| A drawing is a signed snapshot of kind 33401; the `.excalidraw` file is its export and is rewritten by every write and every ingest; a save whose file cannot be written has still saved | `crates/bisa-store/src/drawings.rs` (`a_save_whose_file_cannot_be_written_has_still_saved`); `crates/bisa-store/tests/it/drawings.rs` |
| A deletion waits its turn behind a write, and goes by the row: a drawing whose snapshot will not read is deleted like any other | `drawings.rs` tests (`a_delete_is_never_undone_by_a_save_that_was_in_flight`, `a_drawing_whose_snapshot_will_not_read_can_still_be_deleted`) |
| A write is checked as a write — one element per id, drawable numbers, followable links — and what was stored before still reads and deletes | `bisa_core::draw` tests; `drawings.rs` tests (`a_write_is_checked_as_a_write_…`) |
| A drawing from another machine is checked before it is stored, and said only when it landed | `ingest.rs` (`ingest_drawing_snapshot`); `drawings.rs` tests (`a_peers_drawing_is_checked_before_it_is_stored_…`) |
| A scene is vector only, under 768 KiB and 4000 elements, every element typed, placed and named; an image is refused by name wherever it is written | `bisa_core::draw` tests; the store's and the node's `drawings` tests |
| A scene change is compare-and-swap on the store's hash under the one writer's lock (`drawings_writes`); a stale one answers 409 with the current scene; a title or a pin needs no hash; a write that changes nothing writes nothing and keeps the hash | `Workspace::update_drawing`; `owner::conflict_or`; `crates/bisa-store/tests/it/drawings.rs` (`a_save_of_the_same_scene_writes_nothing_and_keeps_the_hash`) |
| A late answer after the engine's wait is still announced, and a peer's drawing arriving is announced to the canvas | `drawings::answer`, `drawings::spawn_listener`; `crates/bisa-engine/tests/it/drawings.rs` (`a_late_answer_after_the_wait_is_still_announced`, `a_peers_drawing_arriving_is_announced_to_the_canvas`) |
| The canvas is live from its first change, never from the API hand-over; the record is the open canvas's and an offscreen save moves none of it; a reload the canvas cannot perform is owed; the bridge re-reads the scene on a 409; the overlay hands the open drawing's frames to its editor | `desktop/src/draw/liveScene.ts`, `autosaveModel.test.mjs`, `drawRequestModel.test.mjs`, `scenarios/draw.test.mjs` |
| A listing never carries a scene | the `drawings` table's columns; `list_drawings` |
| Who may draw is checked before anything is read or parked; the skill is the assignment; the General Agent and the Workflow Agent always may; the project that speaks is the one the drawing is filed under | `Access`, `policy_project` in `crates/bisa-engine/src/drawings.rs`; `crates/bisa-engine/tests/it/drawings.rs` (`who_may_draw_is_the_word_of_the_project_the_drawing_is_filed_under`) |
| A request is on the desk only while its asker waits; an answer nobody is left to hear is not taken; a canvas request is sent once | `parked.rs` tests; `drawings.rs` (`a_request_whose_session_hung_up_is_withdrawn_from_the_desk`); `bisa-mcp/tests/it/intake.rs` (`a_canvas_request_…`) |
| A canvas's answer is held to the store's facts; a snapshot announces nothing; a deletion is said as one | `drawings.rs` (`a_canvas_answer_is_held_to_the_stores_facts_…`, `a_drawing_nobody_can_read_is_deleted_and_a_deletion_is_said_as_one`) |
| What is saved is the scene the canvas last reported; a 409 leaves the canvas unsaved; a refused save is not sent again until the canvas changes; a read never replaces a canvas drawn on meanwhile | `desktop/src/draw/autosaveModel.test.mjs` |
| Every request is answered within the engine's wait; no wait hangs on a hidden window; an agent's lost save never replaces the open canvas | `desktop/src/draw/drawRequestModel.test.mjs` |
| Nobody home is said at once; a parked request answered lands with `drawing_changed` and its hash | `parked::Desk`; `crates/bisa-engine/tests/it/drawings.rs`; the journey `crates/bisa-cli/tests/it/e2e/addons_drawings_and_notes.rs` (through the real MCP server, no window open: a reading, a new drawing and an erasure answered, a skeleton refused at once) |
| A call naming no drawing means the conversation's; a new drawing in a conversation about a goal is the goal's | `drawings::Standing`; `crates/bisa-engine/tests/it/drawings.rs` |
| The drawings repository excludes `state/` through its own `.git/info/exclude` and offers no pull | `folder_git.rs`; `crates/bisa-engine/tests/it/folder_git.rs`; `crates/bisa-node/tests/it/drawings.rs` |
| Every catalog agent carries the Drawing skill; the seven tools are named in the note, the skill and the reference | `catalog.rs` tests; `bisa_core::draw` tests; `bisa-mcp/tests/it/docs.rs` |
| 26 GEP kinds, 14 addressable, four holes, two reissued, 33416 free | `kind.rs` tests |

## What is open

| Open | Why it stands |
|---|---|
| **Drawings do not sync.** No transport sends a drawing's snapshot; the repository is the way off the machine, and its files do not come back as records | A decision of the hardening pass (2026-11): say it as it is. Sending is a feature with conflict rules to design — two machines editing one drawing meet as last-writer-wins on `(revision, created_at)`, which loses strokes silently |
| **Any reader of `GET /drawings/requests` counts as a desktop.** A client that reads the list and performs nothing makes the engine park requests it then times out, instead of saying *nobody home* at once | The route is behind the workspace token; a second presence signal was not built |
| **The store's file and git work run on the request's thread**, for notes and drawings alike — a save of a large scene, or the repository's status, holds an async worker for its length | A performance refactor across every route of both; left out of a pass that changed behaviour |
| **A scene's caps are the node's to say.** The canvas does not measure a scene against 768 KiB and 4000 elements before it sends it; the refusal comes back from the node, is said once and is not sent again | The canvas's own count would have to track the store's serialization to be right |
| **A false conflict when an agent's save moves the open canvas's record** between the editor reading its base and its `PATCH` landing: the editor freezes although the "somebody" saved this very canvas | Rare (two saves through one canvas within one request); *Keep mine as a new drawing* loses nothing |
| **Not probed in the packaged app**: *Save as image* writing a file, a link on an element opening, Excalidraw's own dialogs over a live browser tab, ⌘K with a selection, focus on opening and closing the panel | They need the real webview; on the by-hand list |
| **A drawing skipped by a rebuild has no row** — a snapshot that will not read is left on disk and listed by nothing, so it can be removed only by hand | The rebuild names it as a workspace problem with its path |

## Tests

`crates/bisa-core/src/draw.rs` (the bounds, the reading, the erasure, the skeleton), `kind.rs`;
`crates/bisa-store/tests/it/drawings.rs` (every scope, the snapshot and the file, the conflict, the
bounds, the rebuild, the cascades, a peer's record); `crates/bisa-engine/tests/it/drawings.rs` (the
op end to end over a socket: data from the store, a skeleton parked and answered with the bus told,
nobody home, the policy, the conversation's drawing and scope, an erasure's repair) and
`tests/it/folder_git.rs` (both repositories); `crates/bisa-node/tests/it/drawings.rs` (the routes, the
409's shape, the parked list, the repository without a pull); `bisa-mcp`'s tool and reference guards;
the desktop's `draw/*.test.mjs` and `scenarios/draw.test.mjs`.
