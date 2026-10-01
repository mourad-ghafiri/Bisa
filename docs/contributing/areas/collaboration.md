# Collaboration

For a person, collaboration is inviting people who run their own node elsewhere into a workspace:
invitations, four roles, the channels they are rostered on, relays with their health, and messages from
outside held until the classifier or the owner lets them through. A hosted person is a human only and
brings no agents. In code, `crates/bisa-collab` is the wire — the gift-wrap envelope, the control
messages, the invite code, the relay pool — `crates/bisa-net` is the host's pump, and `crates/bisa-guest`
is the replica a guest keeps, the crate a mobile client embeds whole. What travels is GEP, the Goal
Engineering Protocol, over ordinary Nostr relays.

## Where it lives

- `crates/bisa-collab/src/wrap.rs`, `crates/bisa-collab/src/control.rs`, `crates/bisa-collab/src/invite_code.rs`, `crates/bisa-collab/src/relays.rs` — the envelope, the words beside the facts, the code a person carries, the pool.
- `crates/bisa-net/src/host.rs`, `crates/bisa-net/src/truth.rs` — the pump and the one reach rule; `crates/bisa-net/src/iroh_sync.rs` — the direct QUIC transport (feature `iroh`).
- `crates/bisa-guest/src/session.rs`, `crates/bisa-guest/src/store.rs` — one membership and its replica.
- `crates/bisa-core/src/kind.rs`, `crates/bisa-core/src/member.rs`, `crates/bisa-core/src/collab_settings.rs` — kind numbers, roles and reach, the rules of the `sync.*` settings.
- `crates/bisa-store/src/ingest.rs`, `crates/bisa-store/src/invites.rs`, `crates/bisa-store/src/held.rs`, `crates/bisa-store/src/members.rs` — admission, hashed invitations, the held list, members.
- `crates/bisa-engine/src/collab.rs`, `crates/bisa-engine/src/membership.rs` — the classifier's hold and membership.
- `crates/bisa-node/src/collab.rs` — the routes; `crates/bisa-cli/src/net.rs` — the node's pump, guest sessions and pool in one process.
- `desktop/src/views/_settings/` (people, invitations, relays) and `desktop/src/shell/hostedModel.mjs` — the hosted sections.

## Read first

- [Collaboration guide](../../guide/collaboration.md) — invitations, roles, relays, held messages, as a person meets them.
- [14 — Collaboration](../../architecture/14-collaboration.md) — the model, the protocol, the security seams, invariants C1–C10.
- [09 — GEP](../../architecture/09-protocol-gep.md) — the one question, the kind registry, who may write what, encryption.
- [GEP reference](../../reference/gep.md) — kinds, envelopes, control messages.
- [10 — Runtime flows § A person on another node](../../architecture/10-runtime-flows.md#a-person-on-another-node-from-an-invitation-to-a-first-message) — from an invitation to a first message.

## Rules a change must keep

- Relays are dumb transport, never authorities; a workspace with no relay works completely.
- A kind exists only if a second node could act on it: nothing local, nothing derived and no credential goes on the wire ([09 § What is deliberately not on the wire](../../architecture/09-protocol-gep.md#what-is-deliberately-not-on-the-wire)).
- A hosted member's node writes only a human's acts under its own key; an agent of another node never writes here (C1, C2, I54).
- A person receives exactly what their role reaches, wrapped to them alone, never under a shared key (C3, I55).
- An invitation's secret is stored hashed, claimed once in constant time, and expires (C4, I56).
- A message from outside reaches an agent only after the classifier's *safe* or the owner's release, and an agent woken by an outsider asks for every tool beyond reading (C5, C6, I57).
- What leaves the machine is redacted before it is signed (C7); a guest applies only what the host's seal carried (C8); a guest replica is apart from the node's own store (C9).
- A peer's run snapshot lands only with a local, signed decision for every approval step it passed (I4).
- No test contacts a public relay: an in-process `LocalRelay`, loopback only.

## Testing a change

- `scripts/test lib collab`, `scripts/test lib guest` — the envelope and the pool against a `LocalRelay`; a join and a first message against a fake host.
- `scripts/test crate net` — reach per role, admission, a relay change live; `FEATURES=iroh scripts/test module net iroh_e2e` — the direct transport on loopback.
- `scripts/test module engine collab`, `scripts/test module node collab`, `scripts/test module store members`, `scripts/test module store studio` (role admission at ingest).
- `scripts/test desktop views/_settings` — people, invitations, relay health.
- The journey `crates/bisa-cli/tests/it/e2e/two_nodes_and_a_relay.rs` — two nodes and a relay on loopback: invited, joined, read by the classifier, removed.
- One module at a time; the whole workspace (`just verify`) only at the end of a pass ([Running](../testing-rules.md#running)).

## Common changes

- [Add a GEP kind](../recipes.md#9-add-a-gep-kind) — the next free number, a route arm in ingest, a path.
- [Add a setting](../recipes.md#2-add-a-setting) — `sync.*`, `collab.*` and `security.*` keys, read live through the engine's settings door.
- [Add an HTTP route](../recipes.md#1-add-an-http-route) — the people, invitation and hosted routes.
- [Say something to a person](../recipes.md#26-say-something-to-a-person) — a refusal on the wire is said in words.

## Compatibility

- [The collaboration wire](../../reference/compatibility.md#the-collaboration-wire) is public: every GEP kind number, and from 0.1.0 a number is never reused — a retired one stays a hole. What a synced record carries follows [the workspace on disk](../../reference/compatibility.md#the-workspace-on-disk).
- Inside 0.x a minor or patch release never breaks the public contract: add a kind, or a field with a default when absent. Nodes on different minor releases in one shared workspace are not promised — upgrade every node together ([not promised in 0.x](../../reference/compatibility.md#not-promised-in-0x)).
- A change that cannot be made by addition waits for 1.0.0, which brings a migration tool ([the first major release](../../reference/compatibility.md#the-first-major-release)).
- Declare the compatibility of your change in the pull request.

## Review focus

- Does the change put something local, derived or secret on the wire, or let a peer write what only the owner may ([security review](../review/security.md))?
- Is reach decided by the one rule and admission at ingest, never by the caller ([code review](../review/code.md))?
- Does a message from outside still wait for the classifier or the owner before any agent hears it?
- Are kind numbers new and never reused, and do the GEP pages change with them ([compatibility review](../review/compatibility.md))?
- Are catch-up, faces and blobs bounded — dropped before a byte is held when over their cap ([performance review](../review/performance.md))?
