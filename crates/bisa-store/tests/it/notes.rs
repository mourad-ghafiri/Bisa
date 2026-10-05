//! Notes: a local scratchpad with compare-and-swap edits and unguarded appends.

use bisa_core::{
    DeletableChannel, Note, OwnerScope, RosterPolicy, Tags, WorkflowOrigin, MAX_NOTE_BYTES,
};
use bisa_store::{
    body_hash, MemoryKeyStore, NewNote, NewProject, NewWorkflow, NotePatch, OwnerFilter, Paths,
    StoreError, Workspace,
};

fn ws() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    (dir, ws)
}

fn note(scope: OwnerScope, title: &str, body: &str) -> NewNote {
    NewNote {
        scope,
        title: title.into(),
        body: body.into(),
    }
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

fn channel(ws: &Workspace, name: &str) -> bisa_core::Channel {
    ws.create_channel(
        name,
        None,
        RosterPolicy::Listed {
            agents: vec![],
            teams: vec![],
            humans: vec![],
        },
        Tags::default(),
    )
    .unwrap()
}

/// The ids a filter lists, in the order it lists them.
fn listed(ws: &Workspace, filter: OwnerFilter) -> Vec<bisa_core::NoteId> {
    ws.list_notes(filter)
        .unwrap()
        .into_iter()
        .map(|n| n.id)
        .collect()
}

#[test]
fn a_note_round_trips_through_every_scope_and_lives_with_what_it_is_about() {
    let (_dir, ws) = ws();
    let goal = ws.create_goal(bisa_store::NewGoal::captured("g")).unwrap();
    let project = ws
        .create_project(NewProject::managed("web").unwrap())
        .unwrap();
    let root = ws.paths().notes_dir();
    let wf = workflow(&ws, "Release");
    let ch = channel(&ws, "engineering");
    let scopes = [
        OwnerScope::Workspace,
        OwnerScope::Goal { id: goal.id },
        OwnerScope::Project { id: project.id },
        OwnerScope::Workflow { id: wf.id },
        OwnerScope::Channel { id: ch.id.clone() },
        OwnerScope::Node,
    ];
    for scope in scopes {
        let n = ws.create_note(note(scope.clone(), "t", "b")).unwrap();
        assert_eq!(ws.get_note(n.id).unwrap(), n);
        assert_eq!(
            ws.list_notes(OwnerFilter::Scope(scope)).unwrap(),
            vec![n.clone()]
        );
    }
    for dir in [
        Paths::scoped_dir(
            &ws.paths().notes_dir(),
            "workflow",
            Some(&wf.id.to_string()),
        ),
        Paths::scoped_dir(&ws.paths().notes_dir(), "channel", Some(ch.id.as_str())),
        Paths::scoped_dir(&ws.paths().notes_dir(), "node", None),
    ] {
        assert!(dir.starts_with(&root), "{}", dir.display());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    }
    // Every note is a Markdown file under the one notes repository root,
    // in its scope's directory — never in a goal's folder, a project's
    // record folder, or the project's own tree.
    let project_notes = Paths::scoped_dir(
        &ws.paths().notes_dir(),
        "project",
        Some(project.slug.as_str()),
    );
    assert!(project_notes.starts_with(&root));
    assert!(!project_notes.starts_with(ws.paths().projects_dir()));
    assert_eq!(std::fs::read_dir(&project_notes).unwrap().count(), 1);
    assert!(
        !ws.project_root_path(&project).exists(),
        "nothing was written into the project's own tree"
    );
    assert!(
        !ws.paths().goal(goal.id).dir().join("notes").exists(),
        "nothing in the goal's folder"
    );
    assert_eq!(
        std::fs::read_dir(Paths::scoped_dir(
            &ws.paths().notes_dir(),
            "goal",
            Some(&goal.id.to_string())
        ))
        .unwrap()
        .count(),
        1
    );
    assert_eq!(
        std::fs::read_dir(Paths::scoped_dir(
            &ws.paths().notes_dir(),
            "workspace",
            None
        ))
        .unwrap()
        .count(),
        1
    );
    let names: Vec<String> = std::fs::read_dir(Paths::scoped_dir(
        &ws.paths().notes_dir(),
        "workspace",
        None,
    ))
    .unwrap()
    .flatten()
    .map(|e| e.file_name().to_string_lossy().to_string())
    .collect();
    assert!(
        names[0].ends_with(".md"),
        "a note is a Markdown file: {names:?}"
    );
}

#[test]
fn a_note_file_is_markdown_with_a_front_matter_block_and_reads_back_as_written() {
    let (_dir, ws) = ws();
    let n = ws
        .create_note(note(
            OwnerScope::Workspace,
            "Why",
            "# Because\n\n---\n\nnot a fence\n",
        ))
        .unwrap();
    let path = Paths::note_file_in(
        &Paths::scoped_dir(&ws.paths().notes_dir(), "workspace", None),
        n.id,
    );
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.starts_with("---\ntitle: Why\npinned: false\n"),
        "{text}"
    );
    assert!(text.ends_with(&n.body));
    assert_eq!(ws.get_note(n.id).unwrap(), n);
    // A file a person wrote by hand in a clone, in the same shape, is a note after a rebuild.
    let other = Paths::note_file_in(
        &Paths::scoped_dir(&ws.paths().notes_dir(), "workspace", None),
        bisa_core::NoteId::from_ulid(ulid::Ulid::from_parts(9, 9)),
    );
    std::fs::write(
        &other,
        "---\ntitle: From a clone\npinned: true\ncreated_at: 5\nupdated_at: 6\n---\nhello\n",
    )
    .unwrap();
    ws.rebuild_index().unwrap();
    let listed = ws
        .list_notes(OwnerFilter::Scope(OwnerScope::Workspace))
        .unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].title, "From a clone", "pinned first");
    assert_eq!(listed[0].body, "hello\n");
}

