//! The drawing bridge (19 — Drawings): a `draw` op answers a list, a reading,
//! a new drawing and an erasure from the store; a skeleton is parked for the
//! desktop and answered when the desktop says so, with `drawing_changed` on
//! the bus; nobody home is said at once when no desktop has read the list;
//! the workspace's word on who may draw is checked before anything; a
//! conversation about a drawing chooses the drawing, and one about a goal
//! files a new drawing under the goal.

use crate::browser::scout;
use crate::common::{engine_with, intake_roundtrip, until};
use bisa_core::settings::Scope as SettingScope;
use bisa_core::{ConversationOrigin, OwnerScope, Scene, SkillId};
use bisa_engine::drawings::{
    self, DrawAction, DrawResult, DRAW_SKILL, NOBODY_HOME, NOBODY_MAY, NOT_ASSIGNED, NO_DRAWING,
    OFF,
};
use bisa_engine::events::EnginePayload;
use bisa_store::{scene_hash, CatalogKind, DrawingPatch, NewConversation, NewDrawing};
use serde_json::json;

fn errors_of(reply: &serde_json::Value) -> Vec<String> {
    reply["errors"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn box_at(id: &str, x: i64) -> serde_json::Value {
    json!({"id": id, "type": "rectangle", "x": x, "y": 40, "width": 160, "height": 80})
}

fn skeleton(
    drawing: bisa_core::DrawingId,
    elements: Vec<serde_json::Value>,
) -> drawings::DrawRequest {
    drawings::DrawRequest {
        action: DrawAction::Draw,
        drawing: Some(drawing),
        scope: None,
        scope_id: None,
        title: None,
        elements: Some(elements),
        text: None,
        replace: false,
        ids: vec![],
    }
}

/// A `drawing_changed` for one drawing, with its hash, within five seconds.
async fn heard_changed(
    rx: &mut tokio::sync::broadcast::Receiver<bisa_engine::events::EngineEvent>,
    drawing: bisa_core::DrawingId,
) -> Option<String> {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let ev = rx.recv().await.unwrap();
            if let EnginePayload::DrawingChanged {
                drawing: d, hash, ..
            } = &ev.payload
            {
                if *d == drawing.to_string() {
                    return hash.clone();
                }
            }
        }
    })
    .await
    .ok()
}

/// The desktop answered after the op gave up waiting — the canvas was said
/// silent: the answer is nobody's, but the save it carries is announced,
/// so the open canvas and the list learn what the store holds. A late
/// refusal announces nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_late_answer_after_the_wait_is_still_announced() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let inner = engine.inner();
    let drawing = drawings::create(
        inner,
        NewDrawing {
            scope: OwnerScope::Workspace,
            title: "Late".into(),
            scene: None,
        },
    )
    .unwrap();
    let (id, rx) = drawings::ask(
        inner,
        skeleton(drawing.id, vec![box_at("a", 0)]),
        drawings::DrawScope::default(),
    );
    let silent = inner
        .draw
        .wait_for(&id, rx, std::time::Duration::from_millis(20), || {
            DrawResult::refused(drawings::DESKTOP_SILENT)
        })
        .await;
    assert_eq!(silent.error.as_deref(), Some(drawings::DESKTOP_SILENT));
    assert!(
        drawings::DESKTOP_SILENT.contains("drawing_read the drawing first"),
        "a retry reads first, so a save that landed after the wait is not drawn twice"
    );
    // The desktop finishes anyway: it saves through the store, then answers.
    let saved = engine
        .workspace()
        .update_drawing(
            drawing.id,
            DrawingPatch {
                scene: Some(Scene {
                    elements: vec![box_at("a", 0)],
                    ..Scene::default()
                }),
                ..Default::default()
            },
            Some(&scene_hash(&drawing.scene)),
        )
        .unwrap();
    let mut events = inner.subscribe();
    let taken = drawings::answer(
        inner,
        &id,
        DrawResult {
            ok: true,
            drawing: Some(drawing.id),
            hash: Some(scene_hash(&saved.scene)),
            element_count: Some(1),
            ..DrawResult::default()
        },
    );
    assert!(!taken, "nobody waits: the op already said silent");
    assert_eq!(
        heard_changed(&mut events, drawing.id).await.as_deref(),
        Some(scene_hash(&saved.scene).as_str()),
        "announced all the same, with the hash the store holds"
    );
    // A late refusal has nothing to announce.
    let mut events = inner.subscribe();
    assert!(!drawings::answer(
        inner,
        &id,
        DrawResult::refused("the canvas could not lay it out")
    ));
    assert_eq!(heard_changed(&mut events, drawing.id).await, None);
    engine.shutdown().await;
}

