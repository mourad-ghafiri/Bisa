//! The change ledger's files: a conversation's turns and the blobs they
//! name, kept under `ide/changes/` and gone with the conversation.

use bisa_core::{
    AgentId, ChangeLedger, ChangeState, ConversationMode, ConversationOrigin, RelPath, TurnChanges,
    TurnId,
};
use bisa_store::{MemoryKeyStore, NewConversation, Workspace, MAX_CHANGE_BLOB, UNREADABLE_LEDGER};

fn ws() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    (dir, ws)
}

fn conversation(ws: &Workspace, mode: ConversationMode) -> bisa_core::Conversation {
    ws.create_conversation(NewConversation {
        origin: ConversationOrigin::Workspace,
        title: None,
        mode,
    })
    .unwrap()
}

fn turn(ledger: &mut ChangeLedger, n: u64) -> TurnId {
    let id = TurnId::from_ulid(ulid::Ulid::from_parts(n, 1));
    ledger.turns.push(TurnChanges {
        turn: id,
        prompt: None,
        reply: None,
        agent: AgentId::new("developer").unwrap(),
        mode: ConversationMode::Manual,
        started_at: n,
        ended_at: None,
        files: Vec::new(),
    });
    id
}

#[test]
fn a_conversation_with_nothing_recorded_has_an_empty_ledger() {
    let (_dir, ws) = ws();
    let c = conversation(&ws, ConversationMode::Manual);
    let ledger = ws.change_ledger(c.id).unwrap();
    assert_eq!(ledger, ChangeLedger::new(c.id));
}

#[test]
fn the_ledger_round_trips_and_a_blob_is_one_file_however_often_it_is_put() {
    let (_dir, ws) = ws();
    let c = conversation(&ws, ConversationMode::Manual);
    let before = ws.put_change_blob(c.id, b"fn main() {}\n").unwrap();
    let again = ws.put_change_blob(c.id, b"fn main() {}\n").unwrap();
    assert_eq!(before, again);
    let after = ws.put_change_blob(c.id, b"fn main() { run() }\n").unwrap();

    let mut ledger = ws.change_ledger(c.id).unwrap();
    let id = turn(&mut ledger, 1);
    let path = RelPath::new("src/main.rs").unwrap();
    ledger.touched(id, &path, Some(before.clone()), Some(after.clone()), false);
    ws.write_change_ledger(&ledger).unwrap();

    assert_eq!(ws.change_ledger(c.id).unwrap(), ledger);
    assert_eq!(ws.change_blob(c.id, &before).unwrap(), b"fn main() {}\n");
    assert_eq!(
        ws.change_blob(c.id, &after).unwrap(),
        b"fn main() { run() }\n"
    );
}

#[test]
fn writing_the_ledger_drops_the_blobs_it_no_longer_names() {
    let (_dir, ws) = ws();
    let c = conversation(&ws, ConversationMode::Auto);
    let base = ws.put_change_blob(c.id, b"one\n").unwrap();
    let image = ws.put_change_blob(c.id, b"two\n").unwrap();
    let mut ledger = ws.change_ledger(c.id).unwrap();
    let first = turn(&mut ledger, 1);
    turn(&mut ledger, 2);
    let path = RelPath::new("a.txt").unwrap();
    ledger.touched(first, &path, Some(base.clone()), Some(image.clone()), false);
    ws.write_change_ledger(&ledger).unwrap();
    assert!(ws.change_blob(c.id, &base).is_ok());

    ledger.settled(&path, ChangeState::Kept);
    ledger.prune(1);
    ws.write_change_ledger(&ledger).unwrap();
    assert!(ws.change_blob(c.id, &base).is_err(), "nothing names it now");
    assert!(ws.change_blob(c.id, &image).is_err());
}

#[test]
fn a_ledger_that_does_not_parse_is_set_aside_with_the_blobs_it_names() {
    let (_dir, ws) = ws();
    let c = conversation(&ws, ConversationMode::Manual);
    let base = ws.put_change_blob(c.id, b"one\n").unwrap();
    let mut ledger = ws.change_ledger(c.id).unwrap();
    let first = turn(&mut ledger, 1);
    let path = RelPath::new("a.txt").unwrap();
    ledger.touched(first, &path, Some(base.clone()), None, false);
    ws.write_change_ledger(&ledger).unwrap();

    let file = ws.change_index_file(c.id).with_file_name("ledger.json");
    for torn in ["", "{", "[]", "{\"conversation\": 7}"] {
        std::fs::write(&file, torn).unwrap();
        assert_eq!(
            ws.change_ledger(c.id).unwrap(),
            ChangeLedger::new(c.id),
            "{torn:?} reads as nothing recorded"
        );
        assert!(!file.exists(), "and no longer stands in the way");
        assert_eq!(
            std::fs::read_to_string(file.with_file_name(UNREADABLE_LEDGER)).unwrap(),
            torn,
            "kept byte for byte"
        );
    }

    // A ledger written afterwards names nothing, and still the blobs of the
    // one set aside are left for whoever reads it.
    let mut fresh = ws.change_ledger(c.id).unwrap();
    turn(&mut fresh, 2);
    ws.write_change_ledger(&fresh).unwrap();
    assert!(ws.change_blob(c.id, &base).is_ok());
    assert_eq!(ws.change_ledger(c.id).unwrap(), fresh);
}

#[test]
fn a_file_too_large_to_review_is_refused_rather_than_kept_without_its_bytes() {
    let (_dir, ws) = ws();
    let c = conversation(&ws, ConversationMode::Manual);
    let big = vec![b'x'; MAX_CHANGE_BLOB as usize + 1];
    assert!(ws.put_change_blob(c.id, &big).is_err());
}

#[test]
fn deleting_a_conversation_takes_its_changes_with_it() {
    let (dir, ws) = ws();
    let c = conversation(&ws, ConversationMode::Manual);
    ws.put_change_blob(c.id, b"kept for a while\n").unwrap();
    let mut ledger = ws.change_ledger(c.id).unwrap();
    turn(&mut ledger, 1);
    ws.write_change_ledger(&ledger).unwrap();
    let held = dir.path().join("ide/changes").join(c.id.to_string());
    assert!(held.is_dir());
    ws.delete_conversation(c.id).unwrap();
    assert!(!held.exists());
    // And a fresh read is an empty ledger, not an error.
    assert!(ws.change_ledger(c.id).unwrap().turns.is_empty());
}

#[test]
fn a_conversation_keeps_its_mode_and_remembers_where_a_plan_came_from() {
    let (_dir, ws) = ws();
    let c = conversation(&ws, ConversationMode::Auto);
    assert_eq!(ws.conversation_row(c.id).unwrap().mode, "auto");
    let planned = ws
        .set_conversation_mode(c.id, ConversationMode::Plan)
        .unwrap();
    assert_eq!(planned.mode_before_plan, Some(ConversationMode::Auto));
    assert_eq!(ws.conversation_row(c.id).unwrap().mode, "plan");
    let built = ws
        .set_conversation_mode(c.id, planned.mode_after_plan())
        .unwrap();
    assert_eq!(
        (built.mode, built.mode_before_plan),
        (ConversationMode::Auto, None)
    );
    assert_eq!(ws.get_conversation(c.id).unwrap(), built);
}
