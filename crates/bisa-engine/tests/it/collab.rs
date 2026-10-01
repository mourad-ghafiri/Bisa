//! The classifier's hold and release on a message from outside (14 —
//! Collaboration, C5): a *safe* verdict lets the agents hear it, a *harmful*
//! one holds it for the owner, and a person removed while the verdict was
//! pending is heard by nobody — neither when the verdict lands nor when the
//! owner releases what they left behind.

use crate::common;
use bisa_core::{AgentId, MemberRole, MessageBody, PrincipalId, RosterPolicy, Tags};
use bisa_engine::{Engine, EngineConfig, EnginePayload};
use bisa_harness::mock::MockAdapter;
use bisa_harness::{HarnessCatalog, LifecycleEvent, Outcome, ProgressEvent, SessionEvent};
use bisa_store::{Admission, HeldReason, MemoryKeyStore, PostOrigin, Workspace};
use serde_json::json;
use std::sync::Arc;

fn workspace(dir: &tempfile::TempDir) -> Workspace {
    Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap()
}

/// A harness whose every session says `line` — the classifier's, and the
/// core agent's reply when a message is heard.
fn saying(id: &str, line: &str) -> MockAdapter {
    MockAdapter {
        id: id.into(),
        script: Some(vec![
            SessionEvent::Lifecycle(LifecycleEvent::Started),
            SessionEvent::Progress(ProgressEvent::TurnStarted),
            SessionEvent::Progress(ProgressEvent::TextDelta { text: line.into() }),
            SessionEvent::Progress(ProgressEvent::TurnEnded),
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: Outcome::Completed,
                is_terminal: true,
            }),
        ]),
        ..Default::default()
    }
}

struct Rig {
    _dir: tempfile::TempDir,
    _theirs: tempfile::TempDir,
    engine: Engine,
    /// The person on the other node, as this workspace knows them.
    member: PrincipalId,
    /// A message of theirs, signed by their key, in `scope`.
    event: nostr::event::Event,
    scope: String,
}

