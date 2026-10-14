//! The pump's edges over an in-process LocalRelay: a restart that reads
//! its own ledger and sends nothing twice, a relay that is down and one
//! that comes back, the relay-only pump that fetches no attachment, an
//! admin's channel definition that lands on the host while a member's does
//! not, and a guest's direct message. No public relay is ever contacted.

use crate::host::{invite, open_ws, pid, post, start_guest, start_host, start_relay, wait_for};
use bisa_collab::{wrap_for_member, Relays};
use bisa_core::{ChannelId, MemberRole, RosterPolicy};
use bisa_guest::HostedState;
use bisa_net::{Host, NetConfig};
use bisa_store::{Admission, Workspace};
use nostr::event::Event;
use std::sync::Arc;
use std::time::Duration;

/// A channel definition as a node signed it: the event its own snapshot
/// store holds for `id`.
fn channel_event(ws: &Workspace, id: &str) -> Event {
    let path = ws
        .paths()
        .state_dir("channels")
        .join(format!("{}-{id}.json", bisa_core::kind::KIND_CHANNEL));
    serde_json::from_str(&std::fs::read_to_string(&path).expect("the channel's snapshot"))
        .expect("an event")
}

fn listed() -> RosterPolicy {
    RosterPolicy::Listed {
        agents: vec![],
        teams: vec![],
        humans: vec![],
    }
}

/// The ledger says what was sent: a pump started over it again sends
/// nothing twice.
#[tokio::test(flavor = "multi_thread")]
async fn a_restarted_pump_reads_its_ledger_and_sends_nothing_twice() {
    std::env::set_var("BISA_JOIN_WAIT_SECS", "10");
    let (_relay, url) = start_relay().await;
    let dir_a = tempfile::tempdir().unwrap();
    let ws_a = open_ws(dir_a.path());
    let (relays_a, host) = start_host(&ws_a, dir_a.path(), &url).await;
    let bob = start_guest(&url).await;
    let code = invite(&ws_a, &url, MemberRole::Member, &[]);
    let (b, state) = bob.guests.join(&code, None, "cli").await.unwrap();
    assert_eq!(state, HostedState::Member);
    post(&ws_a, "general", "said once");
    let b2 = Arc::clone(&b);
    wait_for("the post on Bob", move || {
        b2.messages("general", None, 10)
            .map(|m| m.iter().any(|x| x.text == "said once"))
            .unwrap_or(false)
    })
    .await;
    let sent = host.status().await.published;
    assert!(sent >= 1);
    host.stop().await;
    let ledger = dir_a.path().join("net_published.jsonl");
    let lines = std::fs::read_to_string(&ledger).unwrap().lines().count();
    assert!(lines >= 1, "the ledger has what was sent");

    let again = Host::start(
        Arc::clone(&ws_a),
        relays_a,
        NetConfig {
            relays: vec![url.clone()],
            sync_interval_secs: 1,
            iroh: false,
            ..NetConfig::default()
        },
        dir_a.path().to_path_buf(),
    )
    .await
    .unwrap();
    again.catch_up_now().await;
    assert_eq!(again.status().await.published, 0, "nothing was sent twice");
    assert_eq!(
        std::fs::read_to_string(&ledger).unwrap().lines().count(),
        lines
    );
    again.stop().await;
    bob.relays.stop().await;
}

/// A relay-only pump fetches no attachment — nobody to ask — and takes
/// the relay-list flag like any other `sync.*` reading.
#[tokio::test(flavor = "multi_thread")]
async fn a_relay_only_pump_fetches_no_attachment_and_takes_the_relay_list_flag() {
    let (_relay, url) = start_relay().await;
    let dir_a = tempfile::tempdir().unwrap();
    let ws_a = open_ws(dir_a.path());
    let (relays_a, host) = start_host(&ws_a, dir_a.path(), &url).await;
    let fetch = host.attachment_fetcher();
    assert!(!fetch.fetch(&"0".repeat(64)).await);
    host.apply(&NetConfig {
        relays: vec![url.clone()],
        sync_interval_secs: 1,
        publish_relay_list: true,
        iroh: false,
        ..NetConfig::default()
    })
    .await;
    assert_eq!(relays_a.urls().await, vec![url.clone()]);
    let status = host.status().await;
    assert_eq!(status.connected_relays, 1);
    assert_eq!(status.people, 0);
    host.stop().await;
}

