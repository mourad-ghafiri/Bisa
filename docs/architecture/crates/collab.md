# bisa-collab

The collaboration protocol ([14 — Collaboration](../14-collaboration.md)): everything a node
needs to talk to a human on another node, and nothing else. A host and a hosted member speak
the same envelope over ordinary Nostr relays; this crate is the wire — the wraps, the control
messages, the invite code, the relay pool. It knows no store, no engine, no node and no harness,
which is what lets a mobile client embed it whole.

---

## Where things live

| Module | Owns | Public types worth knowing |
|---|---|---|
| `lib.rs` | the crate's door: the protocol's words re-exported — the envelope, the control messages, the invite code, the relay pool | — |
| `error_text.rs` | `Localize` for the wire's refusals (`error-collab-…`) | — |
| `wrap.rs` | the NIP-59 envelope: a signed GEP event in a carrier rumor (`1060`) or a control message (`1061`), NIP-44 sealed sender → recipient, wrapped with an ephemeral key; unwrap enforces rumor author = seal sender | `wrap_for_member`, `wrap_control`, `unwrap_incoming`, `Incoming { Gep { sender, event }, Control { sender, control }, Other }` |
| `control.rs` | the words a host and a member say beside the facts, read only from the seal's verified sender; a person's face on the wire, bounded before a byte is decoded | `Control { Join, Welcome, Refused, Waiting, Removed, RoleChanged, ChannelsChanged, MembersChanged, OpenDm, Leave, IrohAddr, Profile, WantFace, Face }` (`kind`, `is_members_word`), `HostCard`, `Directory` (with `photo`), `Face { sha256, b64 }` (`from_bytes`, `decode` — the string cap `MAX_FACE_B64_CHARS`, the hash, `bisa_core::is_face`), `FaceRefusal`, `CLIENT_DESKTOP` · `CLIENT_MOBILE` · `CLIENT_CLI` |
| `invite_code.rs` | the code a person carries — the link `bisa://join/<nprofile>/<secret>` and the text `<nprofile>:<secret>`, either parsed; the secret minted once, its SHA-256 kept | `InviteCode { host, relays, secret }` (`new`, `link`, `text`, `parse`), `mint_secret`, `hash_secret`, `hashes_equal` (constant time) |
| `tls.rs` | the wire's one TLS crypto provider: `ensure_crypto_provider()` installs `ring` as the process default, once, respecting one already installed; `crypto_provider_ready()`; `PROVIDER`. Called where a client is made (`Relays::start`, `Relays::check`), because the websocket stack uses the process-wide provider and rustls, with both backends compiled into this workspace, has none until told — and panics without one ([14](../14-collaboration.md#the-protocol)) | `ensure_crypto_provider`, `crypto_provider_ready` |
| `relays.rs` | the pool: one `nostr_sdk::Client` with reconnecting relays, `set_relays` live, `health()` per relay (status, attempts, success rate, latency, bytes), `check(url)` on a throwaway client, `reconnect`, `publish`, `publish_relay_list` (NIP-65), one inbound stream of everything to this key unwrapped once, `catch_up` (NIP-77, then a since-window `REQ`) | `Relays`, `RelayHealth` (`off(url)` — a configured relay the wire is not talking to; `problem` — why a relay that was tried is not connected, `relay_problem`), `RelayStatusWord` (`Off` among them), `RelayCheck` |

---

## Invariants held here

| Invariant | Held by |
|---|---|
| A wrap round-trips with the inner signature intact and names the seal's sender; a host may relay another member's fact and the seal still names the host | `wrap.rs` tests |
| Every control is tagged by type and round-trips; a member's words are the four the host acts on | `control.rs` tests |
| Both code forms parse to the same halves; a code without a hex secret or an nprofile is refused | `invite_code.rs` tests |
| A wrap published by one pool reaches the other unwrapped; a catch-up hands a wrap on once; a relay change is live; a check says yes to a live relay and no to a dead port | `relays.rs` tests, against an in-process `LocalRelay` |

---

## What this crate refuses to do

Decide admission, keep a member, read a store, or hold a secret longer than the code that carries
it.
