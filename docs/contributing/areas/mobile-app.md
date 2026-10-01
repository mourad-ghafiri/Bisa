# Mobile app

The mobile app is a planned client of the platform: a person on a phone, holding a keypair and an
invitation, joins a workspace hosted on another node and reads and writes its channels and direct
channels. It is not the Mobile Development feature of the Project IDE, which develops Flutter apps on
the devices beside the code ([ide/19](../../architecture/ide/19-mobile-development.md)). There is no
code for the app in this repository. What exists is the part of the platform such a client embeds,
and the words reserved for it.

## Where it lives

- `crates/bisa-guest/` — the crate a mobile client embeds whole: a guest replica per host, the session that speaks for the person (join, post, react, read, leave), the faces port. No store, engine, harness or node.
- `crates/bisa-collab/` — the wire the guest speaks over ordinary relays: the envelope, the control messages, the invite code, the relay pool; it knows no store, engine, node or harness, which is what lets a client embed it.
- `crates/bisa-collab/src/control.rs` — `CLIENT_MOBILE`: the join message's `client` value `mobile`, beside `desktop` and `cli`.
- `desktop/api-schema.json` — the JSON schemas of the shapes a client reads (`Hosted`, `HostedMessage`, `Directory`, `Channel`).

## Read first

- [14 — Collaboration](../../architecture/14-collaboration.md) — hosted membership, the roles, the protocol, and [what a guest holds](../../architecture/14-collaboration.md#what-a-guest-holds): a mobile client is the guest crate, the relay pool and a UI.
- [crates/guest](../../architecture/crates/guest.md) and [crates/collab](../../architecture/crates/collab.md).
- [Collaboration](../../guide/collaboration.md) — invitations, roles and relays as a person uses them.
- [The collaboration area](collaboration.md) — where a change to the guest crate or the wire belongs today.

## Rules a change must keep

- A guest holds exactly what the person's role reaches, fed by the host, and writes the person's own signed acts and nothing else ([crates/guest](../../architecture/crates/guest.md)).
- The guest applies the host's words from the host's seal only, a fact by an author the directory names, in a scope the person reaches; a post passes the built-in redactor before it is signed ([14 § What a guest holds](../../architecture/14-collaboration.md#what-a-guest-holds)).
- `bisa-guest` runs no agent, opens no workspace of its own, decides nothing about roles and sends no bytes itself ([crates/guest § What this crate refuses to do](../../architecture/crates/guest.md#what-this-crate-refuses-to-do)).
- Bare *mobile* is reserved for this app, and for a phone or a mobile platform; the IDE's feature is always *Mobile Development* in full — its crate, its settings group, its routes, its tools and its event — and five rules of the lint hold those names (`scripts/lint-terminology`, [Terminology § The lint](../terminology.md#the-lint)).

## Testing a change

- `scripts/test lib guest` · `scripts/test lib collab` — the replica, the session, the faces; the envelope, the control messages, the invite code, the pool's health.
- The journey `crates/bisa-cli/tests/it/e2e/two_nodes_and_a_relay.rs` — a person on another node invited, let in on the channels the invitation named, the words both ways, a removal — on a relay of the test's own, under `scripts/test module cli e2e`.
- There is no client code to test.

## Common changes

- None in this repository yet. To take part, start a conversation in the repository's GitHub Discussions; an idea becomes an issue once a maintainer agrees it belongs ([How to contribute](../how-to-contribute.md)).
- A change the guest crate needs goes through [14 — Collaboration](../../architecture/14-collaboration.md) and the guest crate's invariants, like any other change to [collaboration](collaboration.md).

## Compatibility

- A client speaks [the collaboration wire](../../reference/compatibility.md#the-collaboration-wire): every GEP kind's number is promised and never reused. Nodes on different minor releases in one shared workspace are [not promised in 0.x](../../reference/compatibility.md#not-promised-in-0x).
- The Rust crates' APIs are internals, not contract ([Not promised in 0.x](../../reference/compatibility.md#not-promised-in-0x)); a client that embeds `bisa-guest` builds against a version of it.
- The wire grows by addition — a new kind, a new control word, a new `client` value; declare a change's compatibility in the pull request ([Keeping compatibility](../compatibility.md)).

## Review focus

- `bisa-guest` and `bisa-collab` stay embeddable: no dependency on a store, an engine, a harness or the node ([Code review](../review/code.md), [07 — Layering](../../architecture/07-layering.md)).
- What a guest may read and write, and what it signs ([Security review](../review/security.md)).
- The wire only grows ([Compatibility review](../review/compatibility.md)); *mobile* alone means this app ([Docs and language](../review/docs-and-language.md)).