/// A relay that is down takes nothing: what the pump had to say stays in
/// its outbox, and a relay that comes back takes it on the next flush.
#[tokio::test(flavor = "multi_thread")]
async fn a_relay_that_is_down_keeps_the_outbox_and_one_that_comes_back_takes_it() {
    let dead = "ws://127.0.0.1:1".to_string();
    let dir_a = tempfile::tempdir().unwrap();
    let ws_a = open_ws(dir_a.path());
    let relays = Relays::start(ws_a.owner_keys().clone(), std::slice::from_ref(&dead)).await;
    let cfg = |url: &str| NetConfig {
        relays: vec![url.to_string()],
        sync_interval_secs: 3600,
        publish_relay_list: false,
        iroh: false,
        ..NetConfig::default()
    };
    let host = Host::start(
        Arc::clone(&ws_a),
        Arc::clone(&relays),
        cfg(&dead),
        dir_a.path().to_path_buf(),
    )
    .await
    .unwrap();
    let bob_keys = nostr::key::Keys::generate();
    ws_a.add_member(
        pid(bob_keys.public_key()),
        MemberRole::Member,
        Admission::default(),
    )
    .unwrap();
    post(&ws_a, "general", "while the relay is down");
    tokio::time::sleep(Duration::from_millis(800)).await;
    host.catch_up_now().await;
    let status = host.status().await;
    assert_eq!(status.published, 0, "nothing left while the relay was down");
    assert_eq!(status.connected_relays, 0);
    assert!(!dir_a.path().join("net_published.jsonl").exists());

    let (_relay, url) = start_relay().await;
    host.apply(&cfg(&url)).await;
    host.catch_up_now().await;
    let status = host.status().await;
    assert_eq!(status.connected_relays, 1);
    assert!(status.published >= 1, "{status:?}");
    let ledger = std::fs::read_to_string(dir_a.path().join("net_published.jsonl")).unwrap();
    assert!(ledger.contains(&bob_keys.public_key().to_hex()));
    host.stop().await;
}

/// A hosted admin's word about the room's channels lands on the host — the
/// latest definition, never an older one — and a member's does not.
#[tokio::test(flavor = "multi_thread")]
async fn an_admin_s_channel_definition_lands_and_a_member_s_does_not() {
    let (_relay, url) = start_relay().await;
    let dir_a = tempfile::tempdir().unwrap();
    let ws_a = open_ws(dir_a.path());
    let (_relays_a, host) = start_host(&ws_a, dir_a.path(), &url).await;
    let host_pk = ws_a.owner_keys().public_key();

    let dir_b = tempfile::tempdir().unwrap();
    let ws_b = open_ws(dir_b.path());
    let dir_c = tempfile::tempdir().unwrap();
    let ws_c = open_ws(dir_c.path());
    ws_a.add_member(
        ws_b.owner_principal(),
        MemberRole::Admin,
        Admission::default(),
    )
    .unwrap();
    ws_a.add_member(
        ws_c.owner_principal(),
        MemberRole::Member,
        Admission::default(),
    )
    .unwrap();
    let pool_b = Relays::start(ws_b.owner_keys().clone(), std::slice::from_ref(&url)).await;
    let pool_c = Relays::start(ws_c.owner_keys().clone(), std::slice::from_ref(&url)).await;

    // The admin defines a channel, then changes its topic twice.
    let blue = ChannelId::new("blue").unwrap();
    ws_b.create_channel("blue", Some("sky"), listed(), Default::default())
        .unwrap();
    let first = channel_event(&ws_b, "blue");
    ws_b.update_channel(&blue, Some("sea"), listed(), Default::default())
        .unwrap();
    let second = channel_event(&ws_b, "blue");
    ws_b.update_channel(&blue, Some("lake"), listed(), Default::default())
        .unwrap();
    let third = channel_event(&ws_b, "blue");
    for event in [&first, &third] {
        pool_b
            .publish(&wrap_for_member(ws_b.owner_keys(), host_pk, event).unwrap())
            .await
            .unwrap();
    }
    let ws_a2 = Arc::clone(&ws_a);
    wait_for(
        "the admin's channel on the host, at its latest topic",
        move || {
            ws_a2
                .get_channel(&blue)
                .map(|c| c.topic.as_deref() == Some("lake"))
                .unwrap_or(false)
        },
    )
    .await;
    // The definition in between arrives late: older than what stands.
    pool_b
        .publish(&wrap_for_member(ws_b.owner_keys(), host_pk, &second).unwrap())
        .await
        .unwrap();
    // A member's definition of a channel is not a person's act.
    ws_c.create_channel("green", None, listed(), Default::default())
        .unwrap();
    let green = channel_event(&ws_c, "green");
    pool_c
        .publish(&wrap_for_member(ws_c.owner_keys(), host_pk, &green).unwrap())
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(800)).await;
    host.catch_up_now().await;
    let blue = ChannelId::new("blue").unwrap();
    assert_eq!(
        ws_a.get_channel(&blue).unwrap().topic.as_deref(),
        Some("lake"),
        "an older definition changes nothing"
    );
    assert!(
        ws_a.get_channel(&ChannelId::new("green").unwrap()).is_err(),
        "a member's channel definition is refused"
    );
    assert!(host.status().await.ingested >= 2);
    host.stop().await;
    pool_c.stop().await;
    pool_b.stop().await;
}

