//! Store integration: agents, teams, recall, conversations, ingest, rebuild.

use bisa_core::event::JournalPayload;
use bisa_core::{
    AgentId, AgentOrigin, Assignee, ChannelId, ChannelKind, DeletableChannel, Gate, Home,
    MemberRole, MessageBody, Origin, PrincipalId, RosterPolicy, SkillId, Tags, TeamId,
};
use bisa_store::{
    identity, Admission, CatalogKind, FileKeyStore, GatePolicy, IngestOutcome, MemoryKeyStore,
    NewAgent, NewGoal, NewSkill, Paths, PostOrigin, StoreError, Workspace,
};
use nostr::key::Keys;

fn ws() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    (dir, ws)
}

fn file_ws(dir: &tempfile::TempDir) -> Workspace {
    Workspace::open_with_keystore(
        dir.path(),
        Box::new(FileKeyStore::new(Paths::new(dir.path()).identity_dir())),
    )
    .unwrap()
}

fn new_agent(name: &str) -> NewAgent {
    NewAgent {
        name: name.into(),
        system_prompt: format!("You are {name}."),
        harness: "claude-code".into(),
        ..Default::default()
    }
}

fn skill(id: &str) -> SkillId {
    SkillId::new(id).unwrap()
}

fn team_id(id: &str) -> TeamId {
    TeamId::new(id).unwrap()
}

fn agent_id(id: &str) -> AgentId {
    AgentId::new(id).unwrap()
}

// ---------------------------------------------------------------------------
// Agents
// ---------------------------------------------------------------------------

/// The name's slug is the id an agent is minted under, and this name's slug
/// is the reserved id. No file ever holds that id, so the mint finds it free
/// and hands it over with no suffix — and the definition's own rule refuses
/// it, before anything is written.
#[test]
fn an_agent_named_as_the_decision_making_agent_is_refused_for_its_slug_is_reserved() {
    let (_d, ws) = ws();
    let before = ws.list_agents().unwrap().len();
    let refused = ws.add_agent(new_agent(bisa_core::DECISION_MAKING_AGENT_NAME));
    assert!(
        matches!(
            refused,
            Err(StoreError::Agent(
                bisa_core::AgentError::DecisionMakingAgentIdReserved
            ))
        ),
        "{refused:?}"
    );
    assert_eq!(ws.list_agents().unwrap().len(), before, "nothing was made");
    assert!(matches!(
        ws.get_agent(&AgentId::decision_making()),
        Err(StoreError::DefinitionNotFound { .. })
    ));
    // The refusal is the id's, not the name's: said another way, the name
    // is anybody's.
    let judge = ws.add_agent(new_agent("Decision Making")).unwrap();
    assert_eq!(judge.id.as_str(), "decision-making");
}

