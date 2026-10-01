//! A host and the people it hosts, end to end through an in-process
//! LocalRelay: an invite claimed, a guest reaching only its rostered
//! channel, a member reaching every standing channel, a direct message its
//! audience alone, a hosted member's fact relayed to the others and refused
//! outside its reach, a promotion widening reach, a removal, and the
//! `ask` admission. No public relay is ever contacted.

use bisa_collab::{hash_secret, mint_secret, wrap_for_member, Face, InviteCode, Relays};
use bisa_core::kind::KIND_MESSAGE;
use bisa_core::{MemberRole, MessageBody, PrincipalId, RosterPolicy};
use bisa_guest::{FaceStore as _, GuestError, Guests, HostedState, MemoryFaces, OwnerProfile};
use bisa_net::{Host, NetConfig};
use bisa_store::{MemoryKeyStore, PostOrigin, Workspace};
use nostr::event::{EventBuilder, FinalizeEvent, Kind, Tag};
use nostr::key::{Keys, PublicKey};
use nostr_relay_builder::builder::RelayBuilder;
use nostr_relay_builder::local::LocalRelay;
use std::sync::Arc;
use std::time::Duration;

async fn start_relay() -> (LocalRelay, String) {
    let relay = LocalRelay::new(RelayBuilder::default().addr("127.0.0.1".parse().unwrap()));
    relay.run().await.expect("relay run");
    let url = relay.url().await.to_string();
    (relay, url)
}

fn open_ws(dir: &std::path::Path) -> Arc<Workspace> {
    Arc::new(
        Workspace::open_with_keystore(dir, Box::new(MemoryKeyStore::default()))
            .expect("workspace open"),
    )
}

fn pid(pk: PublicKey) -> PrincipalId {
    PrincipalId::new(pk.to_hex()).unwrap()
}

async fn start_host(ws: &Arc<Workspace>, dir: &std::path::Path, url: &str) -> (Arc<Relays>, Host) {
    let relays = Relays::start(
        ws.owner_keys().clone(),
        std::slice::from_ref(&url.to_string()),
    )
    .await;
    let cfg = NetConfig {
        relays: vec![url.to_string()],
        sync_interval_secs: 1,
        iroh: false,
        ..NetConfig::default()
    };
    let host = Host::start(Arc::clone(ws), Arc::clone(&relays), cfg, dir.to_path_buf())
        .await
        .expect("host start");
    (relays, host)
}

struct Guest {
    keys: Keys,
    relays: Arc<Relays>,
    guests: Arc<Guests>,
    /// Where this person's node keeps faces and what it says of its owner.
    faces: Arc<MemoryFaces>,
    _router: tokio::task::JoinHandle<()>,
    _dir: tempfile::TempDir,
}

async fn start_guest(url: &str) -> Guest {
    let keys = Keys::generate();
    let relays = Relays::start(keys.clone(), std::slice::from_ref(&url.to_string())).await;
    let dir = tempfile::tempdir().unwrap();
    let faces = Arc::new(MemoryFaces::new());
    let guests = Guests::open(
        keys.clone(),
        dir.path(),
        Arc::clone(&relays),
        Arc::clone(&faces) as Arc<dyn bisa_guest::FaceStore>,
    )
    .unwrap();
    let router = guests.spawn();
    Guest {
        keys,
        relays,
        guests,
        faces,
        _router: router,
        _dir: dir,
    }
}

/// An invite on the host, as its code.
fn invite(ws: &Workspace, url: &str, role: MemberRole, channels: &[&str]) -> InviteCode {
    let secret = mint_secret();
    let channels = channels
        .iter()
        .map(|c| bisa_core::ChannelId::new(*c).unwrap())
        .collect();
    ws.create_invite(role, channels, None, 3600, hash_secret(&secret))
        .unwrap();
    InviteCode::new(
        ws.owner_keys().public_key(),
        std::slice::from_ref(&url.to_string()),
        secret,
    )
}

