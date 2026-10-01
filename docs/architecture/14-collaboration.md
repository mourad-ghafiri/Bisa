# 14 — Collaboration: people on other nodes

A workspace has one owner — the keypair on the node it lives on — and it may **host** people
who run their own node elsewhere. A hosted person is a **human only**: they bring no agents,
they write only a human's acts, and they reach exactly what their role and the owner's rosters
allow. This document is the model, the protocol, the security seams, the invariants and what a
client needs to be one.

The person's view is [guide/collaboration.md](../guide/collaboration.md).

---

## The model

**Hosted membership.** The owner's node is the authority for its workspace. A person on another
node who claims an invitation becomes a *hosted member* there: a row in the host's `members.json`
with a role, served pairwise over relays into a small **guest replica** on their own node — never a
second copy of the workspace, never a shared key. Their own node's workspace is untouched: what
they hold of the host lives under `hosts/<host-pubkey>/`, apart.

**Four roles, one matrix.** `MemberRole { Owner, Admin, Member, Guest }` and
`Permission { ReadWorkspace, PostInChannels, OpenDms, MentionAgents, DecideGates, ManageChannels,
ManagePeople, ManageAgents, ManageGovernance, ManageSettings }` in `bisa-core/member.rs`:

| | Owner | Admin | Member | Guest |
|---|---|---|---|---|
| read every standing channel | ● | ● | ● | |
| post in the channels reached | ● | ● | ● | ● |
| open direct messages | ● | ● | ● | ● |
| wake agents by mention | ● | ● | ● | |
| decide gates governance names them for | ● | ● | ● | |
| make and edit channels | ● | ● | | |
| invite, promote and remove people | ● | ● | | |
| install and edit agents, change governance and settings | ● | | | |

The matrix is one function, `MemberRole::permissions()`, tested in place and served by
`GET /workspace/roles` so no screen restates it.

**Reach.** `bisa_core::reaches(channel, role, person)` is the one rule: a direct channel reaches
its audience; a standing channel reaches every role that reads the workspace, and a guest only
when the roster lists them (`RosterPolicy::Listed { agents, teams, humans }`). Everything the
host sends a person passes this rule (`crates/bisa-net/src/truth.rs::reaches_fact`), and every
fact a person sends is checked against it at ingest.

**Invitations.** `Invite { id, role, channels, label, created_at, expires_at, state }` is the
record everyone sees — the wire, the events, the owner's list. `invites.json` (mode `0600`) keeps
each record beside the `secret_hash` of its code, and that hash lives in the store's rows alone:
`Invite` itself never carries it. The secret is minted once and exists only in the
code the person carries — the link `bisa://join/<nprofile>/<secret>` or the text
`<nprofile>:<secret>` (`bisa-collab/invite_code.rs`). A claim is single-use, expiring and
constant-time (`store/invites.rs::claim_invite`); `collab.join = admit` admits on a valid code,
`ask` records a `Requested` claim for the owner's Inbox, and `collab.default_role` says what an
invitation offers unless the owner picks.

---

## The protocol

Everything a guest needs to talk to a host is in `bisa-collab` and in
[reference/gep.md](../reference/gep.md#control-messages): the gift-wrap envelope (a signed GEP
event in a carrier rumor, or a control message; NIP-44 sealed pairwise; NIP-59 wrapped), the
control vocabulary, the invite code, and the relay pool. Relays see kind-1059 ciphertext and a
recipient pubkey. The pool (`Relays`) is one `nostr_sdk::Client` with reconnecting relays, a
health row per relay, a check before a relay is added, a live subscription for wraps to this key
and a periodic catch-up (NIP-77 where the relay speaks it, a since-window `REQ` otherwise). The
window's cursor is the process's alone (`Host::last_catchup`): the first catch-up after a start asks
for everything, so a restart costs one full read of this key's wraps from every relay — safe, and
the price of a durable cursor not yet paid.

**Every real relay is TLS, and the process has one crypto provider for it.** The websocket stack
under `nostr-sdk` builds its TLS client with `rustls::ClientConfig::builder()` — the process-wide
provider — and `rustls` chooses one from its crate features only when exactly one backend is
compiled in. This workspace compiles both (`aws-lc-rs` is rustls' default and several dependents
turn it on; `ring` is what the direct transport and QUIC ask for; feature unification gives every
crate the union), and with both and none installed that call **panics** — inside the relay's own
connection task, so a relay sat at *connecting* for ever and said nothing, while everything over
`reqwest`, which names its own provider, kept working. So `bisa_collab::tls::ensure_crypto_provider`
installs **`ring`** — the backend the direct transport already names, one for the whole wire — as
the process default, once, **where a client is made**: `Relays::start` and `Relays::check`. Every
embedder — the node's pump, a one-shot CLI claim, a guest session, a mobile client — is covered by
construction and cannot forget; the `bisa` binary also calls it first thing in `main`, and a
provider a host application installed is respected. The relay tests run against a local `ws://`
relay, which is why TLS went unexercised: `tls.rs`' test now makes the exact call the stack makes,
and a `wss://` address with no TLS behind it must answer an error in words, never a panic.

