# 03 — Files and editing

The write path, the watcher, large files, the explorer's mutations, and the layout that survives a
restart.

---

## Two writers on one file

An agent writes into a workstream while a person has the same file open. The platform solved this
shape once for notes and reuses it here — send the hash of the body you read, and a mismatch is
refused with the current body attached.

The IDE reuses that answer verbatim. **Compare-and-swap, never last-write-wins.**

```
GET /ide/file/{scope}/{id}?path=<root-relative>
→ 200 { path, size, hash, binary, truncated, text }

PUT /ide/file/{scope}/{id}?path=<root-relative>
   { text, base_hash }                  base_hash: sha256 of the text you read
→ 200 { hash }
→ 409 { error: "the file changed since you read it", current_hash, current_text }
```

- `hash` is `sha256` of the bytes — the dependency already in the tree, and the same function
  `store/notes.rs` uses. No new hash.
- The `409` **carries the current text** so the client needs no second round trip before it can
  merge. That is the one place in the IDE API where a failure body is not just `{error}`, for the
  same reason it is in the notes API: a sentence is not something a client can merge from.
- On `409` the editor opens a three-way merge — base, theirs, mine — in the diff editor, with the
  person's buffer untouched until they choose.
- A `PUT` with no `base_hash` is a **create**, and is refused with `400` (*already exists; send
  the base_hash of what you read*) if the path already exists. There is no way to overwrite without
  saying what you read. This is the save of an **untitled document** (§Tabs below): its buffer has
  no hash, so naming it a taken path is refused rather than clobbering.

Every mutation route — create, rename, move, delete, and the write above — is an
`engine::ide::files` function. The node parses and renders; it decides nothing. A Keep or an Undo of
an agent's change to a checkout goes through this same door: `write_bytes` — the same atomic,
compare-and-swap write, bytes rather than text so a picture's Undo is exactly as safe as a source
file's — is what `engine::changes::settle` calls to land a review's verdict, and a delete of a file
the agent created goes through the same `Disposal` an explorer delete does
([20 — Reviewing agent changes](20-reviewing-agent-changes.md)).

A mutation that is refused writes nothing. A move is refused before a folder is made for it: of
the root, of nothing, onto something that exists, and **into its own subtree** — the parents of
the new place used to be made first, inside the very tree that was about to move, and left behind
by the refusal. A rename that only changes a name's case (`Readme.md` → `README.md`) is a rename on
a disk that folds case too: there the new name resolves to the file itself, and the name asked for
is what it is renamed to. A folder that holds something — or nothing — is deleted only with
`recursive`, and the refusal counts what it holds. What is saved is what was given, byte for byte:
a mark at the head, CRLF, no final newline, nothing at all, and bytes that are no text — the hash
is of those bytes, so the next save's compare-and-swap is against the truth.

### Writable roots

Stated in [01](01-trust-boundary.md#writable-roots) and enforced in one place:
`engine::ide::files::writable_root(scope, id)`. A path is resolved through `resolve_within` against
that root — canonicalised, never prefix-matched — so `..`, an absolute path and a symlink out of the
tree are all one refusal.

---

### Loose files

A file from anywhere on this machine — dropped from the file manager onto the workbench, picked
with `⌘O` (*Open a file from this machine*), or opened from a link card's *Open in the IDE* on an
absolute path outside every root — is a **loose document**: a tab `loose:<absolute path>` in the
root on screen, kept, never a preview, saved and restored in the layout, under no project, goal or
group. Its blast radius is the machine, so **the shell reads and writes it, never the node**
([01](01-trust-boundary.md)): `src-tauri/src/loose.rs` — `read_loose_file`, `write_loose_file` —
keeps the editor route's rules byte for byte (a NUL or invalid UTF-8 is binary, above 20 MiB
refused with the size and the bound — `LooseError::TooLarge { size, limit }`, tagged as the node's
413 is, so the editor says both and offers *Reveal* — above 2 MiB read-only, `hash` the same
sha256, compare-and-swap with the current text in the refusal, an atomic write), and its tagged
refusals are raised on the desktop as the same `ApiError`s the node's routes produce
(`apiModel.looseRefusal`: `conflict` a 409 with the current hash and text, `missing` a 404,
`too_large` a 413 with `size` and `limit` — `desktop/src/apiModel.test.mjs`,
`desktop/src/views/_workbench/editorModel.test.mjs`), so the editor merges a loose conflict exactly
as a root file's and says a loose file's size against the bound with *Reveal* as it does a root
file's. The editor reads and writes through one seam, `docBackend.backendFor(source)` — the
node for a root file, the shell for a loose one, nothing for an untitled one — and the backend's
two flags gate what is a root file's alone (the watcher, the language server, blame, Files) from
what any file on disk has (a read on mount, `⌘S` in place, autosave). No watcher follows a loose
file: a change under it is learnt at the next save, as a conflict.

Three identities keep the workbench's render tree acyclic, and each holds on its own. The centre
shows the tab **as the store holds it** (`workbenchModel.tabIn(docs, id)` — the stored object, or a
parse for an id not adopted yet), never a fresh parse per render. `EditorDoc` keys its backend on
what the source *is* — its tab id — not on the object, so a parent that hands a new `{kind, path}`
costs no re-read and no new `save`. And the editor registers **one** handle per key with
`editorRegistry`, reading the latest `save` and path through refs, because a registration notifies
every subscriber — the workbench among them — and `useDirtyEditors` hands out the same `Set` while
nothing is dirty or clean anew. A registration that followed a closure's identity was a loop
(React's *maximum update depth*), and every text-like document opened from Files hit it.

**Where the paths come from.** The window keeps HTML5 drops (`dragDropEnabled: false` — the
composer's attachments, the New Goal documents and the workflow palette depend on the DOM seeing
them), and the DOM's `File` has bytes and no path. On macOS the drag pasteboard still holds the
dropped file URLs when the DOM's `drop` fires; the shell reads them (`dropped_paths`) and
`dropModel.pathsForDrop` pairs them with the DOM's names, in the DOM's order, each once. A drop
that pairs nothing (another platform, a drag that was not files) opens nothing and points at
`⌘O`. The workbench's root element takes the drop; the composer's own drop has already claimed
its event and keeps its attachments.

**A copy made in the file manager pastes into the tree.** The general pasteboard holds file URLs
the same way the drag board does, and the shell reads them the same way (`pasteboard_holds`); the
explorer's *Paste* with nothing on the tree's own clipboard takes them (`useTreeMutations.paste`
→ `pasteInto`). The copy is the shell's (`src-tauri/src/pasteboard.rs::copy_all`, ide/01 — an
absolute path never reaches the node): into the folder's absolute path (`osPasteModel.pasteDestination`
over the root the listing said), each entry under a free name (`free_name`: Finder's *name 2*,
*name 3*), a folder whole; a symlink, a nested `.git` and a folder pasted into itself are left out
and named in the report, which `pasteWords` turns into the toast. The node's watcher sees what
landed and announces it as any outside write, so every window's tree refreshes. What the machine's
clipboard holds — files, a picture — is `osClipboardStore`'s word — read when the window comes to
the front and as a menu opens — and the menu's label says *Paste from the file manager* or *Paste
the picture* while one of them is the source (`fileTreeMutations.menuSpec`, `clipboard: "tree" |
"os" | "image" | null`; `osPasteModel.pasteSource` puts files before a picture, since a copy of an
image file in the file manager is a file with a name of its own).