/// The same person on another machine draws; the record arrives here by
/// sync: the store adopts it and the engine tells an open canvas with the
/// hash the store now holds, as it does for an agent's erase.
#[tokio::test(flavor = "multi_thread")]
async fn a_peers_drawing_arriving_is_announced_to_the_canvas() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let other = tempfile::tempdir().unwrap();
    let twin = bisa_store::MemoryKeyStore::default();
    bisa_store::KeyStore::set(
        &twin,
        "owner",
        &engine.workspace().owner_keys().secret_key().to_secret_hex(),
    )
    .unwrap();
    let alpha = bisa_store::Workspace::open_with_keystore(other.path(), Box::new(twin)).unwrap();
    let d = alpha
        .create_drawing(NewDrawing {
            scope: OwnerScope::Workspace,
            title: "Shared".into(),
            scene: Some(Scene {
                elements: vec![box_at("a", 0), box_at("b", 240)],
                ..Scene::default()
            }),
        })
        .unwrap();
    let snapshot = alpha
        .paths()
        .state_dir(bisa_store::Paths::NS_DRAWINGS)
        .join(format!("{}-{}.json", bisa_core::kind::KIND_DRAWING, d.id));
    let event: nostr::event::Event =
        serde_json::from_slice(&std::fs::read(&snapshot).unwrap()).unwrap();
    let mut events = engine.inner().subscribe();
    engine.workspace().ingest_remote_event(&event).unwrap();
    assert_eq!(
        heard_changed(&mut events, d.id).await.as_deref(),
        Some(scene_hash(&d.scene).as_str()),
        "the canvas is told what the store now holds"
    );
    assert_eq!(engine.workspace().get_drawing(d.id).unwrap(), d);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_list_and_a_reading_need_no_desktop_and_a_skeleton_is_parked_and_answered() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let socket = engine.socket_path().to_path_buf();
    let drawing = drawings::create(
        engine.inner(),
        NewDrawing {
            scope: OwnerScope::Workspace,
            title: "Orders".into(),
            scene: None,
        },
    )
    .unwrap();

    // Data from the store: no desktop is asked.
    let reply = intake_roundtrip(
        &socket,
        json!({"op": "draw", "request": {"action": "list"}}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true));
    assert_eq!(reply["result"]["drawings"].as_array().unwrap().len(), 1);
    assert_eq!(reply["result"]["drawings"][0]["title"], json!("Orders"));
    let reply = intake_roundtrip(
        &socket,
        json!({"op": "draw", "request": {"action": "read", "drawing": drawing.id}}),
    )
    .await;
    assert_eq!(reply["result"]["ok"], json!(true));
    assert_eq!(
        reply["result"]["description"],
        json!("(the canvas is empty)\n")
    );
    assert_eq!(reply["result"]["hash"], json!(scene_hash(&drawing.scene)));

    // A desktop is home: it read the list, as an open one does every twenty seconds.
    engine.inner().draw.pending();
    let mut bus = engine.inner().subscribe();
    let id = drawing.id;
    let socket2 = socket.clone();
    let call = tokio::spawn(async move {
        intake_roundtrip(
            &socket2,
            json!({"op": "draw", "request": {"action": "draw", "drawing": id,
                "elements": [{"id": "api", "type": "rectangle", "x": 0, "y": 0, "label": {"text": "API"}}]}}),
        )
        .await
    });
    let pending = until("a parked drawing request", || {
        let p = engine.inner().draw.pending();
        (!p.is_empty()).then_some(p)
    })
    .await;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].request.action, DrawAction::Draw);
    assert_eq!(pending[0].request.drawing, Some(drawing.id));
    assert_eq!(pending[0].scope.agent.as_deref(), Some("general-agent"));
    // The desktop draws it and saves through the store, then answers.
    let drawn = engine
        .workspace()
        .update_drawing(
            drawing.id,
            DrawingPatch {
                scene: Some(Scene {
                    elements: vec![box_at("api", 0)],
                    ..Scene::default()
                }),
                ..Default::default()
            },
            Some(&scene_hash(&drawing.scene)),
        )
        .unwrap();
    let answered = engine.inner().draw.answer(
        &pending[0].id,
        DrawResult {
            ok: true,
            drawing: Some(drawing.id),
            hash: Some(scene_hash(&drawn.scene)),
            element_count: Some(1),
            ..DrawResult::default()
        },
    );
    assert!(answered);
    assert!(
        !engine
            .inner()
            .draw
            .answer(&pending[0].id, DrawResult::refused("twice")),
        "answered once"
    );
    let reply = call.await.unwrap();
    assert_eq!(reply["result"]["ok"], json!(true));
    assert_eq!(reply["result"]["element_count"], json!(1));
    // The change is announced with the scene's hash.
    let changed = until("drawing_changed on the bus", || loop {
        match bus.try_recv() {
            Ok(ev) => {
                if let EnginePayload::DrawingChanged {
                    drawing: d, hash, ..
                } = ev.payload
                {
                    if d == drawing.id.to_string() {
                        return Some(hash);
                    }
                }
            }
            Err(_) => return None,
        }
    })
    .await;
    assert_eq!(changed, scene_hash(&drawn.scene));
}

