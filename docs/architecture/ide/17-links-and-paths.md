# 17 — Links and paths

> Part of the [Project IDE architecture](README.md). Prerequisites: [03 — files and editing](03-files-and-editing.md), [06 — terminals](06-terminals.md), [09 — agents in the IDE](09-agents-in-the-ide.md).

**Every path and every URL the desktop shows is a door.** An agent's reply that says `src/main.rs:42`,
a compiler's error in a terminal, a step's output in a goal, a channel message, a row of the Pulse,
a note, a rendered `README.md` — click the path and the file opens in the IDE at that line, or is
revealed in the file manager; click the URL and a small card asks before the browser opens. One
scanner finds them, one resolver names them, one handler opens them; no view threads a callback.

---

## What is found

`ui/linkModel.mjs` is pure and tested. `findLinks(text)` returns every span in a text, in order, of
two kinds:

| Kind | Grammar |
|---|---|
| `url` | `http://` or `https://` to the next whitespace or quote, and a bare `www.` host given `https://`; the sentence's trailing `.` `,` `;` `)` are dropped, a `)` kept when the URL opened one |
| `path` | an optional `~/`, `/`, `./` or `../`, then slash-separated segments; an optional `:line`, `:line:col` or `#L12` address; the sentence's punctuation dropped. A bare name with no slash is a path only with an extension a source tree carries (`Cargo.toml`, `main.rs` — never `e.g.` or `v1.2.3`), and every path needs a letter, so `10/12` is a date |

Two refusals hold whatever the text: **a redacted secret is never a link** — `«secret:kind:tag»`
is skipped as a run, including anything that follows it without a space — and **a URL is never a
path**: URLs are found first and a path never overlaps one.

`parseAddress(raw)` reads the address off a path: `notes/x.md#L12-L15` is that file at line 12.

## Where it is marked

Rendered markdown goes through the string passes in `Markdown.tsx`: in a document, DOMPurify's
prose profile over the raw HTML micromark kept (`markdownHtmlModel.PROSE_PROFILE`; a message's HTML
is escaped by micromark and never reaches it), then the URL sanitizer (no protocol the desktop does
not vouch for, no handlers), then `linkifyHtml`, then the placeholder chips. Every pass reads a tag
to its first `>` outside a double-quoted value (`TAG_REST`), since a document's raw HTML may carry
one inside an attribute, and the chips are made in the text alone, never inside a tag. The link pass walks the HTML by tag: text and inline `<code>` gain anchors where `findLinks`
says; a fenced `<pre>` block, an existing anchor and a redacted secret are left alone; the anchor
micromark made for a URL is tagged `data-link="url"`, a `mailto:` one `data-link="mail"`, a relative
`href` in a document `data-link="doc"`. A path anchor carries `data-path`, `data-line` and
`data-col`; the text between is escaped again on the way out, so an entity around a link survives.

Plain text — a step's output, a work item's result, a closing check's words, the Pulse's expanded
fields — is drawn by `LinkedText`, the same spans as React anchors. A link the app writes itself —
a pull request, a check run, an agent's page — is an `ExternalLink`. **No `target="_blank"`
remains anywhere**: in the desktop shell that anchor is a navigation of the app window itself.

## Where a path points

