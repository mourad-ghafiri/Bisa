# Inbox and Pulse

The Inbox is the one place for what concerns a person: what is owed to them — an **ask**: a gate, a
question, a held `wait` step — and what happened to the things they are tied to — a **notice** —
one row per thing, updated in place. The Pulse is the whole feed: every fact across the platform —
every journal's, every message, and each of the engine's that has a concept —
newest first, narrowed by concept. In code, the store writes every row of the feed through one
writer and says so on the feed's own channel; the engine decides which facts are notices and which
row each lands on, records each of its own facts before its frame goes out, builds every Inbox row
and every page of the feed in one place; the node serves `GET /inbox`, `GET /pulse` and the stream's
`pulse` frames; and the command line and the desktop read the same page and word it with one
renderer each.

## Where it lives

- `crates/bisa-engine/src/notices.rs` — `NoticeKind`, `InboxKind`, the row a fact lands on (`row_of`, `target_of`), and `NOTICE_TAGS`, the activity kinds worth reading.
- `crates/bisa-engine/src/activity.rs` — the engine's facts into the feed, one concept and one source per payload (`concept_of`, `source_of`), recorded before the frame is published through the one door every sender uses (`publish`).
- `crates/bisa-engine/src/feed.rs` — the feed's row and its one page builder (`page`): each record typed, each engine fact that only repeats a journal fact marked as its echo (`echo_of`).
- `crates/bisa-engine/src/inbox/` — **the one builder**: `rows` (the list), `row` and `delta` (one thing's row and its frame), `needs_actions_for` and `needs_actions_for_run` (a goal's and a run's asks), each row made by `build` over one set of facts (`Facts`), read through a `Reader` — the workspace, what is live (`LiveView`: the open gates, the terminal harnesses; none with no engine) and how a sentence is said; `types.rs` holds the wire shapes. `crates/bisa-engine/src/feed.rs` — a feed row typed back from the store (`typed`, `Titles`), which a notice carries verbatim.
- `crates/bisa-store/src/index.rs` (the `activity` table and its three reads) · `crates/bisa-store/src/workspace.rs` (`feed_row_in`, the feed's one writer, and `subscribe_activity`, the channel it says each row on) · `crates/bisa-store/src/activity_log.rs` · `crates/bisa-store/src/inbox.rs` — the feed's index, the activity log on disk; the decision rows and read markers.
- `crates/bisa-node/src/inbox.rs` · `crates/bisa-node/src/pulse.rs` — the two routes: the three buckets (`?filter=`) and five sources a request narrows by, the engine's builder read with the engine's own view of what is live, the feed's keyset pages.
- `crates/bisa-cli/src/inbox.rs` (`bisa inbox`: the node's rows, or the builder's with no node; `hint_of`, the command under an ask) · `crates/bisa-cli/src/main.rs` (`bisa answer`, `bisa approve`) · `crates/bisa-cli/src/pulse.rs` (`bisa pulse`: the node's page, or the same builder's over the workspace with no node) · `crates/bisa-cli/src/studio.rs` (`bisa read`) · `crates/bisa-cli/src/activity.rs` (one renderer: a typed event's line, and everything that arrives as JSON read back into one first) — the same rows and lines in a terminal.
- `desktop/src/views/Inbox.tsx` · `desktop/src/views/_studio/inboxModel.mjs` — the screen, and the model that decides its sources, groups, counts, words and chords.
- `desktop/src/views/Pulse.tsx` · `desktop/src/views/_pulse/pulseModel.mjs` — the feed, its concepts and its pages: what a frame of the feed asks (`nudgeOf`), how a head page joins (`joinHead`), which rows are drawn (an echo is not) and which have a door.
- `desktop/src/activityModel.mjs` — one rendered line per engine payload; `desktop/src/shell/sidebarModel.mjs` — the badge's count.

## Read first

- [The desktop § Inbox](../../guide/the-desktop.md#inbox) — a row is a thing, asks and notices, the three groups, the badge, the three shapes of a decision.
- [The desktop § Pulse](../../guide/the-desktop.md#pulse) — the concepts, the payload rendered verbatim, paging.
- [13 — Conversations § Lists, search, coming back](../../architecture/13-conversations.md#lists-search-coming-back) — a conversation's row, and a row read as it is shown.
- [crates/node](../../architecture/crates/node.md) — the `inbox.rs` · `pulse.rs` row: sources, durable asks, the feed's pages and what it skips.
- [crates/engine](../../architecture/crates/engine.md) — the `notices.rs` and `activity.rs` rows.
- [Terminology](../terminology.md) — *ask*, *notice*, *row*; a *notification* is the OS's.

## Rules a change must keep

- A row is a thing, never an event; it is updated in place, and a row a person read stays ([guide § Inbox](../../guide/the-desktop.md#inbox)).
- A notice is one of the named facts in `NoticeKind` and lands on the thing it concerns; a done step, an allowed guard or a passing script is no notice, and a person's own save of a workflow earns none ([Testing rules § Guard tests](../testing-rules.md#guard-tests)).
- What is owed is the asks alone: a notice never moves a goal card's count or who holds the ball ([guide § Inbox](../../guide/the-desktop.md#inbox)).
- An ask survives a restart: it is rebuilt from the run — a waiting `human` step, a waiting `approval` step, a held `wait` — and from the journal — an un-adopted proposal, a held amendment, an agent's question nothing settled, each judged by its own subject — through one builder, so the Inbox, the goal page, the run page and the command line carry the identical `NeedsAction` ([crates/engine](../../architecture/crates/engine.md)).
- A frame is its row: `delta` projects the row `rows` would hold for the key, so a frame and the list say the same of it; a key with no row says `listed: false`.
- One key, one row: a project and its primary workstream share an id and are the project's row.
- An ask names its question: every ask carries its `subject`, and a decide that sends it decides that question alone — a card rebuilt after a restart has nothing else to name (`decide_at` in `crates/bisa-node/src/goals.rs`, `Engine::decide_subject`).
- A read is up to what was shown: a client sends the row's `latest_at` as `at`, the mark never moves backwards, and a mark is kept only for a key a row could have (`bisa_engine::inbox::is_key`).
- The node ships the event, not a sentence: a Pulse row carries the payload and the desktop renders it, so a row reads the same live and after a reload ([guide § Pulse](../../guide/the-desktop.md#pulse)).
- One broken thing costs one row: a row the build cannot type is skipped and said at `warn`, never the reason a page fails (`crates/bisa-node/tests/it/resilience.rs`) — so a page may hold fewer rows than asked for, or none, and still name a cursor: `next` says whether there is more, a page's length never does.
- A fact is one row: written by the store's one writer (`Workspace::feed_row_in`), once however it arrives — a goal synced again from a peer adds none — and gone when its message is taken back. `seq` is the insertion order, renumbered by a rebuild: a tiebreak, never a fact's name.
- One happening is drawn once: the journal's fact and the engine's fact of it are both rows, and the engine's carries `echo_of` (`feed::page`, the one twin rule). A reader draws the journal's row; the Inbox's notices, which are engine rows, ignore the mark.
- The feed says its own rows: the stream's `pulse` frame names each row as it lands, with the concept it is filed under. No client keeps a table of the engine's facts to guess a concept from a payload.
- *Read* is a local watermark per row, never synced; *Handled* is a signed fact of the workspace ([guide § Inbox](../../guide/the-desktop.md#inbox)).
- The keys never type into an answer: an ask takes focus when the person came for it — a click, the ask key, a link — never on a move or a frame (`askFocus`); a selection the person cleared is not picked again for them, and a row they put back unread is not read again while it is open.
- The model decides and the screen paints: every fact of the Inbox and the Pulse lives in its `.mjs` model with a test ([Add a desktop model](../recipes.md#7-add-a-desktop-model)).

## Testing a change

- `scripts/test lib engine notices` — the notice table; `scripts/test lib engine inbox` — the builder's own rules; `scripts/test lib engine feed` — typing, titles, the twin rule; `scripts/test module engine activity` — every frame of a real run that is a fact is read back from the feed as it is heard, and a run's engine facts are the echoes of its journal's.
- `scripts/test module store activity` — one row a fact, a row that leaves, the feed's notices, the rebuild row for row; `scripts/test lib store ingest` — a peer's facts once; `scripts/test lib store the_page_seeks` — the three reads' query plans.
- `scripts/test lib cli pulse` · `scripts/test lib cli activity` — the verb's rules, and every engine payload's line read three ways (live, streamed, stored).
- `scripts/test module node node` · `scripts/test module node resilience` — the filters and sources, durable rows, a failed run still read through `/inbox` and `/pulse`.
- `scripts/test desktop views/_studio` · `scripts/test desktop views/_pulse` — the models; then the two journeys a person lives, from `desktop/`: `node --test --import ./src/i18n/preload.mjs src/scenarios/inbox.test.mjs src/scenarios/pulse.test.mjs`.
- Guards: `desktop/src/activityModel.test.mjs` (every engine payload renders a line), `desktop/src/shell/sidebarModel.test.mjs` (the badge's count is exact), `desktop/src/views/_studio/inboxModel.test.mjs` (every notice wears a category).
- The journey `crates/bisa-cli/tests/it/e2e/pulse_and_inbox.rs` — an ask until it is answered, a failed run until it is read, the feed paged newest first — runs with the others under `scripts/test module cli e2e`.
- One module at a time while you work; the whole workspace, `npm test` and `just verify` only as the gate at the end ([Testing rules § Running](../testing-rules.md#running)).

## Common changes

- A fact the Pulse should show: [Add an engine event](../recipes.md#4-add-an-engine-event) — its concept and its source in `activity.rs` (both matches are exhaustive), its line in `activityModel.mjs` and in the CLI's `engine_line`, a sample in each one's tests. If a journal fact says the same happening, a pair in `feed::echo_key`.
- A new notice: its kind and tag in `crates/bisa-engine/src/notices.rs` (`notice_of`, `notice_of_payload`, `NOTICE_TAGS`), the row it lands on, its words and tone in `inboxModel.mjs`; `desktop/src/scenarios/inbox.test.mjs` fails until every tag is among the frames that read the list again.
- A filter, a source or a field on the routes: [Add an HTTP route](../recipes.md#1-add-an-http-route).
- What a row says: [Say something to a person](../recipes.md#26-say-something-to-a-person).

## Compatibility

- `GET /inbox`, `GET /pulse`, their query words and row shapes, and the frames of `GET /events` are [the HTTP API and its events](../../reference/compatibility.md#the-http-api-and-its-events); `bisa inbox`, `bisa answer`, `bisa approve` and `bisa pulse` are [the command line](../../reference/compatibility.md#the-command-line).
- The activity log is part of [the workspace on disk](../../reference/compatibility.md#the-workspace-on-disk); read marks live in the index, which a rebuild clears and which is not promised.
- A change stays additive: a new notice kind, source, concept or field sits beside the old ones, and a client ignores what it does not know. Renaming a source word, a concept or a field, or dropping one, cannot be made by addition and waits for 1.0.0 ([Keeping compatibility](../compatibility.md)).
- Inside 0.x a minor or a patch never breaks the public contract; declare your change's compatibility in the pull request ([Compatibility](../../reference/compatibility.md)).

## Review focus

- One decision, one place: which fact is a notice in `notices.rs`, the asks and the rows in the engine's one builder, a page of the feed and its twins in `feed::page`, the words in the desktop's model and the CLI's one renderer — never recomputed in a component, a second route or a second door of the command line ([Code review](../review/code.md)).
- Nothing new is owed by a notice, and an ask outranks a notice on a row's line.
- Reads stay bounded — the Pulse holds ten pages, the Inbox reads the feed once — and a frame patches a row where it sits; the list is read again only when a frame says it must ([Performance review](../review/performance.md)).
- Wire shapes only grow ([Compatibility review](../review/compatibility.md)); the words are *ask*, *notice* and *row* ([Docs and language](../review/docs-and-language.md)).