#[test]
fn agent_crud_keys_and_signing() {
    let (_d, ws) = ws();
    let def = ws.add_agent(new_agent("Tester")).unwrap();
    assert_eq!(def.id.as_str(), "tester", "the id is the name's slug");
    assert_eq!(def.origin, AgentOrigin::Local);
    assert!(def.enabled);
    assert_eq!(
        ws.list_agents().unwrap().len(),
        3,
        "yours and the General Agent and the Workflow Agent"
    );
    assert_eq!(ws.get_agent(&def.id).unwrap().name, "Tester");

    let (keys, attestation) = ws.signer_for(&def.id).unwrap();
    assert_eq!(keys.public_key().to_hex(), def.pubkey.as_hex());
    let goal = ws
        .create_goal(NewGoal::captured("agent-signed work"))
        .unwrap();
    ws.append_journal(
        &Home::Goal { goal: goal.id },
        JournalPayload::Note {
            text: "from the agent".into(),
        },
        &keys,
        attestation,
    )
    .unwrap();
    let note = ws
        .journal(&Home::Goal { goal: goal.id })
        .unwrap()
        .into_iter()
        .find(|je| je.author.as_hex() == def.pubkey.as_hex())
        .expect("agent-authored entry");
    assert!(matches!(&note.payload, JournalPayload::Note { text } if text == "from the agent"));

    let raw = std::fs::read_to_string(ws.paths().goal(goal.id).journal()).unwrap();
    let agent_event: nostr::event::Event = raw
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .find(|e: &nostr::event::Event| e.pubkey.to_hex() == def.pubkey.as_hex())
        .unwrap();
    assert_eq!(
        identity::verify_attestation(&agent_event),
        Some(ws.owner_principal().as_hex().to_string())
    );

    let mut edited = def.clone();
    edited.name = "Edited".into();
    let edited = ws.update_agent(edited).unwrap();
    assert_eq!(edited.pubkey, def.pubkey);
    ws.create_skill(NewSkill {
        id: skill("house-style"),
        name: "House style".into(),
        description: "Use this when writing anything a human reads.".into(),
        tags: Tags::new(["writing"]).unwrap(),
        markdown: "# House style\n\nBe terse.".into(),
    })
    .unwrap();
    ws.attach_skill(&def.id, &skill("house-style")).unwrap();
    let got = ws.get_agent(&def.id).unwrap();
    assert_eq!(got.skills, vec![skill("house-style")]);
    let payloads = ws.skill_payloads(&got.id, &got.skills);
    assert_eq!(payloads.len(), 1);
    assert_eq!(
        payloads[0].description,
        "Use this when writing anything a human reads."
    );
    ws.attach_skill(&def.id, &skill("house-style")).unwrap();
    assert_eq!(ws.get_agent(&def.id).unwrap().skills.len(), 1);
    ws.detach_skill(&def.id, &skill("house-style")).unwrap();
    assert!(ws.get_agent(&def.id).unwrap().skills.is_empty());

    ws.remove_agent(&def.id).unwrap();
    let left = ws.list_agents().unwrap();
    assert_eq!(left.len(), 2);
    assert!(left.iter().all(|a| a.id.is_core_id()));
    assert!(ws.signer_for(&def.id).is_err());
}

#[test]
fn two_agents_with_one_name_get_two_ids() {
    let (_d, ws) = ws();
    let a = ws.add_agent(new_agent("Scout")).unwrap();
    let b = ws.add_agent(new_agent("Scout")).unwrap();
    assert_ne!(a.id, b.id);
    assert!(b.id.as_str().starts_with("scout-"));
}

#[test]
fn installed_definitions_are_editable_and_survive_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let ws1 = file_ws(&dir);
    let installed = ws1.install(CatalogKind::Team, "engineering").unwrap();
    assert_eq!(installed.teams, vec!["engineering".to_string()]);
    assert!(installed.agents.contains(&"developer".to_string()));
    for agent in ws1.list_agents().unwrap() {
        assert_eq!(
            ws1.skill_payloads(&agent.id, &agent.skills).len(),
            agent.skills.len(),
            "agent {} references a skill the install did not bring",
            agent.id,
        );
    }
    assert!(!ws1.team_agents(&team_id("engineering")).unwrap().is_empty());
    let dev = ws1.get_agent(&agent_id("developer")).unwrap();
    assert_eq!(dev.name, "Developer");
    assert_eq!(
        dev.origin,
        AgentOrigin::Catalog {
            slug: "developer".into()
        },
    );
    let mut edited = dev.clone();
    edited.system_prompt = "my custom developer".into();
    ws1.update_agent(edited).unwrap();
    let agent_count = ws1.list_agents().unwrap().len();
    drop(ws1);

    let ws2 = file_ws(&dir);
    assert_eq!(ws2.list_agents().unwrap().len(), agent_count);
    assert_eq!(
        ws2.get_agent(&agent_id("developer")).unwrap().system_prompt,
        "my custom developer",
    );
    ws2.signer_for(&agent_id("developer")).unwrap();
}

// ---------------------------------------------------------------------------
// Teams + governance
// ---------------------------------------------------------------------------

