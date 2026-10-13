//! The edges of a conversation's turn: a reader that unwinds mid-turn, a
//! live session handed the next message, a checkout with no folder, a
//! closed goal, an agent disabled under a pause, the guided cycle holding a
//! goal, a reply that cannot be posted, a turn longer than what is kept, a
//! wall with no attempt left, and the Decision-Making Agent naming who
//! answers an unaddressed message.

use crate::common::{self, catalog_with, design_off_config, until, workspace};
use bisa_core::{
    AgentId, ClosureReason, ConversationId, ConversationOrigin, MessageBody, PrincipalId,
    RespondPolicy,
};
use bisa_engine::events::ConversationChange;
use bisa_engine::{conversations, Engine, EngineConfig, EnginePayload, SessionState};
use bisa_harness::mock::{DeadModel, MockAdapter, ModelFailure};
use bisa_harness::{LifecycleEvent, ProgressEvent, SessionEvent};
use bisa_store::{NewAgent, NewConversation, NewGoal, NewProject, PostOrigin, Workspace};
use std::sync::Arc;
use std::time::Duration;

fn core_on(ws: &Workspace, harness: &str) {
    let mut def = ws.get_agent(&AgentId::general()).unwrap();
    def.harness = harness.into();
    ws.update_agent(def).unwrap();
}

fn general_pubkey(ws: &Workspace) -> String {
    ws.get_agent(&AgentId::general())
        .unwrap()
        .pubkey
        .as_hex()
        .to_string()
}

fn start(ws: &Workspace, origin: ConversationOrigin) -> bisa_core::Conversation {
    ws.create_conversation(NewConversation {
        origin,
        title: None,
        mode: bisa_core::ConversationMode::Auto,
    })
    .unwrap()
}

fn say(ws: &Workspace, scope: &str, what: &str, mentions: &[PrincipalId]) {
    ws.post_message(
        scope,
        MessageBody::post(what),
        None,
        mentions,
        &[],
        None,
        PostOrigin::Asked,
    )
    .unwrap();
}

fn said_by(ws: &Workspace, scope: &str, pubkey: &str) -> Vec<bisa_store::MessageRow> {
    ws.messages(scope, None, 200)
        .unwrap()
        .into_iter()
        .filter(|m| m.author == pubkey && !m.retracted)
        .collect()
}

/// A harness that says its words and stays alive between turns: the shape
/// of a session kept warm for a follow-up.
fn lingering(id: &str, words: &str) -> MockAdapter {
    MockAdapter {
        id: id.into(),
        script: Some(vec![
            SessionEvent::Lifecycle(LifecycleEvent::Started),
            SessionEvent::Progress(ProgressEvent::TurnStarted),
            SessionEvent::Progress(ProgressEvent::TextDelta { text: words.into() }),
            SessionEvent::Progress(ProgressEvent::TurnEnded),
        ]),
        ..Default::default()
    }
}

/// A harness that thinks, says its words and ends.
fn saying(id: &str, thinking: &str, words: Vec<String>) -> MockAdapter {
    let mut script = vec![
        SessionEvent::Lifecycle(LifecycleEvent::Started),
        SessionEvent::Progress(ProgressEvent::TurnStarted),
        SessionEvent::Progress(ProgressEvent::ThinkingDelta {
            text: thinking.into(),
        }),
    ];
    script.extend(
        words
            .into_iter()
            .map(|text| SessionEvent::Progress(ProgressEvent::TextDelta { text })),
    );
    script.push(SessionEvent::Progress(ProgressEvent::TurnEnded));
    script.push(SessionEvent::Lifecycle(LifecycleEvent::Ended {
        outcome: bisa_harness::Outcome::Completed,
        is_terminal: true,
    }));
    MockAdapter {
        id: id.into(),
        script: Some(script),
        ..Default::default()
    }
}

async fn replied(
    events: &mut tokio::sync::broadcast::Receiver<bisa_engine::EngineEvent>,
    scope: &str,
) -> bool {
    let mut posted = None;
    until("the turn's end on the bus", || {
        while let Ok(ev) = events.try_recv() {
            if let EnginePayload::AgentReplied {
                scope: s,
                posted: p,
                ..
            } = ev.payload
            {
                if s == scope {
                    posted = Some(p);
                }
            }
        }
        posted
    })
    .await
}

