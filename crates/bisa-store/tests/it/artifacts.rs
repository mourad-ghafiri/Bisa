//! Artifacts on a message: indexed from the body, read back with presence,
//! listed per conversation, and given a real name on disk on demand.

use bisa_core::{ArtifactKind, ArtifactRef, ArtifactSource, FileScope, MessageBody, RelPath};
use bisa_store::{MemoryKeyStore, PostOrigin, Workspace};

fn ws() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    (dir, ws)
}

fn stored(ws: &Workspace, name: &str, bytes: &[u8], title: &str) -> ArtifactRef {
    let file = ws
        .put_attachment(bytes, name, bisa_core::mime_of_name(name))
        .unwrap();
    ArtifactRef::from_attachment(file, Some(title.into()), None)
}

fn post(ws: &Workspace, scope: &str, text: &str, artifacts: Vec<ArtifactRef>) -> String {
    ws.post_message(
        scope,
        MessageBody::Post {
            text: text.into(),
            context: vec![],
            artifacts,
            thinking: None,
            said: None,
        },
        None,
        &[],
        &[],
        None,
        PostOrigin::Asked,
    )
    .unwrap()
}

#[test]
fn an_artifact_needs_its_bytes_first() {
    let (_dir, ws) = ws();
    let ghost = ArtifactRef {
        sha256: "b".repeat(64),
        name: "chart.svg".into(),
        mime: "image/svg+xml".into(),
        size: 3,
        title: "Chart".into(),
        kind: ArtifactKind::Svg,
        source: None,
    };
    let err = ws
        .post_message(
            "general",
            MessageBody::Post {
                text: "see".into(),
                context: vec![],
                artifacts: vec![ghost],
                thinking: None,
                said: None,
            },
            None,
            &[],
            &[],
            None,
            PostOrigin::Asked,
        )
        .unwrap_err();
    assert!(
        err.to_string().contains("no artifact bytes"),
        "refused by name: {err}"
    );
}

#[test]
fn a_message_with_only_an_artifact_is_a_message() {
    let (_dir, ws) = ws();
    let page = stored(&ws, "dashboard.html", b"<h1>hi</h1>", "Dashboard");
    let id = post(&ws, "general", "", vec![page]);
    let row = ws.get_message(&id).unwrap().unwrap();
    assert_eq!(row.content, "");
    assert_eq!(row.artifacts.len(), 1);
    assert_eq!(row.artifacts[0].artifact.title, "Dashboard");
    assert_eq!(row.artifacts[0].artifact.kind, ArtifactKind::Html);
    assert!(row.artifacts[0].present);
}

#[test]
fn artifacts_are_indexed_read_back_with_presence_and_survive_a_rebuild() {
    let (_dir, ws) = ws();
    let mut sheet = stored(&ws, "q3.csv", b"a,b\n1,2\n", "Q3 numbers");
    sheet.source = Some(ArtifactSource {
        scope: FileScope::Workstream,
        id: "01J0000000000000000000000W".into(),
        path: RelPath::new("out/q3.csv").unwrap(),
    });
    let deck = stored(&ws, "deck.pptx", b"PK\x03\x04deck", "Kickoff deck");
    let id = post(
        &ws,
        "general",
        "the numbers and the deck",
        vec![sheet.clone(), deck.clone()],
    );

    let read = |ws: &Workspace| {
        let rows = ws.messages("general", None, 10).unwrap();
        rows.into_iter().find(|m| m.id == id).unwrap()
    };
    let row = read(&ws);
    assert_eq!(row.artifacts.len(), 2, "both, in order");
    assert_eq!(row.artifacts[0].artifact, sheet);
    assert_eq!(row.artifacts[1].artifact, deck);
    assert!(row.artifacts.iter().all(|a| a.present));
    assert_eq!(
        row.artifacts[0]
            .artifact
            .source
            .as_ref()
            .unwrap()
            .path
            .as_str(),
        "out/q3.csv"
    );

    // The event content carries the descriptors, so a rebuild from truth
    // reindexes them without the blobs' help.
    ws.rebuild_index().unwrap();
    let again = read(&ws);
    assert_eq!(again.artifacts, row.artifacts);
    assert_eq!(
        ws.get_message(&id).unwrap().unwrap().artifacts,
        row.artifacts
    );
    assert!(ws.get_message("nope").unwrap().is_none());
}

#[test]
fn list_artifacts_is_newest_first_and_skips_retracted() {
    let (_dir, ws) = ws();
    let first = stored(&ws, "v1.html", b"<p>one</p>", "Report");
    let second = stored(&ws, "v2.html", b"<p>two</p>", "Report");
    let gone = stored(&ws, "oops.png", b"\x89PNG oops", "Oops");
    let a = post(&ws, "general", "v1", vec![first.clone()]);
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let b = post(&ws, "general", "v2", vec![second.clone()]);
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let c = post(&ws, "general", "mistake", vec![gone]);
    ws.retract(&c).unwrap();

    let rows = ws.list_artifacts("general", 50).unwrap();
    assert_eq!(
        rows.iter()
            .map(|r| r.message_id.as_str())
            .collect::<Vec<_>>(),
        vec![b.as_str(), a.as_str()],
        "newest first, the retracted one gone"
    );
    assert_eq!(rows[0].artifact, second);
    assert_eq!(rows[1].artifact, first);
    assert!(rows.iter().all(|r| r.present && r.scope_id == "general"));
    assert_eq!(ws.list_artifacts("general", 1).unwrap().len(), 1);
    assert!(ws.list_artifacts("no-such-scope", 5).is_err());
}

#[test]
fn a_named_copy_is_idempotent_and_never_leaves_the_store() {
    let (dir, ws) = ws();
    let file = ws
        .put_attachment(b"%PDF-1.4 tiny", "report.pdf", "application/pdf")
        .unwrap();
    let named = ws.put_attachment_named(&file.sha256, "report.pdf").unwrap();
    assert!(named.is_file());
    assert_eq!(std::fs::read(&named).unwrap(), b"%PDF-1.4 tiny");
    assert!(named.starts_with(dir.path().join("attachments").join("named")));
    assert_eq!(named.file_name().unwrap(), "report.pdf");
    assert_eq!(
        ws.put_attachment_named(&file.sha256, "report.pdf").unwrap(),
        named
    );

    // A name that tries to leave keeps only its last component; one that is
    // nothing safe is refused by name.
    let stray = ws
        .put_attachment_named(&file.sha256, "../../etc/report.pdf")
        .unwrap();
    assert_eq!(stray, named);
    for bad in ["", "..", ".", ".hidden", "a\0b"] {
        let err = ws.put_attachment_named(&file.sha256, bad).unwrap_err();
        assert!(
            err.to_string().contains("not a file name"),
            "{bad:?}: {err}"
        );
    }
    let absent = ws
        .put_attachment_named(&"c".repeat(64), "x.txt")
        .unwrap_err();
    assert!(absent.to_string().contains("no attachment"));
    assert!(ws.put_attachment_named("nope", "x.txt").is_err());
}