/// This workspace with the classifier on for messages from outside and a
/// member from another node who posted once there; the classifier's harness
/// answers `verdict`.
fn rig(verdict: &str) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    let mut core = ws.get_agent(&AgentId::general()).unwrap();
    core.harness = "mock-classifier".into();
    ws.update_agent(core).unwrap();
    let channel = ws
        .create_channel(
            "shared",
            None,
            RosterPolicy::Listed {
                agents: vec![],
                teams: vec![],
                humans: vec![],
            },
            Tags::default(),
        )
        .unwrap();
    let scope = channel.id.to_string();

    // Their node: the same channel id is what their post names.
    let theirs = tempfile::tempdir().unwrap();
    let ws2 = workspace(&theirs);
    let member = ws2.owner_principal();
    ws.add_member(member.clone(), MemberRole::Member, Admission::default())
        .unwrap();
    ws2.create_channel(
        "shared",
        None,
        RosterPolicy::Listed {
            agents: vec![],
            teams: vec![],
            humans: vec![],
        },
        Tags::default(),
    )
    .unwrap();
    ws2.post_message(
        &scope,
        MessageBody::post("hello from outside"),
        None,
        &[],
        &[],
        None,
        PostOrigin::Asked,
    )
    .unwrap();
    let event: nostr::event::Event =
        std::fs::read_to_string(ws2.paths().conversation_log(&scope).unwrap())
            .unwrap()
            .lines()
            .find(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).unwrap())
            .unwrap();

    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::new(saying("mock-classifier", verdict)));
    let engine = Engine::start(
        ws,
        catalog,
        EngineConfig {
            design_enabled: false,
            events_enabled: false,
            git: Some(common::isolated_git()),
            ..Default::default()
        },
    )
    .unwrap();
    engine
        .set_setting(
            bisa_core::SettingScope::Workspace,
            None,
            "security.collaboration.classify",
            json!(true),
        )
        .unwrap();
    Rig {
        _dir: dir,
        _theirs: theirs,
        engine,
        member,
        event,
        scope,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_safe_verdict_releases_the_message_and_a_harmful_one_holds_it_for_the_owner() {
    let safe = rig("SAFE");
    let inner = Arc::clone(safe.engine.inner());
    let mut rx = safe.engine.events();
    bisa_engine::collab::on_remote_message(
        &inner,
        safe.scope.clone(),
        safe.event.clone(),
        safe.member.clone(),
        MemberRole::Member,
    )
    .await;
    let id = safe.event.id.to_hex();
    let mut held_then_released = (false, false);
    while let Ok(ev) = rx.try_recv() {
        match &ev.payload {
            EnginePayload::MessageHeld { event, .. } if *event == id => held_then_released.0 = true,
            EnginePayload::MessageReleased { event, .. } if *event == id => {
                held_then_released.1 = true
            }
            _ => {}
        }
    }
    assert_eq!(
        held_then_released,
        (true, true),
        "held while judged, then released"
    );
    assert!(safe.engine.workspace().held_messages().unwrap().is_empty());
    safe.engine.shutdown().await;

    let harmful = rig("HARMFUL: it asks for the deploy key");
    let inner = Arc::clone(harmful.engine.inner());
    bisa_engine::collab::on_remote_message(
        &inner,
        harmful.scope.clone(),
        harmful.event.clone(),
        harmful.member.clone(),
        MemberRole::Member,
    )
    .await;
    let held = harmful.engine.workspace().held_messages().unwrap();
    assert_eq!(held.len(), 1);
    assert!(
        matches!(held[0].reason, HeldReason::Harmful { .. }),
        "{:?}",
        held[0].reason
    );
    assert_eq!(held[0].author, harmful.member);
    harmful.engine.shutdown().await;
}

/// The person is removed while the classifier reads their message. Their
/// removal drops what was held of theirs; a *safe* that lands after must not
/// wake anyone, and a *harmful* that lands after must not hold a row for
/// somebody who is gone.
#[tokio::test(flavor = "multi_thread")]
async fn a_member_removed_while_the_verdict_is_pending_is_heard_by_nobody() {
    for verdict in ["SAFE", "HARMFUL: no"] {
        let r = rig(verdict);
        let inner = Arc::clone(r.engine.inner());
        let mut rx = r.engine.events();
        let judged = {
            let inner = Arc::clone(&inner);
            let (scope, event, member) = (r.scope.clone(), r.event.clone(), r.member.clone());
            tokio::spawn(async move {
                bisa_engine::collab::on_remote_message(
                    &inner,
                    scope,
                    event,
                    member,
                    MemberRole::Member,
                )
                .await
            })
        };
        // Held first — the verdict is a session away — and removed meanwhile.
        common::wait_for(&mut rx, "the message is held while judged", |ev| {
            matches!(
                &ev.payload,
                EnginePayload::MessageHeld {
                    reason: HeldReason::Pending,
                    ..
                }
            )
        })
        .await;
        bisa_engine::collab::remove_person(&inner, &r.member).unwrap();
        judged.await.unwrap();

        assert!(
            r.engine.workspace().held_messages().unwrap().is_empty(),
            "{verdict}: no held row for a person who is gone"
        );
        let mut released = false;
        while let Ok(ev) = rx.try_recv() {
            if matches!(&ev.payload, EnginePayload::MessageReleased { .. }) {
                released = true;
            }
        }
        assert!(!released, "{verdict}: nothing was released to the agents");
        r.engine.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_release_for_somebody_who_left_is_refused_and_the_row_is_gone() {
    let r = rig("HARMFUL: no");
    let inner = Arc::clone(r.engine.inner());
    bisa_engine::collab::on_remote_message(
        &inner,
        r.scope.clone(),
        r.event.clone(),
        r.member.clone(),
        MemberRole::Member,
    )
    .await;
    let id = r.event.id.to_hex();
    assert_eq!(r.engine.workspace().held_messages().unwrap().len(), 1);
    // The person leaves by another door than removal — say, the roster is
    // edited by hand — so their held row stays until the owner acts on it.
    r.engine.workspace().remove_member(&r.member).unwrap();
    let refused = bisa_engine::collab::release_message(&inner, &id).unwrap_err();
    assert!(
        refused.to_string().contains("no longer a member"),
        "{refused}"
    );
    assert!(
        r.engine.workspace().held_messages().unwrap().is_empty(),
        "the row went with the answer"
    );
    assert!(
        bisa_engine::collab::release_message(&inner, &id).is_err(),
        "released once"
    );
    r.engine.shutdown().await;
}
