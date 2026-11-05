//! Conversations: a saved exchange with agents, with an origin — where its
//! turns run, who can be reached, what the roster says of a turn, how a
//! turn streams its words and its thinking, and what happens to its
//! sessions when it is put away.
//!
//! Proven without an LLM: the mock harness answers a prompt with
//! `echo: <prompt>`, so a frame the engine put in a prompt is read back in
//! the reply.

use crate::common::{engine_with, until};
use bisa_core::{
    AgentId, ConversationId, ConversationOrigin, MessageBody, PrincipalId, RespondPolicy,
    SettingScope, WorkstreamId,
};
use bisa_engine::events::{ConversationChange, EnginePayload};
use bisa_engine::registry::SessionKind;
use bisa_engine::{conversations, Engine, EngineConfig};
use bisa_harness::mock::MockAdapter;
use bisa_harness::{LifecycleEvent, ProgressEvent, SessionEvent};
use bisa_store::{
    ConversationFilter, NewAgent, NewConversation, NewGoal, NewProject, PostOrigin, SessionStatus,
    Workspace,
};
use std::time::Duration;

/// Point both core agents at the mock harness, so an unaddressed message —
/// the General Agent's everywhere, the Workflow Agent's in a conversation
/// about a workflow — wakes a session these tests control.
fn core_on(ws: &Workspace, harness: &str) {
    for id in [AgentId::general(), AgentId::workflow()] {
        let mut def = ws.get_agent(&id).unwrap();
        def.harness = harness.into();
        ws.update_agent(def).unwrap();
    }
}

/// An agent of the person's own, on the mock harness, answering its owner.
fn new_agent(name: &str) -> NewAgent {
    NewAgent {
        name: name.into(),
        photo: None,
        description: Some(format!("{name} helps with anything")),
        system_prompt: format!("You are {name}."),
        harness: "mock".into(),
        models: Default::default(),
        skills: vec![],
        mcps: vec![],
        tags: Default::default(),
        respond: RespondPolicy::OwnerOnly,
        decision_making: false,
    }
}

/// A one-step library workflow named `Release`.
fn release_workflow(ws: &Workspace) -> bisa_core::Workflow {
    ws.create_workflow(
        crate::common::new_workflow(
            "Release",
            vec![crate::common::step(
                "end",
                bisa_core::StepKind::End {
                    finish: bisa_core::Finish::Done,
                },
            )],
        ),
        bisa_core::WorkflowOrigin::Workspace,
    )
    .unwrap()
}

