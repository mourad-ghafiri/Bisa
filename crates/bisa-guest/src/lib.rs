//! The guest side of collaboration (14-collaboration): what a human on this
//! node holds of a workspace they joined on another — a **guest replica**
//! per host, fed pairwise by that host with exactly what the person's role
//! reaches, and written to by this person alone.
//!
//! No store, no engine, no harness, no node: a keypair, the host's card, the
//! channels and people the host said, the facts it relayed, and the person's
//! own signed acts on their way back. This is the crate a mobile client
//! embeds whole; the desktop's node hosts it beside the workspace it owns.

pub mod error_text;
pub mod faces;
pub mod session;
pub mod store;

pub use faces::{FaceStore, MemoryFaces, OwnerProfile};
pub use session::{join_wait, GuestSession, Guests, HostedChange, Posted, HOSTS_DIR};
pub use store::{scope_of, GuestStore, Hosted, HostedMessage, HostedReaction, HostedState};

#[derive(Debug, thiserror::Error)]
pub enum GuestError {
    #[error("guest store: {0}")]
    Store(String),
    #[error("not a member of that host")]
    NotHosted,
    #[error("{0}")]
    Refused(String),
    #[error(transparent)]
    Collab(#[from] bisa_collab::CollabError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
