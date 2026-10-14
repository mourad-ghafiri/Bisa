//! The classifier's hold and release on a message from outside (14 —
//! Collaboration, C5): a *safe* verdict lets the agents hear it, a *harmful*
//! one holds it for the owner, and a person removed while the verdict was
//! pending is heard by nobody — neither when the verdict lands nor when the
//! owner releases what they left behind.

use crate::common;
use bisa_collab::hash_secret;
use bisa_core::{
    events_for_enablement, AgentId, InviteState, Member, MemberRole, MessageBody, PrincipalId,
    RosterPolicy, Tags,
};
use bisa_engine::collab::{
    create_invite, on_remote_message, scope_leaves_the_machine, settle_invite,
};
use bisa_engine::{Engine, EngineConfig, EnginePayload};
use bisa_harness::mock::MockAdapter;
use bisa_harness::{HarnessCatalog, LifecycleEvent, Outcome, ProgressEvent, SessionEvent};
use bisa_store::{Admission, HeldReason, MemoryKeyStore, NewAgent, PostOrigin, Workspace};
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
    /// Their node's workspace: what they sign.
    theirs: Workspace,
    engine: Engine,
    /// What the core agent was told, when it was woken.
    prompts: Arc<std::sync::Mutex<Vec<String>>>,
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
    rig_with(verdict, true, false, None)
}

