//! A conversation about a note (13 — Conversations, the `note` origin): the
//! note tools take the conversation's note when a call names none, exactly
//! as the drawing tools take the conversation's drawing; a call naming none
//! outside such a conversation is refused in a sentence that says what to
//! pass; an appended block wears the attribution and announces the change;
//! the frame tells the agent to read first and write only when asked; and
//! the note's conversations leave with the note.

use crate::common::{engine_with, intake_roundtrip};
use bisa_core::{ConversationOrigin, OwnerScope, MAX_NOTE_BYTES};
use bisa_engine::events::EnginePayload;
use bisa_engine::{framing, notes};
use bisa_store::{body_hash, ConversationFilter, NewConversation, NewNote};
use serde_json::json;
use std::time::Duration;

/// A token the redactor knows, as `security.rs` spells it.
const FAKE_TOKEN: &str = "ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";

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

/// The agent's second write: the body replaced at the hash it read and said
/// on the bus with the hash and the agent; a note that moved since refused
/// with the current hash; nothing written for a text that already reads so;
/// an empty text, a body over the cap and a body holding a secret refused.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_rewrites_a_note_at_the_hash_it_read_and_a_stale_write_is_refused_with_the_current(
) {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let ws = engine.workspace();
    let socket = engine.socket_path().to_path_buf();
    let note = notes::create(
        engine.inner(),
        NewNote {
            scope: OwnerScope::Workspace,
            title: "The door".into(),
            body: "Sand it.\n".into(),
        },
    )
    .unwrap();
    let about = ws
        .create_conversation(NewConversation {
            origin: ConversationOrigin::Note { id: note.id },
            title: None,
            mode: bisa_core::ConversationMode::Auto,
        })
        .unwrap();
    // The reading answers the hash a write states.
    let read = intake_roundtrip(&socket, json!({"op": "note_read", "scope": about.id})).await;
    let hash = read["hash"]
        .as_str()
        .expect("the hash beside the body")
        .to_string();
    assert_eq!(hash, body_hash("Sand it.\n"));
    let mut rx = engine.inner().subscribe();
    let reply = intake_roundtrip(
        &socket,
        json!({"op": "note_write", "scope": about.id, "agent": "developer",
               "text": "Sand it.\nThen prime it.\n", "base_hash": hash}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    assert_eq!(reply["changed"], json!(true), "{reply}");
    let after = ws.get_note(note.id).unwrap();
    assert_eq!(
        after.body, "Sand it.\nThen prime it.\n",
        "replaced whole, with no attribution block"
    );
    let new_hash = body_hash(&after.body);
    assert_eq!(reply["hash"], json!(new_hash));
    let (heard_hash, heard_by) = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let ev = rx.recv().await.unwrap();
            if let EnginePayload::NoteChanged {
                note: id, hash, by, ..
            } = &ev.payload
            {
                if *id == note.id.to_string() {
                    return (hash.clone(), by.clone());
                }
            }
        }
    })
    .await
    .expect("the editor is told, with the hash and the agent");
    assert_eq!(heard_hash, new_hash);
    assert_eq!(
        heard_by,
        Some(bisa_core::AgentId::new("developer").unwrap())
    );
    // The hash it first read: the note has moved past it.
    let stale = intake_roundtrip(
        &socket,
        json!({"op": "note_write", "scope": about.id, "text": "mine alone\n", "base_hash": hash}),
    )
    .await;
    assert_eq!(stale["ok"], json!(false), "{stale}");
    assert!(
        errors_of(&stale)
            .iter()
            .any(|e| e.contains("changed since you read it") && e.contains(&new_hash)),
        "refused with the current hash: {stale}"
    );
    assert_eq!(
        ws.get_note(note.id).unwrap().body,
        after.body,
        "nothing moved"
    );
    // The same text at the right hash: nothing to write, nothing changed.
    let same = intake_roundtrip(
        &socket,
        json!({"op": "note_write", "scope": about.id, "text": after.body, "base_hash": new_hash}),
    )
    .await;
    assert_eq!(same["ok"], json!(true), "{same}");
    assert_eq!(same["changed"], json!(false), "{same}");
    assert_eq!(same["hash"], json!(new_hash));
    // Nothing, and too much, are refused.
    let empty = intake_roundtrip(
        &socket,
        json!({"op": "note_write", "scope": about.id, "text": "  \n", "base_hash": new_hash}),
    )
    .await;
    assert!(
        errors_of(&empty)
            .iter()
            .any(|e| e.contains("emptied by its owner")),
        "{empty}"
    );
    let huge = intake_roundtrip(
        &socket,
        json!({"op": "note_write", "scope": about.id,
               "text": "x".repeat(MAX_NOTE_BYTES + 1), "base_hash": new_hash}),
    )
    .await;
    assert_eq!(huge["ok"], json!(false), "{huge}");
    assert_eq!(ws.get_note(note.id).unwrap().body, after.body);
    // Outside a conversation about a note, a call that names none is refused
    // with the word to pass.
    let bare = intake_roundtrip(
        &socket,
        json!({"op": "note_write", "text": "x", "base_hash": new_hash}),
    )
    .await;
    assert!(
        errors_of(&bare).iter().any(|e| e.contains("pass `note`")),
        "{bare}"
    );
    // A body holding a secret: the agent read a placeholder, so writing the
    // body back would store the placeholder over the person's secret.
    let secret = notes::create(
        engine.inner(),
        NewNote {
            scope: OwnerScope::Workspace,
            title: "Keys".into(),
            body: format!("token {FAKE_TOKEN}\n"),
        },
    )
    .unwrap();
    assert!(
        engine.inner().security.redact(&secret.body).count >= 1,
        "the fixture is a secret the redactor knows"
    );
    let read = intake_roundtrip(&socket, json!({"op": "note_read", "note": secret.id})).await;
    assert!(
        !read["body"].as_str().unwrap().contains(FAKE_TOKEN),
        "read redacted: {read}"
    );
    let refused = intake_roundtrip(
        &socket,
        json!({"op": "note_write", "note": secret.id, "text": "token gone\n", "base_hash": read["hash"]}),
    )
    .await;
    assert!(
        errors_of(&refused)
            .iter()
            .any(|e| e.contains("holds a secret")),
        "{refused}"
    );
    assert_eq!(ws.get_note(secret.id).unwrap().body, secret.body);
    engine.shutdown().await;
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
