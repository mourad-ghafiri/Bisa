//! The member file is truth (08 — Persistence): read whole or refused,
//! never read as empty and written back.

use bisa_store::{MemoryKeyStore, StoreError, Workspace};

fn open(dir: &tempfile::TempDir) -> Result<Workspace, StoreError> {
    Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default()))
}

/// A `members.json` that is there and does not parse refuses the open, by
/// name — and its bytes are exactly what they were. The alternative was a
/// workspace that opened, read nobody, and wrote the owner alone over every
/// hosted member and their roles.
#[test]
fn a_garbled_member_file_refuses_the_open_and_is_left_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let path = {
        let ws = open(&dir).unwrap();
        ws.paths().members_file()
    };
    let garbled = b"{\"members\": [ not json";
    std::fs::write(&path, garbled).unwrap();

    let err = open(&dir)
        .err()
        .expect("a member file this build cannot read is refused");
    match err {
        StoreError::Unreadable {
            what, path: named, ..
        } => {
            assert_eq!(what, "member file");
            assert_eq!(named, path.display().to_string());
        }
        other => panic!("refused for another reason: {other}"),
    }
    assert_eq!(
        std::fs::read(&path).unwrap(),
        garbled,
        "the file is untouched"
    );
}

/// The owner is a member from the first open — with the role and no name:
/// nobody has said one yet. A word the store made up would be a name every
/// screen and every guest reads as the person's own, in one language.
#[test]
fn the_owner_is_a_member_from_birth_and_wears_no_name_nobody_gave() {
    let dir = tempfile::tempdir().unwrap();
    let ws = open(&dir).unwrap();
    let owner = ws.owner_principal();
    let row = ws
        .member(&owner)
        .unwrap()
        .expect("the owner is always a member");
    assert_eq!(row.role, bisa_core::MemberRole::Owner);
    assert_eq!(row.label, None, "no name until the person gives one");
    assert_eq!(ws.members().unwrap().len(), 1);
}