**A row says why.** `RelayHealth.problem` is a sentence for a relay that was tried and is not
connected (`relay_problem`, from the status word and the attempt counters the relay layer keeps):
*never connected in 4 attempts — …*, *the connection dropped after 2 of 5 attempts connected — it
is retried on its own*, *the relay banned this node*. It is absent while a relay is connected, off,
or not tried yet, and it names what is known and guesses no cause: the layer keeps no last error,
and a sentence that invented one would send a person looking in the wrong place. Settings ›
Workspace › Relays & sync shows it under the row, whole; `bisa relay list` prints it indented.

**Relays are dialled directly.** `network.proxy.*` is for HTTP: the platform's proxies are HTTP
proxies and the relay layer tunnels only through SOCKS, so a network that lets nothing out but a
proxy reaches no relay. The never-connected sentence and `bisa relay doctor` both say so.

**Off by default.** `sync.enabled` is `false` on a fresh node and `sync.relays` holds four public
relays (`bisa_core::collab_settings::DEFAULT_RELAYS` — the registry's default reads the same
constant): the pool opens with no relay, the host pump does not start, a join and a claim are
refused with the switch named, and `GET /sync` lists every configured relay as `off`
(`RelayHealth::off`). `Relays::check` is a throwaway connection and works either way, which is
what lets a person try the relays before turning the wire on. Turning the switch on is a `sync.*`
write: the pump re-reads, the pool connects, the host pump starts — no restart.

| A member says | The host answers |
|---|---|
| `join { secret, label?, client }` | `welcome { workspace, role, members, channels }`, `waiting`, or `refused { reason }` |
| `profile { label?, photo?, face? }` | `members_changed` to everyone, the changer included |
| `want_face { sha256 }` | `face { face: { sha256, b64 } }` when the host holds it, else nothing |
| `open_dm { participants }` | `channels_changed { channels }` — the direct channel among them |
| `leave` | `removed` to the leaver's key, `members_changed` to everyone else — the leaver's record already says *left*, and that `removed` is the host's housekeeping, not a new fact: it changes nothing on the leaver's side (`guest session::a_members_second_claim_and_a_refusal_never_unseat_the_member`) |
| `iroh_addr { node_id, addrs }` | — (learned for the direct transport) |

And on the host's own moves: `role_changed { role }`, `channels_changed`, `members_changed`,
`removed { reason? }`.