#[tokio::test(flavor = "multi_thread")]
async fn nobody_home_is_said_at_once_and_a_call_naming_no_drawing_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let socket = engine.socket_path().to_path_buf();
    let drawing = drawings::create(
        engine.inner(),
        NewDrawing {
            scope: OwnerScope::Workspace,
            title: "Orders".into(),
            scene: None,
        },
    )
    .unwrap();
    let started = std::time::Instant::now();
    let reply = intake_roundtrip(
        &socket,
        json!({"op": "draw", "request": {"action": "snapshot", "drawing": drawing.id}}),
    )
    .await;
    assert_eq!(reply["result"]["ok"], json!(false));
    assert_eq!(reply["result"]["error"], json!(NOBODY_HOME));
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
    assert!(
        engine.inner().draw.pending().is_empty(),
        "nothing parked for nobody"
    );

    let reply = intake_roundtrip(
        &socket,
        json!({"op": "draw", "request": {"action": "read"}}),
    )
    .await;
    assert_eq!(reply["result"]["error"], json!(NO_DRAWING));
    // A skeleton the canvas cannot draw is refused before the desktop is asked.
    engine.inner().draw.pending();
    let reply = intake_roundtrip(
        &socket,
        json!({"op": "draw", "request": {"action": "draw", "drawing": drawing.id,
            "elements": [{"type": "image", "x": 0, "y": 0}]}}),
    )
    .await;
    assert!(
        reply["result"]["error"]
            .as_str()
            .unwrap()
            .contains("vector only"),
        "{reply}"
    );
    assert!(engine.inner().draw.pending().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_op_refuses_by_the_workspace_policy_and_the_skill_is_the_assignment() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let ws = engine.workspace();
    let socket = engine.socket_path().to_path_buf();
    let list = |agent: &str| json!({"op": "draw", "agent": agent, "request": {"action": "list"}});

    // Everyone, by default: an agent with no skill at all is answered like anyone.
    let plain = ws.add_agent(scout(vec![])).unwrap();
    let reply = intake_roundtrip(&socket, list(plain.id.as_str())).await;
    assert_eq!(reply["result"]["ok"], json!(true));

    // Nobody: the core agents too.
    ws.set_setting(
        SettingScope::Workspace,
        None,
        "draw.agents",
        json!("nobody"),
    )
    .unwrap();
    let reply = intake_roundtrip(&socket, list("general-agent")).await;
    assert_eq!(reply["ok"], json!(false));
    assert_eq!(errors_of(&reply), vec![NOBODY_MAY.to_string()]);

    // Assigned: an agent without the skill is told what to attach; with it,
    // the call is answered like anyone's; a core agent always is.
    ws.set_setting(
        SettingScope::Workspace,
        None,
        "draw.agents",
        json!("assigned"),
    )
    .unwrap();
    let bare = ws.add_agent(scout(vec![])).unwrap();
    let reply = intake_roundtrip(&socket, list(bare.id.as_str())).await;
    assert_eq!(errors_of(&reply), vec![NOT_ASSIGNED.to_string()]);
    ws.install(CatalogKind::Skill, DRAW_SKILL).unwrap();
    let skilled = ws
        .add_agent(scout(vec![SkillId::new(DRAW_SKILL).unwrap()]))
        .unwrap();
    let reply = intake_roundtrip(&socket, list(skilled.id.as_str())).await;
    assert_eq!(reply["result"]["ok"], json!(true));
    let reply = intake_roundtrip(&socket, list("general-agent")).await;
    assert_eq!(reply["result"]["ok"], json!(true));

    // The machine's switch beats every policy.
    ws.set_setting(SettingScope::Machine, None, "draw.enabled", json!(false))
        .unwrap();
    let reply = intake_roundtrip(&socket, list("general-agent")).await;
    assert_eq!(errors_of(&reply), vec![OFF.to_string()]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_conversation_chooses_the_drawing_and_files_a_new_one_where_it_stands_and_erase_repairs()
{
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let ws = engine.workspace();
    let socket = engine.socket_path().to_path_buf();
    let goal = ws.create_goal(bisa_store::NewGoal::captured("g")).unwrap();
    let about_goal = ws
        .create_conversation(NewConversation {
            origin: ConversationOrigin::Goal { id: goal.id },
            title: None,
            mode: bisa_core::ConversationMode::Auto,
        })
        .unwrap();
    // A new drawing in a conversation about a goal is the goal's.
    let reply = intake_roundtrip(
        &socket,
        json!({"op": "draw", "scope": about_goal.id, "request": {"action": "create", "title": "Where the data goes"}}),
    )
    .await;
    assert_eq!(reply["result"]["ok"], json!(true), "{reply}");
    let id: bisa_core::DrawingId = reply["result"]["drawing"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let drawing = ws.get_drawing(id).unwrap();
    assert_eq!(drawing.scope, OwnerScope::Goal { id: goal.id });

    // A conversation about the drawing chooses it for a call naming none.
    let about_drawing = ws
        .create_conversation(NewConversation {
            origin: ConversationOrigin::Drawing { id },
            title: None,
            mode: bisa_core::ConversationMode::Auto,
        })
        .unwrap();
    let reply = intake_roundtrip(
        &socket,
        json!({"op": "draw", "scope": about_drawing.id, "request": {"action": "read"}}),
    )
    .await;
    assert_eq!(reply["result"]["drawing"], json!(id));
    assert_eq!(reply["result"]["title"], json!("Where the data goes"));

    // An erasure repairs what pointed at the removed box.
    let mut a = box_at("a", 0);
    a["boundElements"] = json!([{"id": "ab", "type": "arrow"}]);
    let arrow = json!({"id": "ab", "type": "arrow", "x": 160, "y": 80, "width": 80, "height": 0,
        "startBinding": {"elementId": "a"}, "endBinding": {"elementId": "b"}});
    ws.update_drawing(
        id,
        DrawingPatch {
            scene: Some(Scene {
                elements: vec![a, box_at("b", 240), arrow],
                ..Scene::default()
            }),
            ..Default::default()
        },
        Some(&scene_hash(&drawing.scene)),
    )
    .unwrap();
    let reply = intake_roundtrip(
        &socket,
        json!({"op": "draw", "scope": about_drawing.id, "request": {"action": "erase", "ids": ["a"]}}),
    )
    .await;
    assert_eq!(reply["result"]["ok"], json!(true), "{reply}");
    assert_eq!(reply["result"]["element_count"], json!(2));
    let after = ws.get_drawing(id).unwrap();
    let arrow = after
        .scene
        .elements
        .iter()
        .find(|e| e["id"] == "ab")
        .unwrap();
    assert!(arrow["startBinding"].is_null(), "the loose end");
    assert_eq!(arrow["endBinding"]["elementId"], "b");
    let reply = intake_roundtrip(
        &socket,
        json!({"op": "draw", "scope": about_drawing.id, "request": {"action": "erase", "ids": ["nothing"]}}),
    )
    .await;
    assert!(reply["result"]["error"]
        .as_str()
        .unwrap()
        .contains("drawing_read"));
}

/// A reading's description is cut to its cap on a character boundary and
/// says so; a snapshot the desktop uploaded is named beside the store, and
/// one the store cannot name is a refusal.
#[tokio::test(flavor = "multi_thread")]
async fn a_description_is_cut_on_a_character_boundary_and_a_snapshot_is_named_or_refused() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let inner = engine.inner();
    // Three bytes each, so the cap falls inside a character and the cut
    // steps back to the boundary before it.
    let long = DrawResult {
        ok: true,
        description: Some("€".repeat(bisa_core::draw::MAX_DESCRIBE_BYTES)),
        ..DrawResult::default()
    }
    .bounded();
    let text = long.description.unwrap();
    assert!(
        text.ends_with(&format!(
            "… cut at {} bytes",
            bisa_core::draw::MAX_DESCRIBE_BYTES
        )),
        "{}",
        &text[text.len() - 40..]
    );
    let kept = text.split('\n').next().unwrap();
    assert_eq!(kept.len() % 3, 0, "cut on a character boundary");
    assert!(
        kept.len() < bisa_core::draw::MAX_DESCRIBE_BYTES
            && kept.len() >= bisa_core::draw::MAX_DESCRIBE_BYTES - 2
    );
    let short = DrawResult {
        ok: true,
        description: Some("short".into()),
        ..DrawResult::default()
    }
    .bounded();
    assert_eq!(short.description.as_deref(), Some("short"));

    let shot = engine
        .workspace()
        .put_attachment(b"\x89PNG\r\n\x1a\n", "snap.png", "image/png")
        .unwrap();
    let named = DrawResult {
        ok: true,
        snapshot: Some(shot.clone()),
        ..DrawResult::default()
    }
    .with_named_copy(inner);
    assert!(named.ok);
    let path = named.path.expect("the named copy's path");
    assert!(std::path::Path::new(&path).is_file(), "{path}");
    let nobody = DrawResult {
        ok: true,
        snapshot: Some(bisa_core::AttachmentRef {
            sha256: "0".repeat(64),
            ..shot
        }),
        ..DrawResult::default()
    }
    .with_named_copy(inner);
    assert!(!nobody.ok, "{nobody:?}");
    assert!(nobody.error.unwrap().contains("could not be kept"));
    let bare = DrawResult::default().with_named_copy(inner);
    assert!(bare.path.is_none());
    engine.shutdown().await;
}

/// A new drawing a call files without naming a scope lands where the call
/// stands — the conversation's origin for every kind of conversation, the
/// goal of a goal's session, the workspace for a session of nothing — and a
/// scope named by the call is read as given or refused.
#[tokio::test(flavor = "multi_thread")]
async fn a_drawing_is_filed_where_the_call_stands_for_every_origin_and_a_named_scope_is_read_or_refused(
) {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(
        &dir,
        vec![crate::common::yielding("mock", json!({"summary": "ok"}))],
    );
    let inner = engine.inner();
    let ws = engine.workspace();
    let socket = engine.socket_path().to_path_buf();
    let create_from = |scope: Option<String>| {
        let socket = socket.clone();
        async move {
            let mut op = json!({"op": "draw", "request": {"action": "create", "title": "Filed"}});
            if let Some(scope) = scope {
                op["scope"] = json!(scope);
                op["agent"] = json!("general-agent");
            }
            let reply = intake_roundtrip(&socket, op).await;
            assert_eq!(reply["result"]["ok"], json!(true), "{reply}");
            let id: bisa_core::DrawingId =
                serde_json::from_value(reply["result"]["drawing"].clone()).unwrap();
            ws.get_drawing(id).unwrap().scope
        }
    };
    let about = |origin: ConversationOrigin| {
        ws.create_conversation(NewConversation {
            origin,
            title: None,
            mode: bisa_core::ConversationMode::Auto,
        })
        .unwrap()
        .id
        .to_string()
    };
    let goal = engine
        .submit_goal(bisa_engine::SubmitRequest::captured("file a drawing"))
        .unwrap();
    let project = ws
        .create_project(bisa_store::NewProject::managed("shop").unwrap())
        .unwrap();
    let primary = ws.primary_workstream(project.id).unwrap();
    let workflow = engine
        .create_workflow(crate::common::new_workflow(
            "nightly",
            vec![crate::common::step(
                "hold",
                bisa_core::StepKind::Wait {
                    until: bisa_core::WaitFor::Release,
                },
            )],
        ))
        .unwrap();
    let on_goal = drawings::create(
        inner,
        NewDrawing {
            scope: OwnerScope::Goal { id: goal.id },
            title: "Beside the goal".into(),
            scene: None,
        },
    )
    .unwrap();
    let note = ws
        .create_note(bisa_store::NewNote {
            scope: OwnerScope::Project { id: project.id },
            title: "A note".into(),
            body: String::new(),
        })
        .unwrap();

    assert_eq!(
        create_from(Some(about(ConversationOrigin::Project { id: project.id }))).await,
        OwnerScope::Project { id: project.id }
    );
    assert_eq!(
        create_from(Some(about(ConversationOrigin::Workstream {
            id: primary.id,
            project: project.id
        })))
        .await,
        OwnerScope::Project { id: project.id }
    );
    assert_eq!(
        create_from(Some(about(ConversationOrigin::Workflow {
            id: workflow.id
        })))
        .await,
        OwnerScope::Workflow { id: workflow.id }
    );
    assert_eq!(
        create_from(Some(about(ConversationOrigin::Drawing { id: on_goal.id }))).await,
        OwnerScope::Goal { id: goal.id },
        "beside the drawing the conversation is about"
    );
    assert_eq!(
        create_from(Some(about(ConversationOrigin::Note { id: note.id }))).await,
        OwnerScope::Project { id: project.id },
        "beside the note the conversation is about"
    );
    assert_eq!(
        create_from(Some(about(ConversationOrigin::Node))).await,
        OwnerScope::Node
    );
    assert_eq!(
        create_from(Some(about(ConversationOrigin::Workspace))).await,
        OwnerScope::Workspace
    );
    // A conversation about a drawing or a note that went: the workspace.
    let gone_drawing = about(ConversationOrigin::Drawing { id: on_goal.id });
    let gone_note = about(ConversationOrigin::Note { id: note.id });
    drawings::delete(inner, on_goal.id).unwrap();
    ws.delete_note(note.id).unwrap();
    assert_eq!(create_from(Some(gone_drawing)).await, OwnerScope::Workspace);
    assert_eq!(create_from(Some(gone_note)).await, OwnerScope::Workspace);
    // A goal's session, and a session of nothing.
    let from_goal = intake_roundtrip(
        &socket,
        json!({"op": "draw", "goal": goal.id, "request": {"action": "create", "title": "Of the goal"}}),
    )
    .await;
    let id: bisa_core::DrawingId =
        serde_json::from_value(from_goal["result"]["drawing"].clone()).unwrap();
    assert_eq!(
        ws.get_drawing(id).unwrap().scope,
        OwnerScope::Goal { id: goal.id }
    );
    assert_eq!(create_from(None).await, OwnerScope::Workspace);
    // A session of a run of the workspace: under the run's workflow.
    let (nightly, run) = crate::common::workspace_run(
        &engine,
        crate::common::new_workflow(
            "nightly-run",
            vec![crate::common::agent_step("build", "mock")],
        ),
    );
    crate::common::run_finished(&engine, run.id).await;
    let item = ws
        .list_work_items(&bisa_core::Home::Run { run: run.id })
        .unwrap()
        .into_iter()
        .next()
        .expect("the run's item")
        .id;
    let from_run = intake_roundtrip(
        &socket,
        json!({"op": "draw", "work_item": item, "request": {"action": "create", "title": "Of the run"}}),
    )
    .await;
    let id: bisa_core::DrawingId = serde_json::from_value(from_run["result"]["drawing"].clone())
        .unwrap_or_else(|_| panic!("{from_run}"));
    assert_eq!(
        ws.get_drawing(id).unwrap().scope,
        OwnerScope::Workflow { id: nightly.id }
    );

    // A scope the call names: read as given, or refused.
    let named = intake_roundtrip(
        &socket,
        json!({"op": "draw", "request": {"action": "create", "title": "Named", "scope": "project", "scope_id": project.id}}),
    )
    .await;
    let id: bisa_core::DrawingId =
        serde_json::from_value(named["result"]["drawing"].clone()).unwrap();
    assert_eq!(
        ws.get_drawing(id).unwrap().scope,
        OwnerScope::Project { id: project.id }
    );
    let refused = intake_roundtrip(
        &socket,
        json!({"op": "draw", "request": {"action": "create", "title": "Named", "scope": "shelf"}}),
    )
    .await;
    assert_eq!(
        refused["result"]["error"],
        json!(drawings::BAD_SCOPE),
        "{refused}"
    );
    engine.shutdown().await;
}

/// The list narrowed by a scope word, a kind, a record or refused; a
/// reading, an erasure and a skeleton of a drawing that went; an erasure
/// naming nothing.
#[tokio::test(flavor = "multi_thread")]
async fn the_list_is_narrowed_by_scope_and_a_drawing_that_went_is_said_so() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let inner = engine.inner();
    let socket = engine.socket_path().to_path_buf();
    let goal = engine
        .submit_goal(bisa_engine::SubmitRequest::captured("list drawings"))
        .unwrap();
    let on_goal = drawings::create(
        inner,
        NewDrawing {
            scope: OwnerScope::Goal { id: goal.id },
            title: "Of the goal".into(),
            scene: None,
        },
    )
    .unwrap();
    let on_workspace = drawings::create(
        inner,
        NewDrawing {
            scope: OwnerScope::Workspace,
            title: "Of the workspace".into(),
            scene: None,
        },
    )
    .unwrap();
    let list = |request: serde_json::Value| {
        let socket = socket.clone();
        async move {
            intake_roundtrip(&socket, json!({"op": "draw", "request": request})).await["result"]
                .clone()
        }
    };
    let titles = |result: &serde_json::Value| -> Vec<String> {
        result["drawings"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|d| d["title"].as_str().unwrap_or_default().to_string())
            .collect()
    };
    assert_eq!(
        titles(&list(json!({"action": "list", "scope": "workspace"})).await),
        vec!["Of the workspace"]
    );
    assert_eq!(
        titles(&list(json!({"action": "list", "scope": "goal"})).await),
        vec!["Of the goal"]
    );
    assert_eq!(
        titles(&list(json!({"action": "list", "scope": "goal", "scope_id": goal.id})).await),
        vec!["Of the goal"]
    );
    for bad in [
        json!({"action": "list", "scope": "shelf"}),
        json!({"action": "list", "scope": "goal", "scope_id": "not-an-id"}),
        json!({"action": "list", "scope": "shelf", "scope_id": "x"}),
    ] {
        let said = list(bad.clone()).await;
        assert_eq!(said["error"], json!(drawings::BAD_SCOPE), "{bad}: {said}");
    }

    // An erasure naming nothing; one with no ids; then the drawing goes and
    // every verb says so.
    let said = list(json!({"action": "erase", "ids": ["a"]})).await;
    assert_eq!(said["error"], json!(NO_DRAWING));
    let said = list(json!({"action": "erase", "drawing": on_workspace.id})).await;
    assert!(
        said["error"].as_str().unwrap().contains("needs the ids"),
        "{said}"
    );
    drawings::delete(inner, on_workspace.id).unwrap();
    for request in [
        json!({"action": "read", "drawing": on_workspace.id}),
        json!({"action": "erase", "drawing": on_workspace.id, "ids": ["a"]}),
        json!({"action": "draw", "drawing": on_workspace.id, "elements": [box_at("a", 0)]}),
    ] {
        let said = list(request.clone()).await;
        assert_eq!(said["ok"], json!(false), "{request}: {said}");
        assert!(
            said["error"]
                .as_str()
                .unwrap()
                .contains(&on_workspace.id.to_string()),
            "the drawing is named: {said}"
        );
    }
    let _ = on_goal;

    // A skeleton the canvas refuses: the refusal is the op's answer, and
    // nothing is announced.
    engine.inner().draw.pending();
    let kept = drawings::create(
        inner,
        NewDrawing {
            scope: OwnerScope::Workspace,
            title: "Refused".into(),
            scene: None,
        },
    )
    .unwrap();
    let socket2 = socket.clone();
    let kept_id = kept.id;
    let call = tokio::spawn(async move {
        intake_roundtrip(
            &socket2,
            json!({"op": "draw", "request": {"action": "draw", "drawing": kept_id, "elements": [box_at("a", 0)]}}),
        )
        .await
    });
    let pending = until("a parked drawing request", || {
        let p = engine.inner().draw.pending();
        (!p.is_empty()).then_some(p)
    })
    .await;
    assert!(engine.inner().draw.answer(
        &pending[0].id,
        DrawResult::refused("the canvas could not lay it out")
    ));
    let reply = call.await.unwrap();
    assert_eq!(reply["result"]["ok"], json!(false), "{reply}");
    assert_eq!(
        reply["result"]["error"],
        json!("the canvas could not lay it out")
    );
    engine.shutdown().await;
}
