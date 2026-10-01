//! The collaboration protocol (14-collaboration): everything a node needs to
//! talk to a human on another node, and nothing else.
//!
//! A **host** is the node whose workspace it is; a **hosted member** is a
//! human on another node, admitted by an invite and served pairwise what
//! their role reaches. Both sides speak the same envelope — a signed GEP
//! event, or a control message, gift-wrapped NIP-59 to one pubkey — over
//! ordinary Nostr relays, which see ciphertext and a recipient and nothing
//! else. This crate is the wire: the wraps, the control messages, the
//! invite code. It knows no store, no engine, no node and no harness, which
//! is what lets a mobile client embed it whole.

pub mod control;
pub mod error_text;
pub mod invite_code;
pub mod relays;
pub mod tls;
pub mod wrap;

pub use control::{
    Control, Directory, Face, FaceRefusal, HostCard, CLIENT_CLI, CLIENT_DESKTOP, CLIENT_MOBILE,
    MAX_FACE_B64_CHARS,
};
pub use invite_code::{hash_secret, hashes_equal, mint_secret, InviteCode, INVITE_LINK_PREFIX};
pub use relays::{
    relay_problem, RelayCheck, RelayHealth, RelayStatusWord, Relays, WRAP_TIMESTAMP_SLACK_SECS,
};
pub use tls::{crypto_provider_ready, ensure_crypto_provider};
pub use wrap::{
    unwrap_incoming, wrap_control, wrap_for_member, Incoming, KIND_GEP_CARRIER, KIND_GEP_CONTROL,
};

#[derive(Debug, thiserror::Error)]
pub enum CollabError {
    #[error("wrap: {0}")]
    Wrap(String),
    #[error("invite code: {0}")]
    InviteCode(String),
    #[error("relay: {0}")]
    Relay(String),
}
