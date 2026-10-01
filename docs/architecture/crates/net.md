# bisa-net

The host's side of collaboration on the wire ([14 — Collaboration](../14-collaboration.md)): the
pump between a workspace and the people it hosts on other nodes, over the relay pool
`bisa-collab` opens, and the direct QUIC transport. Relays are dumb transport, never
authorities: they see kind-1059 ciphertext addressed to a pubkey. See
[guide/collaboration.md](../../guide/collaboration.md) for the person's view and
[09 — GEP](../09-protocol-gep.md) for the wire.

---

## Where things live

| Module | Owns |
|---|---|
| `lib.rs` | the module list and the statement of what a relay ever sees |
| `error_text.rs` | `Localize for NetError` (`error-net-…`) |
| `config.rs` | `NetConfig` — the `sync.*` settings read into a value (`from_settings`, `from_workspace`); there is no file of its own |
| `host.rs` | the pump — and a member's profile kept from their own key alone (`Profile` → `clean_label`, `Face::decode`, `accept_attachment`, one `set_member_profile`; `WantFace` answered from the store when the bytes are a face; `ProfileChanged` → the directory whole to everyone, the owner's face riding its row): outbound, every store event read for who reaches it (`reachers_of` — a fact pairwise to each person whose role reaches its channel, never its author; a channel change sends each person the channels they now reach and the backlog of any new to them; a join is welcomed with the card, the role, the directory and the channels, then handed the backlog; a role change, a removal and a refusal said to the one they concern, the directory to the rest), a `(member, event)` ledger `net_published.jsonl` keeping each fact delivered once; inbound, a hosted member's fact to the store's ingest ladder, `join` claimed through `claim_invite` (admitted or `waiting` under `collab.join = ask`, `refused` in words), `leave`, `open_dm`, `iroh_addr`; `Host::start`, `status`, `apply` (a `sync.*` change live), `attachment_fetcher`, `SyncStatus`, `AttachmentFetch` |
| `truth.rs` | `conversation_facts(ws)` — every conversation fact in the workspace's logs, reached through `Paths` — and `reaches_fact(ws, event, person, role)`, the one rule applied to an event |
| `iroh_sync.rs` (feature `iroh`) | the direct QUIC transport: the host's door for a hosted member's client that dials it — a signed hello checked against the hosted members, have/want reconciliation over the facts the peer reaches, attachment bytes by hash — a chunk is taken only for a hash this node asked for (`wanted_blobs`), under the attachment ceiling (`MAX_BLOB_CHUNKS`) and the chunk size, else dropped and said; a removed member's address is forgotten (`forget_peer`, on disk too) and their sessions end; the endpoint's key file is `0600` or the bind fails; a frame to a writer that is gone ends the sender's loop (`send_or_end`), a full live queue is a `debug` line and the next have/want round carries the fact, a peers or seen-ledger write that fails is a `warn`; n0 relays off unless `sync.iroh.n0_relays` |

---

## Entry points

`Host::start(ws, relays, cfg, data_dir)` → `Host`; `Host::status()` for the node's `GET /sync`
through the CLI's `Pump`; `Host::apply(&cfg)` on a `sync.*` write.

---

## Invariants held here

| Invariant | Held by |
|---|---|
| A guest receives only its rostered channels' facts; a member every standing channel's; a direct message its audience alone | `tests/it/host.rs` |
| A person's fact is relayed to the others who reach it and refused outside its reach | `tests/it/host.rs` |
| A valid code admits at once, or waits under `ask`; a spent code is refused with the reason | `tests/it/host.rs` |
| A promotion widens reach and brings the backlog; a removal and a leave are said | `tests/it/host.rs` |
| A relay change reaches the pool without a restart | `tests/it/host.rs` |
| A stranger's hello on the direct transport is closed; a hosted member's session carries the facts its role reaches | `tests/it/iroh_e2e.rs` |
| A blob chunk for a hash nobody here asked for, or announced past the attachment ceiling, is dropped before a byte is held | `iroh_sync.rs` (`receive_blob_chunk`, read with `request_blob`) |
| The hosted count is the people, never the owner: a removal lowers it, a refused code never raises it | `tests/it/host.rs` |

---

## Tests

`tests/it/host.rs` — a host and the people it hosts, end to end through an in-process `LocalRelay`
(`reach_follows_the_role_and_the_roster_and_a_person_s_fact_is_relayed_or_refused`,
`ask_mode_holds_a_claim_for_the_owner_and_a_refusal_reaches_the_joiner`,
`a_person_who_leaves_is_gone_and_a_relay_change_is_live`,
`a_person_says_who_they_are_for_themselves_alone_and_a_face_is_kept_only_when_it_is_one`,
`a_replica_of_a_host_is_nothing_the_nodes_own_store_reads`); `tests/it/iroh_e2e.rs` (feature `iroh`) —
the direct transport on loopback, relays disabled and peers wired by hand
(`a_hosted_member_s_session_carries_the_facts_its_role_reaches`, `a_stranger_s_hello_is_closed`);
unit tests beside `config.rs`, `host.rs` and `iroh_sync.rs`. No test contacts a public relay.