fn pubkey_of(engine: &Engine, id: &AgentId) -> String {
    engine
        .workspace()
        .get_agent(id)
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

fn say(engine: &Engine, conversation: ConversationId, what: &str, mentions: &[PrincipalId]) {
    engine
        .workspace()
        .post_message(
            &conversation.to_string(),
            MessageBody::post(what),
            None,
            mentions,
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap();
}

/// Every message in a conversation authored by one pubkey.
fn said_by(engine: &Engine, conversation: ConversationId, pubkey: &str) -> Vec<String> {
    engine
        .workspace()
        .messages(&conversation.to_string(), None, 200)
        .unwrap()
        .into_iter()
        .filter(|m| m.author == pubkey && !m.retracted)
        .map(|m| m.content)
        .collect()
}

fn general_pubkey(engine: &Engine) -> String {
    pubkey_of(engine, &AgentId::general())
}

/// A project with its primary checkout on disk, and a conversation about it.
fn checkout_conversation(
    ws: &Workspace,
) -> (bisa_core::Project, WorkstreamId, bisa_core::Conversation) {
    let project = ws
        .create_project(NewProject::managed("web-app").unwrap())
        .unwrap();
    let primary = ws.primary_workstream(project.id).unwrap();
    std::fs::create_dir_all(ws.checkout_in(&project, &primary)).unwrap();
    let conversation = start(
        ws,
        ConversationOrigin::Workstream {
            id: primary.id,
            project: project.id,
        },
    );
    (project, primary.id, conversation)
}

/// A turn of a conversation about a checkout runs **in the checkout**, and
/// is a `conversation` session of the roster that names its conversation —
/// never one of the checkout's running agents.
#[tokio::test(flavor = "multi_thread")]
async fn a_turn_in_a_checkouts_conversation_runs_there_and_is_the_conversations_not_the_checkouts()
{
    let dir = tempfile::tempdir().unwrap();
    let mock = MockAdapter::default();
    let launches = mock.launches.clone();
    let engine = engine_with(&dir, vec![mock]);
    let ws = engine.workspace();
    core_on(ws, "mock");
    let (project, primary, conversation) = checkout_conversation(ws);
    let checkout = ws.checkout_in(&project, &ws.get_workstream(primary).unwrap());

    say(&engine, conversation.id, "what is in this tree?", &[]);
    let general = general_pubkey(&engine);
    let reply = until("the general agent answers in the conversation", || {
        said_by(&engine, conversation.id, &general)
            .into_iter()
            .next()
    })
    .await;
    assert!(reply.contains("what is in this tree?"));
    assert!(
        reply.contains("You are working in project `web-app`"),
        "the frame names the project: {reply}"
    );

    {
        let launched = launches.lock().unwrap();
        assert_eq!(launched.len(), 1);
        assert_eq!(launched[0].cwd, checkout, "the turn runs in the checkout");
    }

    let row = until("the turn is on the roster", || {
        engine
            .inner()
            .presence
            .snapshot()
            .into_iter()
            .find(|r| r.conversation == Some(conversation.id))
    })
    .await;
    assert_eq!(row.kind, SessionKind::Conversation);
    assert_eq!(row.workstream, Some(primary));
    assert_eq!(row.project, Some(project.id));
    assert_eq!(row.agent, Some(AgentId::general()));
    let status = bisa_engine::ide::git::workstream_status(engine.inner(), primary)
        .await
        .unwrap();
    assert_eq!(
        status.running_agents, 0,
        "a conversation's turn is not one of the checkout's running agents"
    );
    let stored = ws
        .session_by_id(&row.session_id.unwrap().to_string())
        .unwrap()
        .expect("the turn's row");
    assert_eq!(stored.kind, SessionKind::Conversation);
    assert_eq!(stored.conversation, Some(conversation.id.to_string()));
    assert_eq!(stored.workstream, Some(primary.to_string()));
    let listed = ws.conversation_row(conversation.id).unwrap();
    assert_eq!(listed.agents, vec![AgentId::GENERAL.to_string()]);
    assert_eq!(listed.message_count, 2);

    engine.shutdown().await;
}

/// Where every other origin runs and what it is told: a project's in the
/// primary; a goal's, a workflow's, the workspace's and the node's in the
/// answering agent's own scratch, each with the sentence that says what it
/// is about. A workflow's is the Workflow Agent's; the rest the General
/// Agent's.
#[tokio::test(flavor = "multi_thread")]
async fn every_origin_places_its_turn_and_frames_it() {
    let dir = tempfile::tempdir().unwrap();
    let mock = MockAdapter::default();
    let launches = mock.launches.clone();
    let engine = engine_with(&dir, vec![mock]);
    let ws = engine.workspace();
    core_on(ws, "mock");
    let general = general_pubkey(&engine);
    let workflow_agent = pubkey_of(&engine, &AgentId::workflow());
    let project = ws
        .create_project(NewProject::managed("api").unwrap())
        .unwrap();
    let primary = ws.primary_workstream(project.id).unwrap();
    std::fs::create_dir_all(ws.checkout_in(&project, &primary)).unwrap();
    let goal = ws
        .create_goal(NewGoal::captured("Dark mode for the settings screen"))
        .unwrap();
    let workflow = release_workflow(ws);
    let scratch = ws.paths().agent(&AgentId::general()).scratch();
    let workflow_scratch = ws.paths().agent(&AgentId::workflow()).scratch();
    let note = ws
        .create_note(bisa_store::NewNote {
            scope: bisa_core::OwnerScope::Workspace,
            title: "The door".into(),
            body: "Sand it.\n".into(),
        })
        .unwrap();

    let cases: Vec<(ConversationOrigin, std::path::PathBuf, &str, &str)> = vec![
        // The mock takes the platform's tools, so the note's frame names
        // them: read first, rewrite at the hash read, add under its name.
        (
            ConversationOrigin::Note { id: note.id },
            scratch.clone(),
            "This conversation is about the note `The door`, open beside the person in the notes overlay. Read it with note_read",
            &general,
        ),
        (
            ConversationOrigin::Project { id: project.id },
            ws.checkout_in(&project, &primary),
            "You are working in project `api`, in its own tree",
            &general,
        ),
        (
            ConversationOrigin::Goal { id: goal.id },
            scratch.clone(),
            "This conversation is about the goal `Dark mode for the settings screen`",
            &general,
        ),
        // A workflow's conversation is the Workflow Agent's own: it runs in
        // that agent's scratch and is told the tools read this workflow.
        (
            ConversationOrigin::Workflow { id: workflow.id },
            workflow_scratch,
            "This conversation is about the workflow `Release`, id `",
            &workflow_agent,
        ),
        (
            ConversationOrigin::Workspace,
            scratch.clone(),
            "This conversation is about the workspace as a whole",
            &general,
        ),
        (
            ConversationOrigin::Node,
            scratch.clone(),
            "This conversation is about this machine's node",
            &general,
        ),
    ];
    for (i, (origin, cwd, frame, answerer)) in cases.iter().enumerate() {
        let conversation = start(ws, origin.clone());
        say(&engine, conversation.id, "hello there", &[]);
        let reply = until("the conversation's agent answers", || {
            said_by(&engine, conversation.id, answerer)
                .into_iter()
                .next()
        })
        .await;
        assert!(reply.contains(frame), "{origin:?}: {reply}");
        {
            let launched = launches.lock().unwrap();
            assert_eq!(launched.len(), i + 1);
            assert_eq!(&launched[i].cwd, cwd, "{origin:?} runs in the wrong place");
        }
    }
    // A goal-origin turn names its goal on the roster, so a permission it
    // raises has somewhere to escalate to.
    let goal_row = engine
        .inner()
        .registry
        .list()
        .into_iter()
        .find(|a| a.goal == Some(goal.id))
        .expect("the goal conversation's turn");
    assert_eq!(goal_row.kind, SessionKind::Conversation);
    assert!(goal_row.conversation.is_some());

    engine.shutdown().await;
}

/// The Workflow Agent is reachable in a conversation about a workflow — that
/// is what it is for — and never in one about a checkout.
#[tokio::test(flavor = "multi_thread")]
async fn the_workflow_agent_answers_in_a_workflow_conversation_and_never_in_a_checkouts() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    core_on(ws, "mock");
    let workflow_pk = pubkey_of(&engine, &AgentId::workflow());
    let mention = PrincipalId::new(workflow_pk.clone()).unwrap();
    let general = general_pubkey(&engine);
    let wf = release_workflow(ws);

    let about_workflow = start(ws, ConversationOrigin::Workflow { id: wf.id });
    say(
        &engine,
        about_workflow.id,
        "@Workflow Agent, add a review step",
        std::slice::from_ref(&mention),
    );
    let answered = until("the Workflow Agent answers about a workflow", || {
        said_by(&engine, about_workflow.id, &workflow_pk)
            .into_iter()
            .next()
    })
    .await;
    assert!(answered.contains("add a review step"));
    assert!(answered.contains("This conversation is about the workflow `Release`"));

    let (_, _, about_checkout) = checkout_conversation(ws);
    say(
        &engine,
        about_checkout.id,
        "@Workflow Agent, design something",
        std::slice::from_ref(&mention),
    );
    let took = until("the General Agent takes the checkout's message", || {
        said_by(&engine, about_checkout.id, &general)
            .into_iter()
            .next()
    })
    .await;
    assert!(took.contains("design something"));
    assert!(
        said_by(&engine, about_checkout.id, &workflow_pk).is_empty(),
        "the Workflow Agent never speaks in a conversation about a checkout"
    );

    engine.shutdown().await;
}

/// A conversation about a workflow is the Workflow Agent's own: a message
/// that names nobody wakes it — it alone reads, validates and saves the
/// workflow — in its own scratch, with the frame that names the workflow and
/// its id. The General Agent, who has none of those tools, never wakes there.
#[tokio::test(flavor = "multi_thread")]
async fn an_unaddressed_message_in_a_workflow_conversation_wakes_the_workflow_agent_and_not_the_general_agent(
) {
    let dir = tempfile::tempdir().unwrap();
    let mock = MockAdapter::default();
    let launches = mock.launches.clone();
    let engine = engine_with(&dir, vec![mock]);
    let ws = engine.workspace();
    core_on(ws, "mock");
    let workflow_pk = pubkey_of(&engine, &AgentId::workflow());
    let general = general_pubkey(&engine);
    let wf = release_workflow(ws);
    let conversation = start(ws, ConversationOrigin::Workflow { id: wf.id });

    say(
        &engine,
        conversation.id,
        "add a review step after the end",
        &[],
    );
    let answered = until("the Workflow Agent answers the unaddressed message", || {
        said_by(&engine, conversation.id, &workflow_pk)
            .into_iter()
            .next()
    })
    .await;
    assert!(answered.contains("add a review step after the end"));
    assert!(
        answered.contains(&format!(
            "This conversation is about the workflow `Release`, id `{}`",
            wf.id
        )),
        "the frame names the workflow and its id: {answered}"
    );
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(
        said_by(&engine, conversation.id, &general).is_empty(),
        "the General Agent never wakes in a conversation about a workflow"
    );
    {
        let launched = launches.lock().unwrap();
        assert_eq!(launched.len(), 1, "one turn, the Workflow Agent's");
        assert_eq!(
            launched[0].cwd,
            ws.paths().agent(&AgentId::workflow()).scratch(),
            "it runs in the Workflow Agent's own scratch"
        );
    }
    assert_eq!(
        ws.conversation_row(conversation.id).unwrap().agents,
        vec![AgentId::WORKFLOW.to_string()]
    );

    engine.shutdown().await;
}

/// `agents.default` names who answers an unaddressed message — everywhere
/// but in a conversation about a workflow, whose answerer is fixed by what
/// the subject needs: the setting still rules a workspace conversation.
#[tokio::test(flavor = "multi_thread")]
async fn a_workflow_conversation_keeps_the_workflow_agent_when_agents_default_names_another() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    core_on(ws, "mock");
    let ada = ws.add_agent(new_agent("Ada")).unwrap();
    let ada_pk = ada.pubkey.as_hex().to_string();
    engine
        .set_setting(
            SettingScope::Workspace,
            None,
            "agents.default",
            serde_json::json!(ada.id.to_string()),
        )
        .unwrap();
    let workflow_pk = pubkey_of(&engine, &AgentId::workflow());
    let wf = release_workflow(ws);

    let about_workflow = start(ws, ConversationOrigin::Workflow { id: wf.id });
    say(&engine, about_workflow.id, "rename the end step", &[]);
    let answered = until("the Workflow Agent answers about the workflow", || {
        said_by(&engine, about_workflow.id, &workflow_pk)
            .into_iter()
            .next()
    })
    .await;
    assert!(answered.contains("rename the end step"));

    let about_workspace = start(ws, ConversationOrigin::Workspace);
    say(&engine, about_workspace.id, "what is new?", &[]);
    let took = until("the agent the setting names answers elsewhere", || {
        said_by(&engine, about_workspace.id, &ada_pk)
            .into_iter()
            .next()
    })
    .await;
    assert!(took.contains("what is new?"));
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(
        said_by(&engine, about_workflow.id, &ada_pk).is_empty(),
        "the setting's agent never spoke about the workflow"
    );
    assert!(
        said_by(&engine, about_workspace.id, &workflow_pk).is_empty(),
        "the Workflow Agent never spoke in the workspace conversation"
    );

    // A core agent cannot be switched off (`CoreCannotBeDisabled`), so the
    // owner's only way out is its respond policy; the desktop's model pins
    // the fall-through (`agentRailModel.test.mjs`).
    assert!(matches!(
        ws.get_agent(&AgentId::workflow()).map(|mut a| {
            a.enabled = false;
            ws.update_agent(a)
        }),
        Ok(Err(bisa_store::StoreError::Agent(
            bisa_core::AgentError::CoreCannotBeDisabled
        )))
    ));

    engine.shutdown().await;
}