#[test]
fn team_crud_and_governance_expansion() {
    let (_d, ws) = ws();
    let human = PrincipalId::new(Keys::generate().public_key().to_hex()).unwrap();
    ws.add_member(
        human.clone(),
        MemberRole::Member,
        Admission {
            label: Some("ana".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let agent = ws.add_agent(new_agent("Worker")).unwrap();
    assert!(ws
        .create_team(
            "bad",
            None,
            vec![Assignee::Agent("nope".into())],
            Tags::default()
        )
        .is_err());
    let team = ws
        .create_team(
            "landing",
            Some("ship the landing page"),
            vec![
                Assignee::Human(human.clone()),
                Assignee::Agent(agent.id.to_string()),
            ],
            Tags::new(["product"]).unwrap(),
        )
        .unwrap();
    assert_eq!(ws.list_teams().unwrap().len(), 1);
    assert_eq!(team.origin, Origin::Local);
    assert!(team.enabled);
    assert_eq!(ws.team_humans(&team.id).unwrap(), vec![human.clone()]);
    assert_eq!(ws.team_agents(&team.id).unwrap(), vec![agent.id.clone()]);

    ws.set_gate_policy(
        Gate::Approval,
        GatePolicy::Listed(vec![format!("team:{}", team.id)]),
    )
    .unwrap();
    assert!(ws.gate_policy_allows(Gate::Approval, &human).unwrap());
    assert!(!ws
        .gate_policy_allows(Gate::Approval, &ws.owner_principal())
        .unwrap());
    assert!(!ws
        .gate_policy_allows(Gate::Approval, &agent.pubkey)
        .unwrap());

    // A disabled team expands to nobody but keeps its members.
    let (off, was) = ws.set_team_enabled(&team.id, false).unwrap();
    assert!(was && !off.enabled);
    assert!(ws
        .expand_assignees(vec![Assignee::Team(team.id.to_string())])
        .is_empty());
    assert_eq!(ws.get_team(&team.id).unwrap().members.len(), 2);
}

// ---------------------------------------------------------------------------
// Recall
// ---------------------------------------------------------------------------

#[test]
fn recall_roundtrip_encryption_and_conflicts() {
    let (_d, ws) = ws();
    let agent = ws.add_agent(new_agent("Mem")).unwrap();
    let h1 = ws
        .recall_store(&agent.id, "core", "I am Mem. See [[projects/alpha]].", None)
        .unwrap();
    ws.recall_store(&agent.id, "projects/alpha", "Alpha ships in May.", None)
        .unwrap();
    let core = ws.recall_get(&agent.id, "core").unwrap().unwrap();
    assert_eq!(core.hash, h1);
    assert_eq!(core.links, vec!["projects/alpha".to_string()]);

    let recall_dir = ws.paths().state_dir(&Paths::ns_recall(&agent.id));
    let mut found_file = false;
    for entry in std::fs::read_dir(&recall_dir).unwrap().flatten() {
        let content = std::fs::read_to_string(entry.path()).unwrap();
        assert!(!content.contains("Alpha ships"), "plaintext leaked");
        found_file = true;
        let ev: nostr::event::Event = serde_json::from_str(&content).unwrap();
        let stranger = Keys::generate();
        assert!(
            nostr::nips::nip44::decrypt(stranger.secret_key(), &ev.pubkey, &ev.content).is_err()
        );
    }
    assert!(found_file, "no recall files written");

    match ws.recall_store(&agent.id, "core", "overwrite", None) {
        Err(StoreError::RecallConflict {
            current_value,
            current_hash,
        }) => {
            assert!(current_value.contains("I am Mem"));
            assert_eq!(current_hash, h1);
        }
        other => panic!("expected conflict, got {other:?}"),
    }
    assert!(matches!(
        ws.recall_store(&agent.id, "core", "x", Some("deadbeef")),
        Err(StoreError::RecallConflict { .. })
    ));
    let h2 = ws
        .recall_store(
            &agent.id,
            "core",
            "I am Mem v2. [[projects/alpha]]",
            Some(&h1),
        )
        .unwrap();
    assert_ne!(h1, h2);
    let list = ws.recall_list(&agent.id).unwrap();
    assert_eq!(list.len(), 2);
    assert!(
        !list
            .iter()
            .find(|r| r.slug == "projects/alpha")
            .unwrap()
            .orphan
    );
    assert!(list.iter().find(|r| r.slug == "core").unwrap().orphan);
}

// ---------------------------------------------------------------------------
// Conversations
// ---------------------------------------------------------------------------

#[test]
fn channels_messages_reactions_and_the_channel_handle() {
    let (_d, ws) = ws();
    let agent = ws.add_agent(new_agent("Chatty")).unwrap();
    let channel = ws
        .create_channel(
            "product",
            Some("roadmap"),
            RosterPolicy::Listed {
                agents: vec![agent.id.clone()],
                teams: vec![],
                humans: vec![],
            },
            Tags::new(["product"]).unwrap(),
        )
        .unwrap();
    assert_eq!(channel.kind, ChannelKind::Standing);
    assert_eq!(
        ws.list_channels_of_kind(ChannelKind::Standing)
            .unwrap()
            .len(),
        2,
        "general + product"
    );

    let scope = channel.id.to_string();
    let m1 = ws
        .post_message(
            &scope,
            MessageBody::post("hello"),
            None,
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();
    let m2 = ws
        .post_message(
            &scope,
            MessageBody::post("hello from agent"),
            Some(m1.clone()),
            &ws.resolve_mentions(&scope, std::slice::from_ref(&scope))
                .unwrap(),
            &[],
            Some(&agent.id),
            PostOrigin::Asked,
        )
        .unwrap();
    let rows = ws.messages(&scope, None, 10).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].reply_to.as_deref(), Some(m1.as_str()));
    assert_eq!(rows[1].author, agent.pubkey.as_hex());
    // The handle expanded to the roster plus the general agent.
    let mentioned = ws.mentions_of(agent.pubkey.as_hex(), 10).unwrap();
    assert_eq!(mentioned.len(), 1);
    assert_eq!(
        ws.mentioned_scopes(agent.pubkey.as_hex()).unwrap(),
        vec![scope.clone()]
    );

    let r = ws.react(&m2, "👍", None).unwrap();
    assert_eq!(ws.list_reactions(&scope).unwrap().len(), 1);
    ws.retract(&r).unwrap();
    assert!(ws.list_reactions(&scope).unwrap().is_empty());
    assert!(ws.retract(&m2).is_err(), "only the author retracts");

    ws.delete_channel(DeletableChannel::new(channel).unwrap())
        .unwrap();
    assert!(ws.resolve_scope(&scope).is_err());
}

#[test]
fn open_dm_idempotent_and_unread_counts() {
    let (_d, ws) = ws();
    let other = PrincipalId::new(Keys::generate().public_key().to_hex()).unwrap();
    // A direct channel is between people of this workspace: the other side is a member first.
    ws.add_member(other.clone(), MemberRole::Member, Admission::default())
        .unwrap();
    let dm = ws.open_dm(std::slice::from_ref(&other)).unwrap();
    assert_eq!(
        ws.open_dm(&[other.clone(), ws.owner_principal()])
            .unwrap()
            .id,
        dm.id
    );
    let scope = dm.id.to_string();
    ws.post_message(
        &scope,
        MessageBody::post("psst"),
        None,
        &[],
        &[],
        None,
        PostOrigin::Asked,
    )
    .unwrap();
    // Own messages are never unread; a forced unread is.
    assert!(ws.unread_counts().unwrap().is_empty());
    ws.mark_unread(&scope).unwrap();
    assert_eq!(ws.unread_counts().unwrap(), vec![(scope.clone(), 1)]);
    ws.mark_read(&scope).unwrap();
    assert!(ws.unread_counts().unwrap().is_empty());
    assert_eq!(ws.read_markers().unwrap().len(), 1);
    assert_eq!(ws.scope_activity().unwrap().len(), 1);
}

fn raw_conversation_events(ws: &Workspace, scope: &str) -> Vec<nostr::event::Event> {
    std::fs::read_to_string(ws.paths().conversation_log(scope).unwrap())
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn raw_snapshot(ws: &Workspace, ns: &str, kind: u16, d: &str) -> nostr::event::Event {
    serde_json::from_slice(
        &std::fs::read(ws.paths().state_dir(ns).join(format!("{kind}-{d}.json"))).unwrap(),
    )
    .unwrap()
}

#[test]
fn cross_workspace_message_ingest() {
    let (_d1, ws1) = ws();
    let (_d2, ws2) = ws();
    // An admin may write the room's channels; a message is anyone's act.
    ws1.add_member(
        ws2.owner_principal(),
        MemberRole::Admin,
        Admission::default(),
    )
    .unwrap();
    let channel = ws2
        .create_channel("shared", None, RosterPolicy::default(), Tags::default())
        .unwrap();
    ws2.post_message(
        channel.id.as_str(),
        MessageBody::post("from ws2"),
        None,
        &[],
        &[],
        None,
        PostOrigin::Asked,
    )
    .unwrap();
    let events = raw_conversation_events(&ws2, channel.id.as_str());
    // Message before its channel: rejected, not seen, retried once the
    // channel snapshot arrives.
    assert!(matches!(
        ws1.ingest_remote_event(&events[0]).unwrap(),
        IngestOutcome::Rejected { .. }
    ));
    let snap = raw_snapshot(
        &ws2,
        Paths::NS_CHANNELS,
        bisa_core::kind::KIND_CHANNEL,
        channel.id.as_str(),
    );
    assert!(matches!(
        ws1.ingest_remote_event(&snap).unwrap(),
        IngestOutcome::AppliedConversation { .. }
    ));
    assert!(matches!(
        ws1.ingest_remote_event(&events[0]).unwrap(),
        IngestOutcome::AppliedConversation { .. }
    ));
    assert_eq!(
        ws1.messages(channel.id.as_str(), None, 10).unwrap().len(),
        1
    );
    assert_eq!(
        ws1.ingest_remote_event(&events[0]).unwrap(),
        IngestOutcome::Duplicate
    );
    // A guest reaches a standing channel only by being on its roster; a
    // member reaches every one; a channel definition is a manager's word.
    let (_d3, ws3) = ws();
    let guest = ws3.owner_principal();
    ws1.add_member(guest.clone(), MemberRole::Guest, Admission::default())
        .unwrap();
    // ws3 needs the channel to post in it; a channel definition is a
    // manager's word, and the snapshot is ws2's — so ws2's owner is the admin
    // ws3 takes it from. A stranger's definition would be refused, not applied.
    assert!(matches!(
        ws3.ingest_remote_event(&snap).unwrap(),
        IngestOutcome::Rejected { .. }
    ));
    ws3.add_member(
        ws2.owner_principal(),
        MemberRole::Admin,
        Admission::default(),
    )
    .unwrap();
    assert!(matches!(
        ws3.ingest_remote_event(&snap).unwrap(),
        IngestOutcome::AppliedConversation { .. }
    ));
    ws3.post_message(
        channel.id.as_str(),
        MessageBody::post("from a guest"),
        None,
        &[],
        &[],
        None,
        PostOrigin::Asked,
    )
    .unwrap();
    let guest_post = raw_conversation_events(&ws3, channel.id.as_str())
        .pop()
        .unwrap();
    match ws1.ingest_remote_event(&guest_post).unwrap() {
        IngestOutcome::Rejected { reason } => {
            assert!(reason.contains("does not reach"), "{reason}")
        }
        other => panic!("a guest off the roster does not reach the room: {other:?}"),
    }
    ws1.set_roster_humans(&channel.id, vec![guest.clone()])
        .unwrap();
    assert!(
        matches!(
            ws1.ingest_remote_event(&guest_post).unwrap(),
            IngestOutcome::AppliedConversation { .. }
        ),
        "rostered, the guest reaches it"
    );
    // A guest's channel definition is refused: not a manager's word.
    let guest_channel = ws3
        .create_channel("theirs", None, RosterPolicy::default(), Tags::default())
        .unwrap();
    let their_snap = raw_snapshot(
        &ws3,
        Paths::NS_CHANNELS,
        bisa_core::kind::KIND_CHANNEL,
        guest_channel.id.as_str(),
    );
    match ws1.ingest_remote_event(&their_snap).unwrap() {
        IngestOutcome::Rejected { reason } => assert!(reason.contains("human only"), "{reason}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn dm_audience_enforced_at_ingest_and_post() {
    let (_d1, ws1) = ws();
    let (_d2, ws2) = ws();
    let (_d3, ws3) = ws();
    ws1.add_member(
        ws2.owner_principal(),
        MemberRole::Member,
        Admission::default(),
    )
    .unwrap();
    ws1.add_member(
        ws3.owner_principal(),
        MemberRole::Member,
        Admission::default(),
    )
    .unwrap();
    // A DM between ws1 and ws2, opened on ws1.
    let dm = ws1.open_dm(&[ws2.owner_principal()]).unwrap();
    let snap = raw_snapshot(
        &ws1,
        Paths::NS_CHANNELS,
        bisa_core::kind::KIND_CHANNEL,
        dm.id.as_str(),
    );
    ws3.add_member(
        ws1.owner_principal(),
        MemberRole::Admin,
        Admission::default(),
    )
    .unwrap();
    ws3.ingest_remote_event(&snap).unwrap();
    // ws3 is not a participant: cannot post into it.
    assert!(ws3
        .post_message(
            dm.id.as_str(),
            MessageBody::post("intrude"),
            None,
            &[],
            &[],
            None,
            PostOrigin::Asked
        )
        .is_err());
    // A message from a participant is accepted by a participant's node.
    ws1.post_message(
        dm.id.as_str(),
        MessageBody::post("hi"),
        None,
        &[],
        &[],
        None,
        PostOrigin::Asked,
    )
    .unwrap();
    let ev = raw_conversation_events(&ws1, dm.id.as_str()).pop().unwrap();
    ws2.add_member(
        ws1.owner_principal(),
        MemberRole::Admin,
        Admission::default(),
    )
    .unwrap();
    ws2.ingest_remote_event(&snap).unwrap();
    assert!(matches!(
        ws2.ingest_remote_event(&ev).unwrap(),
        IngestOutcome::AppliedConversation { .. }
    ));
}

#[test]
fn rebuild_covers_the_conversation_tables() {
    let (_d, ws) = ws();
    let agent = ws.add_agent(new_agent("Chatty")).unwrap();
    let c = ws
        .create_channel(
            "room",
            None,
            RosterPolicy::Listed {
                agents: vec![agent.id.clone()],
                teams: vec![],
                humans: vec![],
            },
            Tags::default(),
        )
        .unwrap();
    let scope = c.id.to_string();
    let m = ws
        .post_message(
            &scope,
            MessageBody::post("hello"),
            None,
            std::slice::from_ref(&agent.pubkey),
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();
    ws.react(&m, "🎉", Some(&agent.id)).unwrap();
    ws.mark_read(&scope).unwrap();

    ws.rebuild_index().unwrap();
    assert_eq!(ws.get_channel(&c.id).unwrap(), c);
    assert_eq!(ws.messages(&scope, None, 10).unwrap().len(), 1);
    assert_eq!(ws.list_reactions(&scope).unwrap().len(), 1);
    assert_eq!(ws.mentions_of(agent.pubkey.as_hex(), 10).unwrap().len(), 1);
    assert!(
        ws.read_markers().unwrap().is_empty(),
        "read markers are local and lost by design"
    );
}

#[test]
fn a_read_marker_is_broadcast_locally_and_carries_no_event() {
    let (_d, ws) = ws();
    let mut rx = ws.subscribe_store_events();
    ws.mark_read("general").unwrap();
    let mut saw = false;
    while let Ok(ev) = rx.try_recv() {
        match ev {
            bisa_store::StoreEvent::ReadMarkerSet { scope } => {
                assert_eq!(scope, "general");
                saw = true;
            }
            other => panic!("a read marker produced {other:?}"),
        }
    }
    assert!(saw);
}

#[test]
fn a_disabled_agent_cannot_be_addressed() {
    let (_d, ws) = ws();
    let agent = ws.add_agent(new_agent("Quiet")).unwrap();
    ws.set_agent_enabled(&agent.id, false).unwrap();
    assert!(ws
        .resolve_mentions("general", &[agent.id.to_string()])
        .is_err());
    assert!(!ws
        .channel_roster_pubkeys(&ChannelId::general())
        .unwrap()
        .contains(&agent.pubkey));
}

/// The staff a goal's design may name: its own agents and teams, else its
/// nearest ancestor's that names any — never a wider parent's over a child
/// given its own — people left out, and nothing when no goal on the chain
/// names an agent or a team.
#[test]
fn a_goals_staff_scope_is_its_own_else_its_nearest_ancestors() {
    let (_d, ws) = ws();
    let dev = ws.add_agent(new_agent("Developer")).unwrap().id.to_string();
    let qa = ws.add_agent(new_agent("QA")).unwrap().id.to_string();
    let person = PrincipalId::new(Keys::generate().public_key().to_hex()).unwrap();
    let team = ws
        .create_team(
            "mobile",
            None,
            vec![Assignee::Agent(dev.clone())],
            Tags::default(),
        )
        .unwrap();
    let spawned = |parent: bisa_core::GoalId, s: &str| NewGoal {
        origin: bisa_core::GoalOrigin::Spawned { parent },
        ..NewGoal::captured(s)
    };

    let unscoped = ws.create_goal(NewGoal::captured("anyone")).unwrap();
    assert!(
        ws.staff_scope(unscoped.id).is_empty(),
        "nobody named: the whole staff"
    );
    ws.set_goal_assignees(unscoped.id, vec![Assignee::Human(person.clone())])
        .unwrap();
    assert!(
        ws.staff_scope(unscoped.id).is_empty(),
        "a person carries no step"
    );

    let parent = ws.create_goal(NewGoal::captured("the app")).unwrap();
    ws.set_goal_assignees(
        parent.id,
        vec![
            Assignee::Team(team.id.to_string()),
            Assignee::Human(person.clone()),
            Assignee::Agent(qa.clone()),
            Assignee::Team(team.id.to_string()),
        ],
    )
    .unwrap();
    assert_eq!(
        ws.staff_scope(parent.id),
        vec![
            Assignee::Team(team.id.to_string()),
            Assignee::Agent(qa.clone())
        ],
        "its own agents and teams, once each, in its order"
    );

    let child = ws
        .create_goal(spawned(parent.id, "the login screen"))
        .unwrap();
    let grandchild = ws.create_goal(spawned(child.id, "the button")).unwrap();
    assert_eq!(
        ws.staff_scope(grandchild.id),
        ws.staff_scope(parent.id),
        "inherited through a chain that names nobody"
    );

    ws.set_goal_assignees(child.id, vec![Assignee::Agent(dev.clone())])
        .unwrap();
    assert_eq!(
        ws.staff_scope(grandchild.id),
        vec![Assignee::Agent(dev.clone())],
        "the nearest goal that names any wins, never widened by the parent's"
    );
}

#[test]
fn team_assignment_is_stored_indexed_and_governing() {
    let (_d, ws) = ws();
    let human = PrincipalId::new(Keys::generate().public_key().to_hex()).unwrap();
    let team = ws
        .create_team(
            "deciders",
            None,
            vec![Assignee::Human(human.clone())],
            Tags::default(),
        )
        .unwrap();
    let goal = ws.create_goal(NewGoal::captured("delegated")).unwrap();
    assert!(ws
        .set_goal_assignees(goal.id, vec![Assignee::Team("ghost".into())])
        .is_err());
    ws.set_goal_assignees(goal.id, vec![Assignee::Team(team.id.to_string())])
        .unwrap();
    assert_eq!(
        ws.goals_for_assignee(&Assignee::Team(team.id.to_string()))
            .unwrap()[0]
            .id,
        goal.id
    );
    assert!(ws
        .gate_policy_allows_for(Gate::Approval, &human, Some(goal.id))
        .unwrap());
    ws.rebuild_index().unwrap();
    assert_eq!(
        ws.goals_for_assignee(&Assignee::Team(team.id.to_string()))
            .unwrap()
            .len(),
        1
    );
}
