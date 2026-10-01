# bisa-guest

The guest side of collaboration ([14 — Collaboration](../14-collaboration.md)): what a human on
this node holds of a workspace they joined on another — a **guest replica** per host, fed pairwise
by that host with exactly what the person's role reaches, written to by this person alone. No
store, no engine, no harness, no node: a keypair, the host's card, the channels and people the
host said, the facts it relayed, and the person's own signed acts on their way back. This is the
crate a mobile client embeds whole; the desktop's node hosts it beside the workspace it owns.

---

## Where things live

| Module | Owns | Public types worth knowing |
|---|---|---|
| `lib.rs` | the crate's door: the replica, the faces port and the session re-exported — what a mobile client or the node's pump links | — |
| `error_text.rs` | `Localize for GuestError` (`error-guest-…`) | — |
| `store.rs` | the replica at `<data_dir>/hosts/<host-pubkey>/` — `host.json`, `channels.json`, `members.json`, `conversations/<scope>.jsonl`, `seen.jsonl`, `read.json`; files are the truth, a fold over one scope's log is what a screen needs; every write atomic | `GuestStore` (`open`, `list`, `hosted`, `channels`, `members`, `append`, `events`, `messages`, `latest_at`, `mark_read`, `unread`), `Hosted`, `HostedState { Requested, Member, Refused, Removed, Left }`, `HostedMessage`, `HostedReaction`, `scope_of` |
| `faces.rs` | the port a guest keeps faces through and reads its own profile from — `FaceStore { bytes, hold, profile }`, `OwnerProfile { label, photo }`; `MemoryFaces` for a client with no store and for the tests | — |
| `session.rs` | this person as a member of one host — `join` (the claim, then the host's first word within `join_wait`), `send_profile` / `profile_control` (who this person is, said after the welcome and whenever their row changes), the faces the directory names and this node lacks asked once each (`faces_wanted`, `WantFace`) and held when they come (`Face` → `FaceStore::hold`), `on_incoming` answering what to say back, `channels`, `dms`, `members`, `messages`, `post` (the built-in redactor before signing; refused outside the channels the host said), `react`, `retract` (own only), `open_dm`, `mark_read`, `leave`; `on_incoming` applies the host's words from the host's seal only, a fact by an author the directory names, in a scope the person reaches; `Guests` supervises every membership and routes the pool's stream by the seal's sender | `GuestSession`, `Guests`, `HostedChange`, `Posted`, `HOSTS_DIR` |

---

## Invariants held here

| Invariant | Held by |
|---|---|
| A replica round-trips its host, channels and people; a fact is kept once and folds into messages with reactions and retractions; read marks count what others said after them | `store.rs` tests |
| A guest joins, is welcomed, posts a redacted message, is refused a channel it does not reach — to post into and to read alike, never an empty room; a fact from a stranger is dropped and the host's is kept; a reaction and a retraction of one's own; leaving tells the host | `session.rs` tests, against an in-process `LocalRelay` and a fake host |
| A refusal and a removal are kept in words; another key's word is not the host's; a reopen finds the replica | `session.rs` tests |
| A member's second claim changes nothing — no claim goes out, and a refusal answers a claim a member never made, so it never writes over a standing membership | `session.rs::a_members_second_claim_and_a_refusal_never_unseat_the_member`; the journey `crates/bisa-cli/tests/it/e2e/two_nodes_and_a_relay.rs` |

---

## What this crate refuses to do

Run an agent, open the node's own workspace, decide what a role reaches, or send bytes.