/// With the Decision-Making Agent on and two agents who could answer, an
/// unaddressed message is triaged — except in a conversation about a
/// workflow, which has one answerer: nobody is asked, and the Workflow Agent
/// takes it. The same message in a workspace conversation is triaged.
#[tokio::test(flavor = "multi_thread")]
async fn a_workflow_conversation_asks_no_triage() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    core_on(ws, "mock");
    engine
        .set_setting(
            SettingScope::Workspace,
            None,
            "decisions.enabled",
            serde_json::json!(true),
        )
        .unwrap();
    let general = general_pubkey(&engine);
    let workflow_pk = pubkey_of(&engine, &AgentId::workflow());
    // Asked, the judge would hand the message to the General Agent.
    let provider = std::sync::Arc::new(bisa_decision::ScriptedProvider::new(vec![
        bisa_decision::ScriptedProvider::answers(
            "agent",
            bisa_decision::ScriptedProvider::choice(
                AgentId::GENERAL,
                &[AgentId::GENERAL, AgentId::WORKFLOW],
                0.95,
            ),
        ),
    ]));
    engine.stand_in_decision_provider(Some(provider.clone()));
    let wf = release_workflow(ws);

    let about_workflow = start(ws, ConversationOrigin::Workflow { id: wf.id });
    say(&engine, about_workflow.id, "tighten the steps", &[]);
    let answered = until("the Workflow Agent answers unasked", || {
        said_by(&engine, about_workflow.id, &workflow_pk)
            .into_iter()
            .next()
    })
    .await;
    assert!(answered.contains("tighten the steps"));
    assert!(
        provider.asked().is_empty(),
        "no triage in a conversation about a workflow: {:?}",
        provider.asked()
    );

    let about_workspace = start(ws, ConversationOrigin::Workspace);
    say(&engine, about_workspace.id, "who does what here?", &[]);
    let took = until("the triaged agent answers in the workspace", || {
        said_by(&engine, about_workspace.id, &general)
            .into_iter()
            .next()
    })
    .await;
    assert!(took.contains("who does what here?"));
    let asked = provider.asked();
    assert_eq!(
        asked.len(),
        1,
        "the workspace conversation was triaged once"
    );
    assert!(
        asked[0].state["message"]
            .as_str()
            .is_some_and(|m| m.contains("who does what here?")),
        "the judge read the message: {:?}",
        asked[0].state
    );

    engine.shutdown().await;
}

/// The notes the platform authored under one agent's key in a conversation
/// — what a turn says of itself — as the Fluent id and the English.
fn notes_by(engine: &Engine, conversation: ConversationId, pubkey: &str) -> Vec<(String, String)> {
    engine
        .workspace()
        .messages(&conversation.to_string(), None, 200)
        .unwrap()
        .into_iter()
        .filter(|m| m.author == pubkey && !m.retracted)
        .filter_map(|m| m.said.map(|t| (t.id.to_string(), m.content)))
        .collect()
}

/// Every `agent_replied` the bus said for one scope, in order:
/// `(agent, posted, message)`.
fn replies_on(
    events: &mut tokio::sync::broadcast::Receiver<bisa_engine::EngineEvent>,
    scope: &str,
) -> Vec<(String, bool, Option<String>)> {
    let mut out = Vec::new();
    while let Ok(ev) = events.try_recv() {
        if let EnginePayload::AgentReplied {
            scope: s,
            agent,
            posted,
            message,
        } = ev.payload
        {
            if s == scope {
                out.push((agent, posted, message));
            }
        }
    }
    out
}

/// A failing turn, end to end: the General Agent on `mock` in a workspace
/// conversation, one message said, then the note the turn leaves — its id,
/// its words — and the reply the bus settled it with, which names the note.
async fn failing_turn(
    engine: &Engine,
    message: &str,
) -> (String, String, Vec<(String, bool, Option<String>)>) {
    let ws = engine.workspace();
    core_on(ws, "mock");
    let general = general_pubkey(engine);
    let conversation = start(ws, ConversationOrigin::Workspace);
    let mut events = engine.events();
    say(engine, conversation.id, message, &[]);
    let (id, words) = until("the turn says why it failed", || {
        notes_by(engine, conversation.id, &general)
            .into_iter()
            .next()
    })
    .await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(
        notes_by(engine, conversation.id, &general).len(),
        1,
        "said once"
    );
    assert!(
        said_by(engine, conversation.id, &general)
            .iter()
            .all(|m| m == &words),
        "nothing but the note was posted"
    );
    let replies = replies_on(&mut events, &conversation.id.to_string());
    let note_id = ws
        .messages(&conversation.id.to_string(), None, 200)
        .unwrap()
        .into_iter()
        .find(|m| m.said.is_some())
        .map(|m| m.id)
        .unwrap();
    assert!(
        replies.contains(&(AgentId::GENERAL.to_string(), true, Some(note_id))),
        "the bus settled the turn on the note: {replies:?}"
    );
    (id, words, replies)
}

