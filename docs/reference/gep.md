# GEP — the Goal Engineering Protocol

The wire format is Nostr: every durable fact is a signed event, journals are append-only lines of
them, and the current state of an addressable object is its latest signed snapshot. The design is
[09 — Protocol](../architecture/09-protocol-gep.md); this is the table.

## The one test for what goes on the wire

*Would a second node be able to act on this?* A goal, a workflow, a run, a decision, a project, an
attachment, a channel, a skill, a connector definition — yes. A workstream (a path on one machine),
an MCP server (a command line and an environment), a connector account (one machine's login and its
secrets), a note, IDE layout, terminal scrollback, the diagnostic log, a hook's secret, whether a library workflow listens, a listener's runtime memory, an armed wait or boundary — no. What is true everywhere about a workstream — *the branch was pushed, PR #12 opened*
— is journaled as a progress fact.

## Kinds

Journal facts (regular events, one per line in their home's journal — `goals/<id>/journal.jsonl`
for a goal and its runs, `workflows/runs/<id>/journal.jsonl` for a run in the workspace, which has
no goal; the `a` tag names the home, `33400:<pubkey>:<goal>` or `33413:<pubkey>:<run>`, and a
`once` tag — a ULID minted when the fact is written — makes each fact one of a kind, so the same
thing happening twice in one second is two events):

| Kind | Fact |
|---|---|
| `3400` | goal note — including `attachment { project, attached }`, `document { file }` (a file a person gave the goal as context — the `AttachmentRef`; the bytes travel by hash and `goals/<id>/documents/` is materialised from the facts wherever they are held), `guidance { phase, status, detail?, session? }`, the Workflow Agent's standing on a guided goal, `guard { tool, subject, verdict, by, rule?, reason? }`, the Tool & Commands Guard's judgement on a tool call — `subject` is redacted, `verdict` is `allowed` · `denied` · `asked`, `by` is `rule` · `classifier` · `person`; `judgement { judgement, run?, step? }`, the Decision-Making Agent's judgement for this goal — never a signed decision, kind 3401 stays a person's alone; and `withdrawn { subject, reason }`, a `question` taken back — by its asker, or by a restart that found the asker dead (`interrupted by a restart`) — read as a decision is |
| `3401` | decision on a gate — `approval`, `escalation` or `publish` |
| `3402` | claim of a work item |
| `3403` | progress |
| `3404` | result |
| `3405` | *retired* — never reassigned |
| `3406` | per-turn metrics |
| `3407` | message — a post, one of a kind by its `once` tag (a ULID), or a **membership event**, the same event however often its change is processed |
| `3408` | retraction |
| `3409` | *retired* — never reassigned |
| `3410` | signal — `signal { signal, listener?, source, name?, payload }`, the occurrence that started a run, on that run's home; `source` is `schedule` · `hook` · `message` · `signal` · `project` · `run` · `platform` · `connector` · `check` · `test` |
| `3411` | step or run fact — `step { run, step, event }` with `started`, `waiting`, `done { branches? }`, `failed`, `diverted { by }`, `boundary { boundary }`, `answered`, `decided`, `skipped`, `cancelled`; `run { run, event }` with `queued`, `started { workflow, revision, start?, signal? }`, `amended`, `finished`, `cancelled` (a cancel's cause `stopped` · `restarted` · `withdrawn` · `closed` for a goal's run, `stopped` · `restarted` · `retired` for a run in the workspace) |
| `7` | reaction (NIP-25) |

Addressable snapshots (`d` = the object id, `revision` beside it, tags as NIP-12 `t`):

| Kind | Object | Namespace |
|---|---|---|
| `33400` | goal | `goals/<id>/state/` |
| `33401` | drawing — the record: its title, its scope, its scene (vector only, under 768 KiB); its `.excalidraw` file is an export and never travels | `drawings` |
| `33402` | work item — bound to its run and step | its home's `state/`: `goals/<id>/state/`, or `workflows/runs/<id>/state/` |
| `33403` | agent profile — no command lines, no environment | `agents` |
| `33404` | engram (recall) — blinded `d`, encrypted content | |
| `33405` | channel | `channels` |
| `33406` | *retired* — never reassigned | |
| `33407` | addon — the record: its manifest, `origin`, `enabled`, what was granted; never the bundle | `addons` |
| `33408` | team | `teams` |
| `33409` | project — with its `origin` (`workspace` · `goal` · `step`), a claim signed by its author like a goal's | `projects` |
| `33410` | *retired* — never reassigned | |
| `33411` | skill | `skills` |
| `33412` | workflow — with its `origin` (`workspace` · `catalog` · `goal`) | `workflows` for the library; `goals/<id>/state/` for a goal's design |
| `33413` | workflow run — its `scope` (`goal` with the goal, or `workspace` with its own budget), the frozen workflow, inputs, the start it entered, the event that began it, its chain and every step's record | `goals/<id>/state/` for a goal's run; `workflows/runs/<id>/state/` for a run in the workspace |
| `33414` | connector — a definition: its base URL, the hosts it may reach, its auth scheme, its operations; never an account | `connectors` |
| `33415` | conversation — a saved exchange with agents: its origin, its title, whether it is archived; its messages are `3407` events on its own scope | `conversations` |