/// The rig with its knobs: the classifier on or off for messages from
/// outside, the core agent on the shared channel's roster or not, and the
/// person's label as this workspace keeps it.
fn rig_with(verdict: &str, classify: bool, heard_by_core: bool, label: Option<&str>) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    let mut core = ws.get_agent(&AgentId::general()).unwrap();
    core.harness = "mock-classifier".into();
    if heard_by_core {
        core.respond = bisa_core::RespondPolicy::Members;
    }
    // Their post names this workspace's core agent when it is to be heard
    // by it: a mention wakes it directly, as a person addressing it would.
    let mentions: Vec<PrincipalId> = if heard_by_core {
        vec![core.pubkey.clone()]
    } else {
        vec![]
    };
    ws.update_agent(core).unwrap();
    let channel = ws
        .create_channel(
            "shared",
            None,
            RosterPolicy::Listed {
                agents: if heard_by_core {
                    vec![AgentId::general()]
                } else {
                    vec![]
                },
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
    ws.add_member(
        member.clone(),
        MemberRole::Member,
        Admission {
            label: label.map(str::to_string),
            ..Default::default()
        },
    )
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
        &mentions,
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
    let adapter = saying("mock-classifier", verdict);
    let prompts = Arc::clone(&adapter.prompts);
    catalog.register(Arc::new(adapter));
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
            json!(classify),
        )
        .unwrap();
    Rig {
        _dir: dir,
        _theirs: theirs,
        theirs: ws2,
        engine,
        prompts,
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

/// Off (`security.collaboration.classify`, the default), a message from
/// outside is heard at once: nothing is held, and the core agent reads it
/// with the person named by their label — or by the first letters of their
/// key when this workspace keeps no label for them.
#[tokio::test(flavor = "multi_thread")]
async fn with_the_classifier_off_for_outsiders_a_message_is_heard_at_once_under_the_person_s_name()
{
    for label in [Some("Bob"), None] {
        let r = rig_with("ok", false, true, label);
        let named = match label {
            Some(l) => format!("{l}: hello from outside"),
            None => format!("{}…: hello from outside", &r.member.as_hex()[..8]),
        };
        let inner = Arc::clone(r.engine.inner());
        let mut rx = r.engine.events();
        // The pump applies the fact to the store before the engine hears of it.
        assert!(matches!(
            r.engine.workspace().ingest_remote_event(&r.event).unwrap(),
            bisa_store::IngestOutcome::AppliedConversation { .. }
        ));
        on_remote_message(
            &inner,
            r.scope.clone(),
            r.event.clone(),
            r.member.clone(),
            MemberRole::Member,
        )
        .await;
        assert!(r.engine.workspace().held_messages().unwrap().is_empty());
        let mut heard = false;
        for _ in 0..100 {
            if r.prompts.lock().unwrap().iter().any(|p| p.contains(&named)) {
                heard = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert!(
            heard,
            "the core agent reads {named:?}: {:?}",
            r.prompts.lock().unwrap()
        );
        while let Ok(ev) = rx.try_recv() {
            assert!(
                !matches!(ev.payload, EnginePayload::MessageHeld { .. }),
                "nothing was held"
            );
        }
        r.engine.shutdown().await;
    }
}

/// With the classifier itself off while messages from outside are to be
/// read, there is no verdict to be had: the message stays held, with that
/// as its reason, for the owner.
#[tokio::test(flavor = "multi_thread")]
async fn with_no_verdict_to_be_had_the_message_stays_held_with_the_reason() {
    let r = rig("SAFE");
    r.engine
        .set_setting(
            bisa_core::SettingScope::Workspace,
            None,
            "security.classifier.enabled",
            json!(false),
        )
        .unwrap();
    let inner = Arc::clone(r.engine.inner());
    on_remote_message(
        &inner,
        r.scope.clone(),
        r.event.clone(),
        r.member.clone(),
        MemberRole::Member,
    )
    .await;
    let held = r.engine.workspace().held_messages().unwrap();
    assert_eq!(held.len(), 1);
    assert!(
        matches!(&held[0].reason, HeldReason::NoVerdict { why } if why.contains("classifier is off")),
        "{:?}",
        held[0].reason
    );
    r.engine.shutdown().await;
}

/// A membership note from their node — a message whose body is no post —
/// is read as no words at all, judged and released like any other.
#[tokio::test(flavor = "multi_thread")]
async fn a_membership_note_from_outside_is_read_as_no_words() {
    let r = rig("SAFE");
    let note = events_for_enablement(
        Member::Agent(AgentId::general()),
        true,
        false,
        &r.theirs.list_channels().unwrap(),
        1,
    )
    .into_iter()
    .find(|e| e.channel.as_str() == "general")
    .expect("the general channel's note");
    let id = r.theirs.post_membership(note).unwrap();
    let event: nostr::event::Event =
        std::fs::read_to_string(r.theirs.paths().conversation_log("general").unwrap())
            .unwrap()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str::<nostr::event::Event>(l).unwrap())
            .find(|e| e.id.to_hex() == id)
            .unwrap();
    let inner = Arc::clone(r.engine.inner());
    on_remote_message(
        &inner,
        "general".into(),
        event,
        r.member.clone(),
        MemberRole::Member,
    )
    .await;
    assert!(r.engine.workspace().held_messages().unwrap().is_empty());
    r.engine.shutdown().await;
}

/// A post leaves the machine only into a channel a person on another node
/// reaches: a scope that is no channel reaches nobody.
#[tokio::test(flavor = "multi_thread")]
async fn a_post_leaves_the_machine_only_into_a_channel_a_person_reaches() {
    let r = rig("SAFE");
    let inner = Arc::clone(r.engine.inner());
    assert!(scope_leaves_the_machine(&inner, "general"));
    assert!(scope_leaves_the_machine(&inner, &r.scope));
    assert!(!scope_leaves_the_machine(&inner, "not-a-channel"));
    r.engine.shutdown().await;
}

/// The owner's word on a claim that waited: admitted at the invite's role
/// with the label the person gave, or refused and never a member — and once
/// settled, an invite waits on nobody.
#[tokio::test(flavor = "multi_thread")]
async fn the_owner_s_word_on_a_claim_that_waited_admits_or_refuses() {
    let r = rig("SAFE");
    let inner = Arc::clone(r.engine.inner());
    let ws = r.engine.workspace();
    let (invite, code) = create_invite(&inner, MemberRole::Member, vec![], None).unwrap();
    let dana = PrincipalId::new(nostr::key::Keys::generate().public_key().to_hex()).unwrap();
    let claimed = ws
        .claim_invite(&hash_secret(&code.secret), &dana, Some("Dana".into()), true)
        .unwrap();
    assert!(claimed.waits);
    let settled = settle_invite(&inner, invite.id, true).unwrap();
    assert!(matches!(settled.state, InviteState::Accepted { ref by, .. } if *by == dana));
    assert_eq!(ws.member_role(&dana).unwrap(), Some(MemberRole::Member));
    assert_eq!(
        ws.member(&dana).unwrap().unwrap().label.as_deref(),
        Some("Dana")
    );

    let shared = vec![bisa_core::ChannelId::new(&r.scope).unwrap()];
    let (invite, code) = create_invite(&inner, MemberRole::Guest, shared, None).unwrap();
    let eve = PrincipalId::new(nostr::key::Keys::generate().public_key().to_hex()).unwrap();
    ws.claim_invite(&hash_secret(&code.secret), &eve, None, true)
        .unwrap();
    let refused = settle_invite(&inner, invite.id, false).unwrap();
    assert!(matches!(refused.state, InviteState::Refused { ref by, .. } if *by == eve));
    assert_eq!(ws.member_role(&eve).unwrap(), None);
    let again = settle_invite(&inner, invite.id, false).unwrap_err();
    assert!(again.to_string().contains("not waiting on you"), "{again}");
    r.engine.shutdown().await;
}

/// Flipping an agent's or a team's enablement is announced once in each
/// channel it belongs to; an unchanged flag says nothing.
#[tokio::test(flavor = "multi_thread")]
async fn flipping_an_agent_or_a_team_is_announced_in_its_channels_once() {
    let r = rig("SAFE");
    let ws = r.engine.workspace();
    let scout = ws
        .add_agent(NewAgent {
            name: "Scout".into(),
            photo: None,
            description: None,
            system_prompt: "You are Scout.".into(),
            harness: "mock-classifier".into(),
            models: Default::default(),
            skills: vec![],
            mcps: vec![],
            tags: Default::default(),
            respond: Default::default(),
            decision_making: false,
        })
        .unwrap();
    let said = || ws.messages("general", None, 50).unwrap().len();
    let before = said();
    let agent = r.engine.set_agent_enabled(&scout.id, false).unwrap();
    assert!(!agent.enabled);
    assert_eq!(said(), before + 1, "one note in general");
    let same = r.engine.set_agent_enabled(&scout.id, false).unwrap();
    assert!(!same.enabled);
    assert_eq!(said(), before + 1, "unchanged: nothing said");
    let team = ws
        .create_team("crew", None, vec![], Tags::default())
        .unwrap();
    let team = r.engine.set_team_enabled(&team.id, false).unwrap();
    assert!(!team.enabled);
    r.engine.shutdown().await;
}