/// A turn whose harness cannot be launched says so in the conversation —
/// the harness and its reason — and settles the turn on the bus, instead of
/// a warning in the log and a working dot that never rests.
#[tokio::test(flavor = "multi_thread")]
async fn a_turn_that_cannot_launch_says_why_in_the_conversation_and_settles() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(
        &dir,
        vec![MockAdapter {
            available: false,
            ..Default::default()
        }],
    );
    let (id, words, _) = failing_turn(&engine, "are you there?").await;
    assert_eq!(id, "engine-conversation-turn-not-launched");
    assert!(
        words.contains("I could not start on mock") && words.contains("mock disabled"),
        "{words}"
    );
    assert!(words.contains("under Agents"), "{words}");
    engine.shutdown().await;
}

/// A harness that launched and then refused its first turn is said too, and
/// its row — registered before the prompt — is over, not left running.
#[tokio::test(flavor = "multi_thread")]
async fn a_turn_refused_at_its_prompt_says_so_and_ends_its_row() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(
        &dir,
        vec![MockAdapter {
            refuse_prompt: true,
            ..Default::default()
        }],
    );
    let (id, words, _) = failing_turn(&engine, "hello?").await;
    assert_eq!(id, "engine-conversation-turn-prompt-refused");
    assert!(
        words.contains("I started on mock") && words.contains("session terminated"),
        "{words}"
    );
    assert!(
        engine.workspace().list_live_sessions().unwrap().is_empty(),
        "the row ended"
    );
    assert!(
        engine
            .inner()
            .presence
            .snapshot()
            .iter()
            .all(|r| !r.state.is_live()),
        "no live row stands for a turn that never began"
    );
    engine.shutdown().await;
}

/// A session that ends failed before a word says the harness's own reason
/// — redacted as a reply is, so a harness that dies printing a token puts
/// none in the conversation — as the agent's note, once.
#[tokio::test(flavor = "multi_thread")]
async fn a_turn_that_ends_failed_without_a_word_says_the_harnesss_reason_and_no_secret() {
    // Synthetic: the shape the redactor knows, and nobody's token.
    const TOKEN: &str = "ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(
        &dir,
        vec![MockAdapter {
            script: Some(vec![
                SessionEvent::Lifecycle(LifecycleEvent::Started),
                SessionEvent::Progress(ProgressEvent::TurnStarted),
                SessionEvent::Lifecycle(LifecycleEvent::Ended {
                    outcome: bisa_harness::Outcome::Failed {
                        error: format!("the provider refused the key {TOKEN}."),
                    },
                    is_terminal: true,
                }),
            ]),
            ..Default::default()
        }],
    );
    let (id, words, _) = failing_turn(&engine, "what now?").await;
    assert_eq!(id, "engine-conversation-turn-ended-without-reply");
    assert!(
        words.contains("I could not finish this turn")
            && words.contains("the provider refused the key"),
        "{words}"
    );
    assert!(
        !words.contains(TOKEN),
        "the note carries the token: {words}"
    );
    assert!(words.contains("«secret:"), "{words}");
    engine.shutdown().await;
}

/// A session that fails after its words landed keeps the words as the reply
/// and adds no note: the person has an answer, and the harness's end is the
/// log's. Settled once, on the words.
#[tokio::test(flavor = "multi_thread")]
async fn a_turn_that_fails_after_its_words_landed_keeps_the_words_and_adds_no_note() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(
        &dir,
        vec![MockAdapter {
            script: Some(vec![
                SessionEvent::Lifecycle(LifecycleEvent::Started),
                SessionEvent::Progress(ProgressEvent::TurnStarted),
                SessionEvent::Progress(ProgressEvent::TextDelta {
                    text: "half an answer".into(),
                }),
                SessionEvent::Lifecycle(LifecycleEvent::Ended {
                    outcome: bisa_harness::Outcome::Failed {
                        error: "the provider closed the stream".into(),
                    },
                    is_terminal: true,
                }),
            ]),
            ..Default::default()
        }],
    );
    let ws = engine.workspace();
    core_on(ws, "mock");
    let general = general_pubkey(&engine);
    let conversation = start(ws, ConversationOrigin::Workspace);
    let mut events = engine.events();
    say(&engine, conversation.id, "go on", &[]);
    let words = until("the words land", || {
        said_by(&engine, conversation.id, &general)
            .into_iter()
            .next()
    })
    .await;
    assert_eq!(words, "half an answer");
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(
        notes_by(&engine, conversation.id, &general).is_empty(),
        "no note: the words are the reply"
    );
    let replies = replies_on(&mut events, &conversation.id.to_string());
    assert_eq!(replies.len(), 1, "settled once, on the words: {replies:?}");
    assert!(replies[0].1 && replies[0].2.is_some());
    engine.shutdown().await;
}

/// A turn stopped by a person posts no complaint — the stop said it — but
/// the bus still hears the turn end, with nothing posted.
#[tokio::test(flavor = "multi_thread")]
async fn a_turn_stopped_by_a_person_posts_no_complaint_and_still_settles() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(
        &dir,
        vec![MockAdapter {
            script: Some(vec![
                SessionEvent::Lifecycle(LifecycleEvent::Started),
                SessionEvent::Progress(ProgressEvent::TurnStarted),
                SessionEvent::Progress(ProgressEvent::ToolStarted {
                    name: "Read".into(),
                    args_summary: "the world".into(),
                    tier: bisa_core::ToolTier::Read,
                    id: None,
                }),
            ]),
            ..Default::default()
        }],
    );
    let ws = engine.workspace();
    core_on(ws, "mock");
    let general = general_pubkey(&engine);
    let conversation = start(ws, ConversationOrigin::Workspace);
    let mut events = engine.events();
    let mut heard = engine.events();
    say(&engine, conversation.id, "take your time", &[]);
    // Stopped once the turn is under way — its tool call on the bus — so
    // the stop meets a session, not the window before the prompt.
    until("the turn's tool call is on the bus", || {
        while let Ok(ev) = heard.try_recv() {
            if let EnginePayload::Session {
                event: SessionEvent::Progress(ProgressEvent::ToolStarted { .. }),
            } = ev.payload
            {
                return Some(());
            }
        }
        None
    })
    .await;
    let row = engine
        .inner()
        .presence
        .snapshot()
        .into_iter()
        .find(|r| r.conversation == Some(conversation.id) && r.state.is_live())
        .expect("the turn's live row");
    assert!(bisa_engine::sessions::stop_one(engine.inner(), row.id).unwrap());
    let replies = until("the bus hears the turn end", || {
        let replies = replies_on(&mut events, &conversation.id.to_string());
        (!replies.is_empty()).then_some(replies)
    })
    .await;
    assert_eq!(
        replies,
        vec![(AgentId::GENERAL.to_string(), false, None)],
        "settled, with nothing posted"
    );
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(
        said_by(&engine, conversation.id, &general).is_empty(),
        "a stopped turn says nothing of itself"
    );
    engine.shutdown().await;
}

