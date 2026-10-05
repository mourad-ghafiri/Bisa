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
    AgentId, ConversationId, ConversationOrigin, MessageBody, PrincipalId, WorkstreamId,
};
use bisa_engine::events::{ConversationChange, EnginePayload};
use bisa_engine::registry::SessionKind;
use bisa_engine::{conversations, Engine, EngineConfig};
use bisa_harness::mock::MockAdapter;
use bisa_harness::{LifecycleEvent, ProgressEvent, SessionEvent};
use bisa_store::{
    ConversationFilter, NewConversation, NewGoal, NewProject, PostOrigin, SessionStatus, Workspace,
};
use std::time::Duration;

/// Point the general agent at the mock harness, so an unaddressed message
/// wakes a session these tests control.
fn core_on(ws: &Workspace, harness: &str) {
    let mut def = ws.get_agent(&AgentId::general()).unwrap();
    def.harness = harness.into();
    ws.update_agent(def).unwrap();
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
    engine
        .workspace()
        .get_agent(&AgentId::general())
        .unwrap()
        .pubkey
        .as_hex()
        .to_string()
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
/// agent's own scratch, each with the sentence that says what it is about.
#[tokio::test(flavor = "multi_thread")]
async fn every_origin_places_its_turn_and_frames_it() {
    let dir = tempfile::tempdir().unwrap();
    let mock = MockAdapter::default();
    let launches = mock.launches.clone();
    let engine = engine_with(&dir, vec![mock]);
    let ws = engine.workspace();
    core_on(ws, "mock");
    let general = general_pubkey(&engine);
    let project = ws
        .create_project(NewProject::managed("api").unwrap())
        .unwrap();
    let primary = ws.primary_workstream(project.id).unwrap();
    std::fs::create_dir_all(ws.checkout_in(&project, &primary)).unwrap();
    let goal = ws
        .create_goal(NewGoal::captured("Dark mode for the settings screen"))
        .unwrap();
    let workflow = ws
        .create_workflow(
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
        .unwrap();
    let scratch = ws.paths().agent(&AgentId::general()).scratch();
    let note = ws
        .create_note(bisa_store::NewNote {
            scope: bisa_core::OwnerScope::Workspace,
            title: "The door".into(),
            body: "Sand it.\n".into(),
        })
        .unwrap();

    let cases: Vec<(ConversationOrigin, std::path::PathBuf, &str)> = vec![
        // The mock takes the platform's tools, so the note's frame names
        // them: read first, rewrite at the hash read, add under its name.
        (
            ConversationOrigin::Note { id: note.id },
            scratch.clone(),
            "This conversation is about the note `The door`, open beside the person in the notes overlay. Read it with note_read",
        ),
        (
            ConversationOrigin::Project { id: project.id },
            ws.checkout_in(&project, &primary),
            "You are working in project `api`, in its own tree",
        ),
        (
            ConversationOrigin::Goal { id: goal.id },
            scratch.clone(),
            "This conversation is about the goal `Dark mode for the settings screen`",
        ),
        (
            ConversationOrigin::Workflow { id: workflow.id },
            scratch.clone(),
            "This conversation is about the workflow `Release`",
        ),
        (
            ConversationOrigin::Workspace,
            scratch.clone(),
            "This conversation is about the workspace as a whole",
        ),
        (
            ConversationOrigin::Node,
            scratch.clone(),
            "This conversation is about this machine's node",
        ),
    ];
    for (i, (origin, cwd, frame)) in cases.iter().enumerate() {
        let conversation = start(ws, origin.clone());
        say(&engine, conversation.id, "hello there", &[]);
        let reply = until("the general agent answers", || {
            said_by(&engine, conversation.id, &general)
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
    let mut workflow_agent = ws.get_agent(&AgentId::workflow()).unwrap();
    workflow_agent.harness = "mock".into();
    ws.update_agent(workflow_agent.clone()).unwrap();
    let workflow_pk = workflow_agent.pubkey.as_hex().to_string();
    let mention = PrincipalId::new(workflow_pk.clone()).unwrap();
    let general = general_pubkey(&engine);
    let wf = ws
        .create_workflow(
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
        .unwrap();

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
