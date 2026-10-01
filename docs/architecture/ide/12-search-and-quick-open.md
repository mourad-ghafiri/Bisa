# 12 — Search and quick open

Find and replace across a project, and one palette over files, symbols, commands, workstreams, work
items and agents — reached with the key that already opens the switcher.

---

## Content search

`engine::ide::search` uses ripgrep's own library crates — `grep-searcher`, `grep-regex`, `ignore`
 — so there is no external binary to
install and `.gitignore` is respected by construction; there is no page explaining how to install
`rg`, because there is no `rg`.

```
GET /ide/search/{scope}/{id}?q=&regex=&case=&word=&include=&exclude=&limit=
→ streamed frames { type: "hit", hit: { path, line, column, text, before, after } },
  then one { type: "done", matches, files_with_matches, files_scanned, truncated }
```

Results stream over SSE (`{type: "hit", hit}` frames, then `{type: "done", matches,
files_with_matches, files_scanned, truncated}`) so the first hits render while the rest scans.
Bounded: `limit` defaults to 2,000 matches and a caller's own reaches 20,000 at most (a limit of
nothing is one); a pattern that is no regex, is empty, or is too large to build is `400` before the
stream opens, and the engine is linear in its input, so no pattern sends it round for ever; a
reader that leaves ends the walk at its next hit; binary files are skipped by the crate's own detection; a
symlink is never followed. The desktop's door is the **Files tab's search box** ([03](03-files-and-editing.md#the-explorer)): `api.ideSearch` consumes the stream through one
`EventSource` helper (`api.sse`, the token in the query as the bus does), hits are grouped by file
(`fileSearchModel.mjs`), a hit — clicked, or stepped to with ↑↓ — opens the document **as a preview** at its line and reveals the path in the tree (one tab is reused while stepping; `Enter` keeps it), and
the query is the box's — a search is not a place in the URL. The same box's *Names* mode ranks the
path index below.

**Replace across files** is a preview first and then one CAS write per file through
`engine::ide::files` — `POST /ide/replace/{scope}/{id}` without `apply` answers every change with
the file's hash, with `apply` and those hashes writes them. Its doors are the CLI (`bisa files
replace --apply`) and the API; the Files tab has none ([feature status](../../feature-status.md)).
An agent editing one of those files between the preview and the apply is reported in the result as
*skipped: changed since preview*, never a clobbered; a path that leaves the root is refused unread.

---

## Quick open

`⌘K` is the omnibox — search and jump across the whole workspace. `⌘P` is the workbench's root
switcher, and the studio guide already describes it as *the ⌘K palette in a different mode rather
than a second component*. The IDE extends that mode into quick open rather than adding a third
surface.

Rows, each section capped so one category cannot flood the list (a cap per section is the
reference's `palette-section-render-cap`, and it is a `.mjs` constant here):

| Section | Source | Row carries |
|---|---|---|
| Files | a path index the node streams for the current root (`GET /ide/index/{scope}/{id}`), cached once per root in `shell/pathIndexStore.ts` — shared with the Files tab's name search — and patched by `FileChanged` **taken whole** (`quickOpenScore.patchIndex` over `listed`: a folder made is no file and one made whole is read again, a path the ignore rules match or a hidden name is never put in, a rename out of sight leaves — the store once passed the path alone, so folders were listed as files and every ignored file flooded the index; `quickOpenScore.test.mjs`, `scenarios/files.test.mjs`), forgotten on a `rescan`. The fetch is a **caller-independent single flight** (`shell/singleFlight.mjs`): two askers share one request, one of them leaving does not end it, the last one leaving does, and a flight that is aborted or fails is forgotten so the next asker starts again — which is what lets a component mount, unmount and mount again, as React's strict mode does, and still get its index | path, dirty dot if open |
| Symbols | `workspace/symbol` from the running language server, when one is up | kind glyph, container |
| Commands | the command registry (`keymapModel.mjs`) | the binding under the active preset |
| Projects and workstreams | `GET /projects` (a project row opens its primary) and `GET /workstreams` | branch, ahead/behind, dirty count, running agents |
| Work items | `GET /work-items` for attached goals | state, the goal |
| Agents | the addressing directory | live status, harness installed or not |
| Terminals | the panel's sessions | liveness |
| Conversations | `GET /conversations`, live ones — by a title or a first line ([13 — Conversations](../13-conversations.md)) | the origin in words; one about a checkout opens in the IDE on its Agent panel, one about a goal on its Conversation tab, one about a workflow in the designer's Agent pane, one about the workspace or the node on a page of its own |

Typed prefixes narrow: `@` symbols, `>` commands, `:` a line number in the current file, `#` work
items. `⌘⇧P` opens the same palette with `>` pre-typed.

**Scoring** is a TypeScript `.mjs` model (`quickOpenScore.mjs`), tested, over the streamed index —
the approach VS Code takes and one that meets the budget in [14](14-performance.md) on 100k paths.
Two Rust crates were weighed and not taken: `nucleo` is MPL-2.0 and outside the licence allowlist;
`fuzzy-matcher` is MIT and thinly maintained. The budget is measured by hand ([14](14-performance.md)).

Rows carry **live status** — an agent thinking, a workstream with dirty files, a terminal that exited —
and the keyboard-selected row uses the contrast recipe the notes dock already uses (a wash plus an
inset ring), because on a light popover a flat accent fill is nearly the background and the cursor
vanishes.
