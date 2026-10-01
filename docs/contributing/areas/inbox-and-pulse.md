# Inbox and Pulse

The Inbox is the one place for what concerns a person: what is owed to them — an **ask**: a gate, a
question, a held `wait` step — and what happened to the things they are tied to — a **notice** —
one row per thing, updated in place. The Pulse is the whole feed: every fact across the platform,
newest first, narrowed by concept. In code, the engine decides which facts are notices and which row
each lands on, records every fact into the activity feed before its frame goes out, the node serves
`GET /inbox` and `GET /pulse`, and the desktop renders both from pure models.

## Where it lives

- `crates/bisa-engine/src/notices.rs` — `NoticeKind`, `InboxKind`, the row a fact lands on (`row_of`, `target_of`), and `NOTICE_TAGS`, the activity kinds worth reading.
- `crates/bisa-engine/src/activity.rs` — the engine's facts into the feed, one concept per payload (`concept_of`), recorded before the frame is published.
- `crates/bisa-store/src/activity_log.rs` · `crates/bisa-store/src/inbox.rs` — the activity log on disk; the decision rows and read markers.
- `crates/bisa-node/src/inbox.rs` · `crates/bisa-node/src/pulse.rs` — the two routes: three filters and five sources, the asks rebuilt from a run (`needs_actions_for`, `needs_actions_for_run`), the feed's keyset pages.
- `crates/bisa-cli/src/main.rs` (`bisa inbox`, `bisa answer`, `bisa approve`) · `crates/bisa-cli/src/studio.rs` (`bisa pulse`) · `crates/bisa-cli/src/activity.rs` — the same rows and lines in a terminal.
- `desktop/src/views/Inbox.tsx` · `desktop/src/views/_studio/inboxModel.mjs` — the screen, and the model that decides its sources, groups, counts, words and chords.
- `desktop/src/views/Pulse.tsx` · `desktop/src/views/_pulse/pulseModel.mjs` — the feed, its concepts and its pages.
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
- An ask survives a restart: it is rebuilt from the run — a waiting `human` step, a running `approval` step, a held `wait`, an un-adopted proposal — through one builder, so the Inbox and the goal page carry the identical `NeedsAction` ([crates/node](../../architecture/crates/node.md)).
- The node ships the event, not a sentence: a Pulse row carries the payload and the desktop renders it, so a row reads the same live and after a reload ([guide § Pulse](../../guide/the-desktop.md#pulse)).
- One broken thing costs one row: a row the build cannot type is skipped and said at `warn`, never the reason a page fails (`crates/bisa-node/tests/it/resilience.rs`).
- *Read* is a local watermark per row, never synced; *Handled* is a signed fact of the workspace ([guide § Inbox](../../guide/the-desktop.md#inbox)).
- The model decides and the screen paints: every fact of the Inbox and the Pulse lives in its `.mjs` model with a test ([Add a desktop model](../recipes.md#7-add-a-desktop-model)).

## Testing a change

- `scripts/test lib engine notices` — the notice table; `scripts/test module engine activity` — facts reach the feed before their frames.
- `scripts/test module store activity` — the activity log and its rebuild.
- `scripts/test module node node` · `scripts/test module node resilience` — the filters and sources, durable rows, a failed run still read through `/inbox` and `/pulse`.
- `scripts/test desktop views/_studio` · `scripts/test desktop views/_pulse` — the models; then the two journeys a person lives, from `desktop/`: `node --test --import ./src/i18n/preload.mjs src/scenarios/inbox.test.mjs src/scenarios/pulse.test.mjs`.
- Guards: `desktop/src/activityModel.test.mjs` (every engine payload renders a line), `desktop/src/shell/sidebarModel.test.mjs` (the badge's count is exact), `desktop/src/views/_studio/inboxModel.test.mjs` (every notice wears a category).
- The journey `crates/bisa-cli/tests/it/e2e/pulse_and_inbox.rs` — an ask until it is answered, a failed run until it is read, the feed paged newest first — runs with the others under `scripts/test module cli e2e`.
- One module at a time while you work; the whole workspace, `npm test` and `just verify` only as the gate at the end ([Testing rules § Running](../testing-rules.md#running)).

## Common changes

- A fact the Pulse should show: [Add an engine event](../recipes.md#4-add-an-engine-event) — its concept in `activity.rs`, its line in `activityModel.mjs`.
- A new notice: its kind and tag in `crates/bisa-engine/src/notices.rs` (`notice_of`, `notice_of_payload`, `NOTICE_TAGS`), the row it lands on, its words and tone in `inboxModel.mjs`; `desktop/src/scenarios/inbox.test.mjs` fails until every tag is among the frames that read the list again.
- A filter, a source or a field on the routes: [Add an HTTP route](../recipes.md#1-add-an-http-route).
- What a row says: [Say something to a person](../recipes.md#26-say-something-to-a-person).

## Compatibility

- `GET /inbox`, `GET /pulse`, their query words and row shapes, and the frames of `GET /events` are [the HTTP API and its events](../../reference/compatibility.md#the-http-api-and-its-events); `bisa inbox`, `bisa answer`, `bisa approve` and `bisa pulse` are [the command line](../../reference/compatibility.md#the-command-line).
- The activity log is part of [the workspace on disk](../../reference/compatibility.md#the-workspace-on-disk); read marks live in the index, which a rebuild clears and which is not promised.
- A change stays additive: a new notice kind, source, concept or field sits beside the old ones, and a client ignores what it does not know. Renaming a source word, a concept or a field, or dropping one, cannot be made by addition and waits for 1.0.0 ([Keeping compatibility](../compatibility.md)).
- Inside 0.x a minor or a patch never breaks the public contract; declare your change's compatibility in the pull request ([Compatibility](../../reference/compatibility.md)).

## Review focus

- One decision, one place: which fact is a notice in `notices.rs`, the asks in the node's one builder, the words in the desktop's model — never recomputed in a component or a second route ([Code review](../review/code.md)).
- Nothing new is owed by a notice, and an ask outranks a notice on a row's line.
- Reads stay bounded — the Pulse holds ten pages, the Inbox reads the feed once — and a frame patches a row where it sits; the list is read again only when a frame says it must ([Performance review](../review/performance.md)).
- Wire shapes only grow ([Compatibility review](../review/compatibility.md)); the words are *ask*, *notice* and *row* ([Docs and language](../review/docs-and-language.md)).
