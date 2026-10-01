# 14 — Performance

The IDE must stay responsive on a large repository. These are the budgets it is designed against,
the fixtures the two benches build, and how each budget is measured.

---

## The fixtures

Two synthetic fixtures, each built by its bench into a temporary directory under `target/`, never a
user's repository; nothing in a bench removes anything — a stale fixture is rebuilt beside the old
one.

| Bench | Fixture | Size knob |
|---|---|---|
| `crates/bisa-engine/benches/search.rs` | a tree of source-like files with a realistic size distribution | `SEARCH_BENCH_FILES` |
| `crates/bisa-engine/benches/graph.rs` | a commit log with branches and merges, fed to the layout | `GRAPH_BENCH_COMMITS` |

---

## Budgets

The targets the IDE is designed against, and how each is measured today.

| Budget | Target | Where measured |
|---|---|---|
| Keystroke → glyph | < 16 ms p95 | by hand, with the browser profiler against Monaco's performance marks; not CI (no webview there) |
| Open a ≤ 1 MiB file | < 120 ms p95 | by hand; the route is one read and one hash |
| Explorer expand, 1,000 entries | < 80 ms | `fileTreeModel.test.mjs` exercises the arithmetic; timing by hand |
| `git status`, 50k files | < 400 ms p95 | by hand; the status is cached per workstream for `cache.git_status.ttl_ms` and dropped by the watcher |
| Commit graph first paint | < 150 ms | `benches/graph.rs` — the first thousand rows are laid out inline |
| Commit graph full layout | < 2 s, background | `benches/graph.rs` |
| Quick open first results, 100k paths | < 50 ms | by hand; `quickOpenScore.test.mjs` holds the scoring rules |
| Content search, 50k files, first page | < 500 ms | `benches/search.rs` |
| Terminal throughput | ≥ 10 MB/s sustained | by hand, a piped generator into the WebGL renderer |
| Agent chat first token | < 800 ms | the SSE timing in the desktop |
| Agent chat sustained reveal | one row re-rendered per frame; only the tail block re-parsed | `liveTurnsStore` reads per row, `StreamedMarkdown` over `streamBlocksModel.test.mjs`, the pacer's `streamPacerModel.test.mjs`; by hand on a long reply |
| Settings resolution | < 1 ms | a pure function over the registry; unit-tested, not timed |

---

## How CI holds them

CI compiles every bench (`cargo bench --workspace --no-run`) and runs the commit-graph layout bench
on a small synthetic log (`GRAPH_BENCH_COMMITS=20000`), so a regression that goes quadratic shows up
as a wall-clock jump in the log. **Criterion reports; nothing gates on the numbers** — no run asserts a
ratio ([feature status](../../feature-status.md), *Benchmarks*). `just bench-search` and
`just bench-graph` run the two engine benches by hand; the webview-bound budgets (keystroke,
terminal throughput) cannot run headless and are measured by a person.

---

## Design choices these budgets forced

- The commit graph is laid out once and cached as fixed-width rows ([05](05-commit-graph.md)),
  because layout is stateful from the top and cannot be done per scroll window.
- Per-workstream status is cached and invalidated by the watcher ([07](07-workstreams.md)), because
  running `git status` across every checkout on every keystroke fails the switcher budget outright.
- Files over 2 MiB open read-only with tokenisation off ([03](03-files-and-editing.md)), because
  Monaco fails the keystroke budget past roughly 10 MiB and pretending otherwise is how an IDE earns
  a reputation for hanging.
- The path index for quick open is streamed once and patched by `FileChanged`, never re-walked per
  query.
- A language server runs only while a document of its language is open ([10](10-language-intelligence.md)).

## Photos

A photo is an `AttachmentRef` — content-addressed bytes, drawn at 14 to 32 px wherever its owner
is named — on a **project** and a rail **group**, an **agent**, a **team**, and every **person**:
the workspace owner and every member, whose face is set on their own node and travels with their
profile to every workspace they are a member of ([14 — Collaboration](../14-collaboration.md)).
One pipeline, two **profiles** (`ui/photoModel.mjs` `PHOTO_PROFILES`; `bisa_core::PhotoProfile`):
a **picture** — a project's, a group's, an agent's, a team's — is a 256 px PNG kept on this
machine under `MAX_PHOTO_BYTES` (512 KiB); a **face** — a person's — is a 96 px JPEG under
`MAX_FACE_BYTES` (16 KiB), since it rides the collaboration control channel gift-wrapped once per
recipient and public relays refuse an event past a few dozen kilobytes. Three layers keep both
cheap however they were set:

- **Scaled at intake.** The desktop decodes a picked picture once (`ui/photoScale.ts`
  `scalePhoto(file, profile)`: `createImageBitmap`, a canvas, `toBlob`), cover-crops it to a square
  and draws it at the profile's edge — a picture crisp at 32 px on a 3× display, a few dozen
  kilobytes as PNG; a face as JPEG at the profile's qualities in turn (0.85, 0.7, 0.5) until it
  fits the cap, refused in words when it will not — and *that* is what `POST /attachments`
  receives, named `<stem>-photo.png` or `<stem>-face.jpg` (`ui/photoModel.mjs`: `coverCrop`,
  `photoName`, `isPhotoType`, `fitsProfile`, `photoRefusal`). The camera's file never leaves the
  machine; a picture is never scaled up. **One picker** sets every photo: `ui/PhotoField.tsx` —
  the `Avatar` preview, *Choose a photo* / *Change photo* / *Remove*, the hidden input — in the
  project dialog, the agent editor, the team dialog and Settings › Identity (the rail's menus
  keep their own input, opened inside the gesture).
- **Thumbnailed at draw.** Whatever the stored bytes are — a photo set before scaling, a face a
  host sent — `ui/photoThumbs.ts` (`usePhotoThumb(sha)`) fetches them once per sha per run,
  draws a `THUMB_EDGE` = 64 px square and hands every draw site the same object URL from a
  bounded cache (`THUMB_CACHE_MAX` = 200, the least recently drawn let go — `evictOrder`); a
  workspace reload redraws from the cache and fetches nothing; a photo that cannot be read is
  `null` for the run. `Avatar` takes the photo **by reference** (`photo={…}`) and reads the
  thumbnail itself, so a list row, a message, a picker chip and a menu row are one prop from the
  face; it decodes lazily and asynchronously, its box sized before the bytes. A principal's face is
  one lookup (`useWorkspaceData.photoOf(pubkey)` over `photoModel.photoOfPrincipal`: the member's
  row — the owner's own included — else the agent's; `hostedModel.hostedPhotoOf` for a hosted
  workspace's directory).
- **Bounded and immutable at rest.** Every route that attaches a photo — `PATCH /projects/{pid}`,
  `POST`/`PATCH /agents`, `POST`/`PATCH /teams`, `PUT /workspace/me` — passes the one check
  (`bisa_core::check_photo`, the node's `attachments::photo_check`): a content hash, held on this
  machine, a picture by its header (`image_type`), within the profile's cap, refused by name
  otherwise; `GET /attachments/{sha}` answers every attachment with `Cache-Control: public,
  max-age=31536000, immutable` and an `ETag` of the hash, and a request that already holds it
  gets 304 with no bytes read.