fn row_of(
    engine: &Engine,
    conversation: ConversationId,
) -> Option<bisa_engine::presence::SessionPresence> {
    engine
        .inner()
        .presence
        .snapshot()
        .into_iter()
        .find(|r| r.conversation == Some(conversation))
}

/// A reader of the harness's stream that unwinds mid-turn is the pump's to
/// survive: the row says the turn failed and why, the bus says the agent
/// did not post, and nothing of the turn stays in flight.
#[tokio::test(flavor = "multi_thread")]
async fn a_turn_whose_stream_panics_ends_its_row_as_failed_and_tells_the_bus() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = MockAdapter {
        panic_in_stream_on: Some("boom".into()),
        ..saying("chat-harness", "", vec!["boom".into()])
    };
    let engine = Engine::start(
        workspace(&dir),
        catalog_with(vec![adapter]),
        design_off_config(),
    )
    .unwrap();
    let ws = engine.workspace();
    core_on(ws, "chat-harness");
    let conversation = start(ws, ConversationOrigin::Workspace);
    let scope = conversation.id.to_string();
    let mut events = engine.events();

    say(ws, &scope, "go on", &[]);
    assert!(!replied(&mut events, &scope).await, "nothing was posted");
    let row = until("the row says the turn failed", || {
        row_of(&engine, conversation.id).filter(|r| matches!(r.state, SessionState::Failed { .. }))
    })
    .await;
    let SessionState::Failed { reason } = row.state else {
        unreachable!()
    };
    assert!(reason.contains("panicked"), "{reason}");
    assert!(bisa_engine::conversation::live_turns(engine.inner(), &scope).is_empty());
    assert!(said_by(ws, &scope, &general_pubkey(ws)).is_empty());
    engine.shutdown().await;
}

/// A session kept warm takes the next message as a follow-up: the harness
/// is handed the message itself, answers it in the open turn, and the idle
/// clock, touched by the follow-up, waits another interval before parking.
#[tokio::test(flavor = "multi_thread")]
async fn a_live_session_takes_the_next_message_as_a_follow_up_and_parks_an_interval_later() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = Arc::new(MockAdapter {
        answers_follow_ups: true,
        ..lingering("chat-harness", "here to stay")
    });
    let mut catalog = bisa_harness::HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<dyn bisa_harness::HarnessAdapter>);
    let engine = Engine::start(
        workspace(&dir),
        catalog,
        EngineConfig {
            idle_ttl: Duration::from_millis(700),
            ..design_off_config()
        },
    )
    .unwrap();
    let ws = engine.workspace();
    core_on(ws, "chat-harness");
    let general = general_pubkey(ws);
    let conversation = start(ws, ConversationOrigin::Workspace);
    let scope = conversation.id.to_string();
    let mut events = engine.events();

    say(ws, &scope, "stay a while", &[]);
    assert!(replied(&mut events, &scope).await, "the first reply lands");
    assert_eq!(adapter.launches().len(), 1);

    say(ws, &scope, "and answer this too", &[]);
    assert!(
        replied(&mut events, &scope).await,
        "the follow-up's reply lands"
    );
    let followed = adapter.follow_ups();
    assert_eq!(
        followed.len(),
        1,
        "one follow-up, no second launch: {followed:?}"
    );
    assert!(followed[0].contains("and answer this too"), "{followed:?}");
    assert_eq!(adapter.launches().len(), 1, "the live session took it");
    let posts = said_by(ws, &scope, &general);
    assert_eq!(posts.len(), 2, "{posts:?}");
    assert!(
        posts[1].content.starts_with("follow-up:"),
        "{}",
        posts[1].content
    );

    let row = row_of(&engine, conversation.id).expect("the turn's row");
    let session_id = row.session_id.unwrap().to_string();
    let parked = until(
        "the idle session parks an interval after the follow-up",
        || {
            ws.session_by_id(&session_id)
                .unwrap()
                .filter(|r| r.status == bisa_store::SessionStatus::Parked)
        },
    )
    .await;
    assert!(parked.parked_at.is_some());
    engine.shutdown().await;
}

