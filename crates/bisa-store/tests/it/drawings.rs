//! Drawings: a GEP record whose snapshot is the truth and whose `.excalidraw`
//! file is the export — compare-and-swap edits, bounded scenes, cascades,
//! rebuilds and a peer's record landing here.

use bisa_core::kind::KIND_DRAWING;
use bisa_core::{OwnerScope, Scene, WorkflowOrigin};
use bisa_store::{
    scene_hash, DrawingPatch, MemoryKeyStore, NewDrawing, NewProject, NewWorkflow, OwnerFilter,
    Paths, StoreError, Workspace,
};
use serde_json::json;

fn ws() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    (dir, ws)
}

fn drawing(scope: OwnerScope, title: &str) -> NewDrawing {
    NewDrawing {
        scope,
        title: title.into(),
        scene: None,
    }
}

fn box_at(id: &str, x: i64) -> serde_json::Value {
    json!({"id": id, "type": "rectangle", "x": x, "y": 40, "width": 160, "height": 80})
}

fn scene(elements: Vec<serde_json::Value>) -> Scene {
    Scene {
        elements,
        ..Scene::default()
    }
}

fn workflow(ws: &Workspace) -> bisa_core::Workflow {
    ws.create_workflow(
        NewWorkflow {
            name: "Release".into(),
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

fn file_of(
    ws: &Workspace,
    kind: &str,
    id: Option<&str>,
    drawing: bisa_core::DrawingId,
) -> std::path::PathBuf {
    Paths::drawing_file_in(
        &Paths::scoped_dir(&ws.paths().drawings_dir(), kind, id),
        drawing,
    )
}

#[test]
fn a_drawing_is_a_snapshot_that_travels_and_a_file_where_the_layout_says() {
    let (_dir, ws) = ws();
    let goal = ws.create_goal(bisa_store::NewGoal::captured("g")).unwrap();
    let project = ws
        .create_project(NewProject::managed("shop").unwrap())
        .unwrap();
    let wf = workflow(&ws);
    let cases = vec![
        (OwnerScope::Workspace, "workspace", None),
        (OwnerScope::Node, "node", None),
        (
            OwnerScope::Goal { id: goal.id },
            "goal",
            Some(goal.id.to_string()),
        ),
        (
            OwnerScope::Project { id: project.id },
            "project",
            Some(project.slug.to_string()),
        ),
        (
            OwnerScope::Workflow { id: wf.id },
            "workflow",
            Some(wf.id.to_string()),
        ),
    ];
    for (scope, kind, id) in cases {
        let mut rx = ws.subscribe_store_events();
        let d = ws
            .create_drawing(drawing(scope.clone(), "Where the data goes"))
            .unwrap();
        assert_eq!(d.scope, scope);
        // The snapshot: signed, addressable, on the bus for the peers.
        let snapshot = ws
            .paths()
            .state_dir(Paths::NS_DRAWINGS)
            .join(format!("{KIND_DRAWING}-{}.json", d.id));
        assert!(snapshot.exists(), "{}", snapshot.display());
        let event = rx.try_recv().expect("a drawing is a record that travels");
        match event {
            bisa_store::StoreEvent::ConversationSnapshot { kind, d: who, .. } => {
                assert_eq!(kind, KIND_DRAWING);
                assert_eq!(who, d.id.to_string());
            }
            other => panic!("not a snapshot on the bus: {other:?}"),
        }
        // The file: the standard envelope, named for the person.
        let file = file_of(&ws, kind, id.as_deref(), d.id);
        assert!(file.exists(), "{}", file.display());
        let text: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(text["type"], "excalidraw");
        assert_eq!(text["bisa"]["title"], "Where the data goes");
        assert_eq!(text["bisa"]["scope"], kind);
        assert_eq!(ws.get_drawing(d.id).unwrap(), d);
    }
    assert_eq!(ws.list_drawings(OwnerFilter::All).unwrap().len(), 5);
    assert_eq!(
        ws.list_drawings(OwnerFilter::Kind("goal")).unwrap().len(),
        1
    );
    assert_eq!(
        ws.list_drawings(OwnerFilter::Scope(OwnerScope::Project { id: project.id }))
            .unwrap()
            .len(),
        1
    );
    // A scope this workspace does not have is refused before anything is signed.
    let ghost = bisa_core::GoalId::from_ulid(ulid::Ulid::from_parts(9, 9));
    assert!(ws
        .create_drawing(drawing(OwnerScope::Goal { id: ghost }, "x"))
        .is_err());
}

#[test]
fn a_scene_edit_that_did_not_read_the_latest_is_refused_and_told_what_is_there() {
    let (_dir, ws) = ws();
    let d = ws
        .create_drawing(drawing(OwnerScope::Workspace, "Flow"))
        .unwrap();
    let h0 = scene_hash(&d.scene);
    let one = scene(vec![box_at("a", 0)]);
    let d1 = ws
        .update_drawing(
            d.id,
            DrawingPatch {
                scene: Some(one.clone()),
                ..Default::default()
            },
            Some(&h0),
        )
        .unwrap();
    assert_eq!(d1.scene, one);
    let two = scene(vec![box_at("a", 0), box_at("b", 240)]);
    match ws.update_drawing(
        d.id,
        DrawingPatch {
            scene: Some(two.clone()),
            ..Default::default()
        },
        Some(&h0),
    ) {
        Err(StoreError::EditConflict {
            what,
            current,
            current_hash,
        }) => {
            assert_eq!(what, "drawing");
            assert_eq!(current, serde_json::to_value(&one).unwrap());
            assert_eq!(current_hash, scene_hash(&one));
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        ws.update_drawing(
            d.id,
            DrawingPatch {
                scene: Some(two.clone()),
                ..Default::default()
            },
            None
        ),
        Err(StoreError::EditConflict { .. })
    ));
    // A title or a pin needs no hash: it cannot lose a stroke.
    let d2 = ws
        .update_drawing(
            d.id,
            DrawingPatch {
                title: Some("  Order flow  ".into()),
                pinned: Some(true),
                ..Default::default()
            },
            None,
        )
        .unwrap();
    assert_eq!(d2.title, "Order flow");
    assert!(d2.pinned);
    let listed = ws.list_drawings(OwnerFilter::All).unwrap();
    assert_eq!(listed[0].hash, scene_hash(&one));
    assert_eq!(listed[0].element_count, 1);
    // A deleted element the canvas kept for its undo is not kept here.
    let mut with_ghost = two.clone();
    with_ghost.elements[1]["isDeleted"] = json!(true);
    let d3 = ws
        .update_drawing(
            d.id,
            DrawingPatch {
                scene: Some(with_ghost),
                ..Default::default()
            },
            Some(&scene_hash(&one)),
        )
        .unwrap();
    assert_eq!(d3.scene.elements.len(), 1);
}

#[test]
fn a_scene_over_its_bounds_or_with_an_image_is_refused_and_nothing_is_written() {
    let (_dir, ws) = ws();
    let d = ws
        .create_drawing(drawing(OwnerScope::Workspace, "Big"))
        .unwrap();
    let h = scene_hash(&d.scene);
    let patch = |elements| DrawingPatch {
        scene: Some(scene(elements)),
        ..Default::default()
    };
    let err = ws
        .update_drawing(
            d.id,
            patch(vec![
                json!({"id": "i", "type": "image", "x": 0, "y": 0, "fileId": "f"}),
            ]),
            Some(&h),
        )
        .unwrap_err();
    assert!(err.to_string().contains("vector only"), "{err}");
    let heavy: Vec<_> = (0..400)
        .map(|i| json!({"id": format!("t{i}"), "type": "text", "x": 0, "y": 0, "text": "y".repeat(4000)}))
        .collect();
    let err = ws.update_drawing(d.id, patch(heavy), Some(&h)).unwrap_err();
    assert!(err.to_string().contains("the cap is"), "{err}");
    assert_eq!(ws.get_drawing(d.id).unwrap(), d, "nothing was written");
    assert!(ws
        .create_drawing(drawing(OwnerScope::Workspace, "   "))
        .is_err());
}

#[test]
fn a_rebuild_reads_the_snapshots_and_draws_the_files_again() {
    let (_dir, ws) = ws();
    let d = ws
        .create_drawing(NewDrawing {
            scope: OwnerScope::Workspace,
            title: "Kept".into(),
            scene: Some(scene(vec![box_at("a", 0)])),
        })
        .unwrap();
    let file = file_of(&ws, "workspace", None, d.id);
    std::fs::remove_file(&file).unwrap();
    ws.rebuild_index().unwrap();
    assert_eq!(ws.get_drawing(d.id).unwrap(), d);
    assert_eq!(ws.list_drawings(OwnerFilter::All).unwrap().len(), 1);
    assert!(file.exists(), "the file is drawn again from the snapshot");
}

#[test]
fn deleting_takes_the_file_the_snapshot_and_the_row_and_a_scope_takes_its_drawings() {
    let (_dir, ws) = ws();
    let d = ws
        .create_drawing(drawing(OwnerScope::Workspace, "Gone"))
        .unwrap();
    let file = file_of(&ws, "workspace", None, d.id);
    ws.delete_drawing(d.id).unwrap();
    assert!(!file.exists());
    assert!(ws.get_drawing(d.id).is_err());
    assert!(ws.list_drawings(OwnerFilter::All).unwrap().is_empty());

    let goal = ws.create_goal(bisa_store::NewGoal::captured("g")).unwrap();
    let wf = workflow(&ws);
    let project = ws
        .create_project(NewProject::managed("shop").unwrap())
        .unwrap();
    let on_goal = ws
        .create_drawing(drawing(OwnerScope::Goal { id: goal.id }, "g"))
        .unwrap();
    let on_wf = ws
        .create_drawing(drawing(OwnerScope::Workflow { id: wf.id }, "w"))
        .unwrap();
    let on_project = ws
        .create_drawing(drawing(OwnerScope::Project { id: project.id }, "p"))
        .unwrap();
    ws.delete_goal(goal.id).unwrap();
    ws.delete_workflow(wf.id).unwrap();
    ws.delete_project(project.id).unwrap();
    for gone in [on_goal.id, on_wf.id, on_project.id] {
        assert!(ws.get_drawing(gone).is_err(), "{gone}");
    }
    assert!(ws.list_drawings(OwnerFilter::All).unwrap().is_empty());
    assert!(!Paths::scoped_dir(
        &ws.paths().drawings_dir(),
        "goal",
        Some(&goal.id.to_string())
    )
    .exists());
    let state = ws.paths().state_dir(Paths::NS_DRAWINGS);
    let left: Vec<_> = std::fs::read_dir(&state)
        .map(|d| d.flatten().map(|e| e.file_name()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "no snapshot is left behind: {left:?}");
}

#[test]
fn a_save_of_the_same_scene_writes_nothing_and_keeps_the_hash() {
    // A canvas saving what it just adopted, or a bridge saving what the
    // canvas already saved: the record stands, no new revision, no new hash
    // — so nothing a reader would have to follow moves.
    let (_d, ws) = ws();
    let d = ws
        .create_drawing(NewDrawing {
            scope: OwnerScope::Workspace,
            title: "Same".into(),
            scene: Some(scene(vec![box_at("a", 0)])),
        })
        .unwrap();
    let hash = scene_hash(&d.scene);
    let again = ws
        .update_drawing(
            d.id,
            DrawingPatch {
                scene: Some(d.scene.clone()),
                ..Default::default()
            },
            Some(&hash),
        )
        .unwrap();
    assert_eq!(again, d, "nothing was written");
    assert_eq!(scene_hash(&again.scene), hash);
    // A title beside the same scene still writes; the scene's hash holds.
    let renamed = ws
        .update_drawing(
            d.id,
            DrawingPatch {
                scene: Some(d.scene.clone()),
                title: Some(" Renamed ".into()),
                ..Default::default()
            },
            Some(&hash),
        )
        .unwrap();
    assert_eq!(renamed.title, "Renamed");
    assert_eq!(scene_hash(&renamed.scene), hash);
    // The same scene at a hash the drawing has moved past is still refused.
    let moved = ws
        .update_drawing(
            d.id,
            DrawingPatch {
                scene: Some(scene(vec![box_at("a", 0), box_at("b", 240)])),
                ..Default::default()
            },
            Some(&hash),
        )
        .unwrap();
    assert_ne!(scene_hash(&moved.scene), hash);
    assert!(matches!(
        ws.update_drawing(
            d.id,
            DrawingPatch {
                scene: Some(d.scene.clone()),
                ..Default::default()
            },
            Some(&hash),
        ),
        Err(StoreError::EditConflict { .. })
    ));
}

#[test]
fn a_peers_drawing_lands_here_indexed_and_drawn_into_the_repository() {
    // The same person on two machines: one draws; the other — opened with
    // the same owner key — ingests the snapshot and has the picture.
    let (_a, alpha) = ws();
    let twin_store = MemoryKeyStore::default();
    bisa_store::KeyStore::set(
        &twin_store,
        "owner",
        &alpha.owner_keys().secret_key().to_secret_hex(),
    )
    .unwrap();
    let _b = tempfile::tempdir().unwrap();
    let beta = Workspace::open_with_keystore(_b.path(), Box::new(twin_store)).unwrap();
    let d = alpha
        .create_drawing(NewDrawing {
            scope: OwnerScope::Workspace,
            title: "Shared".into(),
            scene: Some(scene(vec![box_at("a", 0), box_at("b", 240)])),
        })
        .unwrap();
    let snapshot = alpha
        .paths()
        .state_dir(Paths::NS_DRAWINGS)
        .join(format!("{KIND_DRAWING}-{}.json", d.id));
    let event: nostr::event::Event =
        serde_json::from_slice(&std::fs::read(&snapshot).unwrap()).unwrap();
    let mut heard = beta.subscribe_store_events();
    let outcome = beta.ingest_remote_event(&event).unwrap();
    assert!(
        !format!("{outcome:?}").to_lowercase().contains("rejected"),
        "{outcome:?}"
    );
    assert_eq!(beta.get_drawing(d.id).unwrap(), d);
    // Said, so the engine can tell an open canvas what the store now holds.
    let arrived = std::iter::from_fn(|| heard.try_recv().ok()).any(|ev| {
        matches!(ev, bisa_store::StoreEvent::RemoteDrawingArrived { drawing } if drawing.id == d.id)
    });
    assert!(arrived, "a peer's drawing landing is a store event");
    let listed = beta.list_drawings(OwnerFilter::All).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].element_count, 2);
    assert!(file_of(&beta, "workspace", None, d.id).exists());
}
