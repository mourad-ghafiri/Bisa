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