Four numbers are **retired as holes**, never reassigned — `3405` (a criterion's verification),
`33406` and `3409` (the two objects of a feature removed whole), and `33410` (a standalone object
that named the workflow an event starts; a workflow's own `start` steps say that now): nodes that synced before each object
was removed still hold events of that kind, and an incoming one is ignored by ingest. Two retired
numbers were **reissued** before 0.1.0, each when no workspace that version opened could hold an event
of the old kind — from 0.1.0, the first public release, a number is never reissued
([Compatibility](compatibility.md#the-collaboration-wire)): `33407` (a goal's living document once) for the addon record, and `33401` (a plan once) for the
drawing ([19](../architecture/19-drawings.md)). Twenty-six kinds are GEP kinds;
fourteen are addressable; the next free addressable number is `33416`. Ephemeral `23400` observer
frames are never wrapped for relays.

## Envelopes

A fact that leaves a node is published as a NIP-59 gift wrap — carrier rumor (`1060`; `1061` for
control messages), NIP-44 seal sender → recipient, ephemeral wrap — **pairwise to each person who
reaches it** (`bisa-collab/wrap.rs`); there is no shared key. Unwrap enforces rumor author = seal
sender, so a wrap cannot be replayed as somebody else's, and a host may relay another person's
fact with its own seal on the wrap. Ingest re-verifies the inner signature, dedupes by id,
**admits by role** (this owner's key and its attested agents write anything; a hosted member's own
key writes the human kinds — `3407`, `3408`, `7`, `3401` — and `33405` when an admin; an agent of
another node never), checks reach on a conversation fact, and applies: journal append verbatim,
snapshot latest-wins on `(created_at, revision)`. A run snapshot is admitted only when every
approval step it records as done has a qualifying journaled decision locally, and never changes
scope — a goal's run stays its goal's, a run in the workspace stays the workspace's; a goal snapshot
never reopens a closed goal; a run that arrives before its goal is retried, not dropped.

## Principals and attestation

A human is a pubkey. An agent has its own keypair, attested by a member (NIP-OA) so its events
verify as that agent and revoking the member revokes them — the General Agent and the Workflow Agent included. Assignees
are written `agent:<id>`, `human:<64 hex>`, `team:<id>`.

## Messages

A `3407` body is `{ body: "post", text, context?, artifacts?, thinking? }` — `thinking` the reasoning an agent's harness streamed before its words, at most 64 KiB, never on a person's post ([13 — Conversations](../architecture/13-conversations.md#the-reply-streams)) — or a membership event. `context` is a list of
`ContextRef`s — a file, a selection, a diff hunk, a terminal tail, a work item, a commit, an annotated element of a page — with
**project-relative** paths, bounded at 64 KiB, so a peer with the project attached can act on it.
Attachments ride as `imeta` descriptors; the bytes move on demand over the direct link.

## Control messages

A `1061` rumor carries one JSON object tagged `type`, read only from the seal's verified sender
(`bisa-collab/control.rs`; [14 — Collaboration](../architecture/14-collaboration.md)):

| A member says | Fields |
|---|---|
| `join` | `secret`, `label?`, `client` (`desktop` · `mobile` · `cli`) |
| `profile` | `label?`, `photo?` (an `AttachmentRef`, `null` clears), `face?` (`{ sha256, b64 }` — the photo's bytes, a picture within `MAX_FACE_BYTES` = 16 KiB, refused above `MAX_FACE_B64_CHARS` before decoding) — sent after the welcome and whenever the member's own row changes |
| `want_face` | `sha256` — a face the directory names and this node lacks |
| `open_dm` | `participants` — pubkeys the host's directory names; the host answers `channels_changed` |
| `leave` | — |
| `iroh_addr` | `node_id`, `addrs` — for the direct transport |

| The host says | Fields |
|---|---|
| `welcome` | `workspace { pubkey, name, relays }`, `role`, `members [{ pubkey, role, label?, photo? }]`, `channels [Channel]` — the ones the role reaches |
| `face` | `face { sha256, b64 }` — the bytes a `want_face` asked for, when the host holds them |
| `waiting` | the host admits by hand; the claim is recorded |
| `refused` | `reason` |
| `removed` | `reason?` |
| `role_changed` | `role` |
| `channels_changed` | `channels` — whole, the ones the role reaches now |
| `members_changed` | `members` — whole, every row with its face by hash |

The invite code a `join` claims is `bisa://join/<nprofile>/<secret>` or `<nprofile>:<secret>` —
the host's pubkey with relay hints (NIP-19), and a 32-byte hex secret the host keeps only as a
SHA-256.
