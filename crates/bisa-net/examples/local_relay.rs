//! Dev-only local Nostr relay for demos and manual testing.
//!
//! Run: `cargo run -p bisa-net --example local_relay [port]`
//! Prints the ws:// URL, then serves until killed. Never use in production —
//! it has no persistence and no auth.

use nostr_relay_builder::builder::RelayBuilder;
use nostr_relay_builder::local::LocalRelay;

#[tokio::main]
async fn main() {
    let port: Option<u16> = std::env::args().nth(1).and_then(|p| p.parse().ok());
    let mut builder = RelayBuilder::default().addr("127.0.0.1".parse().unwrap());
    if let Some(p) = port {
        builder = builder.port(p);
    }
    let relay = LocalRelay::new(builder);
    relay.run().await.expect("relay failed to start");
    println!("{}", relay.url().await);
    // Serve until killed.
    tokio::signal::ctrl_c().await.ok();
}