/// A turn that completes with no words at all posts nothing, and still ends
/// on the bus — once — so no pane holds a turn that will never settle.
#[tokio::test(flavor = "multi_thread")]
async fn an_empty_turn_still_settles_the_working_indicator_once() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(
        &dir,
        vec![MockAdapter {
            script: Some(vec![
                SessionEvent::Lifecycle(LifecycleEvent::Started),
                SessionEvent::Progress(ProgressEvent::TurnStarted),
                SessionEvent::Progress(ProgressEvent::TurnEnded),
                SessionEvent::Lifecycle(LifecycleEvent::Ended {
                    outcome: bisa_harness::Outcome::Completed,
                    is_terminal: true,
                }),
            ]),
            ..Default::default()
        }],
    );
    let ws = engine.workspace();
    core_on(ws, "mock");
    let general = general_pubkey(&engine);
    let conversation = start(ws, ConversationOrigin::Workspace);
    let mut events = engine.events();
    say(&engine, conversation.id, "say nothing", &[]);
    let replies = until("the bus hears the empty turn end", || {
        let replies = replies_on(&mut events, &conversation.id.to_string());
        (!replies.is_empty()).then_some(replies)
    })
    .await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    let mut all = replies;
    all.extend(replies_on(&mut events, &conversation.id.to_string()));
    assert_eq!(
        all,
        vec![(AgentId::GENERAL.to_string(), false, None)],
        "settled once, with nothing posted"
    );
    assert!(said_by(&engine, conversation.id, &general).is_empty());
    engine.shutdown().await;
}

/// A turn past the wall clock is stopped, what it said so far lands, and
/// the note after it says why — and the bus settles on the note.
#[tokio::test(flavor = "multi_thread")]
async fn a_turn_past_the_wall_clock_keeps_its_words_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let lingering = MockAdapter {
        script: Some(vec![
            SessionEvent::Lifecycle(LifecycleEvent::Started),
            SessionEvent::Progress(ProgressEvent::TurnStarted),
            SessionEvent::Progress(ProgressEvent::TextDelta {
                text: "still thinking about it".into(),
            }),
        ]),
        ..Default::default()
    };
    let engine = Engine::start(
        crate::common::workspace(&dir),
        crate::common::catalog_with(vec![lingering]),
        EngineConfig {
            default_wall_clock_secs: 1,
            ..crate::common::design_off_config()
        },
    )
    .unwrap();
    let ws = engine.workspace();
    core_on(ws, "mock");
    let general = general_pubkey(&engine);
    let conversation = start(ws, ConversationOrigin::Workspace);
    let mut events = engine.events();
    say(&engine, conversation.id, "think hard", &[]);
    let (id, words) = until("the wall clock is said", || {
        notes_by(&engine, conversation.id, &general)
            .into_iter()
            .next()
    })
    .await;
    assert_eq!(id, "engine-conversation-turn-past-wall-clock");
    assert!(words.contains("ran past the 1 s"), "{words}");
    let said = said_by(&engine, conversation.id, &general);
    assert_eq!(said.len(), 2, "the words so far, then the note: {said:?}");
    assert_eq!(said[0], "still thinking about it");
    let replies = replies_on(&mut events, &conversation.id.to_string());
    assert!(
        replies.iter().any(|(_, posted, m)| *posted && m.is_some()),
        "{replies:?}"
    );
    let row = until("the row says the wall clock", || {
        engine
            .inner()
            .presence
            .snapshot()
            .into_iter()
            .find(|r| r.conversation == Some(conversation.id) && !r.state.is_live())
    })
    .await;
    assert!(
        matches!(&row.state, bisa_engine::SessionState::Failed { reason } if reason == "wall clock exceeded"),
        "{:?}",
        row.state
    );
    engine.shutdown().await;
}

/// A conversation about a checkout that is gone from disk says so instead
/// of waking nothing: the project and the path it expected.
#[tokio::test(flavor = "multi_thread")]
async fn a_turn_in_a_checkout_that_is_gone_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    core_on(ws, "mock");
    let general = general_pubkey(&engine);
    let (project, primary, conversation) = checkout_conversation(ws);
    let checkout = ws.checkout_in(&project, &ws.get_workstream(primary).unwrap());
    std::fs::remove_dir_all(&checkout).unwrap();
    say(&engine, conversation.id, "what is in this tree?", &[]);
    let (id, words) = until("the missing checkout is said", || {
        notes_by(&engine, conversation.id, &general)
            .into_iter()
            .next()
    })
    .await;
    assert_eq!(id, "engine-conversation-turn-no-checkout");
    assert!(
        words.contains("project web-app") && words.contains(&checkout.display().to_string()),
        "{words}"
    );
    engine.shutdown().await;
}

/// A harness whose one turn says `thinking`, then each of `words` as a delta.
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

/// What the agent said in a scope, oldest first: its words, and the thinking
/// kept beside them.
fn posts_of(
    ws: &bisa_store::Workspace,
    scope: &str,
    author: &str,
) -> Vec<(String, String, Option<String>)> {
    // As the store reads them: oldest first, and within one second in the
    // order they were said.
    ws.messages(scope, None, 50)
        .unwrap()
        .into_iter()
        .filter(|m| m.author == author)
        .map(|m| (m.id, m.content, m.thinking))
        .collect()
}

