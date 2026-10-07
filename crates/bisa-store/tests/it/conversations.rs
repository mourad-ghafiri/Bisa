//! Conversations: a saved exchange with agents, with an origin — made, listed,
//! searched, titled, summarised, archived, deleted, and gone with what it is
//! about.

use bisa_core::{
    AgentId, Conversation, ConversationOrigin, MessageBody, OwnerScope, WorkflowOrigin,
    WorkstreamId, MAX_CONVERSATION_TITLE_CHARS,
};
use bisa_store::{
    ConversationFilter, MemoryKeyStore, NewAgent, NewConversation, NewDrawing, NewGoal, NewNote,
    NewProject, NewWorkflow, PostOrigin, SessionKind, SessionRow, SessionStatus, StoreError,
    Workspace,
};

fn ws() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    (dir, ws)
}

fn agent(ws: &Workspace, name: &str) -> bisa_core::Agent {
    ws.add_agent(NewAgent {
        name: name.into(),
        system_prompt: format!("You are {name}."),
        harness: "claude-code".into(),
        ..Default::default()
    })
    .unwrap()
}

fn workflow(ws: &Workspace, name: &str) -> bisa_core::Workflow {
    ws.create_workflow(
        NewWorkflow {
            name: name.into(),
            description: String::new(),
            inputs: vec![],
            steps: vec![bisa_core::Step {
                id: bisa_core::StepId::new("end").unwrap(),
                name: "End".into(),
                kind: bisa_core::StepKind::End {
                    finish: bisa_core::Finish::Done,
                },
                then: vec![],
                boundaries: vec![],
                join: bisa_core::Join::All,
                on_fail: bisa_core::OnFail::Fail,
                retries: 0,
                max_visits: bisa_core::DEFAULT_MAX_VISITS,
                position: None,
            }],
            tags: Default::default(),
            decision_making: false,
        },
        WorkflowOrigin::Workspace,
    )
    .unwrap()
}

fn make(ws: &Workspace, origin: ConversationOrigin, title: Option<&str>) -> Conversation {
    ws.create_conversation(NewConversation {
        origin,
        title: title.map(str::to_string),
        mode: bisa_core::ConversationMode::Auto,
    })
    .unwrap()
}

fn say(ws: &Workspace, c: &Conversation, text: &str) -> String {
    ws.post_message(
        &c.id.to_string(),
        MessageBody::post(text),
        None,
        &[],
        &[],
        None,
        PostOrigin::Asked,
    )
    .unwrap()
}

fn say_as(ws: &Workspace, c: &Conversation, agent: &AgentId, text: &str) -> String {
    ws.post_message(
        &c.id.to_string(),
        MessageBody::post(text),
        None,
        &[],
        &[],
        Some(agent),
        PostOrigin::Asked,
    )
    .unwrap()
}

fn ids(rows: &[bisa_store::ConversationRow]) -> Vec<String> {
    rows.iter().map(|r| r.id.clone()).collect()
}

