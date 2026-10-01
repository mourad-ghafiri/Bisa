//! The host's side of collaboration on the wire (14-collaboration): the
//! pump between a workspace and the people it hosts on other nodes, over
//! the relay pool `bisa-collab` opens, and the direct QUIC transport for
//! attachment bytes.
//!
//! Relays only ever see kind-1059 ciphertext addressed to member pubkeys —
//! they carry reach, never authority. The local store stays the truth: what
//! a person receives is what their role reaches, wrapped pairwise to them;
//! what a person sends is admitted by the store's ladder and nothing else.

pub mod config;
pub mod error_text;
pub mod host;
#[cfg(feature = "iroh")]
pub mod iroh_sync;
pub mod truth;

pub use config::NetConfig;
pub use host::{AttachmentFetch, Host, SyncStatus};

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("net config: {0}")]
    Config(String),
    #[error("relay: {0}")]
    Relay(String),
    #[error(transparent)]
    Collab(#[from] bisa_collab::CollabError),
    #[error(transparent)]
    Store(#[from] bisa_store::StoreError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