/// A reply longer than one message holds is said whole, in as many messages
/// as it takes: each within the bound, cut where a paragraph ends, in order,
/// the thinking beside the first — and the bus names the one it ended on.
/// Nothing an agent said is thrown away for being long.
#[tokio::test(flavor = "multi_thread")]
async fn a_reply_longer_than_one_message_lands_whole_in_as_many_as_it_takes() {
    let dir = tempfile::tempdir().unwrap();
    let paragraph = |letter: &str| format!("{}\n\n", letter.repeat(200 * 1024));
    let words = vec![paragraph("a"), paragraph("b"), "c".repeat(200 * 1024)];
    let whole: String = words.concat();
    let engine = engine_with(
        &dir,
        vec![
            saying("talker", "it is a long story", words),
            MockAdapter::default(),
        ],
    );
    let ws = engine.workspace();
    core_on(ws, "talker");
    let general = general_pubkey(&engine);
    let conversation = start(ws, ConversationOrigin::Workspace);
    let scope = conversation.id.to_string();
    let mut events = engine.events();

    say(&engine, conversation.id, "tell me everything", &[]);
    let landed = until("the bus says where the reply ended", || {
        while let Ok(ev) = events.try_recv() {
            if let EnginePayload::AgentReplied {
                scope: s,
                posted,
                message,
                ..
            } = ev.payload
            {
                if s == scope {
                    return Some((posted, message));
                }
            }
        }
        None
    })
    .await;
    let said = posts_of(ws, &scope, &general);
    assert_eq!(said.len(), 3, "three paragraphs, each a message of its own");
    for (_, text, _) in &said {
        assert!(
            text.len() <= bisa_core::MAX_TEXT_BYTES,
            "a part of {} bytes",
            text.len()
        );
    }
    let joined = said
        .iter()
        .map(|(_, text, _)| text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    assert_eq!(joined.len(), whole.len());
    assert!(joined == whole, "the words, whole and in order");
    assert_eq!(said[0].2.as_deref(), Some("it is a long story"));
    assert!(
        said[1].2.is_none() && said[2].2.is_none(),
        "the thinking is said once"
    );
    assert_eq!(landed, (true, Some(said[2].0.clone())));
    assert!(conversations::live_turns(engine.inner(), conversation.id)
        .unwrap()
        .is_empty());
    engine.shutdown().await;
}

/// A turn that never stops talking costs the node a bounded memory: what it
/// says past the most a reply keeps is not gathered, and the person is told
/// the reply was cut, after the words that were kept.
#[tokio::test(flavor = "multi_thread")]
async fn a_reply_without_an_end_is_kept_to_a_bound_and_says_it_was_cut() {
    let dir = tempfile::tempdir().unwrap();
    let line = format!("{}\n", "x".repeat(1023));
    let more_than_is_kept = bisa_engine::conversation::MAX_REPLY_BYTES / line.len() + 300;
    let words = vec![line.repeat(64); more_than_is_kept / 64 + 1];
    let engine = engine_with(
        &dir,
        vec![saying("talker", "", words), MockAdapter::default()],
    );
    let ws = engine.workspace();
    core_on(ws, "talker");
    let general = general_pubkey(&engine);
    let conversation = start(ws, ConversationOrigin::Workspace);
    let scope = conversation.id.to_string();
    let mut events = engine.events();

    say(&engine, conversation.id, "count for me", &[]);
    until("the reply ended", || {
        while let Ok(ev) = events.try_recv() {
            if let EnginePayload::AgentReplied { scope: s, .. } = ev.payload {
                if s == scope {
                    return Some(());
                }
            }
        }
        None
    })
    .await;
    let said = posts_of(ws, &scope, &general);
    let (note, words) = said.split_last().expect("something was said");
    let kept: usize = words.iter().map(|(_, text, _)| text.len()).sum();
    assert!(
        kept <= bisa_engine::conversation::MAX_REPLY_BYTES,
        "{kept} bytes kept"
    );
    assert!(
        kept > bisa_engine::conversation::MAX_REPLY_BYTES - 2 * bisa_core::MAX_TEXT_BYTES,
        "what fits is kept: {kept} bytes"
    );
    for (_, text, _) in words {
        assert!(text.len() <= bisa_core::MAX_TEXT_BYTES);
        assert!(text.starts_with('x'), "a part of the reply");
    }
    assert!(
        note.1.contains("was not kept") && !note.1.starts_with('x'),
        "the last word is the platform's, saying the reply was cut: {}",
        note.1
    );
    engine.shutdown().await;
}

/// A turn streams: while the harness writes, the bus carries `AgentStreamed`
/// frames whose parts add up to the words and the thinking; the reply lands
/// as one message with its thinking beside it; nothing is left in flight.
#[tokio::test(flavor = "multi_thread")]
async fn a_turn_streams_its_words_and_its_thinking_and_posts_both() {
    let dir = tempfile::tempdir().unwrap();
    let talker = MockAdapter {
        id: "talker".into(),
        script: Some(vec![
            SessionEvent::Lifecycle(LifecycleEvent::Started),
            SessionEvent::Progress(ProgressEvent::TurnStarted),
            SessionEvent::Progress(ProgressEvent::ThinkingDelta {
                text: "the palette ".into(),
            }),
            SessionEvent::Progress(ProgressEvent::ThinkingDelta {
                text: "was chosen last week".into(),
            }),
            SessionEvent::Progress(ProgressEvent::TextDelta {
                text: "We ship ".into(),
            }),
            SessionEvent::Progress(ProgressEvent::TextDelta {
                text: "the muted palette.".into(),
            }),
            SessionEvent::Progress(ProgressEvent::TurnEnded),
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: bisa_harness::Outcome::Completed,
                is_terminal: true,
            }),
        ]),
        ..Default::default()
    };
    let engine = engine_with(&dir, vec![talker, MockAdapter::default()]);
    let ws = engine.workspace();
    core_on(ws, "talker");
    let general = general_pubkey(&engine);
    let conversation = start(ws, ConversationOrigin::Workspace);
    let scope = conversation.id.to_string();
    let mut events = engine.events();

    say(&engine, conversation.id, "which palette?", &[]);
    let (text, thinking) = until("the reply is posted", || {
        ws.messages(&scope, None, 10)
            .unwrap()
            .into_iter()
            .find(|m| m.author == general)
            .map(|m| (m.content, m.thinking))
    })
    .await;
    assert_eq!(text, "We ship the muted palette.", "the words, whole");
    assert_eq!(
        thinking.as_deref(),
        Some("the palette was chosen last week"),
        "the thinking rides beside the words"
    );

    // The frames add up to the same words and the same thinking, and the
    // bus says when the message landed.
    let mut streamed_text = String::new();
    let mut streamed_thinking = String::new();
    let mut replied = false;
    until("the bus carried the stream and the reply", || {
        while let Ok(ev) = events.try_recv() {
            match ev.payload {
                EnginePayload::AgentStreamed {
                    scope: s,
                    agent,
                    text,
                    thinking,
                    working: _,
                } if s == scope => {
                    assert_eq!(agent, "general-agent");
                    streamed_text.push_str(&text);
                    streamed_thinking.push_str(&thinking);
                }
                EnginePayload::AgentReplied {
                    scope: s, posted, ..
                } if s == scope => replied = posted,
                _ => {}
            }
        }
        replied.then_some(())
    })
    .await;
    assert_eq!(streamed_text, "We ship the muted palette.");
    assert_eq!(streamed_thinking, "the palette was chosen last week");
    assert!(
        conversations::live_turns(engine.inner(), conversation.id)
            .unwrap()
            .is_empty(),
        "nothing is in flight once the reply is posted"
    );

    engine.shutdown().await;
}

/// While a tool runs the live row names it — `Read src/app.ts` on the frame
/// and on the turn in flight — and the line clears when the tool ends; the
/// words that follow ride a frame with no tool line.
#[tokio::test(flavor = "multi_thread")]
async fn a_turn_says_which_tool_runs_while_its_words_wait_and_clears_it_after() {
    let dir = tempfile::tempdir().unwrap();
    let reader = MockAdapter {
        id: "reader".into(),
        script: Some(vec![
            SessionEvent::Lifecycle(LifecycleEvent::Started),
            SessionEvent::Progress(ProgressEvent::TurnStarted),
            SessionEvent::Progress(ProgressEvent::ThinkingDelta {
                text: "let me look".into(),
            }),
            SessionEvent::Progress(ProgressEvent::ToolStarted {
                name: "Read".into(),
                args_summary: "src/app.ts".into(),
                tier: bisa_core::ToolTier::Read,
                id: None,
            }),
        ]),
        ..Default::default()
    };
    let engine = engine_with(&dir, vec![reader, MockAdapter::default()]);
    let ws = engine.workspace();
    core_on(ws, "reader");
    let conversation = start(ws, ConversationOrigin::Workspace);
    let scope = conversation.id.to_string();
    let mut events = engine.events();
    say(&engine, conversation.id, "what does app.ts do?", &[]);

    // The turn in flight names the tool while it runs.
    let turns = until("the tool line is on the turn in flight", || {
        let turns = conversations::live_turns(engine.inner(), conversation.id).unwrap();
        turns
            .first()
            .filter(|t| t.working.is_some())
            .map(|_| turns.clone())
    })
    .await;
    assert_eq!(turns[0].working.as_deref(), Some("Read src/app.ts"));
    assert_eq!(turns[0].thinking, "let me look");

    // …and so does the frame that carried it, thinking or no thinking.
    let working_frame = until("a frame carried the tool line", || {
        while let Ok(ev) = events.try_recv() {
            if let EnginePayload::AgentStreamed {
                scope: s, working, ..
            } = ev.payload
            {
                if s == scope && working.is_some() {
                    return working;
                }
            }
        }
        None
    })
    .await;
    assert_eq!(working_frame, "Read src/app.ts");

    engine.shutdown().await;
}