/// A conversation about a checkout runs in the checkout; one whose folder is
/// not on disk wakes nobody rather than running the agent somewhere else.
#[tokio::test(flavor = "multi_thread")]
async fn a_conversation_about_a_checkout_with_no_folder_wakes_nobody() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = Arc::new(MockAdapter {
        id: "chat-harness".into(),
        ..Default::default()
    });
    let mut catalog = bisa_harness::HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<dyn bisa_harness::HarnessAdapter>);
    let engine = Engine::start(workspace(&dir), catalog, design_off_config()).unwrap();
    let ws = engine.workspace();
    core_on(ws, "chat-harness");
    let project = ws
        .create_project(NewProject::managed("web-app").unwrap())
        .unwrap();
    let primary = ws.primary_workstream(project.id).unwrap();
    assert!(
        !ws.checkout_in(&project, &primary).is_dir(),
        "no folder yet"
    );
    let conversation = start(
        ws,
        ConversationOrigin::Workstream {
            id: primary.id,
            project: project.id,
        },
    );
    let scope = conversation.id.to_string();
    let general = ws.get_agent(&AgentId::general()).unwrap().pubkey;

    say(ws, &scope, "anybody home?", std::slice::from_ref(&general));
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert!(adapter.launches().is_empty(), "no turn ran");
    assert!(
        row_of(&engine, conversation.id).is_none(),
        "no row was registered"
    );
    engine.shutdown().await;
}

/// A checkout's turn is framed with the goals the project is attached to —
/// by title, else by the statement's first line — the message's chips as a
/// block, its files and its capture by their paths on this machine, an
/// artifact not held here said so, and the agent's skills appended to the
/// prompt; and, nobody named, the workspace's default agent answers.
#[tokio::test(flavor = "multi_thread")]
async fn a_checkouts_turn_is_framed_with_its_goals_its_chips_its_files_and_the_agents_skills() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = Arc::new(MockAdapter {
        id: "chat-harness".into(),
        ..Default::default()
    });
    let mut catalog = bisa_harness::HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<dyn bisa_harness::HarnessAdapter>);
    let engine = Engine::start(workspace(&dir), catalog, design_off_config()).unwrap();
    let ws = engine.workspace();
    core_on(ws, "chat-harness");
    ws.create_skill(bisa_store::NewSkill {
        id: bisa_core::SkillId::new("tidy").unwrap(),
        name: "Tidy".into(),
        description: "keeps things tidy".into(),
        tags: Default::default(),
        markdown: "Keep every change small and tidy.".into(),
    })
    .unwrap();
    let general = ws.get_agent(&AgentId::general()).unwrap();
    let tidy = ws
        .add_agent(NewAgent {
            name: "Tidy".into(),
            photo: None,
            description: None,
            system_prompt: "You are Tidy.".into(),
            harness: "chat-harness".into(),
            models: Default::default(),
            skills: vec![bisa_core::SkillId::new("tidy").unwrap()],
            mcps: vec![],
            tags: Default::default(),
            respond: RespondPolicy::OwnerOnly,
            decision_making: false,
        })
        .unwrap();

    let project = ws
        .create_project(NewProject::managed("web-app").unwrap())
        .unwrap();
    let primary = ws.primary_workstream(project.id).unwrap();
    std::fs::create_dir_all(ws.checkout_in(&project, &primary)).unwrap();
    let titled = ws
        .create_goal(NewGoal {
            title: Some("Ship the shelf".into()),
            ..NewGoal::captured("ship it")
        })
        .unwrap();
    let untitled = ws
        .create_goal(NewGoal::captured("make the oak shelf\nand nothing else"))
        .unwrap();
    ws.attach(titled.id, project.id).unwrap();
    ws.attach(untitled.id, project.id).unwrap();
    // A default agent named that is no agent's id: the workspace's own answers.
    engine
        .set_setting(
            bisa_core::SettingScope::Workspace,
            None,
            "agents.default",
            serde_json::json!("nobody by that name"),
        )
        .unwrap();
    let conversation = start(
        ws,
        ConversationOrigin::Workstream {
            id: primary.id,
            project: project.id,
        },
    );
    let scope = conversation.id.to_string();

    let notes = ws
        .put_attachment(b"measure twice", "notes.txt", "text/plain")
        .unwrap();
    let shot = ws
        .put_attachment(b"\x89PNG\r\n\x1a\nIHDR-tiny", "home.png", "image/png")
        .unwrap();
    let chart = ws
        .put_attachment(
            b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
            "chart.svg",
            "image/svg+xml",
        )
        .unwrap();
    let body = MessageBody::Post {
        text: "look at this".into(),
        context: vec![
            bisa_core::ContextRef::File {
                path: bisa_core::RelPath::new("src/lib.rs").unwrap(),
            },
            bisa_core::ContextRef::Capture {
                device: "sim".into(),
                label: "home screen".into(),
                shot: shot.clone(),
                mark: None,
                note: "the header".into(),
            },
        ],
        artifacts: vec![bisa_core::ArtifactRef {
            sha256: chart.sha256.clone(),
            name: chart.name.clone(),
            mime: chart.mime.clone(),
            size: chart.size,
            title: "Chart".into(),
            kind: bisa_core::ArtifactKind::Svg,
            source: None,
        }],
        thinking: None,
        said: None,
    };
    ws.post_message(
        &scope,
        body,
        None,
        &[],
        std::slice::from_ref(&notes),
        None,
        PostOrigin::Asked,
    )
    .unwrap();

    until("the default agent's reply", || {
        said_by(ws, &scope, general.pubkey.as_hex())
            .into_iter()
            .next()
    })
    .await;
    let prompts = adapter.prompts();
    assert_eq!(prompts.len(), 1, "{prompts:?}");
    let prompt = &prompts[0];
    for needle in [
        "Ship the shelf",
        "make the oak shelf",
        "src/lib.rs",
        "[capture file] home screen",
        "[file] notes.txt",
        "[artifact] ",
    ] {
        assert!(
            prompt.contains(needle),
            "{needle:?} not in the prompt:\n{prompt}"
        );
    }
    assert!(
        !prompt.contains("and nothing else"),
        "a goal with no title is named by its statement's first line alone"
    );
    let launch = &adapter.launches()[0];
    assert_eq!(
        launch.cwd,
        ws.checkout_in(&project, &primary),
        "the turn runs in the checkout"
    );
    // An agent with a skill the harness cannot host natively is handed it
    // at the end of its prompt.
    say(
        ws,
        &scope,
        "tidy, your turn",
        std::slice::from_ref(&tidy.pubkey),
    );
    until("the skilled agent's reply", || {
        said_by(ws, &scope, tidy.pubkey.as_hex()).into_iter().next()
    })
    .await;
    let prompts = adapter.prompts();
    assert_eq!(prompts.len(), 2, "{prompts:?}");
    assert!(
        prompts[1].contains("Keep every change small and tidy."),
        "the skill is appended:\n{}",
        prompts[1]
    );
    engine.shutdown().await;
}