/// Wait until `pred` is true, or panic after ~15s.
async fn wait_for(what: &str, mut pred: impl FnMut() -> bool) {
    for _ in 0..150 {
        if pred() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("timed out waiting for: {what}");
}

fn post(ws: &Workspace, scope: &str, text: &str) -> String {
    ws.post_message(
        scope,
        MessageBody::post(text),
        None,
        &[],
        &[],
        None,
        PostOrigin::Asked,
    )
    .unwrap()
}

/// A PNG of `len` bytes: the header a picture is known by, then filler.
fn a_png(len: usize, filler: u8) -> Vec<u8> {
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.resize(len, filler);
    png
}

/// A picture as a person's node refers to it: by the hash of its bytes.
fn a_ref(name: &str, bytes: &[u8]) -> bisa_core::AttachmentRef {
    bisa_core::AttachmentRef {
        sha256: Face::from_bytes(bytes).sha256,
        name: name.into(),
        mime: "image/png".into(),
        size: bytes.len() as u64,
    }
}

/// A person says who they are for themselves alone: the name and the face a
/// member's node sends land on that member's row and no other. A face is
/// kept only when it is one — its bytes hashing to the reference that names
/// them, a picture, under the cap; one that is not leaves the picture as it
/// was and the name lands all the same. No face at all takes the picture
/// away.
#[tokio::test(flavor = "multi_thread")]
async fn a_person_says_who_they_are_for_themselves_alone_and_a_face_is_kept_only_when_it_is_one() {
    std::env::set_var("BISA_JOIN_WAIT_SECS", "10");
    let (_relay, url) = start_relay().await;
    let dir_a = tempfile::tempdir().unwrap();
    let ws_a = open_ws(dir_a.path());
    let (_relays_a, _host) = start_host(&ws_a, dir_a.path(), &url).await;
    let bob = start_guest(&url).await;
    let carol = start_guest(&url).await;
    for (person, name) in [(&bob, "Bob"), (&carol, "Carol")] {
        let code = invite(&ws_a, &url, MemberRole::Member, &[]);
        let (_, state) = person
            .guests
            .join(&code, Some(name.into()), "desktop")
            .await
            .unwrap();
        assert_eq!(state, HostedState::Member);
    }
    let (b_id, c_id) = (pid(bob.keys.public_key()), pid(carol.keys.public_key()));
    let row = |who: &PrincipalId| ws_a.member(who).unwrap().expect("a member");
    // What Bob's node says next, and the wait for the host to have heard it.
    let says = |label: &str, photo: Option<bisa_core::AttachmentRef>| {
        bob.faces.set_profile(OwnerProfile {
            label: Some(label.into()),
            photo,
        });
    };
    async fn heard(ws: &Arc<Workspace>, who: &PrincipalId, label: &str) {
        let (ws, who, label) = (Arc::clone(ws), who.clone(), label.to_string());
        wait_for("the host to hear the name", move || {
            ws.member(&who)
                .ok()
                .flatten()
                .is_some_and(|m| m.label.as_deref() == Some(label.as_str()))
        })
        .await;
    }

    // --- a name and a face: on Bob's row, and on nobody else's ---
    let face = a_png(600, 3);
    let photo = a_ref("bob.png", &face);
    bob.faces.hold(&photo.sha256, &face).unwrap();
    says("Bob B.", Some(photo.clone()));
    bob.guests.announce_profile().await;
    heard(&ws_a, &b_id, "Bob B.").await;
    assert_eq!(row(&b_id).photo, Some(photo.clone()));
    assert_eq!(
        ws_a.attachment_bytes(&photo.sha256).unwrap().as_deref(),
        Some(face.as_slice()),
        "the face is kept under its hash"
    );
    let carols = row(&c_id);
    assert_eq!(carols.label.as_deref(), Some("Carol"));
    assert_eq!(carols.photo, None, "what Bob says is said of Bob alone");

    // --- bytes that are not the face their reference names ---
    let other = a_png(700, 5);
    let claimed = a_ref("bob-2.png", &a_png(650, 4));
    bob.faces.hold(&claimed.sha256, &other).unwrap();
    says("Robert", Some(claimed.clone()));
    bob.guests.announce_profile().await;
    heard(&ws_a, &b_id, "Robert").await;
    assert_eq!(
        row(&b_id).photo,
        Some(photo.clone()),
        "a face that lies about its bytes changes no picture"
    );
    assert_eq!(ws_a.attachment_bytes(&claimed.sha256).unwrap(), None);

    // --- a picture over the cap ---
    let big = a_png(bisa_core::MAX_FACE_BYTES as usize + 1, 7);
    let too_big = a_ref("bob-big.png", &big);
    bob.faces.hold(&too_big.sha256, &big).unwrap();
    says("Bobby", Some(too_big.clone()));
    bob.guests.announce_profile().await;
    heard(&ws_a, &b_id, "Bobby").await;
    assert_eq!(
        row(&b_id).photo,
        Some(photo.clone()),
        "a face over the cap is not kept, and the one before it stands"
    );
    assert_eq!(ws_a.attachment_bytes(&too_big.sha256).unwrap(), None);

    // --- no face at all takes the picture away ---
    says("Bob", None);
    bob.guests.announce_profile().await;
    heard(&ws_a, &b_id, "Bob").await;
    assert_eq!(row(&b_id).photo, None);
    assert_eq!(row(&c_id).label.as_deref(), Some("Carol"));
}

/// C9 — what a person holds of a host is apart from their own workspace.
/// A node that is also somebody's guest keeps the replica under `hosts/` in
/// its own data folder, and its own store — rebuilt from its files with the
/// replica beside them — reads none of it: no channel of the host's, no
/// message, no member.
#[tokio::test(flavor = "multi_thread")]
async fn a_replica_of_a_host_is_nothing_the_nodes_own_store_reads() {
    std::env::set_var("BISA_JOIN_WAIT_SECS", "10");
    let (_relay, url) = start_relay().await;
    let dir_a = tempfile::tempdir().unwrap();
    let ws_a = open_ws(dir_a.path());
    ws_a.create_channel(
        "design",
        Some("shapes"),
        RosterPolicy::Listed {
            agents: vec![],
            teams: vec![],
            humans: vec![],
        },
        Default::default(),
    )
    .unwrap();
    post(&ws_a, "general", "hello from the host");
    post(&ws_a, "design", "shapes, on the host");
    let (_relays_a, _host) = start_host(&ws_a, dir_a.path(), &url).await;

    // Bob has a node of his own: a workspace, and what he holds of the host
    // in the same folder.
    let dir_b = tempfile::tempdir().unwrap();
    let ws_b = open_ws(dir_b.path());
    post(&ws_b, "general", "a note to myself");
    let keys = ws_b.owner_keys().clone();
    let relays = Relays::start(keys.clone(), std::slice::from_ref(&url.to_string())).await;
    let guests = Guests::open(
        keys,
        dir_b.path(),
        Arc::clone(&relays),
        Arc::new(MemoryFaces::new()),
    )
    .unwrap();
    let _router = guests.spawn();
    let code = invite(&ws_a, &url, MemberRole::Member, &[]);
    let (b, state) = guests
        .join(&code, Some("Bob".into()), "desktop")
        .await
        .unwrap();
    assert_eq!(state, HostedState::Member);
    let b2 = Arc::clone(&b);
    wait_for("the host's words on Bob's replica", move || {
        let said = |scope: &str, words: &str| {
            b2.messages(scope, None, 10)
                .map(|m| m.iter().any(|x| x.text == words))
                .unwrap_or(false)
        };
        said("general", "hello from the host") && said("design", "shapes, on the host")
    })
    .await;

    // The replica is on disk, under `hosts/` in Bob's own folder …
    let replica = dir_b
        .path()
        .join(bisa_guest::HOSTS_DIR)
        .join(ws_a.owner_keys().public_key().to_hex());
    assert!(replica.is_dir(), "{}", replica.display());

    // … and Bob's own store, rebuilt from its files, holds his own things
    // and nothing of the host's.
    ws_b.rebuild_index().unwrap();
    let own: Vec<String> = ws_b
        .list_channels_of_kind(bisa_core::ChannelKind::Standing)
        .unwrap()
        .iter()
        .map(|c| c.id.to_string())
        .collect();
    assert_eq!(own, vec!["general"], "the host's channels are not his own");
    let said: Vec<String> = ws_b
        .messages("general", None, 10)
        .unwrap()
        .into_iter()
        .map(|m| m.content)
        .collect();
    assert_eq!(said, vec!["a note to myself"]);
    assert!(
        ws_b.messages("design", None, 10)
            .map(|rows| rows.is_empty())
            .unwrap_or(true),
        "what was said in a channel of the host's is not in his own index"
    );
    let people = ws_b.people().unwrap();
    assert!(
        people.is_empty(),
        "the host's directory is not his own: {people:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn reach_follows_the_role_and_the_roster_and_a_person_s_fact_is_relayed_or_refused() {
    std::env::set_var("BISA_JOIN_WAIT_SECS", "10");
    let (_relay, url) = start_relay().await;
    let dir_a = tempfile::tempdir().unwrap();
    let ws_a = open_ws(dir_a.path());
    let design = ws_a
        .create_channel(
            "design",
            Some("shapes"),
            RosterPolicy::Listed {
                agents: vec![],
                teams: vec![],
                humans: vec![],
            },
            Default::default(),
        )
        .unwrap();
    let before_anyone = post(&ws_a, "general", "hello general");
    let (_relays_a, host) = start_host(&ws_a, dir_a.path(), &url).await;

    // --- a guest on #design, a member everywhere ---
    let bob = start_guest(&url).await;
    let carol = start_guest(&url).await;
    let code_b = invite(&ws_a, &url, MemberRole::Guest, &["design"]);
    let (b, state) = bob
        .guests
        .join(&code_b, Some("Bob".into()), "desktop")
        .await
        .unwrap();
    assert_eq!(state, HostedState::Member, "a valid code admits at once");
    let code_c = invite(&ws_a, &url, MemberRole::Member, &[]);
    let (c, state) = carol
        .guests
        .join(&code_c, Some("Carol".into()), "mobile")
        .await
        .unwrap();
    assert_eq!(state, HostedState::Member);

    let b_id = pid(bob.keys.public_key());
    let c_id = pid(carol.keys.public_key());
    assert_eq!(ws_a.member_role(&b_id).unwrap(), Some(MemberRole::Guest));
    assert_eq!(ws_a.member_role(&c_id).unwrap(), Some(MemberRole::Member));
    assert_eq!(
        ws_a.member(&b_id).unwrap().unwrap().client.as_deref(),
        Some("desktop")
    );
    assert!(
        ws_a.get_channel(&design.id)
            .unwrap()
            .roster
            .lists_human(&b_id),
        "the guest was put on the channel the invite named"
    );
    let b_channels: Vec<String> = b
        .channels()
        .unwrap()
        .iter()
        .map(|c| c.id.to_string())
        .collect();
    assert_eq!(
        b_channels,
        vec!["design"],
        "a guest reaches its rostered channel only"
    );
    let mut c_channels: Vec<String> = c
        .channels()
        .unwrap()
        .iter()
        .map(|c| c.id.to_string())
        .collect();
    c_channels.sort();
    assert_eq!(
        c_channels,
        vec!["design", "general"],
        "a member reaches every standing channel"
    );
    assert_eq!(
        b.hosted().unwrap().unwrap().host.name,
        format!("{}…", &ws_a.owner_principal().as_hex()[..8])
    );
    assert_eq!(
        c.members().unwrap().len(),
        3,
        "the directory: the owner and two people"
    );

    // --- the general backlog reached the member, never the guest ---
    let c2 = Arc::clone(&c);
    wait_for("general backlog on Carol", move || {
        c2.messages("general", None, 10)
            .map(|m| m.iter().any(|x| x.text == "hello general"))
            .unwrap_or(false)
    })
    .await;
    // The guest was never told of `general`: asking for it is refused in
    // words, as a post into it is — never answered as a room with nothing
    // in it, which would hide what was not sent.
    let refused = b.messages("general", None, 10);
    assert!(
        matches!(&refused, Err(GuestError::Refused(why)) if why.contains("general")),
        "{refused:?}"
    );

    // --- a post in #design reaches both; Bob's answer reaches A and Carol ---
    post(&ws_a, "design", "shapes?");
    let (b2, c3) = (Arc::clone(&b), Arc::clone(&c));
    wait_for("design post on Bob", move || {
        b2.messages("design", None, 10)
            .map(|m| !m.is_empty())
            .unwrap_or(false)
    })
    .await;
    wait_for("design post on Carol", move || {
        c3.messages("design", None, 10)
            .map(|m| !m.is_empty())
            .unwrap_or(false)
    })
    .await;
    let posted = b.post("design", "squares", &[], None).await.unwrap();
    let ws_a2 = Arc::clone(&ws_a);
    wait_for("Bob's post on A", move || {
        ws_a2
            .messages("design", None, 10)
            .map(|m| m.iter().any(|r| r.content.contains("squares")))
            .unwrap_or(false)
    })
    .await;
    let c4 = Arc::clone(&c);
    wait_for("Bob's post relayed to Carol", move || {
        c4.messages("design", None, 10)
            .map(|m| m.iter().any(|x| x.id == posted.id))
            .unwrap_or(false)
    })
    .await;

    // --- outside reach: refused on Bob's node, and refused at A's door when forced ---
    assert!(b.post("general", "sneaky", &[], None).await.is_err());
    let forced = EventBuilder::new(
        Kind::Custom(KIND_MESSAGE),
        serde_json::to_string(&MessageBody::post("forced")).unwrap(),
    )
    .tags([Tag::parse([
        "a",
        &format!("33405:{}:general", ws_a.owner_principal().as_hex()),
    ])
    .unwrap()])
    .finalize(&bob.keys)
    .unwrap();
    let wrap = wrap_for_member(&bob.keys, ws_a.owner_keys().public_key(), &forced).unwrap();
    bob.relays.publish(&wrap).await.unwrap();
    // A goal snapshot from a person is not a human's act either.
    tokio::time::sleep(Duration::from_millis(800)).await;
    host.catch_up_now().await;
    assert!(
        !ws_a
            .messages("general", None, 10)
            .unwrap()
            .iter()
            .any(|r| r.content.contains("forced")),
        "a guest's fact outside its roster is refused at ingest"
    );

    // --- a direct message reaches its audience alone ---
    let dm = ws_a.open_dm(std::slice::from_ref(&c_id)).unwrap();
    let c5 = Arc::clone(&c);
    wait_for("the DM on Carol", move || {
        c5.dms().map(|d| d.len() == 1).unwrap_or(false)
    })
    .await;
    post(&ws_a, dm.id.as_str(), "for carol");
    let (c6, dm_id) = (Arc::clone(&c), dm.id.to_string());
    wait_for("the DM's message on Carol", move || {
        c6.messages(&dm_id, None, 10)
            .map(|m| m.iter().any(|x| x.text == "for carol"))
            .unwrap_or(false)
    })
    .await;
    assert!(
        b.dms().unwrap().is_empty(),
        "Bob is not in that conversation"
    );
    let ledger = std::fs::read_to_string(dir_a.path().join("net_published.jsonl")).unwrap();
    assert!(
        !ledger
            .lines()
            .any(|l| l.starts_with(b_id.as_hex()) && l.contains(&before_anyone)),
        "the general backlog was never wrapped to the guest: {ledger}"
    );

    // --- a promotion widens reach and brings the backlog ---
    ws_a.set_role(&b_id, MemberRole::Member).unwrap();
    let b3 = Arc::clone(&b);
    wait_for("Bob promoted", move || {
        b3.hosted()
            .ok()
            .flatten()
            .map(|h| h.role == MemberRole::Member)
            .unwrap_or(false)
    })
    .await;
    let b4 = Arc::clone(&b);
    wait_for("general on Bob after the promotion", move || {
        b4.messages("general", None, 10)
            .map(|m| m.iter().any(|x| x.text == "hello general"))
            .unwrap_or(false)
    })
    .await;

    // --- a removal is said to the one removed, and the directory to the rest ---
    ws_a.unroster_human_everywhere(&c_id).unwrap();
    ws_a.remove_member(&c_id).unwrap();
    let c7 = Arc::clone(&c);
    wait_for("Carol removed", move || {
        matches!(
            c7.hosted().ok().flatten().map(|h| h.state),
            Some(HostedState::Removed { .. })
        )
    })
    .await;
    let b5 = Arc::clone(&b);
    wait_for("Bob's directory without Carol", move || {
        b5.members().map(|m| m.len() == 2).unwrap_or(false)
    })
    .await;

    // --- a spent code is refused with the reason ---
    let dave = start_guest(&url).await;
    let (_, state) = dave.guests.join(&code_b, None, "cli").await.unwrap();
    assert!(
        matches!(&state, HostedState::Refused { reason } if reason.contains("unknown or was already used")),
        "{state:?}"
    );

    let status = host.status().await;
    assert_eq!(status.connected_relays, 1);
    assert!(status.published > 0);
    assert!(status.ingested >= 1);
    assert_eq!(
        status.people, 1,
        "Bob alone is hosted here: the owner is no person, Carol was removed, Dave was refused"
    );

    host.stop().await;
    dave.relays.stop().await;
    carol.relays.stop().await;
    bob.relays.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn ask_mode_holds_a_claim_for_the_owner_and_a_refusal_reaches_the_joiner() {
    // One second is as long as a fixture needs to wait for the owner: the
    // property is that the join holds until the word comes, not for how long.
    std::env::set_var("BISA_JOIN_WAIT_SECS", "1");
    let (_relay, url) = start_relay().await;
    let dir_a = tempfile::tempdir().unwrap();
    let ws_a = open_ws(dir_a.path());
    ws_a.set_setting(
        bisa_core::settings::Scope::Workspace,
        None,
        "collab.join",
        serde_json::Value::String("ask".into()),
    )
    .unwrap();
    let (_relays_a, host) = start_host(&ws_a, dir_a.path(), &url).await;

    let bob = start_guest(&url).await;
    let code = invite(&ws_a, &url, MemberRole::Member, &[]);
    let (b, state) = bob
        .guests
        .join(&code, Some("Bob".into()), "desktop")
        .await
        .unwrap();
    assert_eq!(
        state,
        HostedState::Requested,
        "the host asked to admit by hand"
    );
    let waiting = ws_a.invites().unwrap();
    let id = waiting[0].id;
    assert!(
        matches!(&waiting[0].state, bisa_core::InviteState::Requested { by, label, .. } if *by == pid(bob.keys.public_key()) && label.as_deref() == Some("Bob"))
    );
    assert!(ws_a
        .member_role(&pid(bob.keys.public_key()))
        .unwrap()
        .is_none());

    // The owner admits: the engine's door does these two store calls.
    let settled = ws_a.settle_invite(id, true).unwrap();
    ws_a.admit_claimed(
        &settled,
        &pid(bob.keys.public_key()),
        Some("Bob".into()),
        Some("desktop".into()),
    )
    .unwrap();
    let b2 = Arc::clone(&b);
    wait_for("Bob welcomed after the owner's word", move || {
        b2.hosted()
            .ok()
            .flatten()
            .map(|h| h.state.is_member())
            .unwrap_or(false)
    })
    .await;

    // A second person is turned down.
    let eve = start_guest(&url).await;
    let code = invite(&ws_a, &url, MemberRole::Guest, &[]);
    let (e, state) = eve.guests.join(&code, None, "cli").await.unwrap();
    assert_eq!(state, HostedState::Requested);
    let id = ws_a
        .invites()
        .unwrap()
        .into_iter()
        .find(|i| matches!(i.state, bisa_core::InviteState::Requested { .. }))
        .unwrap()
        .id;
    ws_a.settle_invite(id, false).unwrap();
    let e2 = Arc::clone(&e);
    wait_for("Eve refused", move || {
        matches!(
            e2.hosted().ok().flatten().map(|h| h.state),
            Some(HostedState::Refused { .. })
        )
    })
    .await;
    assert!(ws_a
        .member_role(&pid(eve.keys.public_key()))
        .unwrap()
        .is_none());

    host.stop().await;
    eve.relays.stop().await;
    bob.relays.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_person_who_leaves_is_gone_and_a_relay_change_is_live() {
    std::env::set_var("BISA_JOIN_WAIT_SECS", "10");
    let (_relay, url) = start_relay().await;
    let dir_a = tempfile::tempdir().unwrap();
    let ws_a = open_ws(dir_a.path());
    let (relays_a, host) = start_host(&ws_a, dir_a.path(), &url).await;
    let bob = start_guest(&url).await;
    let code = invite(&ws_a, &url, MemberRole::Member, &[]);
    let (_b, state) = bob.guests.join(&code, None, "cli").await.unwrap();
    assert_eq!(state, HostedState::Member);
    let b_id = pid(bob.keys.public_key());
    bob.guests
        .leave(bob.guests.list().await[0].host.pubkey.as_hex())
        .await
        .unwrap();
    let ws_a2 = Arc::clone(&ws_a);
    wait_for("Bob gone from A", move || {
        ws_a2
            .member_role(&b_id)
            .map(|r| r.is_none())
            .unwrap_or(false)
    })
    .await;

    let (_relay2, url2) = start_relay().await;
    host.apply(&NetConfig {
        relays: vec![url2.clone()],
        sync_interval_secs: 1,
        iroh: false,
        ..NetConfig::default()
    })
    .await;
    assert_eq!(relays_a.urls().await, vec![url2.clone()]);
    let status = host.status().await;
    assert_eq!(status.relays.len(), 1);
    assert_eq!(status.relays[0].url, url2);
    assert!(status.relays[0].connected);

    host.stop().await;
    bob.relays.stop().await;
}
