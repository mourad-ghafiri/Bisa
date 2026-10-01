//! The one crypto provider every TLS connection of the wire uses.
//!
//! Every real relay is `wss://`, and the websocket stack under `nostr-sdk`
//! builds its TLS client with `rustls::ClientConfig::builder()` — the
//! **process-wide** provider. `rustls` chooses that provider from its crate
//! features only when exactly one backend is compiled in; this workspace
//! compiles both (`aws-lc-rs` is rustls' default and several dependents turn
//! it on; `ring` is what the direct transport and QUIC ask for, and the
//! workspace's feature unification gives every crate the union). With both
//! and none installed, `builder()` **panics** — inside the relay's connection
//! task, so a relay sat at *connecting* for ever and said nothing, while
//! everything over `reqwest`, which names its own provider, kept working.
//!
//! So the provider is installed here, once, and **where a client is made**
//! ([`crate::Relays::start`], [`crate::Relays::check`]): every embedder — the
//! node's pump, a one-shot CLI claim, a guest session, a mobile client — is
//! covered by construction and cannot forget. `ring`, because it is the
//! backend the direct transport already names (`bisa-net`'s `iroh_sync`): one
//! backend for the whole wire.

use std::sync::OnceLock;

/// The name of the backend the wire uses, for a report.
pub const PROVIDER: &str = "ring";

/// Install the process's default crypto provider, once. A provider that is
/// already installed — this function's own from an earlier call, or a host
/// application's — is respected: installing over one is refused by `rustls`
/// and that refusal is the answer wanted, never an error here.
pub fn ensure_crypto_provider() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        if rustls::crypto::CryptoProvider::get_default().is_none() {
            // `Err` means another thread installed one between the check and
            // here: there is a provider either way.
            if rustls::crypto::ring::default_provider()
                .install_default()
                .is_err()
            {
                tracing::debug!(target: "bisa_collab", "a TLS crypto provider was installed meanwhile; it is kept");
            }
        }
    });
}

/// Whether a TLS client can be built in this process right now — what a
/// report says before anybody dials anything.
pub fn crypto_provider_ready() -> bool {
    rustls::crypto::CryptoProvider::get_default().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tls_client_can_be_built_once_the_provider_is_ensured() {
        ensure_crypto_provider();
        assert!(crypto_provider_ready());
        // The exact call the websocket stack makes for a `wss://` relay
        // (`tokio-tungstenite`'s `ClientConfig::builder()`): with both
        // backends compiled in and no provider installed, this panics.
        let _config = rustls::ClientConfig::builder()
            .with_root_certificates(rustls::RootCertStore::empty())
            .with_no_client_auth();
    }

    #[test]
    fn ensuring_twice_is_once_and_a_provider_already_there_is_kept() {
        ensure_crypto_provider();
        let first = rustls::crypto::CryptoProvider::get_default().map(std::sync::Arc::as_ptr);
        ensure_crypto_provider();
        let second = rustls::crypto::CryptoProvider::get_default().map(std::sync::Arc::as_ptr);
        assert!(first.is_some());
        assert_eq!(
            first, second,
            "the provider installed first is the process's"
        );
        assert_eq!(PROVIDER, "ring");
    }
}