/// A closed goal's thread wakes nobody: what is said there is read, not
/// answered.
#[tokio::test(flavor = "multi_thread")]
async fn a_closed_goals_thread_wakes_nobody() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = Arc::new(MockAdapter {
        id: "chat-harness".into(),
        ..Default::default()
    });
    let mut catalog = bisa_harness::HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<dyn bisa_harness::HarnessAdapter>);
    let engine = Engine::start(workspace(&dir), catalog, design_off_config()).unwrap();
    let ws = engine.workspace();
    core_on(ws, "chat-harness");
    let goal = engine
        .submit_goal(bisa_engine::SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..common::guided("a goal that is over")
        })
        .unwrap();
    engine
        .close_goal(goal.id, ClosureReason::Abandoned { rationale: None })
        .unwrap();
    let general = ws.get_agent(&AgentId::general()).unwrap().pubkey;
    say(
        ws,
        &goal.id.to_string(),
        "one last word",
        std::slice::from_ref(&general),
    );
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert!(
        adapter.launches().is_empty(),
        "no turn in a closed goal's thread"
    );
    engine.shutdown().await;
}

/// An agent disabled between a message's dispatch and its wake — here under
/// a pause of the engine — is not launched when the wake runs.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_to_an_agent_disabled_under_a_pause_wakes_nobody() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = Arc::new(MockAdapter {
        id: "chat-harness".into(),
        ..Default::default()
    });
    let mut catalog = bisa_harness::HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<dyn bisa_harness::HarnessAdapter>);
    let ws = workspace(&dir);
    let agent = ws
        .add_agent(NewAgent {
            name: "Burst".into(),
            photo: None,
            description: None,
            system_prompt: "You are Burst.".into(),
            harness: "chat-harness".into(),
            models: Default::default(),
            skills: vec![],
            mcps: vec![],
            tags: Default::default(),
            respond: RespondPolicy::OwnerOnly,
            decision_making: false,
        })
        .unwrap();
    let dm = ws.open_dm(std::slice::from_ref(&agent.pubkey)).unwrap();
    let engine = Engine::start(ws, catalog, design_off_config()).unwrap();
    let ws = engine.workspace();

    engine.pause();
    say(ws, dm.id.as_str(), "are you there?", &[]);
    // The wake is scheduled and waits on the pause; the agent goes away
    // meanwhile.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut def = ws.get_agent(&agent.id).unwrap();
    def.enabled = false;
    ws.update_agent(def).unwrap();
    engine.resume();
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert!(
        adapter.launches().is_empty(),
        "a disabled agent is not launched"
    );
    assert!(said_by(ws, dm.id.as_str(), agent.pubkey.as_hex()).is_empty());
    engine.shutdown().await;
}