#[test]
fn a_note_survives_a_rebuild_because_the_file_is_the_truth() {
    let (_dir, ws) = ws();
    let goal = ws.create_goal(bisa_store::NewGoal::captured("g")).unwrap();
    let a = ws
        .create_note(note(OwnerScope::Workspace, "a", "1"))
        .unwrap();
    let b = ws
        .create_note(note(OwnerScope::Goal { id: goal.id }, "b", "2"))
        .unwrap();
    ws.rebuild_index().unwrap();
    assert_eq!(ws.get_note(a.id).unwrap(), a);
    assert_eq!(ws.get_note(b.id).unwrap(), b);
    ws.delete_goal(goal.id).unwrap();
    assert!(ws.get_note(b.id).is_err(), "a goal takes its notes with it");
    ws.rebuild_index().unwrap();
    assert!(ws.get_note(b.id).is_err());
}

#[test]
fn an_edit_that_did_not_read_the_latest_body_is_refused_and_told_what_is_there() {
    let (_dir, ws) = ws();
    let n = ws
        .create_note(note(OwnerScope::Workspace, "t", "v1"))
        .unwrap();
    let h1 = body_hash("v1");
    let n2 = ws
        .update_note(
            n.id,
            NotePatch {
                body: Some("v2".into()),
                ..Default::default()
            },
            Some(&h1),
        )
        .unwrap();
    assert_eq!(n2.body, "v2");
    match ws.update_note(
        n.id,
        NotePatch {
            body: Some("v3".into()),
            ..Default::default()
        },
        Some(&h1),
    ) {
        Err(StoreError::EditConflict {
            what,
            current,
            current_hash,
        }) => {
            assert_eq!(what, "note");
            assert_eq!(current, serde_json::Value::String("v2".into()));
            assert_eq!(current_hash, body_hash("v2"));
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        ws.update_note(
            n.id,
            NotePatch {
                body: Some("x".into()),
                ..Default::default()
            },
            None
        ),
        Err(StoreError::EditConflict { .. })
    ));
    // A title or a pin needs no hash: it cannot lose a paragraph.
    let n3 = ws
        .update_note(
            n.id,
            NotePatch {
                title: Some("  new  ".into()),
                pinned: Some(true),
                ..Default::default()
            },
            None,
        )
        .unwrap();
    assert_eq!(n3.title, "new");
    assert!(n3.pinned);
}

#[test]
fn appending_never_loses_what_was_there() {
    let (_dir, ws) = ws();
    let n = ws
        .create_note(note(OwnerScope::Workspace, "t", ""))
        .unwrap();
    let n = ws.append_note(n.id, "first\n").unwrap();
    assert_eq!(
        n.body, "first",
        "an empty note does not start with blank lines"
    );
    let n = ws.append_note(n.id, "second").unwrap();
    assert_eq!(n.body, "first\n\nsecond");
    assert!(ws.append_note(n.id, "   ").is_err());
    assert!(ws.append_note(n.id, &"x".repeat(MAX_NOTE_BYTES)).is_err());
}