/// A guest's direct message with another person is opened by the host and
/// said to both; asking again for the same one opens nothing new.
#[tokio::test(flavor = "multi_thread")]
async fn a_guest_opens_a_direct_message_and_asking_again_opens_nothing_new() {
    std::env::set_var("BISA_JOIN_WAIT_SECS", "10");
    let (_relay, url) = start_relay().await;
    let dir_a = tempfile::tempdir().unwrap();
    let ws_a = open_ws(dir_a.path());
    let (_relays_a, host) = start_host(&ws_a, dir_a.path(), &url).await;
    let bob = start_guest(&url).await;
    let carol = start_guest(&url).await;
    let code = invite(&ws_a, &url, MemberRole::Member, &[]);
    let (b, state) = bob
        .guests
        .join(&code, Some("Bob".into()), "cli")
        .await
        .unwrap();
    assert_eq!(state, HostedState::Member);
    let code = invite(&ws_a, &url, MemberRole::Member, &[]);
    let (c, state) = carol
        .guests
        .join(&code, Some("Carol".into()), "cli")
        .await
        .unwrap();
    assert_eq!(state, HostedState::Member);
    let b2 = Arc::clone(&b);
    wait_for("Carol in Bob's directory", move || {
        b2.members().map(|m| m.len() == 3).unwrap_or(false)
    })
    .await;

    let c_id = pid(carol.keys.public_key());
    b.open_dm(std::slice::from_ref(&c_id)).await.unwrap();
    let (b3, c2) = (Arc::clone(&b), Arc::clone(&c));
    wait_for("the direct message on both", move || {
        b3.dms().map(|d| d.len() == 1).unwrap_or(false)
            && c2.dms().map(|d| d.len() == 1).unwrap_or(false)
    })
    .await;
    b.open_dm(std::slice::from_ref(&c_id)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(
        b.dms().unwrap().len(),
        1,
        "the same conversation, said again"
    );
    assert_eq!(
        ws_a.list_channels_of_kind(bisa_core::ChannelKind::Direct)
            .unwrap()
            .len(),
        1
    );
    host.stop().await;
    carol.relays.stop().await;
    bob.relays.stop().await;
}

/// A pump with no relay at all says nothing and keeps nothing: a person
/// admitted is not welcomed over a wire there is none of, a post reaching
/// nobody is relayed to nobody, and a read marker is the node's own.
#[tokio::test(flavor = "multi_thread")]
async fn a_pump_with_no_relay_says_nothing_and_keeps_nothing() {
    let dir_a = tempfile::tempdir().unwrap();
    let ws_a = open_ws(dir_a.path());
    let relays = Relays::start(ws_a.owner_keys().clone(), &[]).await;
    let host = Host::start(
        Arc::clone(&ws_a),
        Arc::clone(&relays),
        NetConfig {
            relays: vec![],
            sync_interval_secs: 3600,
            iroh: false,
            ..NetConfig::default()
        },
        dir_a.path().to_path_buf(),
    )
    .await
    .unwrap();
    post(&ws_a, "general", "to nobody");
    let bob_keys = nostr::key::Keys::generate();
    ws_a.add_member(
        pid(bob_keys.public_key()),
        MemberRole::Member,
        Admission::default(),
    )
    .unwrap();
    post(&ws_a, "general", "still to no wire");
    ws_a.mark_read("general").unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    host.catch_up_now().await;
    let status = host.status().await;
    assert_eq!(status.connected_relays, 0);
    assert_eq!(status.published, 0);
    assert_eq!(status.people, 1);
    assert!(!dir_a.path().join("net_published.jsonl").exists());
    host.stop().await;
    relays.stop().await;
}