/// The tool ends and the words begin: the frame that carries the words has
/// no tool line, and the turn in flight has none either.
#[tokio::test(flavor = "multi_thread")]
async fn a_tool_that_ended_leaves_no_line_under_the_words() {
    let dir = tempfile::tempdir().unwrap();
    let reader = MockAdapter {
        id: "reader".into(),
        script: Some(vec![
            SessionEvent::Lifecycle(LifecycleEvent::Started),
            SessionEvent::Progress(ProgressEvent::TurnStarted),
            SessionEvent::Progress(ProgressEvent::ToolStarted {
                name: "Read".into(),
                args_summary: "src/app.ts".into(),
                tier: bisa_core::ToolTier::Read,
                id: None,
            }),
            SessionEvent::Progress(ProgressEvent::ToolEnded {
                name: "Read".into(),
                ok: true,
                id: None,
            }),
            SessionEvent::Progress(ProgressEvent::TextDelta {
                text: "It boots the app.".into(),
            }),
        ]),
        ..Default::default()
    };
    let engine = engine_with(&dir, vec![reader, MockAdapter::default()]);
    let ws = engine.workspace();
    core_on(ws, "reader");
    let conversation = start(ws, ConversationOrigin::Workspace);
    let scope = conversation.id.to_string();
    let mut events = engine.events();
    say(&engine, conversation.id, "what does app.ts do?", &[]);

    let turns = until("the words are on the turn in flight", || {
        let turns = conversations::live_turns(engine.inner(), conversation.id).unwrap();
        turns
            .first()
            .filter(|t| !t.text.is_empty())
            .map(|_| turns.clone())
    })
    .await;
    assert_eq!(turns[0].working, None, "the tool ended before the words");
    let mut saw_tool = false;
    let words_frame_working = until("the frame with the words", || {
        while let Ok(ev) = events.try_recv() {
            if let EnginePayload::AgentStreamed {
                scope: s,
                text,
                working,
                ..
            } = ev.payload
            {
                if s != scope {
                    continue;
                }
                if working.is_some() {
                    saw_tool = true;
                }
                if !text.is_empty() {
                    return Some(working);
                }
            }
        }
        None
    })
    .await;
    assert!(saw_tool, "a frame named the tool while it ran");
    assert_eq!(
        words_frame_working, None,
        "the words ride a frame with no tool line"
    );

    engine.shutdown().await;
}

/// The reply lands naming the message it became: `AgentReplied.message` is
/// the stored message's id, so a timeline keeps the turn on screen until
/// that very message is in its page.
#[tokio::test(flavor = "multi_thread")]
async fn a_reply_lands_naming_the_message_it_posted() {
    let dir = tempfile::tempdir().unwrap();
    let talker = MockAdapter {
        id: "talker".into(),
        script: Some(vec![
            SessionEvent::Lifecycle(LifecycleEvent::Started),
            SessionEvent::Progress(ProgressEvent::TurnStarted),
            SessionEvent::Progress(ProgressEvent::TextDelta {
                text: "Muted.".into(),
            }),
            SessionEvent::Progress(ProgressEvent::TurnEnded),
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome: bisa_harness::Outcome::Completed,
                is_terminal: true,
            }),
        ]),
        ..Default::default()
    };
    let engine = engine_with(&dir, vec![talker, MockAdapter::default()]);
    let ws = engine.workspace();
    core_on(ws, "talker");
    let general = general_pubkey(&engine);
    let conversation = start(ws, ConversationOrigin::Workspace);
    let scope = conversation.id.to_string();
    let mut events = engine.events();
    say(&engine, conversation.id, "which palette?", &[]);

    let posted = until("the reply is posted", || {
        ws.messages(&scope, None, 10)
            .unwrap()
            .into_iter()
            .find(|m| m.author == general)
            .map(|m| m.id)
    })
    .await;
    let named = until("the bus named the message", || {
        while let Ok(ev) = events.try_recv() {
            if let EnginePayload::AgentReplied {
                scope: s,
                posted: true,
                message,
                ..
            } = ev.payload
            {
                if s == scope {
                    return Some(message);
                }
            }
        }
        None
    })
    .await;
    assert_eq!(named.as_deref(), Some(posted.as_str()));

    engine.shutdown().await;
}

/// Words right after thinking go out at once: the switch of kind is a frame
/// boundary of its own, so the first frame is thinking alone and the words
/// ride the next — the fold lands with the words, not a beat later.
#[tokio::test(flavor = "multi_thread")]
async fn words_after_thinking_go_out_at_once_not_a_beat_later() {
    let dir = tempfile::tempdir().unwrap();
    let talker = MockAdapter {
        id: "talker".into(),
        script: Some(vec![
            SessionEvent::Lifecycle(LifecycleEvent::Started),
            SessionEvent::Progress(ProgressEvent::TurnStarted),
            SessionEvent::Progress(ProgressEvent::ThinkingDelta {
                text: "weighing".into(),
            }),
            SessionEvent::Progress(ProgressEvent::TextDelta {
                text: "Muted.".into(),
            }),
        ]),
        ..Default::default()
    };
    let engine = engine_with(&dir, vec![talker, MockAdapter::default()]);
    let ws = engine.workspace();
    core_on(ws, "talker");
    let conversation = start(ws, ConversationOrigin::Workspace);
    let scope = conversation.id.to_string();
    let mut events = engine.events();
    say(&engine, conversation.id, "which palette?", &[]);

    let mut frames: Vec<(String, String)> = Vec::new();
    until("both frames arrived", || {
        while let Ok(ev) = events.try_recv() {
            if let EnginePayload::AgentStreamed {
                scope: s,
                text,
                thinking,
                ..
            } = ev.payload
            {
                if s == scope {
                    frames.push((text, thinking));
                }
            }
        }
        frames.iter().any(|(t, _)| !t.is_empty()).then_some(())
    })
    .await;
    let first_words = frames.iter().position(|(t, _)| !t.is_empty()).unwrap();
    assert!(
        frames[..first_words].iter().any(|(_, th)| !th.is_empty()),
        "the thinking went out in a frame of its own before the words: {frames:?}"
    );
    assert!(
        frames[first_words].1.is_empty(),
        "the frame with the words carries no thinking: {frames:?}"
    );

    engine.shutdown().await;
}