#[test]
fn a_note_leaves_nothing_on_the_wire_and_is_deleted_whole() {
    let (_dir, ws) = ws();
    let mut rx = ws.subscribe_store_events();
    let n = ws
        .create_note(note(OwnerScope::Workspace, "t", "b"))
        .unwrap();
    assert!(rx.try_recv().is_err(), "a note emits no store event");
    let path = Paths::note_file_in(
        &Paths::scoped_dir(&ws.paths().notes_dir(), "workspace", None),
        n.id,
    );
    assert!(path.exists());
    ws.delete_note(n.id).unwrap();
    assert!(!path.exists());
    assert!(ws.get_note(n.id).is_err());
    ws.rebuild_index().unwrap();
    assert!(ws
        .list_notes(OwnerFilter::Scope(OwnerScope::Workspace))
        .unwrap()
        .is_empty());
}

#[test]
fn a_note_needs_a_real_scope_a_title_and_a_bounded_body() {
    let (_dir, ws) = ws();
    let ghost = bisa_core::GoalId::from_ulid(ulid::Ulid::from_parts(9, 9));
    assert!(ws
        .create_note(note(OwnerScope::Goal { id: ghost }, "t", "b"))
        .is_err());
    assert!(ws
        .create_note(note(OwnerScope::Workspace, "  ", "b"))
        .is_err());
    let huge = "x".repeat(MAX_NOTE_BYTES + 1);
    assert!(ws
        .create_note(note(OwnerScope::Workspace, "t", &huge))
        .is_err());
    let ok: Note = ws
        .create_note(note(OwnerScope::Workspace, "t", "b"))
        .unwrap();
    assert!(ok.validate().is_ok());
}

#[test]
fn two_writers_at_once_lose_nothing() {
    // The person's editor and an agent's tool each read the note, check it
    // and write it back: under the one writer's lock neither writes the
    // body as it was before the other, so every block lands.
    let (_d, ws) = ws();
    let n = ws
        .create_note(note(OwnerScope::Workspace, "Shared", "first\n"))
        .unwrap();
    std::thread::scope(|s| {
        for who in ["a", "b"] {
            let ws = &ws;
            s.spawn(move || {
                for i in 0..20 {
                    ws.append_note(n.id, &format!("{who}-{i}")).unwrap();
                }
            });
        }
    });
    let body = ws.get_note(n.id).unwrap().body;
    assert!(body.starts_with("first"), "{body}");
    for who in ["a", "b"] {
        for i in 0..20 {
            assert!(
                body.contains(&format!("{who}-{i}")),
                "{who}-{i} was lost: {body}"
            );
        }
    }
}