#[test]
fn a_conversation_is_made_for_every_origin_and_an_origin_that_is_not_here_is_refused() {
    let (_d, ws) = ws();
    let goal = ws.create_goal(NewGoal::captured("dark mode")).unwrap();
    let wf = workflow(&ws, "Release");
    let project = ws
        .create_project(NewProject::managed("web-app").unwrap())
        .unwrap();
    let primary = WorkstreamId::primary_of(project.id);
    let drawing = ws
        .create_drawing(NewDrawing {
            scope: OwnerScope::Workspace,
            title: "Orders".into(),
            scene: None,
        })
        .unwrap();
    let note = ws
        .create_note(NewNote {
            scope: OwnerScope::Workspace,
            title: "Why".into(),
            body: "because\n".into(),
        })
        .unwrap();
    let origins = [
        ConversationOrigin::Node,
        ConversationOrigin::Workspace,
        ConversationOrigin::Goal { id: goal.id },
        ConversationOrigin::Workflow { id: wf.id },
        ConversationOrigin::Project { id: project.id },
        ConversationOrigin::Workstream {
            id: primary,
            project: project.id,
        },
        ConversationOrigin::Drawing { id: drawing.id },
        ConversationOrigin::Note { id: note.id },
    ];
    assert_eq!(
        origins.iter().map(|o| o.kind()).collect::<Vec<_>>(),
        ConversationOrigin::KINDS,
        "every kind is made here"
    );
    for origin in &origins {
        let c = make(&ws, origin.clone(), None);
        assert_eq!(&c.origin, origin);
        assert_eq!(c.title, None);
        assert!(!c.archived);
        assert_eq!(
            ws.get_conversation(c.id).unwrap(),
            c,
            "{origin:?} reads back"
        );
        let row = ws.conversation_row(c.id).unwrap();
        assert_eq!(row.origin_kind, origin.kind());
        assert_eq!(row.origin_id, origin.id());
        assert_eq!(row.project, origin.project().map(|p| p.to_string()));
        assert_eq!(row.message_count, 0);
        assert_eq!(row.last_message_at, None);
        assert!(row.agents.is_empty());
        assert_eq!(
            ws.resolve_scope(&c.id.to_string()).unwrap().kind,
            bisa_core::ScopeKind::Conversation
        );
    }
    let all = ws
        .list_conversations(&ConversationFilter::all(100))
        .unwrap();
    assert_eq!(all.len(), origins.len());

    let stranger = ulid::Ulid::from_parts(1, 1);
    for origin in [
        ConversationOrigin::Goal {
            id: bisa_core::GoalId::from_ulid(stranger),
        },
        ConversationOrigin::Workflow {
            id: bisa_core::WorkflowId::from_ulid(stranger),
        },
        ConversationOrigin::Project {
            id: bisa_core::ProjectId::from_ulid(stranger),
        },
        ConversationOrigin::Workstream {
            id: WorkstreamId::from_ulid(stranger),
            project: project.id,
        },
        ConversationOrigin::Drawing {
            id: bisa_core::DrawingId::from_ulid(stranger),
        },
        ConversationOrigin::Note {
            id: bisa_core::NoteId::from_ulid(stranger),
        },
    ] {
        assert!(
            ws.create_conversation(NewConversation {
                origin: origin.clone(),
                title: None,
                mode: bisa_core::ConversationMode::Auto,
            })
            .is_err(),
            "{origin:?} names nothing here"
        );
    }
}

#[test]
fn a_workstream_origin_names_its_own_project_and_nothing_else() {
    let (_d, ws) = ws();
    let web = ws
        .create_project(NewProject::managed("web").unwrap())
        .unwrap();
    let api = ws
        .create_project(NewProject::managed("api").unwrap())
        .unwrap();
    let err = ws
        .create_conversation(NewConversation {
            origin: ConversationOrigin::Workstream {
                id: WorkstreamId::primary_of(web.id),
                project: api.id,
            },
            title: None,
            mode: bisa_core::ConversationMode::Auto,
        })
        .unwrap_err();
    assert!(
        matches!(err, StoreError::Invalid(ref m) if m.to_string().contains("belongs to project")),
        "{err}"
    );
}