/// While the guided cycle holds a goal, a message that names the Workflow
/// Agent in the goal's thread is queued rather than woken — two sessions of
/// the agent never run on one goal at once — and the cycle's end wakes it.
#[tokio::test(flavor = "multi_thread")]
async fn a_chat_message_waits_while_the_guided_cycle_holds_the_goal_and_follows_it() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    common::drive_on(&ws, &AgentId::workflow(), "guided-harness");
    let adapter = Arc::new(MockAdapter {
        id: "guided-harness".into(),
        intake_script: Some(bisa_harness::mock::IntakeScript::new(vec![
            serde_json::json!({"op": "get_goal", "goal": "{{goal}}"}),
        ])),
        turn_delay: Duration::from_millis(1200),
        ..Default::default()
    });
    let mut catalog = bisa_harness::HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<dyn bisa_harness::HarnessAdapter>);
    let engine = Engine::start(
        ws,
        catalog,
        EngineConfig {
            design_enabled: true,
            events_enabled: false,
            ..Default::default()
        },
    )
    .unwrap();
    let ws = engine.workspace();
    let goal = engine.submit_goal(common::guided("design me")).unwrap();
    let scope = goal.id.to_string();
    until("the design wake is in flight", || {
        (adapter.launches().len() == 1).then_some(())
    })
    .await;
    assert!(
        engine.inner().guided.busy(goal.id),
        "the cycle holds the goal"
    );

    let workflow_agent = ws.get_agent(&AgentId::workflow()).unwrap().pubkey;
    say(
        ws,
        &scope,
        "while you design: what is the plan?",
        std::slice::from_ref(&workflow_agent),
    );
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        bisa_engine::conversation::deferred(engine.inner(), &scope),
        vec![AgentId::WORKFLOW.to_string()],
        "the chat wake is held, not dropped"
    );
    assert_eq!(
        adapter.launches().len(),
        1,
        "no second session while the cycle runs"
    );

    until("the cycle ends and the held wake runs", || {
        (adapter.launches().len() == 2).then_some(())
    })
    .await;
    until("the hold is released", || {
        bisa_engine::conversation::deferred(engine.inner(), &scope)
            .is_empty()
            .then_some(())
    })
    .await;
    engine.shutdown().await;
}

/// Putting away a conversation already put away changes nothing and says
/// nothing; and the idle clock of a session stopped before it fired finds
/// nothing to park.
#[tokio::test(flavor = "multi_thread")]
async fn archiving_twice_is_once_and_a_stopped_sessions_idle_clock_finds_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::start(
        workspace(&dir),
        catalog_with(vec![lingering("chat-harness", "here to stay")]),
        EngineConfig {
            idle_ttl: Duration::from_millis(500),
            ..design_off_config()
        },
    )
    .unwrap();
    let ws = engine.workspace();
    core_on(ws, "chat-harness");
    let conversation = start(ws, ConversationOrigin::Workspace);
    let scope = conversation.id.to_string();
    let mut events = engine.events();
    say(ws, &scope, "stay a while", &[]);
    assert!(replied(&mut events, &scope).await);
    let session_id = row_of(&engine, conversation.id)
        .expect("the turn's row")
        .session_id
        .unwrap()
        .to_string();

    let away = conversations::set_archived(engine.inner(), conversation.id, true)
        .await
        .unwrap();
    assert!(away.archived);
    let again = conversations::set_archived(engine.inner(), conversation.id, true)
        .await
        .unwrap();
    assert_eq!(again.archived, away.archived);
    // Past the idle clock: the session the stop took is not parked twice.
    tokio::time::sleep(Duration::from_millis(900)).await;
    let mut archived = 0;
    while let Ok(ev) = events.try_recv() {
        if let EnginePayload::ConversationChanged { id, change } = ev.payload {
            if id == conversation.id && change == ConversationChange::Archived {
                archived += 1;
            }
        }
    }
    assert_eq!(archived, 1, "one change on the bus");
    let row = ws.session_by_id(&session_id).unwrap().expect("the row");
    assert_eq!(row.status, bisa_store::SessionStatus::Parked);
    engine.shutdown().await;
}