#[test]
fn listing_puts_pinned_notes_first_and_the_recently_touched_above_the_rest() {
    let (_dir, ws) = ws();
    let a = ws
        .create_note(note(OwnerScope::Workspace, "a", ""))
        .unwrap();
    let b = ws
        .create_note(note(OwnerScope::Workspace, "b", ""))
        .unwrap();
    let c = ws
        .create_note(note(OwnerScope::Workspace, "c", ""))
        .unwrap();
    ws.update_note(
        a.id,
        NotePatch {
            pinned: Some(true),
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let ids = listed(&ws, OwnerFilter::Scope(OwnerScope::Workspace));
    assert_eq!(ids[0], a.id, "pinned first");
    assert!(ids.contains(&b.id) && ids.contains(&c.id));
}

#[test]
fn a_listing_is_every_note_one_kinds_notes_or_one_scopes_pinned_first_across_all_of_them() {
    let (_dir, ws) = ws();
    let shop = ws
        .create_project(NewProject::managed("shop").unwrap())
        .unwrap();
    let site = ws
        .create_project(NewProject::managed("site").unwrap())
        .unwrap();
    let goal = ws.create_goal(bisa_store::NewGoal::captured("g")).unwrap();
    let wf = workflow(&ws, "Release");
    let ws_note = ws
        .create_note(note(OwnerScope::Workspace, "workspace", ""))
        .unwrap();
    let shop_note = ws
        .create_note(note(OwnerScope::Project { id: shop.id }, "shop", ""))
        .unwrap();
    let site_note = ws
        .create_note(note(OwnerScope::Project { id: site.id }, "site", ""))
        .unwrap();
    let goal_note = ws
        .create_note(note(OwnerScope::Goal { id: goal.id }, "goal", ""))
        .unwrap();
    let wf_note = ws
        .create_note(note(OwnerScope::Workflow { id: wf.id }, "workflow", ""))
        .unwrap();
    let node_note = ws.create_note(note(OwnerScope::Node, "node", "")).unwrap();
    // A pin on a project's note puts it first in *every* listing that admits
    // it — the tab's order and the All tab's order are the one rule.
    ws.update_note(
        shop_note.id,
        NotePatch {
            pinned: Some(true),
            ..Default::default()
        },
        None,
    )
    .unwrap();

    let all = listed(&ws, OwnerFilter::All);
    assert_eq!(all.len(), 6, "every note, whatever it is about");
    assert_eq!(all[0], shop_note.id, "pinned first across scopes");
    for id in [
        ws_note.id,
        site_note.id,
        goal_note.id,
        wf_note.id,
        node_note.id,
    ] {
        assert!(all.contains(&id));
    }

    let projects = listed(&ws, OwnerFilter::Kind("project"));
    assert_eq!(
        projects,
        vec![shop_note.id, site_note.id],
        "one kind, every record of it"
    );
    assert_eq!(listed(&ws, OwnerFilter::Kind("goal")), vec![goal_note.id]);
    assert_eq!(listed(&ws, OwnerFilter::Kind("workflow")), vec![wf_note.id]);
    assert_eq!(listed(&ws, OwnerFilter::Kind("channel")), vec![]);
    assert_eq!(listed(&ws, OwnerFilter::Kind("node")), vec![node_note.id]);
    assert_eq!(
        listed(&ws, OwnerFilter::Scope(OwnerScope::Project { id: site.id })),
        vec![site_note.id],
        "one scope alone"
    );
    // The kind filter is a filter over the same rows the rebuild produces.
    ws.rebuild_index().unwrap();
    assert_eq!(listed(&ws, OwnerFilter::All).len(), 6);
    assert_eq!(listed(&ws, OwnerFilter::Kind("project")).len(), 2);
}

#[test]
fn a_workflow_and_a_channel_take_their_notes_with_them_and_a_ghost_of_either_takes_none() {
    let (_dir, ws) = ws();
    let wf = workflow(&ws, "Release");
    let ch = channel(&ws, "engineering");
    let on_wf = ws
        .create_note(note(OwnerScope::Workflow { id: wf.id }, "w", ""))
        .unwrap();
    let on_ch = ws
        .create_note(note(OwnerScope::Channel { id: ch.id.clone() }, "c", ""))
        .unwrap();
    let kept = ws.create_note(note(OwnerScope::Node, "n", "")).unwrap();
    ws.rebuild_index().unwrap();
    assert_eq!(
        ws.get_note(on_wf.id).unwrap(),
        on_wf,
        "a rebuild finds a workflow's note"
    );
    assert_eq!(ws.get_note(on_ch.id).unwrap(), on_ch, "and a channel's");

    ws.delete_workflow(wf.id).unwrap();
    assert!(
        ws.get_note(on_wf.id).is_err(),
        "a workflow takes its notes with it"
    );
    assert!(!Paths::scoped_dir(
        &ws.paths().notes_dir(),
        "workflow",
        Some(&wf.id.to_string())
    )
    .exists());
    ws.delete_channel(DeletableChannel::new(ch.clone()).unwrap())
        .unwrap();
    assert!(
        ws.get_note(on_ch.id).is_err(),
        "a channel takes its notes with it"
    );
    assert!(!Paths::scoped_dir(&ws.paths().notes_dir(), "channel", Some(ch.id.as_str())).exists());
    assert_eq!(listed(&ws, OwnerFilter::All), vec![kept.id]);

    let ghost_wf = bisa_core::WorkflowId::from_ulid(ulid::Ulid::from_parts(9, 9));
    assert!(
        ws.create_note(note(OwnerScope::Workflow { id: ghost_wf }, "t", "b"))
            .is_err(),
        "a note on a workflow that does not exist is refused"
    );
    let ghost_ch = bisa_core::ChannelId::new("nowhere").unwrap();
    assert!(
        ws.create_note(note(OwnerScope::Channel { id: ghost_ch }, "t", "b"))
            .is_err(),
        "and on a channel that does not exist"
    );
}
