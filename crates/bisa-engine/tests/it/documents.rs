//! A goal's documents through the engine: given at capture, on disk and in
//! the journal before anything runs, announced on the bus, named to a
//! session by `get_goal` and in the first prompt of a work item.

use crate::common::{intake_roundtrip, run_spec, wait_for};
use bisa_core::event::JournalPayload;
use bisa_core::workitem::{WorkItemSpec, WorkItemState};
use bisa_core::{AgentId, AttachmentRef, Budget, GoalId, ToolTier, WorkItemId};
use bisa_engine::{Engine, EngineConfig, EnginePayload, SubmitRequest};
use bisa_harness::mock::MockAdapter;
use bisa_harness::{HarnessAdapter, HarnessCatalog};
use bisa_store::{MemoryKeyStore, Workspace};
use serde_json::json;
use std::sync::Arc;
use std::time::SystemTime;

fn engine_with_adapter(dir: &tempfile::TempDir) -> (Engine, Arc<MockAdapter>) {
    let adapter = Arc::new(MockAdapter::default());
    let mut catalog = HarnessCatalog::new();
    catalog.register(Arc::clone(&adapter) as Arc<dyn HarnessAdapter>);
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    let mut general = ws.get_agent(&AgentId::general()).unwrap();
    general.harness = adapter.id().into();
    ws.update_agent(general).unwrap();
    let config = EngineConfig {
        design_enabled: false,
        ..Default::default()
    };
    (Engine::start(ws, catalog, config).unwrap(), adapter)
}

fn stored(ws: &Workspace, name: &str, bytes: &[u8]) -> AttachmentRef {
    ws.put_attachment(bytes, name, bisa_core::mime_of_name(name))
        .unwrap()
}

fn item(goal: GoalId) -> WorkItemSpec {
    WorkItemSpec {
        id: WorkItemId::from_ulid(ulid::Ulid::from_datetime(SystemTime::now())),
        home: bisa_core::Home::Goal { goal },
        run: None,
        step: None,
        instructions: "do the work".into(),
        state: WorkItemState::Open,
        project: None,
        harness_candidates: vec!["mock".into()],
        model: None,
        effort: None,
        output_schema: None,
        budget: Budget::default(),
        assignees: vec![],
        tier_ceiling: ToolTier::Write,
        agent: None,
        spawn_allowlist: vec![],
        depth_budget: 0,
        result_attempts: 0,
        interruptions: 0,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_capture_with_documents_has_them_on_disk_in_the_journal_and_on_the_bus() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, _adapter) = engine_with_adapter(&dir);
    let ws = engine.workspace();
    let brief = stored(ws, "brief.pdf", b"%PDF-1.7 brief");
    let mockup = stored(ws, "mockup.png", b"\x89PNG mock");
    let mut bus = engine.events();

    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            documents: vec![brief.clone(), mockup.clone()],
            ..SubmitRequest::captured("ship the checkout")
        })
        .unwrap();

    let docs = ws.goal_documents(goal.id).unwrap();
    assert_eq!(
        docs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        ["brief.pdf", "mockup.png"]
    );
    assert!(docs.iter().all(|d| d.present));
    assert_eq!(
        std::fs::read(ws.paths().goal(goal.id).document("brief.pdf")).unwrap(),
        b"%PDF-1.7 brief"
    );
    let facts = ws
        .journal(&bisa_core::Home::from(goal.id))
        .unwrap()
        .into_iter()
        .filter(|e| matches!(e.payload, JournalPayload::Document { .. }))
        .count();
    assert_eq!(facts, 2, "one fact per document");

    let added = wait_for(&mut bus, "document.added", |e| {
        matches!(&e.payload, EnginePayload::DocumentAdded { file, .. } if file.name == "mockup.png")
    })
    .await;
    assert_eq!(added.goal, Some(goal.id));
    match added.payload {
        EnginePayload::DocumentAdded { goal: g, file } => {
            assert_eq!(g, goal.id);
            assert_eq!(file, mockup);
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hash_this_machine_does_not_hold_refuses_the_capture_before_anything_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, _adapter) = engine_with_adapter(&dir);
    let ws = engine.workspace();
    let held = stored(ws, "brief.pdf", b"%PDF-1.7 brief");
    let ghost = AttachmentRef {
        sha256: "cd".repeat(32),
        name: "ghost.pdf".into(),
        mime: "application/pdf".into(),
        size: 9,
    };
    let err = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            documents: vec![held, ghost],
            ..SubmitRequest::captured("ship it")
        })
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("ghost.pdf") && err.contains("upload the bytes first"),
        "{err}"
    );
    // The bytes are checked before the goal is written: no goal exists for
    // documents it cannot have, and so no document either.
    let goals = ws.list_goals(None).unwrap();
    assert!(goals.is_empty(), "{goals:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn get_goal_names_each_document_by_its_absolute_path_and_a_later_one_lands_the_same_way() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, _adapter) = engine_with_adapter(&dir);
    let ws = engine.workspace();
    let brief = stored(ws, "brief.pdf", b"%PDF-1.7 brief");
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            documents: vec![brief],
            ..SubmitRequest::captured("ship it")
        })
        .unwrap()
        .id;
    let spec = stored(ws, "spec.md", b"# spec");
    let added = engine.add_goal_documents(goal, &[spec]).unwrap();
    assert_eq!(added[0].name, "spec.md");

    let reply = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "get_goal", "goal": goal.to_string()}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    let docs = reply["documents"].as_array().unwrap();
    assert_eq!(docs.len(), 2, "{reply}");
    let expected = ws.paths().goal(goal).document("brief.pdf");
    assert_eq!(docs[0]["name"], json!("brief.pdf"));
    assert_eq!(docs[0]["path"], json!(expected.display().to_string()));
    assert_eq!(docs[0]["mime"], json!("application/pdf"));
    assert_eq!(docs[1]["name"], json!("spec.md"));
    assert_eq!(docs[1]["size"], json!(6));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_first_prompt_of_a_work_item_names_the_documents_folder() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, adapter) = engine_with_adapter(&dir);
    let ws = engine.workspace();
    let brief = stored(ws, "brief.pdf", b"%PDF-1.7 brief");
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            documents: vec![brief],
            ..SubmitRequest::captured("ship it")
        })
        .unwrap()
        .id;
    run_spec(&engine, goal, item(goal)).await;

    let prompts = adapter.prompts();
    let first = prompts.first().expect("the session was prompted");
    let folder = ws.paths().goal(goal).documents().display().to_string();
    assert!(
        first.contains(&folder),
        "the first prompt names the folder:\n{first}"
    );
    assert!(
        first.contains("1 document as context") && first.contains("brief.pdf"),
        "{first}"
    );
}