/// A reply the store cannot take — here the conversation's record deleted
/// under the turn, behind the engine's back — is said on the bus as not
/// posted, the cut note with it, and the turn ends cleanly.
#[tokio::test(flavor = "multi_thread")]
async fn a_reply_whose_conversation_is_gone_is_said_as_not_posted() {
    let dir = tempfile::tempdir().unwrap();
    let line = format!("{}\n", "x".repeat(1023));
    let more_than_is_kept = bisa_engine::conversation::MAX_REPLY_BYTES / line.len() + 300;
    let words = vec![line.repeat(64); more_than_is_kept / 64 + 1];
    let engine = Engine::start(
        workspace(&dir),
        catalog_with(vec![MockAdapter {
            turn_delay: Duration::from_millis(400),
            ..saying("talker", "", words)
        }]),
        design_off_config(),
    )
    .unwrap();
    let ws = engine.workspace();
    core_on(ws, "talker");
    let general = general_pubkey(ws);
    let conversation = start(ws, ConversationOrigin::Workspace);
    let scope = conversation.id.to_string();
    let mut events = engine.events();

    say(ws, &scope, "count for me", &[]);
    tokio::time::sleep(Duration::from_millis(150)).await;
    ws.delete_conversation(conversation.id).unwrap();
    let posted = replied(&mut events, &scope).await;
    assert!(!posted, "the bus says the reply did not land");
    assert!(
        ws.messages(&scope, None, 10).unwrap_or_default().is_empty(),
        "nothing landed"
    );
    assert!(bisa_engine::conversation::live_turns(engine.inner(), &scope).is_empty());
    let _ = general;
    engine.shutdown().await;
}

/// A turn that thinks more than is kept keeps the tail of its thinking, on
/// a character boundary, and a reply cut mid-character is cut before it.
#[tokio::test(flavor = "multi_thread")]
async fn a_long_turn_keeps_the_tail_of_its_thinking_and_cuts_its_words_on_a_character() {
    let dir = tempfile::tempdir().unwrap();
    // Three-byte characters: neither the thinking's tail nor the reply's
    // bound falls on a character boundary by itself.
    let thinking = "€".repeat(66 * 1024);
    let line = "€".repeat(341);
    let event = line.repeat(64);
    let events_needed = bisa_engine::conversation::MAX_REPLY_BYTES / event.len() + 2;
    let words = vec![event; events_needed];
    let engine = Engine::start(
        workspace(&dir),
        catalog_with(vec![saying("talker", &thinking, words)]),
        design_off_config(),
    )
    .unwrap();
    let ws = engine.workspace();
    core_on(ws, "talker");
    let general = general_pubkey(ws);
    let conversation = start(ws, ConversationOrigin::Workspace);
    let scope = conversation.id.to_string();
    let mut events = engine.events();

    say(ws, &scope, "think hard", &[]);
    assert!(replied(&mut events, &scope).await, "the reply lands");
    let posts = said_by(ws, &scope, &general);
    let (note, parts) = posts.split_last().expect("something was said");
    assert!(note.content.contains("was not kept"), "{}", note.content);
    let kept: usize = parts.iter().map(|m| m.content.len()).sum();
    assert!(kept <= bisa_engine::conversation::MAX_REPLY_BYTES, "{kept}");
    assert!(
        parts.iter().all(|m| m.content.chars().all(|c| c == '€')),
        "every part is whole characters"
    );
    let thought = parts[0]
        .thinking
        .as_deref()
        .expect("the thinking rides the first part");
    assert!(
        thought.len() <= bisa_core::MAX_THINKING_BYTES,
        "{}",
        thought.len()
    );
    assert!(thought.chars().all(|c| c == '€'), "cut on a character");
    engine.shutdown().await;
}