**A picture on the clipboard pastes into the tree, named.** A screenshot or a copy from a picture
app leaves `public.png` or `public.tiff` on the general pasteboard and no file URL. The explorer's
*Paste* then opens a **draft row** under the folder — the same row a new file is named in — pre-filled
with `pastedImageName()` (`pasted-image-<stamp>.png`, `ui/pastedImageModel.mjs`), the stem selected.
The name is checked as typed (`imageNameError`: the tree's rule plus the store's — no dot-file — over
the folder's children; `withExtension` gives a bare name `.png`), and `Enter` asks the shell
(`paste_image_into`, `src-tauri/src/pasteboard.rs`): it reads the picture *then* — its PNG as it
is, else its TIFF encoded through `png.rs`, the browser snapshot's encoder — and `write_png` writes
it under the confirmed name into the folder's absolute path, refusing a taken or improper name in a
sentence that stays under the row. The bytes never cross the IPC: the node learns of the file from
its watcher, like any outside write. The toast is `pasteWords` over the one entry. Outside the
desktop shell there is no picture to paste; a checkout with no place on this machine has nowhere
to write it.

**Save to….** A document that is not the root's — untitled, or loose — carries a *Save to…* menu:
*Into this project…* asks for a path relative to the root (`savePathProblem`) and creates it
through the node (no hash, refused if taken), *Elsewhere on this machine…* opens the OS's save
dialog and the shell writes there — a path the dialog offers that already holds a file was
confirmed there, so its hash is read first and the write carries it, never a clobber. Either way
the tab becomes the saved file's, in place (`replaceTab`). A root file keeps plain *Save*.

## The watcher

Without an editor the platform needs no filesystem watcher: the file tree refreshes on engine
events and polls while a run is live, and a `project` start looks at a project on the event
ticker. An editor changes the calculus —
a person looking at a buffer needs to know within a second that an agent changed the file under it.
The watcher is `notify`, scoped and bounded:

| | |
|---|---|
| watched | the roots of projects and workstreams currently open in a workbench, registered by the client with `POST /ide/watch/{scope}/{id}` and released on close or after a 5-minute silence |
| debounce | 50 ms per path; events are coalesced so a save that writes a temp file and renames produces one `Modified` |
| ignored | `.git/` internals except the closed list the git panel shows — `HEAD`, `index`, `packed-refs`, `FETCH_HEAD`, the operation markers (`ORIG_HEAD`, `MERGE_HEAD`, `REBASE_HEAD`, `CHERRY_PICK_HEAD`, `REVERT_HEAD`) and everything under `refs/`, `rebase-merge/`, `rebase-apply/` and `sequencer/`; `objects/` and `logs/` stay dropped, since one commit writes dozens of objects and one `HEAD` frame is the whole message; paths matched by the root's `.gitignore` are watched but flagged `ignored: true` so the explorer can dim them |
| delivered | on the `engine` stream: `FileChanged { scope, id, path, kind: Created \| Modified \| Removed \| Renamed{from} , ignored, dir }` — `dir` says the path is a folder: a whole tree made, moved or copied in one frame, which a reader holding a list of *files* cannot patch from the name alone; `false` for a path that is gone |
| on overflow | the OS queue overflowing is reported as `FileChanged { kind: Rescan }`, and the client refetches the open directories |

What the client does with a frame:

- an **untouched** open buffer reloads silently;
- a **dirty** buffer raises a non-blocking *changed on disk — review* affordance beside the tab,
  which opens the same three-way merge a `409` does;
- the explorer patches the row in place; the git panel refreshes its status.

A `project` start or wait with `change = "files"` does not use this watcher: a listener runs
unattended, whether or not anybody has the project open, so each tick of the event ticker reads
the project itself — a git project's status (`.gitignore` honoured), a plain folder's bounded scan
(`events.files.max_depth`, `events.files.max_entries`) — and compares it with what the listener
last saw. The IDE watcher runs on roots a person has open. Different bounds, different mechanisms,
and [the guide](../../guide/events.md#project) says so.

---

## Read caps and large files

`GET /file` (the inspector's route) keeps its 256 KiB cap. `GET /ide/file` calls the same
`Workspace::read_file` with a caller-supplied `ReadCap` — one implementation of the read, the
containment check and the binary sniff, two callers. The editor's policy, with its two bounds as
nobody set them — **this machine moves both** (Settings › Project IDE › Editor: `editor.large_file.editable_mib`,
1–8, and `editor.large_file.refuse_mib`, 8–64; `engine::ide::files::EditorCaps`, read where a file is
read and where it is saved, so a change holds from the next request). A loose file is the shell's
and keeps the two as written here; what a review keeps of a change has its own bound
(`MAX_CHANGE_BLOB`):

| Size | Behaviour |
|---|---|
| ≤ 2 MiB | editable, tokenised — and saved whole: the save route's body is sized from the most the bound may be set to (`WRITE_BODY_BYTES`, twice that text and a hash, since every byte may need a JSON escape), and `PUT /ide/file` refuses a text over the bound in force with the read side's own 413 `{size, limit}`. Above 2 MiB a file this machine calls editable is edited and drawn plain |
| 2–20 MiB | opens **read-only**, tokenisation off, a banner saying why — the node's word (`editable: false`, its reason kept on the buffer as `readOnlyWhy`); the editor decides nothing about size itself (`editorModel.loaded`, `sizeNote`) |
| > 20 MiB | refused; the response carries the size and the limit **this machine set**, and the client's sentence says both (`editorModel.loadFailure` reads the 413's `{size, limit}`; `fileDocModel.tooLargeWords(size, limit)` for a rendered document) and offers *Reveal in Finder* (the platform's word, `revealLabel`) — a save over the bound in force is the same sentence with the node's limit (`saveFailure`) |
| binary (sniffed from bytes, never from the extension) | never served as text; the editor shows *Binary — N bytes, and no text to show* with *Reveal in Finder*. A file whose **name** says it renders — a PDF, a picture, a recording, a sheet, a document, a deck — never reaches this route: the workbench opens it as a **rendered document** from its bytes (below) |

Monaco degrades badly past roughly 10 MiB and pretending otherwise is how an IDE earns a reputation
for hanging.

---

## Rendered documents

A file opens one of three ways, decided **from its path alone** by `views/_workbench/fileDocModel.mjs`
(pure, tested) over the artifact kinds' extension table (`artifactModel.kindOf`, the mirror of the
Rust `ArtifactKind::of` — one table, so a PDF is a PDF whether an agent posted it or a person
opened it; the desktop's test reads the core's table from `artifact.rs` and holds the two equal,
extension by extension and media type by media type. A name with an extension is decided by it
alone — one the table does not know is a `file`, a card, whatever type was declared beside it — and
a bare name, a dotfile or a trailing dot by its type):

| Kind | How it opens | Modes |
|---|---|---|
| markdown · diagram · html | the editor, rendered — a page **of the buffer as typed**, in the same sandbox an agent's page runs in (`PageFrame`, `artifacts.html.libraries`) | *Rendered · Split · Source* (a diagram says *Preview*); a page opens on *Source*, since a repository's HTML rarely carries its assets |
| sheet by text (csv, tsv) · svg | the editor, with a rendered view **of the buffer as typed** — the grid, the figure | *Rendered · Source* |
| pdf · sheet by bytes (xlsx, xls, ods) · document (docx) · slides (pptx) · image · video · audio | a **rendered document** — `RenderedFileDoc`, no editor behind it | *Rendered* |
| everything else | the editor | — |

`docKindOf(path)` answers the kind, `docModes(kind)` the modes, `defaultMode` the one it opens on,
`isRenderedDoc` whether there is an editor at all; `docLink.isMarkdown` reads the same table. The
mode control is glyphs alone — an eye for the rendering, brackets for the source, two columns for
both (`modeGlyph`, `SegmentedControl`'s `iconOnly`) — with the word as the tooltip (`modeLabel`),
and the mode a document is in is remembered per document in the checkout's session (`docModeKey`,
`useSessionDraft`), never in the layout. The control sits at the right end of the bar in every
mode: both of `EditorDoc`'s bars draw one right-hand cluster after the one spacer — the find bar,
the mode glyphs, the wand, then Blame and Save where those are — so it keeps its place whether the
crumbs stand on the left or nothing does.

**A fourth thing the editor can show.** A file with a pending change from a conversation's agent can
show a **review lens** beside Rendered, Split and Source — the hunks of
`GET /conversations/{id}/changes/file`, cut against the file's own base, kept or undone in place
through the review's own routes rather than the compare-and-swap write above
([20 — Reviewing agent changes](20-reviewing-agent-changes.md)).

**The bytes.** `GET /ide/raw/{scope}/{id}?path=` serves a file as it is: contained by the same
`resolve_within` every file route applies (`Workspace::file_path`, `engine::ide::files::raw_file`),
refused with 413 `{error, size, limit}` above **`RAW_REFUSE_BYTES`** (256 MiB), and typed by the
crate's one rule for bytes into a webview with no CSP (`attachments.rs`): an image is recognised
from its **header** and served inline as that; everything else — a PDF, a page called `.png`, plain
text — is `application/octet-stream` with `nosniff` and `Content-Disposition: attachment`, never a
page, whatever the name says. The desktop fetches it **with the bearer header** into a `Uint8Array`
(`api.ideRaw`, `useIdeFileBytes` in `ui/artifact/fileBytes.ts`) and makes a blob URL where a
renderer wants one — an `<img>`, a `<video>`, an `<audio>`, which seek a blob natively — so nothing
in the webview points a `src` at the node, and no `Range` route is needed. Nothing is cached across
documents: a `file_changed` frame for the path reads it again; the blob URL is revoked on leave.

**The document.** `RenderedFileDoc` is chosen by the workbench's `renderDoc` before the editor is
asked anything — a PDF is never read as text and never sniffed. Its toolbar is the editor's grammar:
the path as crumbs, the kind's word and glyph, the size, the fact the renderer learns (*12 pages*,
*120 rows × 6 columns*, *9 slides*) and **Reveal in Finder** (the platform's word); its body is
**`KindView`** — the one renderer per kind the artifact viewer draws ([12](../12-artifacts.md)):
pdf.js page by page, SheetJS as a grid with a tab per sheet, mammoth as prose, the deck's outline,
the picture, the recording with controls. A file above the cap says so with the size and the limit the node answered (`fileBytesModel.bytesFailure` keeps the 413's two numbers; `tooLargeWords`) and reveals; a
recording WebKit cannot play says so in `MediaView`'s words. A rendered document is never an
editor: it registers no line requests and has no dirty state.

**A relative image in a rendered Markdown file draws.** `Markdown.tsx` keeps a relative `src` as
`data-doc-src` under `relativeLinks`; `EditorDoc`'s rendered view resolves each against the
document (`resolveDocLink` — one that climbs out of the root stays blank) and reads it through the
byte route into a blob URL ([17](17-links-and-paths.md)).

**The glyphs.** `ui/fileIcons.mjs` knows the rendered kinds — pdf, video, audio, sheet, document,
slides — so the tree and the tab wear the artifact kinds' marks.

**Bounds.** No `Range` streaming — a blob URL seeks, and a file above 256 MiB is revealed, not
rendered. A legacy `.doc` has no renderer (reveal); `.xls` and `.ods` read as far as SheetJS
Community Edition reads them. A repository's HTML renders in the agent page's sandbox, without its
relative assets — the Browser menu serves the checkout so the same page renders whole in the IDE's
browser, and is annotated there too ([18](18-browser-and-servers.md)). A rendered document cannot be edited here
— the bytes are the file's.

**Annotate.** On a page in a workstream, the wand on the bar (`ICON.annotate`, in *Rendered* and
*Split*) turns the rendering into an inspector (`PageAnnotator.tsx` over `annotationModel.mjs`):
hover outlines the element under the pointer with its tag, as an inspector would, a click opens a
box over it — *What should change here?* — and the answer becomes annotation *n*: a numbered badge
on the element and a line in the tray under the page (`AnnotationTray.tsx`). The inspector is in
one of two modes (`INSPECT_MODES`, said to the frame as `{mode}`): *off*, the page's own, and
*picking*, the pointer outlines and a click picks. **The box is the frame's own**: the inspector
core draws it over the clicked element — the crumbs, the element's text, the question, *Add* or
*Change* when the element already carries a note (the badges ride with their notes) — places it
above the element when there is room and below it otherwise, clamped to the viewport, and moves it
with a scroll; the caret lands in it because it is in the page. While it is open the pick is
**held**, the page's own state: the pointer outlines nothing and a click outside picks nothing (it
is still swallowed: a link under the box must not navigate while a note is typed) while a click
inside is the box's own; a crumb re-picks that element in place and keeps the words typed so far.
Enter or *Add* says the note with the whole pick (`NOTE {selector, tag, excerpt, text, note}`) and
closes the box; Escape or *Never mind* closes it alone and says so (`CLOSED`); Escape with no box
open leaves the inspector (`ESCAPE`, the wand off); the app's *off* closes everything. The frame
draws the outline, the badges and the box itself, because only it knows its scroll — and because
a browser tab's page ([18](18-browser-and-servers.md)) is a native view nothing of ours paints
over, so one implementation serves both: `pageDocument(html, {inspector: theme})` injects
`pageInspector.mjs`'s script — dressed in the app's theme, `inspectorTheme.mjs` over the role tokens
([18](18-browser-and-servers.md) §Annotating a page); the frame's `usePageInspector` says the theme
again (`bisa:theme`) when it moves, first after every load, rather than rebuilding the document,
which would close a box open in the page — right after the policy metas, and the two sides talk by
`postMessage` alone — the app (`usePageInspector`) accepts only messages whose `source` is the
frame's window (the origin is opaque, so `"null"` proves nothing) and re-bounds every field
(`parseInspectorMessage`: the excerpt at `MAX_EXCERPT_BYTES`, the rest by characters); the frame
accepts only its parent's. The selector is a locator hint — a unique id, else a `tag:nth-of-type`
chain to `body`, or `html` and `body` themselves — and the excerpt, the head of the element's HTML,
is the identity. **The whole document is a pick**: a click on the document element picks `html`, on
the body's own area `body`, and every pick carries the element's ancestors up to `html`
(`ancestors`, outermost first, bounded like the rest), which the note box draws as crumbs — *html ›
body › main › section › button* — each ancestor a door: picking it re-picks that element in the
page and the frame says a fresh pick for it, so the page's root, its body or any container is one
click from any element. The annotations
are a **draft of the session** on the document (`annotationsKey`, `useSessionDraft`), never a
record: **Send** posts them to the agent the chip beside it names (`AgentPicker`, the selection
toolbar's chip — the project's `agents.default`, the General Agent unless the project says
otherwise, chosen up front so one click sends; another is one pick away, and the send remembers it
for the hand-offs that lead with the last one used) to the
conversation the checkout is on — started about the workstream when there is none
([13 — Conversations](../13-conversations.md)) — as one `annotation` chip each under the edit contract (`annotationsContent`,
`annotationsTarget`, `editContent` — [09](09-agents-in-the-ide.md)), records the ask
(`agentEditsStore`) and opens the Agent pane; **Attach** puts the same chips in the pane's tray and
sends nothing; either empties the draft, and `Esc` leaves the inspector. In *Split* the frame
reloads on every keystroke and the inspector starts blank: the hook says inspect and marks again on
`load`, a box open at that moment closes, and an element the page no longer has is said in the tray
(`marked {lost}`) while its chip keeps the element as it was. Off a workstream the page is the plain
frame — nowhere for an agent to run.

**The page follows the agent.** The rendered page is the buffer, so the agent's write reaches it the
way any change on disk does: a `file_changed` for the path — one read per 300 ms burst
(`useCoalesced`), since a save is a temp file and a rename and an agent may write several times —
reloads a clean buffer and the frame re-renders; a dirty buffer is told, and in *Rendered* the banner
offers *Reload the page*, *Keep mine* and *Review* (which opens the source with the difference) where
it stands, not only in the source view. The disk holding what this editor last saved — the same
hash — is no change at all (`changedOnDisk`), so an editor's own announced save never raises the
banner under a person still typing. And the ask is followed to its end: the record the tray (or the
selection toolbar's *Edit*) left names the agent, and the roster's next move of that agent's session
in this workstream from a live state to a settled one (`agentEditsModel.settles` —
`starting · thinking · running · waiting` to `idle · done · failed · aborted · parked`, entered after
the ask) reads the file again whether or not a watcher frame arrived, and says how it ended
(`settledWords`): *finished with index.html — keep or undo the change in the conversation, or in the file*, or that
it failed or was stopped.

---

## Autosave, dirty state, closing

- **Autosave** is delay-then-CAS. The delay is the `editor.autosave.delay_ms` setting (floor 300 ms —
  below that the timer fires between two words and every fire is a request). `editor.autosave.mode`
  is `off | after_delay | on_focus_change`. It never runs for an untitled document: that save asks
  for a path, and a dialog that opens itself mid-sentence is worse than an unsaved buffer.
- **The buffer is the tab's, not the editor's** (`docBuffersStore.ts`). A pane draws one tab at a
  time, so an editor that is not on screen is not mounted; a buffer kept in the editor's own state
  left with it — the text typed, and the fact that there was any — and a tab could then close, or
  the app quit, with no question. So each open document's `Buffer` lives in a module store under its
  registry key (`editorKey`: `<root>|<tab id>`) from the first keystroke until its **tab** closes:
  `useDocBuffer` is what the editor reads and writes, and a document that comes back on screen keeps
  its unsaved text exactly as typed — the read that follows compares it with the disk
  (`editorModel.remounted`, then `changedOnDisk`), so a file an agent wrote meanwhile raises the
  three-way affordance rather than replacing what was typed. A buffer is forgotten in one place —
  `workbenchStore`'s `set`, which every change to the tabs passes through, over
  `workbenchModel.closedTabs` — so a close, a replace (an untitled document saved under a name) or a
  root past the cap forgets it, and a tab that merely changed pane forgets nothing; a rename on disk
  carries the buffer to the new path (`moveBuffers`). **The cap on roots spares unsaved work**: the
  roots holding a dirty buffer are named first (`heldRoots`) and `capRoots` evicts around them, so a
  ninth project opened never forgets the first's typed text unasked (`workbenchModel.test.mjs`,
  `scenarios/files.test.mjs`). **Reads land in order**: the editor's reads on one tab take a ticket
  (`shell/latestModel.createLatest` — `begin`, `lands`, `close`), so a slow first read never
  overwrites a fast second, and a read that answers after the tab closed writes nothing; a re-read
  that fails under typed work keeps the buffer as typed (`editorModel.loadFailed`) and says the
  failure beside it instead of blanking the screen (`editorModel.test.mjs`, `latestModel.test.mjs`). Autosave is the mounted editor's: a document
  off screen keeps its unsaved text and saves when it is next on screen, or when a close asks.
- **A document keeps its place** (`docViewStore.ts`, beside the buffers and under the same key). The
  same unmount that lost unsaved text lost where the person was: a tab came back at the top, every
  time. Three kinds of place are kept, each the way its surface allows:
  - **the code editor** — `CodeEditor` takes `initialViewState` and reports `onViewState` as it
    unmounts: Monaco's own `saveViewState()`, opaque here, so scroll, cursor, selection and folds
    come back together; a line asked for meanwhile (a search hit, a `:42`) is revealed after the
    restore and wins;
  - **every scrolling rendering** — one hook at the document's root, `ui/useKeptScroll` over
    `keptScrollModel.mjs`, not one per viewer. A scrollport says it wants its place kept where it is
    made, `data-scroll-keep="<name>"` — the Markdown box, `SheetView` (the grid sideways, the rows
    down through `VirtualList`'s `keepScroll`, the sheet tabs), `PdfView`, `DocumentView`,
    `SlidesView`, `TextView`, `ImageView` — and the hook hears its `scroll` from the root in the
    capture phase (scroll does not bubble), reading the offsets as the scroll happens and writing
    them once a frame: by an unmount's cleanup the element is detached and reads zero. On mount
    each is put back, and because a rendering grows as it loads — a PDF's pages, a sheet's rows,
    pictures — `restoreStep` is asked again on every resize and mutation until the kept offset
    fits, the grace starting when the first scrollport is there, not while the bytes are read. **It
    gives up the moment the person scrolls, clicks or types**: a restore never fights the hand on
    the wheel. `EditorDoc` (rendered and split, the identity carrying the mode and whether the
    document is drawn) and `RenderedFileDoc` use it; `keptScrollModel.test.mjs` refuses a viewer
    under `ui/artifact/` whose scrollport is unmarked;
  - **an HTML page** — a sandboxed `srcdoc` frame, whose scroll the parent cannot read and which
    reloads at the top on every `srcdoc` change: a tab come back to, and **every keystroke in
    Split**. The inspector's wire carries it (`pageInspector.mjs`): the page says `bisa:place` once
    a frame while it scrolls, and after each load the app says `bisa:place-to` with the kept
    offsets, asked again for a few frames while the page lays out (`usePageInspector`'s `place` and
    `onPlace`). The page listens to its scroll only once the app has said a place — the origin
    included — so a page of the browser tab, which carries the same core, never reports one. A
    find that is up wins over the kept place: its match is where the person is looking.
  A place is forgotten with its tab and carried across a rename on disk, in `workbenchStore`'s one
  `set` (`forgetViews`, `moveViews`), as a buffer is. It is furniture, read once at mount, so a
  scroll re-renders nobody — and it outlives the window: `docViewStore.ts` keeps a document's view
  in the view memory's documents (`bisa.view.docs`, under the document's key — the code editor's
  state, each marked scrollport's place, a page's scroll, the document's mode), made safe as it is
  read back (`docViewModel.mjs`), so a document the layout reopens after a restart opens on its
  cursor, its folds and its scroll. The code editor hands its view over a beat after the last
  cursor or scroll move as well as on unmount, which a closing window never does; a tab that is
  closed keeps nothing, and a root that is gone for good takes every document's view with it
  (`workbenchStore.forgetRootMemory`). A rendering whose text changed while it was off screen
  gets its offset back, not its passage; a diagram's pan and zoom are its own.
- **Dirty state** is the buffer's fact, read by everyone the same way (`editorModel.isUnsaved` —
  text that differs from the disk's, or a conflict nobody settled — through `useDirtyEditors`,
  `anyDirty` and `dirtyCount`): a dot on the tab, the document title, a count in the workbench
  header — of a document on screen or not. It is furniture, not URL state. `editorRegistry.ts`
  keeps only what is truly the mounted editor's: the handle that saves, reveals a line and reads a
  selection.
- **Closing a tab with unsaved work asks** — *Save*, *Don't save*, *Cancel*, the platform's own
  words for the question (`closeGuardModel.guardWords`: the document named, several counted and
  listed, an untitled one told that saving asks for its name), drawn by the one `shell/UnsavedDialog.tsx`
  the Notes and Draw editors ask through too (their words `shell/leaveGuardModel.leaveWords`, their
  guard `shell/documentGuard.ts`) — through the `TabStrip` close path,
  so ✕, middle-click, ⌘W, *Close others*, *Close to the right*, *Close all* and a delete under an
  open document ask alike. **Saving a document that is off screen brings it on screen first**
  (`editorRegistry.saveDocument`: the workbench registers how, `setRevealer`, and the save waits for
  the editor's handle): the editor's own save is the one place that names an untitled document,
  merges a conflict and says why a save failed, so there is no second save path — and one that
  cannot save stays, on screen, saying why. Closing the window, or quitting with ⌘Q, is one flow
  (`useCloseGuard`): when `desktop.confirm_quit` is on (the default) it asks first, naming what is
  running and how many documents are unsaved; then it saves every one, one at a time
  (`flushAll`), and goes — a save that fails keeps the window, with an OS notice (*Bisa — still
  open*: the window was about to go, so the word is the system's, and the log has it when the
  notice cannot be shown). The order is a model's (`shell/closeFlowModel.mjs`: `closeFlow` —
  ask, save, go, a second request dropped while one is being answered, a flow that throws still
  ending; `saveEvery` — every document tried after one fails, a save that throws logged and
  counted as failed; `holdsClose` — the window goes as the OS meant when there is nothing to ask
  and nothing to save), and the hook hands it the window's own hands. A document of a
  root that is not on screen has nobody to show it, so it counts as one that did not save. The question comes before the save, so a
  cancelled quit has written nothing. ⌘Q reaches the same flow because the shell holds Tauri's
  exit request and hands it to the webview (`bisa:quit-requested`), which finishes through
  the `quit_app` command.
- **Format on save** runs the language server's formatter when one is up and the
  `editor.format_on_save` setting is on; it never runs a program the settings did not name.

---

## Tabs, splits, session restore

The document tab strip is `TabStrip` over `workbenchModel.mjs`: one strip per root, holding
documents and the terminals rooted there side by side **in one order — the order they were
opened, newest last** (the root's `strip`: a file opened after a shell is after it, a harness
opened after that is last; a document appends itself on open, a terminal the strip has not seen
is recorded at the end when it is drawn — `reconcileStrip` — a close drops the id). A document is a file or the workstream's
*Diff against base*; the Git views are the right panel's alone and never a tab.

- **An untitled document** — `⌘N` (`new_document`, workbench scope) opens `untitled:<seq>`,
  *Untitled-1*: the same `EditorDoc` over a `DocSource` with no file behind it
  (`editorModel.untitledBuffer` — empty, clean, editable, no hash). It is opened kept and is never
  a preview (a preview is replaced by the next glance; this is a buffer somebody is about to type
  into); the read, the language server, blame and reveal are a file's and stay off. `⌘S`, the
  *Save as…* button, the close guard's *Save* and the quit guard all reach the one
  `save()`, which for an untitled document asks for a path in the kit's `PromptDialog`
  (`editorModel.savePathProblem`: relative, forward slashes, no `.`/`..` segment, a file), writes
  it with no hash, shows the node's refusal of a taken name under the field and asks again, and
  on success hands the path to the workbench — `replaceTab` makes `untitled:1` into
  `file:<path>` in place (its position, its pane, its pin), the editor remounts at the path and
  reads what was just written, and Files selects it. Cancel is `false`: the tab stays, dirty. The
  number is the lowest one free (`nextUntitledSeq`), so a closed *Untitled-2* comes back as
  *Untitled-2*. An untitled tab is the one whose id does not describe its content: it is never
  saved in the layout (below) and never adopted from a `?doc=` the store lacks — a reload steps
  the URL to what the strip holds.

- **Documents split and pin** — a root's documents sit in a pane tree
  (`paneTreeModel.mjs`, the same model the terminal layer uses; `workbenchModel.mjs` owns the
  root's tree, its focused pane and its pins). *Split right* / *Split down* on a strip with two or
  more documents moves the active one into the new pane; each pane has its own strip; a tab dragged
  onto another pane moves there; *Close pane* folds its tabs into the neighbour; nothing is ever
  dropped. A **pinned** tab sits first in its strip, has no ×, is kept by *Close others* and *Close
  all*, and ⌘W says so instead of closing it. Terminals are not in the tree: they stay tabs of the
  root's first strip, in the strip's order, and are drawn by their own layer over the centre;
  *Close to the right* closes the documents after a tab and leaves a shell alone.
- **A glance opens a preview; an act keeps it.** A single click in the explorer, or a search hit,
  opens the file as a **preview** tab — drawn in italics, titled *… — preview*, of which a pane
  holds at most one: the next glance takes its place, and the replaced one is not remembered as
  closed, because nobody closed it (`workbenchModel.mjs` § Preview tabs, `preview: {leafId: tabId}`
  on the root). It becomes a kept tab on the first edit (the dirty transition, which a save passes
  through too), a double-click on the row, *Keep open* (`⌘⌥⏎`, the tab's menu), a pin, a drag along
  the strip or to another pane, a split that takes it across, or a pane closing under it (the pane
  next door may have its own). `open_entry` from the keyboard, quick open (`⌘P`) and a `?doc=` a
  reload or a pasted link adopts open **kept** — a file asked for by name is wanted; `Back` to a
  replaced preview's URL reopens it kept the same way. Previews are never saved: the layout leaves
  them out of the tabs and of every pane, so a click through files writes nothing, and what a
  restart brings back is what you meant to keep.
- **Tabs reorder and have a menu**. The strip is a `SortableList` (`ui/dnd`): a tab dragged along
  it slides its neighbours aside and takes the slot it is dropped on (`sortModel.sortableDrop`);
  from the keyboard, Space lifts the focused tab, the arrows move it and Space drops it. A tab
  carries a `docTabDrag` payload, so another pane — its strip or its body, a `DropZone` — takes it
  and the move is `moveDocToPane`. A pinned tab moves only among the pinned, an unpinned one
  among the rest, and a drag across that boundary is refused with the words (`moveTab`). The
  root's `strip` is what a strip draws and what moves (`leafStrip`: a pane's documents and, in
  the first pane, the terminals, in that order, pinned first); the root's `tabs` list keeps the
  order things were opened in and the pane tree the membership. Terminals ride the first strip
  and are **one run with the documents**: a shell drags between two files and a file past a
  harness, and `moveTab` takes the strip as it was shown. Right-click a document tab for *Close · Close others · Close
  to the right · Close saved · Close all · Keep open* (a preview) *· Pin · Split right · Split down ·
  Copy relative path · Copy absolute path · Reveal in Files · Reveal in Finder*; a double-click on a
  tab pins or unpins it; a terminal or harness shell for *Focus · New shell
  here · Restart* (exited) *· Send the last lines to the agent · Show in the Agent panel*
  (harness) *· Terminate* (a live harness) or *Close* (a shell — *Close (ends the shell)* while it lives) *· Close the other tabs · Close N exited* — the specs are `tabMenuModel.mjs`,
  each item showing its keymap chord. `Ctrl+Tab` / `Ctrl+Shift+Tab` cycle the strip, terminals
  included (`sortModel.cycle`), in that one order; `⌘1` … `⌘8` go to the strip's nth tab and `⌘9` to its last; `⌘⌥W`
  closes the others, `⌘⌥⇧W` every tab with nothing unsaved (`closeSavedTabs` — the dirty ones are
  exactly what stays, so nothing asks); `⌘J` shows the terminal and, pressed on it, goes back to the
  document that was showing.
- **Session restore** — the open tabs, the active one, the pane tree and the pins are written to
  `ide/layout/<scope>-<id>.json` on a debounce (`ideLayoutModel.mjs`, `LAYOUT_VERSION = 5` — the
  strip's order saved with them, terminal ids included, since the terminal store restores its
  sessions under the same keys) and
  read back when the workbench opens on that root; a file of another version is refused, never
  reinterpreted; the file is capped at 256 KiB (`ide/layout.rs`), and a layout past it is refused
  rather than cut; a tab the saved tree forgot lands in the first pane. A preview, and an untitled
  document, is left out — of the tabs and of its pane — and an active one is saved as the kept
  neighbour its pane would show without it, so the file never names a tab it does not hold.

The active document stays in the URL (`?doc=`), as today: it is a *place*. The layout is
*furniture* and is not in the URL — `Back` moves between documents, never between layouts. The
layout file is local state with no GEP kind, by the test 09 applies to workstreams: a pane arrangement
on one machine is not a fact a second node could act on.

A layout that names a file which no longer exists opens the tab in a *missing* state with the path
shown, rather than dropping it: a tab you had open is a thing you meant to come back to.

---

## Drag and drop is the kit's, never the browser's

Every drag in the IDE — a file onto a folder or the agent pane, a hunk onto the agent pane, a tab
along its strip or into another pane, a rail row among its siblings — goes through `ui/dnd/`, one
wrapper of `@dnd-kit` that `ui/imports.test.mjs` keeps the only one. The HTML5 protocol is not
used: WebKit refuses to start a drag with an empty data store, `dragover` cannot read the payload,
and nothing scrolls a list under the pointer. `dnd-kit` drives the drag from pointer and keyboard
events, so it starts in the desktop webview, shows a ghost the app draws (the payload's `label`
behind the glyph its kind names — `dragGlyph`: a folder, a project, a workstream, a shell, a file, a
document — and a count when a selection moves, `dragCount`; it lifts off its row with `.motion-lift`
and dims while the target under it refuses, the target's word reaching it through `useDragCue`),
auto-scrolls the container under the pointer, and lifts on Space for the keyboard. A surface may
draw its own payload's ghost — the Board draws the card itself — and have it **settle**: travel to
the element that carries its id when the drag ends, over the slow token (`useDragGhost(type,
render, {settle})`, one `DragOverlay` still); a type nobody registered is the chip, with no drop
animation, since the list has already moved the item.

- **One provider** (`DragProvider`, mounted once in `App.tsx`) is the app's drag world. Every
  target stamps its droppable data with a **zone id** and registers a handler under it
  (`useDropHandler`); when a drag ends over anything, the provider looks the zone up and hands it
  the payload. Collision is *innermost first*: a row inside a tree inside a pane is three
  containers under one point, and the smallest is the one that means something.
- **Typed payloads** (`dragData.mjs`): `pathDrag` `{scope, id, path, dir}`, `hunkDrag`,
  `docTabDrag` `{id, pane}`, `terminalTabDrag` `{key, pane}`, `railRowDrag` `{kind, id, ctx}`,
  `navRowDrag` `{key, glyph}` (a sidebar destination, along its rows or the rail) — one closed union
  with a `label`, matched with `isDragOf`. No MIME strings anywhere.
- **Three kinds of target.** `DropZone` — a surface with an `accepts` predicate that lights while
  an acceptable drag is over it (the agent pane, a document pane, a terminal pane's header).
  `SortableList` — a headless sortable over items, horizontal or vertical (the tab strips); lists
  of one **family** (a Kanban's columns) name an item the same in every list
  (`sortModel.sortableId`), so a caller may move an item into the list it hovers while it is still
  dragged (`onHover`, with the slot `sortModel.hoverIndex` gives it) and the drag keeps tracking
  it. A `DropZone` lights with a soft fill, never a ring around the whole surface, and says when
  its well is hovered (`onHover`). `TreeList` — the tree, below.

## The tree

`ui/tree/` is the one tree component: `TreeList.tsx` over `VirtualList`, with the rules in
`treeListModel.mjs`. The explorer, the project rail and Git › Changes ([04](04-git.md)) are drawn
with it — the last `unbounded`, as tall as its rows, so the panel around it stays the one
scrollport, and its rows from `pathTree.mjs`, the kit's one builder of a tree from a flat list of
paths: folders nested as on disk, nothing compacted, `sortEntries`' order, so Git › Changes and
the explorer read alike — and everything the three would otherwise each implement is written once: rows are `{id, depth, label, focusable,
expandable, expanded}` in display order (a row's parent is the nearest shallower row above it —
`parentsFromDepth` — so callers keep no parent links); the cursor takes ↑↓, Home, End, PageUp,
PageDown, → (open, then step in), ← (close, then go up), `*` (open every sibling) and **type-ahead**
(`keyAction`, `typeAhead`), never landing on a message row; indent guides mark each level (`guideInset` says where in the column the line sits and `guideClassName` its ink — the rail hangs its guides from its chevrons' centre, `railLayoutModel.guideInset`, the explorer keeps the kit's default);
`aria-activedescendant` names the cursor row. The cursor is one row; a caller that keeps a
**selection set** hands it in (`selected`) and the tree marks membership with `aria-selected` under
`aria-multiselectable`, reporting every cursor move with whether Shift was held
(`onCursor(id, { extend })`) — how the set changes is the caller's rule, not the tree's. A drag
over the tree is **projected** (`dropPlan(rows, actives, over, ratio)`, `actives` the dragged row
or every row a selection drags together — `drag.actives`): from the row under the pointer and
where in it the pointer is, the plan is `{parent, index, mode: before | after | inside, depth,
noop}` — inside across the middle half of a row that may nest, beside it at the edges, *first
child* just below an open row with children — refused onto a dragged row or into its own subtree
(one row has an exact *already there*, its index; several are already there when every one sits
under the target parent), and refused by the caller's `canNest` / `canReorder`. The indicator is the plan: **one bar per tree** on the gap at the projected depth for a
reorder (`indicator: "bar"`, the rail) — drawn once over the rows through `VirtualList`'s `overlay`
slot and slid there by a transition (`indicatorStyle`), a cap at its left marking the depth, faded
out where it last stood rather than remounted inside each row — or the parent the row would join
(`indicator: "parent"`, the explorer, where entries sort by name and only the folder is a fact),
which wears an inset ring easing in (`.tree-nest`, `data-drop-inside`). The rows beside the gap
part by two pixels each way (`neighbourShift`, `--drop-shift` on the droppable itself, so the hit
rect the library measures never moves); the rows a drop just put there wear their wash for a beat
(`landed`); and what the drag is being told is one word (`dragCue`: *refused*, *stay*, *nest*,
*move*) on the tree's `data-drop-cue` — a *not-allowed* cursor, the ghost dimmed — and on the ghost.
From the keyboard, Space on the cursor row hands the sensor's lifter the key (focus stays on the
tree, as `aria-activedescendant` wants); while the row is lifted the tree steps its own **slots**
(`dropSlots`: every distinct plan top to bottom, the root's end last; `stepSlot`: ↑↓ the next, ←→ the
same gap one level out or in), the sensor moving nothing for a tree's row, and Space drops through
the zone's handler with the tree's plan.

## The explorer

`ui/FileTree.tsx` draws the file tree with `TreeList`; `fileTreeModel.mjs` keeps what a *file* is —
the per-directory listing state, `pendingLoads`, the sort, `withDraft`, `dirsToRefresh`,
`activate` — lazy, one level per request, bounded, never following a symlink. A listing answers
its two bounds apart: `deeper` says there is more below the depth asked for — every one-level
listing of a folder with subfolders, which the tree lists when the folder is opened and never
shows — and `truncated` says the entry cap (`TREE_MAX_ENTRIES`, 2 000) cut *this* level, which is
the one row the tree draws: *…and more — this listing hit its size bound*.

**Three facts, kept apart** (`fileTreeModel.mjs`): the **cursor** is where the keyboard is; the
**selection** is what a verb acts on — one row or many: a plain click or arrow collapses it to that
row, Cmd-click toggles a row, Shift-click and Shift+↑↓/Home/End take the range from the **anchor**
over the *visible* rows (a folded folder is one row, its hidden children are not swept in), `⌘A`
(`select_all_entries`) takes every visible row, `Esc` clears; what folds, fails or vanishes from a
listing leaves the selection (`pruned`) — what you cannot see you cannot be acting on. The third,
**shown**, is the file the tree previews inline when nobody else opens files for it (the goal
inspector). A verb asked at a row acts on the selection when that row is in it, else on that row
alone, and never on a child of a folder already chosen (`targetsOf`); a right-click on a row outside
the selection selects it alone first. The engine's file routes take one path each and there is no
batch route, so a verb over many **fans out** in the desktop (`useTreeMutations`): sequentially,
stopping at the first refusal, the toast saying what was done and what was not (`fanOutSummary`);
a cut clipboard is spent only for what moved; the tabs of deleted files close together through one
guard. A drag of many carries every selected path (`pathDrag({ …, paths })`, the ghost reads *N
items*), lands as a paste of a cut (`pasteTargets` — never into itself, never over a sibling,
*already here* skipped), and the Agents pane takes one chip per path. Its verbs:

| Action | Route | Refused when |
|---|---|---|
| new file / new folder | `POST /ide/files/{scope}/{id}` `{path, kind}` | exists · leaves the root · root is read-only |
| rename / move | `POST /ide/files/{scope}/{id}/move` `{from, to}` | target exists · either side leaves the root |
| duplicate | `POST /ide/files/{scope}/{id}/copy` `{from, to}` — a file, or a folder entry by entry, symlinks recreated and never followed; named `foo copy.txt`, `foo copy 2.txt` (`fileTreeMutations.mjs`) | target exists · into itself · leaves the root |
| delete | `DELETE /ide/files/{scope}/{id}?path=` — to the OS trash when `editor.delete.trash` is on (the default; a setting of this machine, never a project's), else unlinked; the answer's `disposal` says which, and `GET …/disposal` says it *before* the click so the confirmation promises the right thing | leaves the root; a directory is refused unless `?recursive=true` and the client has shown the count; a trash that refuses is an error, never a fall-through |
| cut · copy · paste | the copy and move routes, one per entry — a cut pastes as a move, a copy as a copy named the way a duplicate is when the name is taken (`fileClipboard.mjs`; the clipboard is one module store, `fileClipboardStore.ts`, so a cut in one window pastes in another on the same root); with nothing on the tree's clipboard, a paste takes what the file manager's holds — the shell copies the files into the folder (§Where the paths come from) | across roots · a cut into its own subtree · a moved name a sibling already holds · a link, a nested `.git`, a folder into itself from the file manager (left out and said) |
| copy relative / absolute path · *Reveal in Finder* | no route — the clipboard (the shell's in the desktop, `copy_text`, because the webview writes its own only in a gesture and a menu's action runs after the menu has left; the webview's in a browser — `ui/clipboard.ts`, the one door, and every copy says *Copied.* or that the clipboard refused), and the desktop's opener (`opener:allow-reveal-item-in-dir`); the label is the platform's — *Reveal in Finder* · *Reveal in File Explorer* · *Reveal in file manager*, one verb — from `revealLabel`, in the kit, the one source every reveal reads; no surface spells it, the keymap's command description alone stays platform-neutral | — |
| reveal in Files | no route — a `reveal` action on the tree's reducer opens every folder above the path, puts the cursor on it and scrolls it into view once (`TreeList`'s `scrollTo` request: resolved when asked, never when rows move, so opening a folder above it afterwards leaves the tree where you are); asked for through `explorerStore.requestReveal(rootKey, path)` by a document tab's menu, a search hit and the `reveal_in_files` command, and honoured when the tree mounts if it was not there yet | — |
| drag within the tree | the move route, one per dragged path — a `pathDrag` row, or the selection it belongs to, onto a folder (the plan's parent) or the tree's frame (the root); a drop where every row already is (`plan.noop`) lights nothing | as above |
| drag onto the Agent tab | no route — a `ContextRef::File` chip ([09](09-agents-in-the-ide.md)) | — |

**Rename is inline**: the row becomes a field with the stem selected; `Enter` commits,
`Escape` or leaving cancels, and what is wrong with a name — empty, a slash, `..`, a sibling's — is
said under the row before the call (`renameError`), the node's refusal in the same place after it.
A double-click **keeps** the file a single click previewed (and toggles a folder twice, as every
file manager does); a rename is `Enter` (`F2` under the `vscode` preset) or the menu. **New file and
new folder are inline too**:
a draft row appears in the target folder (`fileTreeModel.withDraft`) and the name is typed there,
validated the same way, created through `ideCreate` on `Enter` — no modal. **A new file opens at
once** as a kept document in the middle panel (`onCreated` → the workbench's `openFile`), and the
tree selects it through the reveal that follows the open document; a new folder is revealed alone —
there is nothing to open. **On an empty root the
draft is the list's only row and mounts all the same**: the root's own notice — listing, empty, or
an error with a retry — is the panel frame's (`bodyRows` leaves it out of the rows), so *New file…*
on a folder with nothing in it paints the name field at once. **`Escape` anywhere in the tree**
cancels the draft or the rename in progress; a different root forgets both. A drag shows its drop
target: the folder (or the tree's frame, to move an entry out) lights up as you drag over it.

**The keyboard is the keymap's** ([15](15-keymap.md)). The tree answers the arrows, Home, End, the
page keys, `*` and type-ahead itself (`treeListModel.keyAction`); everything else — `Enter` renames, `Space` opens (kept), `⌘⌫` deletes, `⌘C` `⌘X` `⌘V`
`⌘D`, `⌘⌥N` and `⌘⌥⇧N`, `⌘A` selects all — is a command in the `files` scope, live while focus is
inside a tree's frame (the frame carries `data-files-tree`), delivered to that tree through
`ui/explorerStore.ts` and answered by the same `run` its context menu calls, so a chord and a menu
item can never disagree. On macOS `⌘C` `⌘X` `⌘V` `⌘A` are the Edit menu's key equivalents, which
fire before the webview sees the key: the shell performs the verb natively and says it, and the
keymap replays it where the focus is (ide/15 §Dispatch) — so `⌘V` on a tree pastes what Finder or
a picture app copied, and `⌘V` in a field pastes text once. Every menu item shows its chord. A read-only tree (the goal inspector's)
consumes the chords and offers only the paths and the reveal.

**Menus.** A row's (`menuSpec(targets)`): *New file… · New folder… · Rename · Duplicate · Cut ·
Copy · Paste into folder / beside · Copy relative path · Copy absolute path · Reveal in Finder ·
Delete…*; for several rows only the verbs a set can take, each counting what it acts on — *Duplicate
3 · Cut 3 · Copy 3 · Copy relative paths · Copy absolute paths · Delete 3 items…* — and the delete
confirmation names up to five of them and tallies the folders' top-level counts (`deleteCopy`). The
viewport's (`viewportMenuSpec`, on the tree's background): *New file… · New folder… · Paste ·
Reveal in Finder (root) · Refresh*; the toolbar's `+` menu is the same list. A chosen item runs
**after the menu has left** (`ui/Menu.tsx` and `ui/ContextMenu.tsx` run it in `onCloseAutoFocus`,
with the focus return prevented), so a draft or a rename it opens mounts with nothing left to steal
its focus; a single `focus()` sticks, and blur — judged a frame later — cancels. The one exception
is an item marked `immediate`: it runs inside the pointer's own event, because a file dialog — the
rail's *Set photo…* on a project or a group, the composer's *Files from disk…* — opens only in a
user gesture, and the moment after the menu has left is no longer one. A copy is not an exception:
the clipboard door writes through the shell in the desktop, which needs no gesture.

**The Files occupant around the tree.** The tree's header is a `SectionHeader` whose actions are
search, the `+` menu, **Collapse all** (`collapse_all`) and refresh. A row whose document has unsaved changes wears the dot too (`dirtyPaths`, from the editor
registry); **a row git has something to say about wears its standing**: the name painted in the
kind's tone — `ok` for what is new (added, untracked), `warn` for what moved (modified, renamed,
deleted), `danger` for a conflict — with the Changes view's mark at its right (`~` `+` `?` `!`
`−` `→`), and a folder holding one painted in the strongest kind beneath it, so a conflict three
levels down is red at the top. The facts are one read per root shared with the Git panel
(`views/_work/gitFilesStore.ts`: `GET /git/files` once, again per burst of `file_changed` frames
for that root, after the frames that mean something wrote into the tree, on the session's `stale`,
and the rows a write answered with committed straight in); the git model turns the letters into
kinds (`gitFiles.treeStandings` — the working tree's letter first, the index's when the tree is
quiet); the kit's model folds them up the folders and names the tone, the mark and the tooltip's
words (`ui/fileStandingModel.mjs`), so the tree imports no view. A root without a repository is
asked nothing. Every file wears the glyph its name earns (`fileIcons.mjs`, the same one its tab wears),
and the tree **follows the active document**: a tab clicked in the strip is revealed in Files, now
or when the tree mounts — once; opening folders afterwards leaves the tree where you are, until
another document becomes active or you ask to reveal again. The occupant **fills its column by
flex, never by percentage**, from the panel's body down: the body is a scrollport only for the
occupants that scroll as one (`rightPanelModel.occupantScroll` — Git › Changes under its sticky
composer, every `PanelBody`), and for the explorer and the Agent pane a flex column that clips
(`overflow-hidden`); the Files branch, the tree's root, its menu region and the `TreeList` are
`min-h-0 flex-1`, the header and the search box `shrink-0`, so the tree's `VirtualList` is the one
scrollport and the rows reach the panel's bottom with the search open or shut, however deep a
folder is opened — a `100%` height asked for the whole column regardless of the header above it,
and ran past the bottom by exactly that much; a `100%` inside a scrolling body was content-sized
when it did not resolve, so the tree was exactly as tall as its rows and the whole occupant
scrolled instead. The number inside the box is measured, never guessed: `VirtualList` observes
its scroll element from the element itself (a callback ref, a layout effect before paint) and its
window is `ui/virtualListModel.mjs`'s — empty until the viewport is measured, the row under the
bottom edge always inside, the scroll clamped when the count shrinks; an `unbounded` tree is
`overflow-visible` and windows against the panel body's `data-scrollport` — a list that once
painted twenty rows because it never learnt its height was a list cut at the bottom of every
panel taller than that.

**Search** sits above the tree in the Files occupant — a search icon in the toolbar,
`⌘⇧F` from anywhere in the root — one box, two modes. *Names* ranks the root's path index as you
type (`nameResults` over `quickOpenScore.rankPaths`, the palette's scorer, over the index
`shell/pathIndexStore.ts` keeps once per root and patches from `file_changed` —
`quickOpenScore.patchIndex`: a path that went takes every path under it, a folder renamed moves
its files and is never a file itself, and a frame that says more than a list of files can take — a
folder made or copied whole, a rename out of what was ignored — forgets the index, which is read
again). *Contents* streams
the node's search ([12](12-search-and-quick-open.md)) through `api.ideSearch` — a hit per line as
ripgrep finds it, debounced 180 ms, the previous run aborted, painted a frame at a time — with
regex, case and whole-word toggles, and `in:src/**` / `-in:*.lock` tokens in the box as include and
exclude globs (`parseQuery`). Hits fold into their files in arrival order, twenty shown per file
and the rest counted (`groupHits`, `resultRows`); a hit opens the document at its line and reveals
the path in the tree; `Enter` and `Shift+Enter` step through them (`nextOpenable`); the footer says
*12 matches in 4 files · 1 203 scanned* or *stopped at the cap; narrow it* (`searchStatus`).
`Escape` clears, then closes. Facts in `fileSearchModel.mjs`.

The engine does the disposing through one `Remover` — `TrashRemover` or `UnlinkRemover`, chosen by
the `Disposal` the node resolved from the setting; the engine never reads the setting
itself. A delete of a tracked file in a git root is also an ordinary working-tree change that
`git status` shows and the git panel can stage. No test exercises the trash remover: a test that
moved a fixture into the developer's Trash would leave something behind on a real machine.

**The explorer holds the watcher's lease** while it shows a writable root (`useWatchLease`, the same
hook the editor uses), and a `file_changed` frame re-reads only the listings it touched
(`dirsToRefresh`), coalesced over 100 ms, so an agent's write appears without a click. Rows the
root's ignore rules match are **dimmed, never hidden** (`FileEntry.ignored`, computed by the store's
one `ignore_rules`, the same the watcher stamps frames with). A **rename is followed by the open
document**: the workbench hears the `renamed` frame, saves a dirty buffer first, and re-points the
tab (`retargetTabs`), pane and pin included; one that will not save keeps its tab and says so.
Every explorer mutation lands in the same watcher stream as any other change, so a second window on
the same project sees it.

The CLI has the same verbs: `bisa files new|mv|cp|rm`, which say how a delete went.

---

## Editing features, and where each comes from

| Feature | Source |
|---|---|
| syntax highlighting | Monaco's tokeniser — there are no semantic tokens ([10](10-language-intelligence.md)) |
| multi-cursor, bracket matching | Monaco, built in |
| find, find and replace in the file | Monaco's own widgets, opened by the app's chords — `find` (⌘F) and `replace` (⌘R), `when: document` ([15](15-keymap.md)) — through `CodeEditor.handle.trigger`; Monaco's ⌘⌥F still works |
| find in a **rendering** | the kit's one bar (`ui/find/FindBar.tsx` over `findModel.mjs`), on the same chords: markdown and a diagram are walked and drawn with the CSS Custom Highlight API (`useDomFind`, `domFindModel.mjs` — the DOM is never rewritten), a csv searches its parsed rows and scrolls the grid to the cell (`sheetModel.findCells`), a page finds inside its frame (`pageInspector` `FIND` / `FOUND`, the frame highlighting its own text), an svg is a picture and its bar goes to the source; a rendering of bytes (`RenderedFileDoc`: a sheet, a document) finds and never replaces |
| replace in a rendering | into the **buffer** — the rendering is drawn from it — through `edited`, so the page follows, the tab dirties and autosave and the on-disk conflict work as for any edit |
| find/replace across files | [12 — search](12-search-and-quick-open.md), through the CAS write per file |
| go-to-symbol, go-to-definition, hover, diagnostics | [10 — language intelligence](10-language-intelligence.md) |
| theme | derived from the token roles ([`editorTheme.mjs`](02-component-model.md#desktop-side)); a test asserts every role the theme contract requires maps to a Monaco colour |
| rendered documents | §Rendered documents: Markdown with **Rendered · Split · Source** — three glyphs, the mode remembered per document — Mermaid blocks through [11](11-mermaid.md), a relative link (`./notes.md`) resolving against the document by `docLink.mjs` — inside the root or inert — and opening as another document, a relative image drawn through the byte route; a csv and an svg with **Rendered · Source**; a page with **Rendered · Split · Source** and **Annotate** — elements pointed at in the rendering, each with the change wanted, sent to an agent as chips or attached to the Agent pane ([09](09-agents-in-the-ide.md)); a PDF, a picture, a recording, a sheet, a document and a deck as documents with no editor; every path and URL in the text a door through [17](17-links-and-paths.md) |
| word wrap | `editor.word_wrap` (`off` · `on` · `bounded`), read by `useEditorTypography` and applied to a mounted editor without a remount; `Alt+Z` (`toggle_word_wrap`) flips it at machine scope |
| breadcrumbs | the path above the editor as crumbs (`breadcrumbsOf`): a folder reveals itself in Files, the file copies its path |
| the caret in the footer | *Ln 12, Col 4* and the language of the editor you are in (`editorStatusStore.ts`, published on every caret move, read by the status bar) |
| go to line | `⌘G` (`go_to_line`) opens the omnibox in its `line` mode, seeded with `:`; a line asked for before the editor is up waits for it (`editorRegistry.requestLine`) — a search hit, a definition, stepping fast through results |

Monaco is wrapped once, in `ui/CodeEditor.tsx` and `ui/DiffEditor.tsx`. Views never import it.
Models are disposed when their tab closes.
