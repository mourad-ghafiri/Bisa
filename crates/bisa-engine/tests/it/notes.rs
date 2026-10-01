//! A conversation about a note (13 — Conversations, the `note` origin): the
//! note tools take the conversation's note when a call names none, exactly
//! as the drawing tools take the conversation's drawing; a call naming none
//! outside such a conversation is refused in a sentence that says what to
//! pass; an appended block wears the attribution and announces the change;
//! the frame tells the agent to read first and write only when asked; and
//! the note's conversations leave with the note.

use crate::common::{engine_with, intake_roundtrip};
use bisa_core::{ConversationOrigin, OwnerScope};
use bisa_engine::events::EnginePayload;
use bisa_engine::{framing, notes};
use bisa_store::{ConversationFilter, NewConversation, NewNote};
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

#[tokio::test(flavor = "multi_thread")]
async fn a_conversation_chooses_the_note_for_a_call_naming_none_and_a_bare_call_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let ws = engine.workspace();
    let socket = engine.socket_path().to_path_buf();
    let note = notes::create(
        engine.inner(),
        NewNote {
            scope: OwnerScope::Workspace,
            title: "Why the cache".into(),
            body: "because\n".into(),
        },
    )
    .unwrap();

    // Outside a conversation about a note, a call that names none is refused
    // with the word to pass.
    let reply = intake_roundtrip(&socket, json!({"op": "note_read"})).await;
    assert_eq!(reply["ok"], json!(false), "{reply}");
    assert!(
        errors_of(&reply).iter().any(|e| e.contains("pass `note`")),
        "{reply}"
    );

    // In one, the note is chosen: read, then append.
    let about = ws
        .create_conversation(NewConversation {
            origin: ConversationOrigin::Note { id: note.id },
            title: None,
            mode: bisa_core::ConversationMode::Auto,
        })
        .unwrap();
    let reply = intake_roundtrip(&socket, json!({"op": "note_read", "scope": about.id})).await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    assert_eq!(reply["title"], json!("Why the cache"));
    assert_eq!(reply["body"], json!("because\n"));

    let mut rx = engine.inner().subscribe();
    let reply = intake_roundtrip(
        &socket,
        json!({"op": "note_append", "scope": about.id, "agent": "developer", "text": "and a TTL"}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    let after = ws.get_note(note.id).unwrap();
    assert!(
        after.body.starts_with("because\n"),
        "nothing they wrote moved"
    );
    assert!(
        after.body.contains("**developer**") && after.body.trim_end().ends_with("and a TTL"),
        "the block is attributed and at the end: {}",
        after.body
    );
    let changed = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let ev = rx.recv().await.unwrap();
            if let EnginePayload::NoteChanged { note: id, .. } = &ev.payload {
                if *id == note.id.to_string() {
                    break;
                }
            }
        }
    })
    .await;
    assert!(changed.is_ok(), "the overlay is told somebody else wrote");

    // A named note still wins over the conversation's.
    let other = notes::create(
        engine.inner(),
        NewNote {
            scope: OwnerScope::Node,
            title: "Other".into(),
            body: String::new(),
        },
    )
    .unwrap();
    let reply = intake_roundtrip(
        &socket,
        json!({"op": "note_read", "scope": about.id, "note": other.id}),
    )
    .await;
    assert_eq!(reply["title"], json!("Other"));

    // The frame: read first, write only when asked, leave `note` out.
    let frame = framing::origin_frame(
        &ConversationOrigin::Note { id: note.id },
        Some("Why the cache"),
    );
    for word in [
        "note_read",
        "note_append",
        "leave `note` out",
        "only when asked",
    ] {
        assert!(frame.contains(word), "{word} missing from: {frame}");
    }

    // The note's conversations leave with it.
    notes::delete(engine.inner(), note.id).unwrap();
    assert!(ws.get_conversation(about.id).is_err());
    let left = ws.list_conversations(&ConversationFilter::all(10)).unwrap();
    assert!(left.is_empty(), "{left:?}");
}
