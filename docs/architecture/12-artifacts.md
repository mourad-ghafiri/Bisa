# 12 — Artifacts

An **artifact** is something an agent made for a person to look at — a page, a chart, a report, a
sheet, a deck, an image, a recording — carried by a message and rendered live where the person
reads: a direct channel, a standing channel, a goal's thread, a conversation, the Project IDE's Agent panel and
Agent Mode, the Inbox. A person can save it where they like, reveal it in the file manager, open
it with the default application and, when it was written inside a checkout, open the file itself in
the IDE. A person can share one too, from the composer or the CLI.

The word is one concept. A work item's captured patch — the `.patch` a copy workstream leaves when
its item settles — is its **result** (its home's `results/<WorkItemId>.patch`, a goal's or a run in the
workspace's, `EntryKind::Result`, `GET /work-items/{item}/result`), and a journal `Result` fact's `artifacts` list is the A2A
protocol's word, left as the protocol spells it.

---

## The model

```rust
pub struct ArtifactRef {
    pub sha256: String,          // the blob's address and proof — an attachment's
    pub name: String,            // what the maker called the file; never a path
    pub mime: String,
    pub size: u64,
    pub title: String,           // what it is called where it renders
    pub kind: ArtifactKind,      // Html · Svg · Image · Video · Audio · Pdf · Sheet · Document · Slides · Markdown · Diagram · Code · Data · Text · File
    pub source: Option<ArtifactSource { scope: FileScope, id, path: RelPath }>,
}

pub enum MessageBody {
    Post { text, context: Vec<ContextRef>, artifacts: Vec<ArtifactRef> },
    Membership(MembershipEvent),
}
```

**An artifact is an attachment with a purpose.** Its bytes are an attachment's bytes —
content-addressed under `attachments/`, bounded at 25 MiB, moved on demand by hash over the direct
link exactly as an attachment's are. What makes a file an artifact rides beside the descriptor: a
**title** (the same title on a later post is a new version of the same thing), a **kind** the
desktop picks a renderer by, and, when the file was written inside a checkout or a home's scratch —
a goal's, or a run of the workspace's — a **source** naming that root and the path under it. An attachment is a file handed over *as a
file*; an artifact is a thing to look at.

The descriptor is **in the typed body** — the kind-3407 event's `content` — not an `imeta` tag, so
a peer that syncs the message knows the title and the kind before it holds a byte, and the one
indexing arm (`index_conversation_fact`) fills the index on a local post, a rebuild from truth and
a peer's ingest alike. A message carries at most eight; each must be well-formed (`validate`); a
post with artifacts and no words is a message. `ArtifactKind::of(name, mime)` and `mime_of_name`
are the one table for a name's kind and type — the extension decides first, the mime family answers
for a bare name, `File` is the honest fallback — and the desktop's `artifactModel.mjs` mirrors both
with a test that reads the Rust source.

`FileScope` — the four roots a relative path is read under: a goal, a workstream, a work item and a
run of the workspace (`FileScope::Run`, the run's own folder, its scratch where a step with no
project leaves what it made) — lives in the core (`path.rs`) since an artifact's source names one
on the wire; the store and the node re-export it.

### On disk and in the index

| What | Where |
|---|---|
| the bytes | `attachments/<2 hex>/<62 hex>` — an attachment's and an artifact's alike |
| the named copy | `attachments/named/<sha256>/<name>` — the blob under its maker's name, made on demand by hard link (else copy), idempotent; the one file a file manager can reveal and the default application open, since the blob has no extension. `sanitise_file_name` keeps the last component, drops control characters and separators, refuses `.`, `..` and a dotfile |
| the index | an `artifacts` table beside `attachments` — `message_id, ordinal, sha256, name, mime, size, kind, title, source_scope, source_id, source_path` — rebuilt from the bodies with every other table, at the one `SCHEMA_VERSION` (`crates/bisa-store/src/index.rs`) |

The store answers `get_message(id)` (one message with its artifacts and their presence),
`list_artifacts(scope, limit)` (the conversation's artifacts newest first, a retracted message's
left out — its gallery) and `put_attachment_named(sha, name)` (the named copy; the store's one
writer of it, reached by the node only through `engine::admin::attachment_named`).

---

## Producing one

**The tool.** `post_message` gains `artifacts: [{path, title?}]` beside `attachments`. The title
defaults to the file's stem; the kind comes from the name. The tool's description and
`CONVERSATION_FRAMING` say when to use which: *what you made for the person to look at goes in
`artifacts`; a file handed over as a file goes in `attachments`*. The executor's result protocol
tells a work-item session the same. The catalog skill `artifacts` (*Make artifacts*) is the
procedure: a self-contained page, the format per thing, one title per thing and the same title when
revising it.

**The door widens.** An agent could once attach only what sat in its own scratch folder — so a
session working in a checkout could not publish what it wrote there. `intake::session_roots`
answers the roots a session may publish from, in the order they are tried: the agent's scratch
(always); a work-item session's open workstream checkout and its home's scratch — its goal's, or
its run's when the run is the workspace's, recorded as `run`; the checkout a
conversation's origin names — a workstream's, a project's primary; the goal a goal scope, a
conversation with a goal origin or the session's `goal` names. `resolve_in_roots`
strips a canonicalised root's prefix off an absolute path before `resolve_within`, tries a relative
path against each root in order, and refuses with every boundary named. Attachments and artifacts
both go through it; more than eight is refused before anything is read; a file under a workstream,
goal or run root records its `source` — the agent's scratch never does, since it is nobody's root but
the agent's.

**What the next turn sees.** The transcript lists an artifact as `[artifact] title · kind ·
absolute path (name)` — the blob's path has no extension, so the name is said beside it — and the
newest message's artifact images reach an image-taking harness the way a photo does.

**A person's.** The composer's file chip has a switch — *file* or *artifact* — and a file switched
to *artifact* posts as one, titled by its stem, its kind from its name (`artifactFromFile`). The CLI
does the same: `bisa msg <scope> "…" --artifact <path>[:title] --attach <path>`, the bytes
stored first, then the message.

**The node.** Every post body takes `artifacts?: [ArtifactRef]` (the bytes uploaded first through
`POST /attachments`, which answers the descriptor). `GET /messages/{id}` is one message with its
artifacts and their presence; `GET /artifacts/{scope}` the gallery; `POST /attachments/{sha256}/file
{name}` → `{path}` the named copy. The byte route keeps refusing to serve anything an agent wrote
as a page.

---

## Rendering — the desktop

**Bytes, never a URL.** `artifactBytes.ts` fetches `GET /attachments/{sha}` with the bearer
header and keeps one bounded cache of blob URLs, revoked as they fall out. Nothing renders from a
node URL: the webview runs with `csp: null`, the node serves a blob as a download unless its magic
number says it is an image, and a viewer that holds the bytes can hand them to a sandboxed frame, a
PDF renderer, a spreadsheet parser or a blob URL as it needs. A file in a checkout follows the same
rule through its own route — `GET /ide/raw/{scope}/{id}?path=`, typed by the same header check,
fetched by `fileBytes.ts` with the bearer header ([ide/03](ide/03-files-and-editing.md#rendered-documents)).

**One viewer, one renderer per kind** — `ui/artifact/`, every library wrapped there and banned
elsewhere by `imports.test.mjs`. The switch is **`KindView`** (kind, name, bytes and text in, the
body out), which `ArtifactView` draws under its toolbar and the IDE's `RenderedFileDoc` draws under
its own — one renderer per kind wherever a kind is shown:

| Kind | Renderer | Library (licence) |
|---|---|---|
| html | `PageFrame` — `<iframe sandbox="allow-scripts" srcdoc=…>`; in the IDE, with the inspector script when a page is being annotated (ide/03); see *Security* | none |
| svg · image | `ImageView` — an `<img>` from the blob URL (an image never runs a script); *View the source* for the figure's text | none |
| video · audio | `MediaView` — `<video>` / `<audio controls>`; a codec WebKit cannot play says so and offers *Open with the default app* | none |
| pdf | `PdfView` — pages to canvas as they scroll into view, a page counter, a zoom | `pdfjs-dist` (Apache-2.0), its worker a Vite asset like Monaco's |
| sheet | `SheetView` — lettered columns, a header row, rows through `VirtualList`, a tab per sheet; csv/tsv by `sheetModel.mjs` (RFC 4180, the delimiter told from the first line), xlsx, xls and ods by SheetJS | `xlsx` — SheetJS Community Edition (Apache-2.0), the npm package |
| document | `DocumentView` — docx → HTML by mammoth, sanitised by DOMPurify, drawn in `prose-i` | `mammoth` (BSD-2), `dompurify` (Apache-2.0 / MPL-2.0) |
| slides | `SlidesView` — one card per slide: title, text, pictures, notes, read off the zip's XML by `pptxModel.mjs`; an outline, honest about being one — *Open with the default app* is the deck as designed | `jszip` (MIT) |
| markdown · diagram | `Markdown` · `MermaidView` | existing |
| code · data · text | `CodeEditor` read-only, the language from the name, plain above a megabyte | existing |
| file | the card alone | — |

`ArtifactView` is the whole thing: the toolbar (the kind's glyph, the title, the kind's word, the
size, a fact the renderer learns — *12 pages*, *120 rows × 6 columns*, *9 slides*) and the verbs —
**Save as…** (the save dialog, then the shell copies the named copy there), **Reveal in Finder**
(the platform's word from `revealLabel`), **Open with the default app**, *Copy the text* for a text
kind, *Open in the IDE* when the artifact has a source (through the link handler, [ide/17](ide/17-links-and-paths.md)),
*View the source* for a page or a figure. The named copy is asked of the node once per hash.

**The shell's two commands** (`src-tauri/src/artifacts.rs`): `open_artifact(path)` and
`copy_artifact(from, to)`. Both refuse a path that is not a named copy — the file's parent sixty-four
hex characters, its grandparent `named`, that one's parent `attachments` — a structural check, since
the shell does not know the node's data directory. No bytes cross the IPC bridge and no plugin
permission widens: `dialog:allow-save` and `opener:allow-reveal-item-in-dir` already exist, and the
opener's Rust API opens the file from the shell's side.

### Where it renders

1. **Under the message** — `ArtifactCard`: the glyph, the title, the size, and the first thing to
   see: a picture inline; a page **live** in its sandbox at a bounded height, mounted only while on
   screen and at most three at once (`MAX_LIVE_FRAMES`; the rest posters until opened); everything
   else a poster with *Open*. A click on the title opens the pane; ⌘-click opens the stage. Absent
   bytes show *Request* — the existing fetch door — and a dashed outline.
2. **The aux pane** — an occupant of its own, `artifact`, at `?aux=artifact&auxId=<message id>:<ordinal>`:
   the shell draws it itself from the URL, so it opens beside any screen a message shows on and
   survives Back and a reload. `ArtifactPane` loads the message, draws the viewer with *Expand* and,
   in the IDE, *Open as a tab*; under it, folded, **In this conversation**: the gallery grouped by
   title with versions (`artifactVersions`), newest first — a click switches, the versions listed
   *v3 · who · when*.
3. **A workbench tab** — `{kind: "artifact", message, ordinal, title}`, opened from the pane; the
   tab's word is the title; `ArtifactDocument` draws the viewer in the centre.
4. **The stage** — `ArtifactStage`, the viewer over the whole window at `z-50` (`?stage=1`); Esc
   leaves, ← and → move through the conversation's artifacts.

Every conversation surface is one component, so the card is everywhere a message is: the Inbox's
timeline, a goal's thread, channels, direct channels, the Agent panel, Agent Mode.

---

## Security

A page an agent wrote is untrusted. Two walls, neither depending on the other:

- **The sandbox.** `sandbox="allow-scripts"` and nothing else: the frame's origin is opaque, so the
  page cannot reach the app's origin, the bearer token, `localStorage`, or the parent. The frame has
  no `src`, only `srcdoc` — the node never serves a page.
- **The page's own policy.** `pageDocument.mjs` puts a `<meta http-equiv="Content-Security-Policy">`
  first in the page's head: `default-src 'none'`, `connect-src 'none'` (a page that fetches the
  node's port gets nothing back), `frame-src 'none'`, `form-action 'none'`, `base-uri 'none'`;
  scripts, styles and fonts from cdnjs, jsDelivr and unpkg (and Google Fonts) only while
  `artifacts.html.libraries` is on — off, a page runs with only what it carries; images from
  `data:`, `blob:` and `https:`. The policy stands whether or not the app's own CSP ever does.
- **The inspector adds nothing.** When the IDE annotates a page (ide/03 §Annotate) it injects one
  inline script after the policy (`pageInspector.mjs`; a test holds it to naming no `fetch`,
  `XMLHttpRequest`, storage or cookie). It runs inside the same sandbox under the same policy, so
  the page still has no origin, no token and no storage; the one wire out is `postMessage`, whose
  target is `"*"` because an opaque origin cannot be named, and whose messages the app accepts only
  from the frame's own window and re-bounds field by field. A page can at most post a false pick —
  which becomes a chip the person sees and can remove. An agent's artifact card or stage never
  carries the script. The embedded browser's tab runs the same core in every page it shows (ide/18,
  `browserScript(relay, theme)`), with one difference of transport: its words leave through the shell's one
  script message handler on the tab's webview, the same on a page of this machine and of the web,
  and no page has IPC — what a page says is data the desktop bounds, never an instruction.
  **Open in the browser** on an html or svg artifact keeps the rule from the other side:
  `POST /artifacts/{sha256}/serve` serves the named copy's folder on a loopback port of its own,
  so the page gets a real origin — a port, never the node's — and its scripts and assets load as
  a page's do (ide/18 §Artifacts in the browser).

An SVG is drawn as an `<img>`, never as a page: an image element runs no script. A document's HTML
is DOMPurify's prose profile — no styles, no handlers, no frames. A deck is read as text and
pictures, never executed. Artifact bytes are not redacted, like an attachment's: what an agent
hands back keeps its placeholders, and a viewer draws what it was given. The named copy is a file
inside the store; the shell opens or copies only that shape of path.

An **addon** ([18 — Addons](18-addons.md)) is the other page the desktop runs that is not its
own, and it stands behind the same first wall — `sandbox="allow-scripts"`, an opaque origin — with
two more of its own: its files come from the node under one policy, and the window refuses every
navigation off them. Where a page artifact is `srcdoc` and may reach three CDNs, an addon is a `src`
on the node's token-less files route and may reach nothing.

---

## What is not an artifact

- A file handed over as a file — an **attachment**, a chip that downloads, an image inline.
- A work item's captured patch — its **result**.
- A relative link inside a rendered document — the document's own, through `docLink.mjs`.
- A context chip on a sent message — what the person gave the agent, not what the agent made.

---

## Tests

| Promise | Held by |
|---|---|
| the kind follows the extension then the mime; every kind has a type; the title defaults to the stem; a ninth artifact, a bad hash, a long title, a slash in a name and an oversized size are refused; artifacts serialise inside the body and are absent when empty; `FileScope` parses its four names | `crates/bisa-core/src/artifact.rs`, `path.rs` unit tests |
| an artifact needs its bytes first; artifacts are indexed, read back with presence and survive a rebuild; the gallery is newest first and skips a retracted message; a named copy is idempotent and never leaves the store; a message with only an artifact is a message | `crates/bisa-store/tests/it/artifacts.rs` |
| an agent posts from its scratch and the title defaults; a conversation's turn in a checkout posts from it with the source recorded, by a relative or an absolute path; a goal session from the goal's scratch; outside every root is refused naming the boundaries and a person's post cannot publish an agent's file; a ninth is refused before anything is stored; the transcript lists artifacts and the framing says how | `crates/bisa-engine/tests/it/artifacts.rs` |
| an artifact rides the socket as `{path, title}` only when given, and every post says who speaks — the scope's agent, else the work item — so an agent's artifact is its own to post | `crates/bisa-mcp/tests/it/intake.rs` (`tool_core_texts`, `a_post_through_the_tool_says_who_speaks`) |
| a post with an unknown artifact is 400; a message reads back with its artifacts and presence; the gallery lists newest first; materialising names the file and is idempotent; a missing result is 404 | `crates/bisa-node/tests/it/node.rs` |
| `msg --artifact` stores then posts, a missing file is refused | `crates/bisa-cli/tests/it/cli.rs` |
| through the binary, while a node runs: an agent's sheet posted from its scratch with `post_message` — read back with its bytes and its kind, no source, its named copy the file itself — and a person's `--artifact` beside it in the gallery, newest first | the journey `crates/bisa-cli/tests/it/e2e/a_channel_and_what_is_said_in_it.rs` |
| the desktop's kinds and extension table mirror the Rust; the address round-trips; versions group by title; the page policy and sandbox, the inspector after the policy and only when asked; RFC 4180 and the grid's facts; a deck's slides, pictures and notes | `desktop/src/ui/artifact/artifactModel.test.mjs`, `pageDocument.test.mjs`, `sheetModel.test.mjs`, `pptxModel.test.mjs` |
| the gallery beside an open artifact lists one conversation's artifacts at a time: it empties when the conversation changes, so another conversation's are never listed under this one while its read is on its way | `desktop/src/scenarios/channelsAndMessages.test.mjs` |
| the inspector script names no network, storage or cookie, answers a click with the element's path, tag, bounded excerpt and text, drops its listeners when inspecting stops, says which badges lost their element, relays Escape, and ignores a message not from its parent; the app's parser bounds every field | `desktop/src/ui/artifact/pageInspector.test.mjs` (the script run in `node:vm` against a fake document) |
| only a named copy inside the store is opened or copied | `desktop/src-tauri/src/artifacts.rs` unit test |
| the viewer libraries are imported under `ui/artifact/` and nowhere else | `desktop/src/ui/imports.test.mjs` |