/// A reader that joins mid-turn is given the turn in flight: the words and
/// the thinking so far, by the agent that speaks — and an unknown
/// conversation is not found.
#[tokio::test(flavor = "multi_thread")]
async fn a_reader_joining_mid_turn_reads_the_live_turn() {
    let dir = tempfile::tempdir().unwrap();
    // A harness that starts a turn and never ends it.
    let stuck = MockAdapter {
        id: "stuck".into(),
        script: Some(vec![
            SessionEvent::Lifecycle(LifecycleEvent::Started),
            SessionEvent::Progress(ProgressEvent::TurnStarted),
            SessionEvent::Progress(ProgressEvent::ThinkingDelta {
                text: "weighing it".into(),
            }),
            SessionEvent::Progress(ProgressEvent::TextDelta {
                text: "so far…".into(),
            }),
        ]),
        ..Default::default()
    };
    let engine = engine_with(&dir, vec![stuck, MockAdapter::default()]);
    let ws = engine.workspace();
    core_on(ws, "stuck");
    let conversation = start(ws, ConversationOrigin::Workspace);
    say(&engine, conversation.id, "take your time", &[]);

    let turns = until("the turn in flight is readable", || {
        let turns = conversations::live_turns(engine.inner(), conversation.id).unwrap();
        (!turns.is_empty() && !turns[0].text.is_empty() && !turns[0].thinking.is_empty())
            .then_some(turns)
    })
    .await;
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].agent, "general-agent");
    assert_eq!(turns[0].text, "so far…");
    assert_eq!(turns[0].thinking, "weighing it");
    assert!(turns[0].since > 0);
    assert!(
        ws.messages(&conversation.id.to_string(), None, 10)
            .unwrap()
            .iter()
            .all(|m| m.author != general_pubkey(&engine)),
        "nothing is posted before the turn ends"
    );
    assert!(
        matches!(
            conversations::live_turns(
                engine.inner(),
                ConversationId::from_ulid(ulid::Ulid::from_datetime(std::time::SystemTime::now()))
            ),
            Err(bisa_engine::EngineError::Store(_))
        ),
        "an unknown conversation is not found"
    );

    engine.shutdown().await;
}

/// A live turn's row is parked when the session idles out and ended when its
/// stream closes; archiving or deleting the conversation stops it first.
#[tokio::test(flavor = "multi_thread")]
async fn a_turns_row_is_parked_on_idle_and_stopped_with_its_conversation() {
    let dir = tempfile::tempdir().unwrap();
    // A harness that answers and then stays alive between turns.
    let lingering = MockAdapter {
        script: Some(vec![
            SessionEvent::Lifecycle(LifecycleEvent::Started),
            SessionEvent::Progress(ProgressEvent::TurnStarted),
            SessionEvent::Progress(ProgressEvent::TextDelta {
                text: "here to stay".into(),
            }),
            SessionEvent::Progress(ProgressEvent::TurnEnded),
        ]),
        ..Default::default()
    };
    let engine = Engine::start(
        crate::common::workspace(&dir),
        crate::common::catalog_with(vec![lingering]),
        EngineConfig {
            idle_ttl: Duration::from_millis(300),
            ..crate::common::design_off_config()
        },
    )
    .unwrap();
    let ws = engine.workspace();
    core_on(ws, "mock");
    let general = general_pubkey(&engine);
    let conversation = start(ws, ConversationOrigin::Workspace);
    let mut events = engine.events();

    say(&engine, conversation.id, "stay a while", &[]);
    until("the reply", || {
        said_by(&engine, conversation.id, &general)
            .into_iter()
            .next()
    })
    .await;
    let row = until("the turn's row", || {
        engine
            .inner()
            .presence
            .snapshot()
            .into_iter()
            .find(|r| r.conversation == Some(conversation.id))
    })
    .await;
    let session_id = row.session_id.unwrap().to_string();
    let parked = until("the idle session parks its row", || {
        ws.session_by_id(&session_id)
            .unwrap()
            .filter(|r| r.status == SessionStatus::Parked)
    })
    .await;
    assert!(parked.parked_at.is_some());

    // Renaming and archiving reach the bus; archiving stops the turns.
    conversations::rename(engine.inner(), conversation.id, Some("Kept")).unwrap();
    conversations::set_archived(engine.inner(), conversation.id, true)
        .await
        .unwrap();
    let mut seen = Vec::new();
    until("the bus says renamed and archived", || {
        while let Ok(ev) = events.try_recv() {
            if let EnginePayload::ConversationChanged { id, change } = ev.payload {
                if id == conversation.id {
                    seen.push(change);
                }
            }
        }
        (seen.contains(&ConversationChange::Renamed)
            && seen.contains(&ConversationChange::Archived))
        .then_some(())
    })
    .await;
    assert!(ws.get_conversation(conversation.id).unwrap().archived);
    assert!(
        ws.list_conversations(&ConversationFilter {
            archived: Some(false),
            limit: 10,
            ..Default::default()
        })
        .unwrap()
        .is_empty(),
        "an archived conversation is out of the live list"
    );

    conversations::delete(engine.inner(), conversation.id)
        .await
        .unwrap();
    assert!(ws.get_conversation(conversation.id).is_err());
    assert!(
        engine
            .inner()
            .presence
            .snapshot()
            .iter()
            .all(|r| r.conversation != Some(conversation.id) || !r.state.is_live()),
        "no live turn of a deleted conversation"
    );
    until("the bus says deleted", || {
        while let Ok(ev) = events.try_recv() {
            if let EnginePayload::ConversationChanged { id, change } = ev.payload {
                if id == conversation.id && change == ConversationChange::Deleted {
                    return Some(());
                }
            }
        }
        None
    })
    .await;

    engine.shutdown().await;
}

/// Starting a conversation through the engine tells the bus, scoped to the
/// goal when the origin is one.
#[tokio::test(flavor = "multi_thread")]
async fn a_conversation_started_through_the_engine_is_announced_on_its_goal() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![MockAdapter::default()]);
    let ws = engine.workspace();
    let goal = ws.create_goal(NewGoal::captured("announce me")).unwrap();
    let mut events = engine.events();
    let made = conversations::create(
        engine.inner(),
        NewConversation {
            origin: ConversationOrigin::Goal { id: goal.id },
            title: Some("Planning".into()),
            mode: bisa_core::ConversationMode::Auto,
        },
    )
    .unwrap();
    assert_eq!(made.title.as_deref(), Some("Planning"));
    let announced = until("the bus says created", || {
        while let Ok(ev) = events.try_recv() {
            if let EnginePayload::ConversationCreated { id, origin } = ev.payload {
                if id == made.id {
                    return Some((ev.goal, origin));
                }
            }
        }
        None
    })
    .await;
    assert_eq!(announced.0, Some(goal.id), "scoped to the goal it is about");
    assert_eq!(announced.1, ConversationOrigin::Goal { id: goal.id });
    assert_eq!(
        conversations::of(
            engine.inner(),
            &ConversationOrigin::Goal { id: goal.id },
            10
        )
        .unwrap()
        .len(),
        1
    );
    assert!(
        conversations::create(
            engine.inner(),
            NewConversation {
                origin: ConversationOrigin::Goal {
                    id: bisa_core::GoalId::from_ulid(ulid::Ulid::from_parts(1, 1))
                },
                title: None,
                mode: bisa_core::ConversationMode::Auto,
            },
        )
        .is_err(),
        "an origin that is not here is refused"
    );
    engine.shutdown().await;
}