#[test]
fn a_list_is_newest_activity_first_and_narrows_by_origin_agent_archived_and_words() {
    let (_d, ws) = ws();
    let dev = agent(&ws, "Developer");
    let reviewer = agent(&ws, "Reviewer");
    let goal = ws.create_goal(NewGoal::captured("dark mode")).unwrap();
    let project = ws
        .create_project(NewProject::managed("web").unwrap())
        .unwrap();
    // Made first, so that within one second it ranks after the two that
    // moved: a conversation nothing was said in ranks by its birth.
    let quiet = make(&ws, ConversationOrigin::Workspace, Some("Quiet one"));
    let in_goal = make(
        &ws,
        ConversationOrigin::Goal { id: goal.id },
        Some("Palette"),
    );
    let in_project = make(&ws, ConversationOrigin::Project { id: project.id }, None);

    say(&ws, &in_goal, "which palette do we ship?");
    say_as(&ws, &in_goal, &dev.id, "the muted one, as the brief says");
    // A mention counts the agent in, whether or not it answered.
    ws.post_message(
        &in_project.id.to_string(),
        MessageBody::post("please review the header\nsecond line"),
        None,
        std::slice::from_ref(&reviewer.pubkey),
        &[],
        None,
        PostOrigin::Asked,
    )
    .unwrap();

    let all = ws.list_conversations(&ConversationFilter::all(10)).unwrap();
    assert_eq!(
        ids(&all),
        vec![
            in_project.id.to_string(),
            in_goal.id.to_string(),
            quiet.id.to_string()
        ],
        "the one that moved last is first; one nothing was said in ranks by its birth"
    );
    let project_row = &all[0];
    assert_eq!(project_row.message_count, 1);
    assert_eq!(
        project_row.first_line.as_deref(),
        Some("please review the header")
    );
    assert_eq!(project_row.agents, vec![reviewer.id.to_string()]);
    let goal_row = &all[1];
    assert_eq!(goal_row.message_count, 2);
    assert_eq!(goal_row.agents, vec![dev.id.to_string()]);
    assert_eq!(
        goal_row.first_line.as_deref(),
        Some("which palette do we ship?")
    );

    let by_origin = ws
        .list_conversations(&ConversationFilter::of(
            &ConversationOrigin::Goal { id: goal.id },
            10,
        ))
        .unwrap();
    assert_eq!(ids(&by_origin), vec![in_goal.id.to_string()]);
    let by_kind = ws
        .list_conversations(&ConversationFilter {
            origin_kind: Some("workspace".into()),
            limit: 10,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(ids(&by_kind), vec![quiet.id.to_string()]);
    let by_agent = ws
        .list_conversations(&ConversationFilter {
            agent: Some(reviewer.id.clone()),
            limit: 10,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(ids(&by_agent), vec![in_project.id.to_string()]);

    // Words: in the messages through the full-text index, or in the title.
    let by_words = ws
        .list_conversations(&ConversationFilter {
            query: Some("palette".into()),
            limit: 10,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(ids(&by_words), vec![in_goal.id.to_string()]);
    let by_title = ws
        .list_conversations(&ConversationFilter {
            query: Some("quiet".into()),
            limit: 10,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(ids(&by_title), vec![quiet.id.to_string()]);
    let nothing = ws
        .list_conversations(&ConversationFilter {
            query: Some("unicorn OR (".into()),
            limit: 10,
            ..Default::default()
        })
        .unwrap();
    assert!(nothing.is_empty(), "a query is words, never operators");

    // Archived ones sort last and are filtered on request.
    ws.set_conversation_archived(in_project.id, true).unwrap();
    let all = ws.list_conversations(&ConversationFilter::all(10)).unwrap();
    assert_eq!(ids(&all).last().unwrap(), &in_project.id.to_string());
    assert!(!all[0].archived && all.last().unwrap().archived);
    let live = ws
        .list_conversations(&ConversationFilter {
            archived: Some(false),
            limit: 10,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(live.len(), 2);
    let put_away = ws
        .list_conversations(&ConversationFilter {
            archived: Some(true),
            limit: 10,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(ids(&put_away), vec![in_project.id.to_string()]);
    assert!(
        ws.set_conversation_archived(in_project.id, true)
            .unwrap()
            .archived,
        "archiving twice is the same record"
    );
    let limited = ws.list_conversations(&ConversationFilter::all(1)).unwrap();
    assert_eq!(limited.len(), 1);
}

/// A project's list is every conversation standing in it — the project's
/// own and every checkout's — and nothing of another project's or a goal's.
#[test]
fn a_project_lists_its_own_and_every_checkouts_conversations_and_no_others() {
    let (_d, ws) = ws();
    let web = ws
        .create_project(NewProject::managed("web").unwrap())
        .unwrap();
    let api = ws
        .create_project(NewProject::managed("api").unwrap())
        .unwrap();
    let goal = ws.create_goal(NewGoal::captured("dark mode")).unwrap();
    // A second checkout of `web`: a worktree beside the primary.
    let worktree = bisa_core::Workstream {
        id: WorkstreamId::from_ulid(ulid::Ulid::from_parts(7, 7)),
        project: web.id,
        name: Some("feature".into()),
        note: None,
        pinned: false,
        kind: bisa_core::WorkstreamKind::Worktree {
            branch: "feature/checkout".into(),
            base: "main".into(),
        },
        goal: None,
        work_item: None,
        agent: None,
        state: bisa_core::WorkstreamState::Open,
        created_at: 9,
        board: Default::default(),
    };
    ws.put_workstream(&worktree).unwrap();

    let about_project = make(&ws, ConversationOrigin::Project { id: web.id }, Some("Web"));
    let about_primary = make(
        &ws,
        ConversationOrigin::Workstream {
            id: WorkstreamId::primary_of(web.id),
            project: web.id,
        },
        None,
    );
    let about_worktree = make(
        &ws,
        ConversationOrigin::Workstream {
            id: worktree.id,
            project: web.id,
        },
        Some("Feature"),
    );
    let _elsewhere = make(&ws, ConversationOrigin::Project { id: api.id }, None);
    let _goals = make(&ws, ConversationOrigin::Goal { id: goal.id }, None);
    say(&ws, &about_primary, "the primary moved last");

    let webs = ws
        .list_conversations(&ConversationFilter::of_project(web.id, 10))
        .unwrap();
    // The three of web and nothing of api's or the goal's. Two were made in
    // the same second the primary's moved, so the order among them is the
    // tie-break's; the order rule itself is the other list test's.
    let mut listed = ids(&webs);
    listed.sort();
    let mut expected = vec![
        about_primary.id.to_string(),
        about_worktree.id.to_string(),
        about_project.id.to_string(),
    ];
    expected.sort();
    assert_eq!(listed, expected);
    for row in &webs {
        assert_eq!(row.project.as_deref(), Some(web.id.to_string().as_str()));
    }
    // The project filter composes with the rest.
    ws.set_conversation_archived(about_worktree.id, true)
        .unwrap();
    let live = ws
        .list_conversations(&ConversationFilter {
            archived: Some(false),
            ..ConversationFilter::of_project(web.id, 10)
        })
        .unwrap();
    assert_eq!(
        ids(&live),
        vec![about_primary.id.to_string(), about_project.id.to_string()]
    );
    let by_words = ws
        .list_conversations(&ConversationFilter {
            query: Some("feature".into()),
            ..ConversationFilter::of_project(web.id, 10)
        })
        .unwrap();
    assert_eq!(ids(&by_words), vec![about_worktree.id.to_string()]);
    let none = ws
        .list_conversations(&ConversationFilter::of_project(
            bisa_core::ProjectId::from_ulid(ulid::Ulid::from_parts(9, 9)),
            10,
        ))
        .unwrap();
    assert!(
        none.is_empty(),
        "a project with none, or none at all, is empty"
    );
}

#[test]
fn a_title_is_given_taken_away_and_kept_within_its_bound() {
    let (_d, ws) = ws();
    let c = make(&ws, ConversationOrigin::Workspace, None);
    let titled = ws
        .rename_conversation(c.id, Some("  Ship it\nby Friday  "))
        .unwrap();
    assert_eq!(titled.title.as_deref(), Some("Ship it by Friday"));
    assert_eq!(
        ws.conversation_row(c.id).unwrap().title.as_deref(),
        Some("Ship it by Friday")
    );
    let bare = ws.rename_conversation(c.id, None).unwrap();
    assert_eq!(bare.title, None);
    assert!(ws.rename_conversation(c.id, Some("   ")).is_err());
    let long = "x".repeat(MAX_CONVERSATION_TITLE_CHARS + 1);
    assert!(ws.rename_conversation(c.id, Some(&long)).is_err());
    assert!(ws
        .create_conversation(NewConversation {
            origin: ConversationOrigin::Node,
            title: Some(" ".into()),
            mode: bisa_core::ConversationMode::Auto,
        })
        .is_err());
}

#[test]
fn a_posts_thinking_round_trips_and_a_persons_post_has_none() {
    let (_d, ws) = ws();
    let dev = agent(&ws, "Developer");
    let c = make(&ws, ConversationOrigin::Workspace, None);
    let asked = say(&ws, &c, "why is the build red?");
    let answered = ws
        .post_message(
            &c.id.to_string(),
            MessageBody::post_with_thinking(
                "the test fixture points at a port that moved",
                "first read the failure, then the fixture; the port changed in the last commit",
            ),
            None,
            &[],
            &[],
            Some(&dev.id),
            PostOrigin::Asked,
        )
        .unwrap();
    let rows = ws.messages(&c.id.to_string(), None, 10).unwrap();
    assert_eq!(
        rows.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        vec![asked.as_str(), answered.as_str()]
    );
    assert_eq!(rows[0].thinking, None, "a person's post carries none");
    assert_eq!(
        rows[1].thinking.as_deref(),
        Some("first read the failure, then the fixture; the port changed in the last commit"),
        "the agent's reasoning rides with its words"
    );
    assert_eq!(rows[1].body_kind, "post", "two kinds and only two");
    ws.rebuild_index().unwrap();
    let again = ws.messages(&c.id.to_string(), None, 10).unwrap();
    assert_eq!(again[1].id, answered, "still there");
    assert_eq!(again[1].thinking, rows[1].thinking, "a rebuild keeps it");
    assert!(
        ws.post_message(
            &c.id.to_string(),
            MessageBody::post_with_thinking("x", "t".repeat(bisa_core::MAX_THINKING_BYTES + 1)),
            None,
            &[],
            &[],
            Some(&dev.id),
            PostOrigin::Asked,
        )
        .is_err(),
        "thinking past its bound is refused"
    );
}

#[test]
fn the_transcript_tail_is_the_newest_messages_oldest_first() {
    let (_d, ws) = ws();
    let c = make(&ws, ConversationOrigin::Workspace, None);
    for word in ["one", "two", "three", "four"] {
        say(&ws, &c, word);
    }
    let all = ws.transcript_tail(&c.id.to_string(), 50).unwrap();
    assert_eq!(
        all.iter().map(|m| m.content.as_str()).collect::<Vec<_>>(),
        vec!["one", "two", "three", "four"]
    );
    let tail = ws.transcript_tail(&c.id.to_string(), 2).unwrap();
    assert_eq!(
        tail.iter().map(|m| m.content.as_str()).collect::<Vec<_>>(),
        vec!["three", "four"],
        "the bound keeps the newest, oldest first"
    );
    let row = ws.conversation_row(c.id).unwrap();
    assert_eq!(
        row.message_count, 4,
        "the count is every message; nothing is folded"
    );
}

#[test]
fn deleting_a_conversation_removes_its_record_its_log_and_its_rows() {
    let (_d, ws) = ws();
    let dev = agent(&ws, "Developer");
    let c = make(&ws, ConversationOrigin::Workspace, Some("Gone soon"));
    let scope = c.id.to_string();
    say(&ws, &c, "hello");
    say_as(&ws, &c, &dev.id, "hi");
    ws.mark_read(&scope).unwrap();
    let log = ws.paths().conversation_log(&scope).unwrap();
    assert!(log.is_file());

    ws.delete_conversation(c.id).unwrap();
    assert!(!log.exists());
    assert!(ws.get_conversation(c.id).is_err());
    assert!(ws.conversation_row(c.id).is_err());
    assert!(ws.resolve_scope(&scope).is_err());
    assert!(ws.messages(&scope, None, 10).unwrap().is_empty());
    assert!(ws.read_markers().unwrap().is_empty());
    assert!(ws
        .list_conversations(&ConversationFilter::all(10))
        .unwrap()
        .is_empty());
    assert!(
        ws.list_conversations(&ConversationFilter {
            query: Some("hello".into()),
            limit: 10,
            ..Default::default()
        })
        .unwrap()
        .is_empty(),
        "its words left the search"
    );
    assert!(ws.delete_conversation(c.id).is_err(), "twice is not found");
}

#[test]
fn a_thing_takes_its_conversations_with_it_when_it_goes() {
    let (_d, ws) = ws();
    let goal = ws.create_goal(NewGoal::captured("dark mode")).unwrap();
    let wf = workflow(&ws, "Release");
    let project = ws
        .create_project(NewProject::managed("web").unwrap())
        .unwrap();
    let primary = WorkstreamId::primary_of(project.id);
    let of_goal = make(&ws, ConversationOrigin::Goal { id: goal.id }, None);
    let of_wf = make(&ws, ConversationOrigin::Workflow { id: wf.id }, None);
    let of_project = make(&ws, ConversationOrigin::Project { id: project.id }, None);
    let of_checkout = make(
        &ws,
        ConversationOrigin::Workstream {
            id: primary,
            project: project.id,
        },
        None,
    );
    let drawing = ws
        .create_drawing(NewDrawing {
            scope: OwnerScope::Workspace,
            title: "Orders".into(),
            scene: None,
        })
        .unwrap();
    let note = ws
        .create_note(NewNote {
            scope: OwnerScope::Goal { id: goal.id },
            title: "Why".into(),
            body: "because\n".into(),
        })
        .unwrap();
    let of_drawing = make(&ws, ConversationOrigin::Drawing { id: drawing.id }, None);
    let of_note = make(&ws, ConversationOrigin::Note { id: note.id }, None);
    let kept = make(&ws, ConversationOrigin::Workspace, None);
    for c in [
        &of_goal,
        &of_wf,
        &of_project,
        &of_checkout,
        &of_drawing,
        &of_note,
        &kept,
    ] {
        say(&ws, c, "words");
    }

    // A drawing deleted by hand, and a note that leaves with its goal.
    ws.delete_drawing(drawing.id).unwrap();
    assert!(
        ws.get_conversation(of_drawing.id).is_err(),
        "the drawing's conversations left with it"
    );

    ws.delete_goal(goal.id).unwrap();
    assert!(ws.get_conversation(of_goal.id).is_err());
    assert!(
        ws.get_conversation(of_note.id).is_err(),
        "the goal's note left, and its conversations with it"
    );
    assert!(!ws
        .paths()
        .conversation_log(&of_goal.id.to_string())
        .unwrap()
        .exists());
    ws.delete_workflow(wf.id).unwrap();
    assert!(ws.get_conversation(of_wf.id).is_err());
    ws.delete_project(project.id).unwrap();
    assert!(ws.get_conversation(of_project.id).is_err());
    assert!(
        ws.get_conversation(of_checkout.id).is_err(),
        "the primary's conversations left with its record"
    );
    let left = ws.list_conversations(&ConversationFilter::all(10)).unwrap();
    assert_eq!(ids(&left), vec![kept.id.to_string()]);
    assert_eq!(ws.get_conversation(kept.id).unwrap(), kept);
}

#[test]
fn a_rebuild_reproduces_the_conversations_row_for_row_and_a_former_scope_is_none() {
    let (_d, ws) = ws();
    let dev = agent(&ws, "Developer");
    let project = ws
        .create_project(NewProject::managed("web").unwrap())
        .unwrap();
    let c = make(
        &ws,
        ConversationOrigin::Workstream {
            id: WorkstreamId::primary_of(project.id),
            project: project.id,
        },
        Some("Header review"),
    );
    say(&ws, &c, "look at the header");
    say_as(&ws, &c, &dev.id, "done");
    say(&ws, &c, "thanks");
    ws.set_conversation_archived(c.id, true).unwrap();
    let before = ws.list_conversations(&ConversationFilter::all(10)).unwrap();

    ws.rebuild_index().unwrap();
    let after = ws.list_conversations(&ConversationFilter::all(10)).unwrap();
    assert_eq!(after, before);
    assert_eq!(after[0].message_count, 3);
    assert_eq!(after[0].agents, vec![dev.id.to_string()]);
    assert_eq!(after[0].first_line.as_deref(), Some("look at the header"));
    assert!(after[0].archived);
    assert_eq!(ws.transcript_tail(&c.id.to_string(), 50).unwrap().len(), 3);
    assert!(
        ws.resolve_scope(&project.id.to_string()).is_err(),
        "a project's or a workstream's id is not a message scope"
    );
    assert!(ws.list_artifacts(&project.id.to_string(), 5).is_err());
}

#[test]
fn a_session_row_says_its_kind_and_the_conversation_it_is_a_turn_of() {
    let (_d, ws) = ws();
    let c = make(&ws, ConversationOrigin::Workspace, None);
    ws.record_session(&SessionRow {
        id: "turn-1".into(),
        adapter: "mock".into(),
        kind: SessionKind::Conversation,
        conversation: Some(c.id.to_string()),
        status: SessionStatus::Live,
        ..Default::default()
    })
    .unwrap();
    ws.record_session(&SessionRow {
        id: "work-1".into(),
        adapter: "mock".into(),
        status: SessionStatus::Live,
        ..Default::default()
    })
    .unwrap();
    let turn = ws.session_by_id("turn-1").unwrap().unwrap();
    assert_eq!(turn.kind, SessionKind::Conversation);
    assert_eq!(
        turn.conversation.as_deref(),
        Some(c.id.to_string().as_str())
    );
    let work = ws.session_by_id("work-1").unwrap().unwrap();
    assert_eq!(
        work.kind,
        SessionKind::Worker,
        "a row says worker unless it says otherwise"
    );
    assert_eq!(work.conversation, None);

    ws.rebuild_index().unwrap();
    let turn = ws.session_by_id("turn-1").unwrap().unwrap();
    assert_eq!(turn.kind, SessionKind::Conversation);
    assert_eq!(
        turn.conversation.as_deref(),
        Some(c.id.to_string().as_str())
    );

    ws.delete_conversation(c.id).unwrap();
    assert_eq!(
        ws.session_by_id("turn-1").unwrap().unwrap().conversation,
        None,
        "a turn of a conversation that is gone keeps its row and loses the reference"
    );
    assert_eq!(SessionKind::parse("nonsense"), SessionKind::Worker);
    for kind in SessionKind::ALL {
        assert_eq!(SessionKind::parse(kind.as_str()), kind);
    }
    assert!(SessionKind::Worker.is_work() && SessionKind::Terminal.is_work());
    assert!(!SessionKind::Conversation.is_work());
    assert_eq!(
        SessionKind::ALL.len(),
        5,
        "a note's answer is a conversation's turn now; a one-shot ask is the fifth kind"
    );
    assert!(!SessionKind::Guided.is_work());
    assert!(
        !SessionKind::Ask.is_work(),
        "an ask stands in no checkout: never a row of one"
    );
}
