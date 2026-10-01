//! The direct QUIC transport (feature `iroh`), loopback only: endpoints bind
//! with relays disabled and no address-lookup services; peers are wired by
//! hand with 127.0.0.1 socket addresses. No relay — Nostr or iroh — is ever
//! contacted.
//!
//! A session forms only between nodes that host each other's key: the hello
//! is checked against the hosted members, so a stranger's dial is closed
//! and a hosted member's carries the facts its role reaches.
#![cfg(feature = "iroh")]

use bisa_collab::Relays;
use bisa_core::{ManualPeer, MemberRole, MessageBody, PrincipalId};
use bisa_net::{Host, NetConfig};
use bisa_store::{Admission, MemoryKeyStore, PostOrigin, Workspace};
use std::sync::Arc;
use std::time::Duration;

/// Log lines when asked for (`RUST_LOG=bisa_net=debug`), into the test's
/// own capture; installed once, so a second test's call is a no-op.
fn trace() {
    let _already_installed = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .try_init();
}

fn open_ws(dir: &std::path::Path) -> Arc<Workspace> {
    Arc::new(
        Workspace::open_with_keystore(dir, Box::new(MemoryKeyStore::default()))
            .expect("workspace open"),
    )
}

fn relayless_cfg(peers: Vec<ManualPeer>) -> NetConfig {
    NetConfig {
        sync_interval_secs: 1,
        iroh: true,
        iroh_peers: peers,
        ..NetConfig::default()
    }
}

async fn start(ws: &Arc<Workspace>, dir: &std::path::Path, cfg: NetConfig) -> Host {
    let relays = Relays::start(ws.owner_keys().clone(), &[]).await;
    Host::start(Arc::clone(ws), relays, cfg, dir.to_path_buf())
        .await
        .expect("host start")
}

async fn wait_for(what: &str, mut pred: impl FnMut() -> bool) {
    for _ in 0..200 {
        if pred() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("timed out waiting for: {what}");
}

fn pid(ws: &Workspace) -> PrincipalId {
    ws.owner_principal()
}

fn host_each_other(a: &Workspace, b: &Workspace) {
    a.add_member(pid(b), MemberRole::Member, Admission::default())
        .unwrap();
    b.add_member(pid(a), MemberRole::Member, Admission::default())
        .unwrap();
}

/// Two nodes that host each other's key form a session over loopback, and
/// a message posted on one lands on the other — the backlog by have/want,
/// the live one by push — with zero relays configured anywhere.
#[tokio::test(flavor = "multi_thread")]
async fn a_hosted_member_s_session_carries_the_facts_its_role_reaches() {
    trace();
    std::env::set_var("BISA_IROH_RECONNECT_SECS", "1");
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let ws_a = open_ws(dir_a.path());
    let ws_b = open_ws(dir_b.path());
    host_each_other(&ws_a, &ws_b);

    // Backlog: said before any pump exists.
    ws_a.post_message(
        "general",
        MessageBody::post("before the session"),
        None,
        &[],
        &[],
        None,
        PostOrigin::Asked,
    )
    .unwrap();

    let host_a = start(&ws_a, dir_a.path(), relayless_cfg(vec![])).await;
    assert_eq!(
        host_a.status().await.connected_relays,
        0,
        "no relays configured"
    );
    let (node_id, addrs) = host_a.iroh_endpoint().expect("iroh running on A");
    assert!(!addrs.is_empty(), "A advertises direct addresses");

    let peer_a = ManualPeer {
        member_pubkey: ws_a.owner_principal().as_hex().to_string(),
        node_id,
        addrs,
    };
    let host_b = start(&ws_b, dir_b.path(), relayless_cfg(vec![peer_a])).await;

    let ws_b2 = Arc::clone(&ws_b);
    wait_for("the backlog message on B", move || {
        ws_b2
            .messages("general", None, 10)
            .map(|m| m.iter().any(|r| r.content.contains("before the session")))
            .unwrap_or(false)
    })
    .await;

    ws_a.post_message(
        "general",
        MessageBody::post("live"),
        None,
        &[],
        &[],
        None,
        PostOrigin::Asked,
    )
    .unwrap();
    let ws_b3 = Arc::clone(&ws_b);
    wait_for("the live message on B", move || {
        ws_b3
            .messages("general", None, 10)
            .map(|m| m.iter().any(|r| r.content == "live"))
            .unwrap_or(false)
    })
    .await;

    assert!(host_a.status().await.iroh_peers_connected >= 1);
    assert!(host_b.status().await.iroh_peers_connected >= 1);

    host_b.stop().await;
    host_a.stop().await;
}

/// A node that knows A's address but is not hosted on A gets its hello
/// closed and never receives a fact.
#[tokio::test(flavor = "multi_thread")]
async fn a_stranger_s_hello_is_closed() {
    trace();
    std::env::set_var("BISA_IROH_RECONNECT_SECS", "1");
    let dir_a = tempfile::tempdir().unwrap();
    let dir_c = tempfile::tempdir().unwrap();
    let ws_a = open_ws(dir_a.path());
    let ws_c = open_ws(dir_c.path());
    // C hosts A so that C even tries to dial; A does not host C.
    ws_c.add_member(pid(&ws_a), MemberRole::Member, Admission::default())
        .unwrap();
    ws_a.post_message(
        "general",
        MessageBody::post("members only"),
        None,
        &[],
        &[],
        None,
        PostOrigin::Asked,
    )
    .unwrap();

    let host_a = start(&ws_a, dir_a.path(), relayless_cfg(vec![])).await;
    let (node_id, addrs) = host_a.iroh_endpoint().unwrap();
    let peer_a = ManualPeer {
        member_pubkey: ws_a.owner_principal().as_hex().to_string(),
        node_id,
        addrs,
    };
    let host_c = start(&ws_c, dir_c.path(), relayless_cfg(vec![peer_a])).await;

    // Give the stranger several dial/reject cycles.
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert!(
        ws_c.messages("general", None, 10).unwrap().is_empty(),
        "a stranger must never receive a fact"
    );
    assert_eq!(
        host_a.status().await.iroh_peers_connected,
        0,
        "no authenticated session for a stranger"
    );

    host_c.stop().await;
    host_a.stop().await;
}