**A person's profile travels with them.** Their **name** and their **face** are their own row of
their own workspace's members — set in Settings › You › Identity, `PUT /workspace/me` — and every
workspace they are a member of hears it: right after the welcome, and whenever it changes (the
store's `PeopleChange::ProfileChanged`, which the node's pump turns into a `profile` to every host,
`crates/bisa-cli/src/net.rs`). The host keeps a hosted member's profile only from that member's
own key (`hosted_role` first, then `bisa_core::clean_label` and `Face::decode`), writes it in one
move (`set_member_profile`, one `ProfileChanged`) and sends the directory whole. A face is
**bounded for the wire**: `MAX_FACE_BYTES` = 16 KiB (`bisa-core/attachment.rs`), a 96 px JPEG the
desktop scales to fit (ide/14 §Photos), since a control message is gift-wrapped once per recipient
and public relays refuse an event past a few dozen kilobytes; the base64 is refused above
`MAX_FACE_B64_CHARS` **before** a byte is decoded, then the hash must be the bytes' own and the
bytes a picture (`Face::decode`; a face that fails is dropped and the label still lands). The
directory carries every face **by reference** (`Directory.photo: AttachmentRef`, the owner's row
too, so the host's face reaches its guests the same way); a node that lacks the bytes asks
`want_face` once per hash (a set of in-flight asks, `GuestSession.asked`) and holds what comes back
under its hash — on the desktop's node through the `FaceStore` port over its own attachment store
(`bisa-guest/faces.rs`; `MemoryFaces` for a client with no store), so `GET /attachments/{sha}`
serves the face to its screens. A join carries no face: a stranger's join decodes nothing. A conversation fact — a message, a reaction, a retraction — travels as
itself, wrapped pairwise to each person who reaches it, the host's seal on every wrap. The host
relays a person's fact to the others who reach it; the person's node keeps its own copy the
moment it is signed. A `(member, event)` ledger, `net_published.jsonl`, keeps each fact delivered
once across restarts and one-shot processes.

**Admission at the host** is the store's ingest ladder ([09 — GEP](09-protocol-gep.md#who-may-write-what)),
which admits a hosted member's own key for the human kinds — `MESSAGE`, `REACTION`, `RETRACTION`,
`DECISION` — and `CHANNEL` for an admin, refuses every other kind with *a hosted member writes as a
human only*, refuses an attestation from any owner but this node's (*an agent of another node
never writes here — its person may*), and checks reach and `PostInChannels` on every conversation
fact. Nothing in the net crate decides admission. A person's `DECISION` is **filed by its gate's
home**: its `a` tag names the journal the gate hangs off — a goal's, or a run of the workspace's
(`33413:<pk>:<RunId>`, [09](09-protocol-gep.md#the-kind-registry)) — the gate's governance policy
decides it there, and it lands on that journal and in the index's `approvals` under that goal or
that run, the same row a local decision writes (`record_decision`).

---

## The security seams

1. **The classifier reads a message from outside before an agent hears it.** The store applies the
   fact and says `RemoteMessageArrived`; `engine/collab.rs::on_remote_message` holds it
   (`held.json`, reason `pending`), puts its redacted text to the classifier with the message
   brief (`bisa-security/classify.rs::MESSAGE_BRIEF` — prompt injection, secrets asked for, social
   engineering, links to run), and on *safe* releases it into the ordinary dispatch — every
   session it wakes working *for* that person. A workflow's `message` start, wait or boundary
   hears a message only there, so one from another node is heard after the hold and never
   before it; and a filter that says *you* or *agents* never matches a hosted person — a start
   names the person it listens to. *Harmful* or no verdict keeps it held with the
   reason, said as `MessageHeld`, shown to the owner with a caution, and reaches no agent until
   `POST /messages/{id}/release`. Nothing bounds `held.json` but a release or a refusal: a host
   whose owner never answers keeps every held message. Every message is read in a task of its
   own, so the list has one writer at a time (`Workspace::held_writes`): a release of one that
   lands as the verdict of another loses neither. `security.collaboration.classify` off
   dispatches at once.
2. **An agent woken by an outsider runs with every tool beyond read-only put to the owner.**
   `Judge.on_behalf_of` carries the person and their role through `decide_tool`; with
   `security.collaboration.agent_tools = ask` a rule that would fall through or classify becomes
   *ask*, and the fallthrough ceiling clamps to the read tier. `as_owner` runs the agent as the
   owner's own message would.
3. **A message that leaves the machine is redacted before it is signed** — on the host,
   `messaging::post_as_person` when the scope reaches a hosted member
   (`collab::scope_leaves_the_machine`); on a guest, the built-in redactor in
   `GuestSession::post`. Invitation labels pass the redactor too.
4. **Who may wake an agent** is the role's: `RespondPolicy::Members` answers a hosted member only
   when the role holds `MentionAgents`; a guest's mention wakes nobody.

---

## What a guest holds

`bisa-guest` is the crate a mobile client embeds whole: `bisa-core` + `bisa-collab` +
`bisa-security` + `nostr` + `serde` + `tokio`, and **no** store, engine, harness or node.
`GuestStore` keeps one host's replica at `<data_dir>/hosts/<host-pubkey>/` — `host.json` (the
card, the role, the state), `channels.json`, `members.json` (each row with its face by hash),
`conversations/<scope>.jsonl`, `seen.jsonl`, `read.json`; the faces themselves live behind the
`FaceStore` port the embedder gives every session. `GuestSession` speaks: `join`, `channels`,
`dms`, `members`, `messages`, `post`, `react`, `retract`, `open_dm`, `mark_read`, `leave`,
`send_profile`, and `on_incoming` applies the host's words — from the host's seal only, a fact by an
author the directory names, in a scope the person reaches — and answers what to say back (the
profile after a welcome, the faces it lacks), which `Guests::route` sends. `Guests` supervises every membership on a node and routes the pool's inbound
stream by the seal's sender.

**Clients.** The desktop's node runs the pool, the host's pump and the guest sessions in one
process (`crates/bisa-cli/src/net.rs::Pump`), lends them to the node as `CollabDoors`, and the
desktop draws them through `/hosts/…`. A mobile client is the guest crate, the relay pool and a
UI: it needs a keypair, the invite code, and the relays the code names; it does not need a
workspace, an engine, a harness or the node. The wire shapes it reads — `Hosted`, `HostedMessage`,
`Directory`, `Channel` — are `bisa-guest`'s and `bisa-core`'s, with JSON schemas in
`desktop/api-schema.json`.

---

## Invariants

| | Invariant | Held by |
|---|---|---|
| C1 | A hosted member's node writes only a human's acts: its own key, the human kinds, a channel when an admin; every other kind is refused by name | `store/ingest.rs` (role admission); `tests/it/studio.rs` (`cross_workspace_message_ingest`) |
| C2 | An agent of another node never writes here, whatever it carries: an attestation is admitted only from this node's owner | `store/ingest.rs`; `tests/it/studio.rs` |
| C3 | A person receives exactly what their role reaches, wrapped to them alone; a guest sees only the channels it is rostered on | `net/truth.rs::reaches_fact`, `net/host.rs::reachers_of`; `tests/it/host.rs` |
| C4 | An invite secret is stored hashed, claimed once, in constant time, and expires | `store/invites.rs`; its tests and `tests/it/host.rs` |
| C5 | A message from outside reaches an agent only after the classifier's *safe*, or the owner's release — and never once its author is no longer a member, whenever that happened | `engine/collab.rs`, `store/held.rs` (one writer of the held list); `tests/it/collab.rs`; the unit test `a_release_and_a_verdict_at_once_lose_neither` in `store/held.rs` |
| C6 | An agent woken by an outsider asks for every tool beyond read-only unless the owner said otherwise | `engine/security.rs`, `engine/inputs.rs`; `tests/it/collab.rs` |
| C7 | What leaves the machine is redacted before it is signed | `engine/messaging.rs`, `guest/session.rs`; the guest's unit test `a_guest_joins_is_welcomed_posts_a_redacted_message_and_refuses_an_unreached_channel` in `crates/bisa-guest/src/session.rs` |
| C8 | The host is the authority: a guest applies only what the host's seal carried | `guest/session.rs::on_incoming`; its unit tests `a_guest_joins_is_welcomed_posts_a_redacted_message_and_refuses_an_unreached_channel` (a stranger's fact dropped) and `a_refusal_and_a_removal_are_kept_in_words_and_a_reopen_finds_the_replica` (another key's welcome ignored) |
| C9 | A guest replica is apart from the node's own workspace: `hosts/` holds nothing the store reads | `guest/store.rs`, `guest/session.rs` (`HOSTS_DIR`); `crates/bisa-net/tests/it/host.rs::a_replica_of_a_host_is_nothing_the_nodes_own_store_reads` (a node that is also a guest, its own index rebuilt beside the replica) |
| C10 | A membership is unseated only by the host's word to a member — a removal — never by a refusal: a member claiming again claims nothing, and a refusal a member hears is dropped | `guest/session.rs::join`, `on_incoming`; its tests; the journey `crates/bisa-cli/tests/it/e2e/two_nodes_and_a_relay.rs` |

---

## Tests

`scripts/test module core member` (the matrix, reach), `store invites`, `store studio` (role
admission at ingest), `collab wrap` and `collab relays` (the envelope, the pool against an in-process
`LocalRelay`), `guest session` (a join and a first message against a fake host), `net host` (reach
per role, admission `admit` and `ask`, a leave, a relay change live; a member's name and face
kept on their own row alone, a face that lies about its bytes or is over the cap not kept and the
name landing all the same — `a_person_says_who_they_are_for_themselves_alone_and_a_face_is_kept_only_when_it_is_one`;
C9 — `a_replica_of_a_host_is_nothing_the_nodes_own_store_reads`), `engine collab` (the
classifier's hold and release, a member removed while the verdict was pending, a release for
somebody who left), `node collab` (the routes with
a fake pump), the journeys of `crates/bisa-cli/tests/it/e2e/two_nodes_and_a_relay.rs` — two nodes and
a relay on loopback: invited, joined, read by the classifier, removed; and the wire off until it
is turned on, said by the verb and the route alike, then on with no restart, the relays added,
tried and removed by their verbs, a node that knows no relay learning the host's from the code —
written as its `sync.relays` through the engine's settings door, so whoever listens hears the
setting move and the list of relays names it —, an invitation withdrawn, a role changed and the reach with it,
a guest who leaves — and the desktop's `peopleModel` (a key typed under *Admit by key* told which
of its three faults it has; a read that refused said where its answer goes), `inviteCodeModel`,
`relayHealthModel` (a relay's state in the catalog's words; `checkAbout` — a check's answer drawn
only under the address it was about; `wireMoved` — the one rule the panel, People and the footer
read the wire again on), `hostedModel` (`hostedScreen` — a skeleton until the memberships
answered, never *not a member* unchecked), `workspaceLoadModel` (`degradedReads` — the hosted
sections' failures their own list, taken back by the next read that answers) and
`gonePlacesModel` (`membershipsKnown` — a host's places given up only on memberships the node
answered).
