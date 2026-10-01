# Channels and conversations

For a person, this is where people and agents talk: standing channels with rosters and the permanent
`general`, direct channels with one other person or agent, conversations started with agents about the
node, the workspace, a goal, a workflow, a project, a workstream, a note or a drawing — and the artifacts
an agent makes for a person to look at, a page, a sheet, a deck, rendered live under the message. In
code, channels, messages and conversations are the core's types, logs and records in the store, triage,
turns and the streamed reply in the engine, and routes in the node.

## Where it lives

- `crates/bisa-core/src/channel.rs`, `crates/bisa-core/src/message.rs`, `crates/bisa-core/src/conversation.rs`, `crates/bisa-core/src/artifact.rs` — rosters, `DeletableChannel`, a message's scope and context chips, a conversation's origin, an artifact's kind.
- `crates/bisa-store/src/channels.rs`, `crates/bisa-store/src/conversation.rs`, `crates/bisa-store/src/conversations.rs` — the logs, mentions, read marks, the conversation records.
- `crates/bisa-engine/src/conversation.rs`, `crates/bisa-engine/src/conversations.rs`, `crates/bisa-engine/src/messaging.rs`, `crates/bisa-engine/src/framing.rs` — triage and wakes, a turn and its streamed reply, posting, what a session is told.
- `crates/bisa-node/src/conversation.rs`, `crates/bisa-node/src/conversations.rs`, `crates/bisa-node/src/attachments.rs`, `crates/bisa-node/src/inbox.rs` — channels, direct channels and messages; conversations; attachments and artifacts; the Inbox.
- `desktop/src/views/_studio/` — the conversation surface, the timeline, the Inbox; `desktop/src/ui/artifact/` — the artifact viewers.

## Read first

- [Channels guide](../../guide/channels.md), [Artifacts guide](../../guide/artifacts.md) — as a person meets them.
- [05 — Channels](../../architecture/05-channels.md) — standing and direct channels, `general`, membership as a policy, triage.
- [13 — Conversations](../../architecture/13-conversations.md) — origins, where a turn runs, lists and search, [the reply streaming](../../architecture/13-conversations.md#the-reply-streams).
- [12 — Artifacts](../../architecture/12-artifacts.md) — the model, producing one, rendering, the two walls.
- [ide/09 — Agents in the IDE](../../architecture/ide/09-agents-in-the-ide.md) — context as chips a person can see.

## Rules a change must keep

- `general` cannot be deleted: `delete` takes a `DeletableChannel`, which cannot be made for it (I16).
- Implicit membership is never stored: a roster that means everyone stores no list (I18).
- A disabled agent cannot be addressed and an unknown mention is refused (I20, I21); a message that names nobody wakes the General Agent alone, a chain is two hops by construction, and an announcement wakes only whom it names (I22, [05 § Triage](../../architecture/05-channels.md#triage-a-message-that-names-nobody)).
- A direct channel is one stored object with a restricted audience and needs somebody else; a surface that means "rooms you can join" asks for standing channels, and the filter lives in the store.
- A post is words or nothing and at most 256 KiB (`MAX_TEXT_BYTES`), refused in words with the way to send a file; a page's `limit` is clamped to 1–200, and a page cut inside one second loses nothing.
- Nothing an agent session receives is absent from the chips the person sees above the composer (I43).
- A conversation is not a channel and not a goal's thread: an origin instead of a name, no roster, nothing triaged between conversations; the platform summarises nothing.
- An agent's page is untrusted: a sandboxed frame with its own Content-Security-Policy, given as `srcdoc` — the node never serves a page; an SVG is drawn as an image; a ninth artifact on a message is refused ([12 § Security](../../architecture/12-artifacts.md#security)).
- The words: *direct channel*, *conversation*, *thread* (a goal's alone), *artifact*, *attachment*, *result* ([Terminology](../terminology.md)).

## Testing a change

- `scripts/test module store channels`, `scripts/test module store conversations`, `scripts/test module store artifacts`.
- `scripts/test module engine conversation`, `scripts/test module engine conversations`, `scripts/test module engine artifacts`.
- `scripts/test module node node` (channels, messages, direct channels), `scripts/test module node conversations`.
- `scripts/test desktop views/_studio`, `scripts/test desktop ui/artifact`; the scenarios `desktop/src/scenarios/channelsAndMessages.test.mjs` and `desktop/src/scenarios/conversations.test.mjs`.
- Journeys: `crates/bisa-cli/tests/it/e2e/a_channel_and_what_is_said_in_it.rs`, `crates/bisa-cli/tests/it/e2e/words_from_the_command_line.rs`, `crates/bisa-cli/tests/it/e2e/a_conversation_about_a_checkout.rs`, `crates/bisa-cli/tests/it/e2e/pulse_and_inbox.rs`.
- One module at a time; the whole workspace (`just verify`) only at the end of a pass ([Running](../testing-rules.md#running)).

## Common changes

- [Add an HTTP route](../recipes.md#1-add-an-http-route)
- [Add an engine event](../recipes.md#4-add-an-engine-event) — what the Inbox and the Pulse hear.
- [Add a desktop model](../recipes.md#7-add-a-desktop-model) — the timeline, the live turn and the Inbox decide in models.
- [Say something to a person](../recipes.md#26-say-something-to-a-person)
- A new origin, scope kind or context chip: [crates/core § Extension points](../../architecture/crates/core.md#extension-points).

## Compatibility

- Channel, message and conversation logs are [the workspace on disk](../../reference/compatibility.md#the-workspace-on-disk); a channel and a message travel as GEP kinds ([the collaboration wire](../../reference/compatibility.md#the-collaboration-wire)); the routes and the `conversation` and `inbox` streams of `GET /events` are [the HTTP API and its events](../../reference/compatibility.md#the-http-api-and-its-events); the messaging verbs are [the command line](../../reference/compatibility.md#the-command-line).
- Inside 0.x a minor or patch release never breaks the public contract: add an optional field, an origin, a frame field — a client ignores what it does not know.
- Renaming a scope, an origin or a field cannot be made by addition; it waits for 1.0.0, which brings a migration tool ([the first major release](../../reference/compatibility.md#the-first-major-release)).
- Declare the compatibility of your change in the pull request.

## Review focus

- Can a message wake more than it names, or start a loop between agents ([code review](../review/code.md))?
- Does what an agent made stay behind both walls, and does nothing beyond the chips reach a session ([security review](../review/security.md))?
- Are pages cut by a cursor that loses nothing, live turns settled, stores bounded by what is on screen ([performance review](../review/performance.md))?
- Do the words follow the vocabulary — a direct channel, a conversation, a goal's thread ([docs and language review](../review/docs-and-language.md))?