/// A model that dies mid-turn with no attempt left ends the turn: the row
/// says the wall, no switch is announced, and nothing is relaunched.
#[tokio::test(flavor = "multi_thread")]
async fn a_wall_with_no_attempt_left_ends_the_turn_with_no_switch() {
    let dir = tempfile::tempdir().unwrap();
    let adapter = Arc::new(MockAdapter {
        id: "chat-harness".into(),
        dead_models: vec![ModelFailure::new("only-model", DeadModel::MidRun).reason("quota")],
        ..Default::default()
    });
    let mut catalog = bisa_harness::HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<dyn bisa_harness::HarnessAdapter>);
    let ws = workspace(&dir);
    let agent = ws
        .add_agent(NewAgent {
            name: "Scout".into(),
            photo: None,
            description: None,
            system_prompt: "You are Scout.".into(),
            harness: "chat-harness".into(),
            models: bisa_harness::ModelPlan {
                strategy: bisa_harness::ModelStrategy::Fallback,
                effort: None,
                models: vec![bisa_harness::ModelChoice::new("only-model")],
            },
            skills: vec![],
            mcps: vec![],
            tags: Default::default(),
            respond: RespondPolicy::OwnerOnly,
            decision_making: false,
        })
        .unwrap();
    let dm = ws.open_dm(std::slice::from_ref(&agent.pubkey)).unwrap();
    let engine = Engine::start(
        ws,
        catalog,
        EngineConfig {
            max_model_attempts: 1,
            ..design_off_config()
        },
    )
    .unwrap();
    let ws = engine.workspace();
    let mut events = engine.events();
    say(ws, dm.id.as_str(), "hello there", &[]);
    let row = until("the row says the wall", || {
        engine.inner().presence.snapshot().into_iter().find(|r| {
            r.agent.as_ref() == Some(&agent.id) && matches!(r.state, SessionState::Failed { .. })
        })
    })
    .await;
    let SessionState::Failed { reason } = row.state else {
        unreachable!()
    };
    assert!(reason.contains("only-model"), "{reason}");
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        adapter.launches().len(),
        1,
        "no relaunch with no attempt left"
    );
    while let Ok(ev) = events.try_recv() {
        assert!(
            !matches!(ev.payload, EnginePayload::ModelSwitched { .. }),
            "no switch is announced: {:?}",
            ev.payload
        );
    }
    assert!(
        said_by(ws, dm.id.as_str(), agent.pubkey.as_hex()).len() <= 1,
        "at most the half-said words, never a second attempt's"
    );
    engine.shutdown().await;
}

/// With the Decision-Making Agent on, an unaddressed message in a standing
/// channel is read by it, and the agent it names answers — not the default.
#[tokio::test(flavor = "multi_thread")]
async fn the_decision_making_agent_names_who_answers_an_unaddressed_message() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::start(
        workspace(&dir),
        catalog_with(vec![MockAdapter::default()]),
        design_off_config(),
    )
    .unwrap();
    let ws = engine.workspace();
    core_on(ws, "mock");
    let general = general_pubkey(ws);
    let add = |name: &str, what: &str| {
        ws.add_agent(NewAgent {
            name: name.into(),
            photo: None,
            description: Some(what.into()),
            system_prompt: format!("You are {name}."),
            harness: "mock".into(),
            models: Default::default(),
            skills: vec![],
            mcps: vec![],
            tags: Default::default(),
            respond: RespondPolicy::OwnerOnly,
            decision_making: false,
        })
        .unwrap()
    };
    let scout = add("Scout", "research and reading");
    let mason = add("Mason", "builds things");
    engine
        .set_setting(
            bisa_core::SettingScope::Workspace,
            None,
            "decisions.enabled",
            serde_json::json!(true),
        )
        .unwrap();
    let provider = Arc::new(bisa_decision::ScriptedProvider::new(vec![
        bisa_decision::ScriptedProvider::answers(
            "agent",
            bisa_decision::ScriptedProvider::choice(
                scout.id.as_str(),
                &[scout.id.as_str(), mason.id.as_str(), AgentId::GENERAL],
                0.9,
            ),
        ),
    ]));
    engine.stand_in_decision_provider(Some(provider.clone()));

    say(ws, "general", "could somebody look into this?", &[]);
    let reply = until("the named agent's reply", || {
        said_by(ws, "general", scout.pubkey.as_hex())
            .into_iter()
            .next()
    })
    .await;
    assert!(
        reply.content.contains("could somebody"),
        "{}",
        reply.content
    );
    assert_eq!(provider.asked().len(), 1, "read once");
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(
        said_by(ws, "general", &general).is_empty(),
        "the default stayed quiet"
    );
    assert!(said_by(ws, "general", mason.pubkey.as_hex()).is_empty());
    engine.shutdown().await;
}