`resolveLink(hit, roots, {from})` answers over the roots a surface knows, each `{scope, id, root,
label, paths}` — the root's absolute path when it is on disk (the workspace's word for a
workstream, `GET /placement` for a goal's or a project's root), and its path index (the node's
list of every file under it, the desktop's first existence check) — and from **where the surface
stands** when it says: `from`, a shell's current directory, absolute.

| The path | The answer |
|---|---|
| absolute, or `~/…` | matched to the **longest root prefix** and named relative to it. `~` is expanded through the roots alone — the webview has no home directory — so `~/Projects/app/README.md` is under `/Users/me/Projects/app` because that root ends that way. Under a root but not in its index (an ignored `target/out.log`) still opens: the node reads any contained path |
| absolute under no root | `outside` — *Reveal in Finder*, copy, or **open in the IDE** as a loose document ([03 §Loose files](03-files-and-editing.md#loose-files)) |
| `./a`, `../a`, `a/b`, `a.rs` | read **from where the surface stands** first, when it says (`from`): `absolutePath(from, path)` — `.` and `..` resolved — matched to the longest root; listed there, it is that document (the `src/lib.rs` of the crate a shell `cd`'d into, not the root's), unlisted it is the `unlisted` guess below; under no root, a path that says where it starts (`./`, `../`) is `outside`, a bare one is looked further. Then **from each root**, exact (`./` stripped). Then **by its tail** across the roots, at most `MAX_CANDIDATES` = 8 — `src/lib.rs` is every crate's, `main.rs` every `src/main.rs` — one is a document, several a `choice`. A climb with no `from` to climb from is `unknown` |
| listed by no index | `unlisted` — the guess the node is asked about before it is offered: from `from` when it stands in a root, else the surface's first root with the path as written. The handler lists the guess's folder (`GET /tree/{scope}/{id}?path=<parent>&depth=1`, which lists ignored and hidden entries too) and `confirmListing` answers — the document or directory the index skipped (`target/out.log`, `.github/workflows/ci.yml`, a file made since the index was read), `indexed: false`, or `unknown`. A word that merely looks like a path costs one listing and is *Not found*, as before |
| a directory of a root | `dir` — the root opens in the IDE |

**The trust boundary stands.** The webview never sends an absolute path to the node: the prefix
match happens here on the desktop, the node is told `(scope, id, relative)` as every file route is,
and the node still checks containment by canonicalisation
([03](03-files-and-editing.md)). Reveal and *Open in the IDE* are the two acts that carry an
absolute path, and both go to the shell — the platform's opener, and the shell's own loose-file
reader — never the node.

## The handler

`ui/linkContext.ts` is the contract the kit defines and the shell provides: a `LinkHandler` with
`onLink(hit, at, roots, {direct})`, read from context by `Markdown`, `LinkedText`, `ExternalLink`,
the chips on a sent message and the terminal, through one delegated click (`delegateLinkClick` —
the closest anchor with a `data-link` goes to the handler and the click is stopped; any other
anchor is left to bubble). **A click that ends a selection is not a click**: a rendering marks every
path and URL, and a browser fires `click` at the end of a drag whose two ends share an ancestor and
on the second press of a double-click — so a handler that opened the link on any click inside an
anchor opened it when a person was only selecting the words, and the card took the selection with
it. `ui/selectionModel.endsSelection(selection, container)` says when text of the container is left
selected; `delegateLinkClick` then swallows the click — prevented and stopped, since an anchor's own
default is to navigate the window and an outer handler would open a relative link — and opens
nothing, as does the document's own relative-link handler. A plain click leaves the selection
collapsed and opens the door as ever. `<LinkRoots roots>` is how a surface says which roots its text is read
against:

| Surface | Roots |
|---|---|
| a conversation about a checkout (Agent Mode, the Agent panel) | this checkout, then the project's other checkouts on disk (`useConversationPane.linkRoots`) |
| a goal's thread | the attached projects' primary checkouts |
| a rendered document (`EditorDoc`'s preview, `FileView`) | the document's own root |
| a terminal or a harness session | its own root (`scope`, `id`), and where its shell stands — `from`, the shell's current directory read at the click (`session.cwd()` → `terminal_cwd`: the process table's word for the shell's pid, else the directory it was started in), since the tool that printed the path ran there |
| a channel, a direct message, a conversation about a goal, a workflow, the workspace or the node, the Pulse, the Inbox, a note | none named: the provider's default — the root on screen first, then every checkout on disk, at most eight |

`shell/linkHandler.tsx` is the provider, mounted once in `App`. It describes the roots (a
workstream's path and label from the workspace, a goal's or a project's root placed by
`GET /placement`, the index from `pathIndexStore` — cached, or loaded once), resolves — from where
the surface stands when it said so — asks the node about an `unlisted` guess (one listing of its
folder, held to the click's ticket like the describe), and opens one of two things at the pointer. **The card is the model's**
(`shell/linkCardModel.mjs`): `defaultRoots` names the roots a surface left unnamed (the root on
screen, then every checkout on disk, `MAX_DEFAULT_ROOTS` = 8), `rootLabel` a root's word,
`pathCard(hit, resolution)` and `urlCard(url)` the title, the subtitle and the verbs — each verb an
id the component binds to an act (`open`, `reveal`, `open_loose`, `copy`; `open_here`,
`open_machine`) — and `copiedWords` the toast; the component holds no verb of its own. **The click
that lands last is the card drawn**: describing the roots reads an index, and two clicks in a row
once raced, the slower resolution replacing the card the person was already reading —
`shell/latestModel.createLatest` gives each click a ticket and a resolution whose ticket is stale
draws nothing (`linkCardModel.test.mjs`, `latestModel.test.mjs`, `scenarios/files.test.mjs`).

- **A path** — `LinkCard` with verbs: *Open src/a.rs:42* (primary), *Open in <checkout>* per
  candidate when several hold the file, *Reveal in Finder* — the platform's word from
  `revealLabel` — and *Copy the path*. An `outside` path offers *Open in the IDE*, reveal and copy; an `unknown` one
  copy alone. **⌘-click** (Ctrl elsewhere) opens at once when the door is plain: one document,
  no card.
- **A URL** — the host in bold, the URL beneath, **Open in Bisa's browser** (primary while the
  embedded browser is on, [18](18-browser-and-servers.md)), **Open in this machine's browser** and
  **Copy the URL**. Only `http` and `https` open (`opener:allow-open-url`); a `mailto:` or any other
  scheme copies. The browser never opens on a bare click, and the window never navigates: a URL an agent
  or a collaborator wrote is untrusted text.

Opening a document is `requestLine(editorKey(rootKey(scope, id), "file:" + path), line)` — the
line parked until the editor mounts, so it survives the route change — then
`setIdeMode(root, DOCUMENT_MODE)` — the root put in the mode that shows documents
(`ideModeModel.mjs`; a path opened from Agent Mode's thread was otherwise adopted as a tab behind
the conversation, invisible until the switch was pressed by hand) — then
`navigate({name: "workbench", scope, id}, {doc: "file:" + path})`
([03](03-files-and-editing.md), [15](15-keymap.md) for `openAt`).

### Why a card and not a dialog

The kit's rule says a decision belongs in a dialog. This is a peek with one act: *never mind* is
the common answer to a link clicked in passing, so Esc or a click anywhere else leaves it, the
first verb has focus for Enter, and it stands at the pointer rather than in the middle of the
window. It is a dialog by role (`role="dialog"`, labelled by its title) for the reader who arrives
by keyboard.

## The terminal

Shells and harness sessions are one component, so one change covers both. One link provider
(`registerLinkProvider`) over `findLinks` finds URLs and paths alike and hands the handler the hit
with the terminal's own root — no web-links addon, so the terminal reads the same words as every
other surface. It reads the **logical line** a row belongs to (`terminal/terminalLinksModel.mjs`,
tested): the rows xterm folded from one long line (`isWrapped`), and the rows a harness's own frame
broke at the column edge with a real newline — a row full to its last column with a non-space
there, followed by a row starting with a non-space — joined a bounded eight rows each way
(`logicalLine`); every span is mapped back to the cells it covers (`linksAt`), so a link the edge
cut in two is underlined across its rows and a ⌘-click on either row opens the same door. A row
that ends short, or a next row that starts with a space, is a line break someone meant.
**⌘-click**, as before: a plain click selects. The URL no longer opens silently.

**A relative path is read from where the shell stands.** The click asks the shell for its current
directory (`session.cwd()`, the desktop shell's `terminal_cwd` — `sysinfo`'s word for the login
shell's pid, one process refreshed and never the table; the directory it was started in once the
process is gone, so a dead tab's scrollback still reads its paths) and hands it to the handler as
`from`: the `src/lib.rs` a compiler printed after a `cd` into a crate opens that crate's file, a
`../README.md` climbs, a `target/out.log` or `.github/workflows/ci.yml` no index lists is asked of
the node, and a shell that cannot say where it is reads from its root alone, as before. The
directory is read at the click, not kept: the shell is asked, never told, and the webview still
names no path to anyone ([06 §What does not change](06-terminals.md)). A nested shell's own `cd` is
not followed — its paths are found by their tail or asked of the node.

## What is not a door

- The end of a text selection — a drag that finishes on a path, a double-click on one of its words
  (§The handler).

- A row of the Pulse or the Inbox is already one button; its door is the row. The paths inside its
  expanded detail are doors. The Inbox's detail pane has doors of its own: *Open* in its header,
  and each notice under *What happened* opens what the Pulse's row for it would.
- A port chip on a workstream row and the code host's *Authenticate* open at once: a named button
  is the person's own act, not a link in text.
- A `«secret:…»` placeholder, and anything that follows it without a space.
- A relative link inside a rendered document (`./notes.md`) is the document's own, resolved
  against the file it sits in by `docLink.mjs` — it opens as another document and never reaches
  the handler; one that climbs out of the root is inert. A same-document link (`#build-and-upload`,
  a table of contents') names a heading — every heading of a document carries GitHub's anchor
  (`ui/headingAnchorsModel.mjs`) — and the rendering scrolls to it (`docLink.fragmentOf`,
  `ui/docAnchors.scrollToFragment`); the window never follows the hash ([03 §Rendered
  documents](03-files-and-editing.md#rendered-documents)). A relative **image** (an `![…]` whose
  source is a sibling file such as `diagram.png`)
  is the document's own the same way: the sanitizer keeps its path as `data-doc-src`, and the
  rendered view reads it through the node's byte route into a blob URL
  ([03](03-files-and-editing.md#rendered-documents)); one that climbs out stays blank.

## Tests

| Promise | Held by |
|---|---|
| the grammar: every prefix, `:42`, `:42:7`, `#L12`, the punctuation left behind, a bare name only with a known extension, a number never, a boundary before a path | `ui/linkModel.test.mjs` |
| a URL is a URL, a `www.` host given its scheme, a URL's own `)` kept, the sentence's dropped | `ui/linkModel.test.mjs` |
| a redacted secret is never a link, whatever it holds or is followed by | `ui/linkModel.test.mjs` |
| the HTML pass: an anchor in text and inline code, a `<pre>` block untouched, an existing anchor tagged and never nested, a neutered `#` left alone, entities kept | `ui/linkModel.test.mjs` |
| the resolver: an absolute and a `~` path under the longest root, ignored-but-contained still opens, a directory, outside, a relative path in one root, in two, a bare name by its file, `..` with no base refused; from where the shell stands — a sub-crate's own `src/lib.rs`, a climb to the root and past it, into another root, the root itself; unlisted — the guess from the shell or the first root, confirmed by the node's listing as a file, a directory, or nothing; a slashed path by its tail, bounded; `absolutePath` | `ui/linkModel.test.mjs` |
| the grammar climbs as far as it says (`../../lib/a.ts`) and reads a hidden folder's path (`.github/workflows/ci.yml`); a dotfile alone is a word | `ui/linkModel.test.mjs` |
| where a shell stands is its process's word, follows its `cd`, and is where it started once the process is gone; a pid nobody holds says nothing; an unknown terminal is refused | `desktop/src-tauri/src/terminal.rs` unit tests |
| a message of two thousand lines is scanned and marked well inside a frame | `ui/linkModel.test.mjs` |
| a relative link in a document resolves and refuses to climb; a same-document link names its heading's fragment, decoded and lower-cased, and a document's link, a bare `#` or nothing names none | `views/_workbench/docLink.test.mjs` |
| a document's headings carry GitHub's anchors — GitHub's own example, a numbered heading, markup and entities, repeats counted, a heading with an id or no words left alone — in a document alone, and a table of contents' fragment is the anchor its heading got | `ui/headingAnchorsModel.test.mjs`, `scenarios/files.test.mjs` |
| the card: a path's verbs per resolution — one document opens, several are a door each, outside opens loose or reveals, unknown copies alone; a URL's — the embedded browser first, then the machine's, `http` and `https` alone; the roots a surface names none of, capped at eight | `shell/linkCardModel.test.mjs`, `scenarios/browser.test.mjs` |
| the click that lands last is the card drawn; a resolution for a click superseded draws nothing | `shell/latestModel.test.mjs` |
| a path from a message and from the terminal's grammar reaches the same card, with the same roots; a relative path read from where the shell stands reaches the same card, the crate's own file revealed; the terminal asks the shell at the click and hands `from` | `scenarios/files.test.mjs` |
| no view reaches an anchor library or the opener around the kit | `ui/imports.test.mjs` |
