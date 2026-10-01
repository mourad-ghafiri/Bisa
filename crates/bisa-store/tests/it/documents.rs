//! A goal's documents: the files a person gave it as context, under
//! `goals/<id>/documents/`, recorded as journal facts and derived from them.

use bisa_core::event::JournalPayload;
use bisa_core::{AttachmentRef, FileScope, Home};
use bisa_store::{MemoryKeyStore, NewGoal, Workspace};

fn ws() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    (dir, ws)
}

fn stored(ws: &Workspace, name: &str, bytes: &[u8]) -> AttachmentRef {
    ws.put_attachment(bytes, name, bisa_core::mime_of_name(name))
        .unwrap()
}

#[test]
fn a_document_lands_under_the_goal_with_its_fact_and_a_taken_name_is_numbered() {
    let (dir, ws) = ws();
    let goal = ws.create_goal(NewGoal::captured("ship it")).unwrap().id;
    let brief = stored(&ws, "brief.pdf", b"%PDF-1.7 the brief");
    let mockup = stored(&ws, "mockup.png", b"\x89PNG\r\n\x1a\n mock");
    let again = stored(&ws, "brief.pdf", b"%PDF-1.7 another brief");

    let first = ws.add_goal_document(goal, &brief).unwrap();
    assert_eq!(first.name, "brief.pdf");
    assert!(first.present);
    assert_eq!(
        first.path,
        dir.path()
            .join("goals")
            .join(goal.to_string())
            .join("documents")
            .join("brief.pdf")
    );
    assert_eq!(std::fs::read(&first.path).unwrap(), b"%PDF-1.7 the brief");
    ws.add_goal_document(goal, &mockup).unwrap();
    let third = ws.add_goal_document(goal, &again).unwrap();
    assert_eq!(
        third.name, "brief (2).pdf",
        "the same name twice is numbered, never overwritten"
    );
    assert_eq!(
        std::fs::read(&third.path).unwrap(),
        b"%PDF-1.7 another brief"
    );

    // The record is the journal, one fact per document, the descriptor verbatim.
    let facts: Vec<AttachmentRef> = ws
        .journal(&Home::Goal { goal })
        .unwrap()
        .into_iter()
        .filter_map(|e| match e.payload {
            JournalPayload::Document { file } => Some(file),
            _ => None,
        })
        .collect();
    assert_eq!(facts, vec![brief.clone(), mockup.clone(), again.clone()]);

    // The listing is the facts with their settled names and presence.
    let docs = ws.goal_documents(goal).unwrap();
    assert_eq!(
        docs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        ["brief.pdf", "mockup.png", "brief (2).pdf"]
    );
    assert!(docs.iter().all(|d| d.present));

    // The goal's file scope lists the folder as documents, one level down.
    let tree = ws
        .list_tree(FileScope::Goal, &goal.to_string(), "", Some(2))
        .unwrap();
    let kinds: Vec<(String, String)> = tree
        .entries
        .iter()
        .filter(|e| e.path.starts_with("documents"))
        .map(|e| (e.path.clone(), e.kind.as_str().to_string()))
        .collect();
    assert!(
        kinds.contains(&("documents".to_string(), "document".to_string())),
        "{kinds:?}"
    );
    assert!(
        kinds.contains(&(
            "documents/brief (2).pdf".to_string(),
            "document".to_string()
        )),
        "{kinds:?}"
    );
}

/// A peer's fact is a peer's word: a name nothing safe is left of and a
/// hash that is not one name the file whole rather than slice a panic.
#[test]
fn a_document_fact_with_a_malformed_hash_is_named_without_a_panic() {
    let bad = AttachmentRef {
        name: ".".into(),
        sha256: "ab".into(),
        mime: "application/octet-stream".into(),
        size: 1,
    };
    assert_eq!(bisa_store::distinct_name(&Default::default(), &bad), "ab");
    let wide = AttachmentRef {
        name: "..".into(),
        sha256: "é".repeat(8),
        mime: "application/octet-stream".into(),
        size: 1,
    };
    assert_eq!(
        bisa_store::distinct_name(&Default::default(), &wide),
        "é".repeat(8)
    );
}

#[test]
fn bytes_this_machine_does_not_hold_are_refused_and_a_rebuild_materialises_again() {
    let (dir, ws) = ws();
    let goal = ws.create_goal(NewGoal::captured("ship it")).unwrap().id;
    let ghost = AttachmentRef {
        sha256: "cd".repeat(32),
        name: "ghost.pdf".into(),
        mime: "application/pdf".into(),
        size: 9,
    };
    let err = ws.add_goal_document(goal, &ghost).unwrap_err().to_string();
    assert!(err.contains("upload the bytes first"), "{err}");

    let spec = stored(&ws, "spec.md", b"# spec");
    let doc = ws.add_goal_document(goal, &spec).unwrap();
    std::fs::remove_file(&doc.path).unwrap();
    assert!(
        !ws.goal_documents(goal).unwrap()[0].present,
        "the file is gone, the fact stays"
    );
    drop(ws);

    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    ws.rebuild_index().unwrap();
    let docs = ws.goal_documents(goal).unwrap();
    assert_eq!(docs.len(), 1);
    assert!(
        docs[0].present,
        "derived from the fact and the bytes, so a rebuild brings it back"
    );
    assert_eq!(std::fs::read(&docs[0].path).unwrap(), b"# spec");
}
