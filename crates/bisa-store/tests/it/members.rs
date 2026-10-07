//! The member file is truth (08 — Persistence): read whole, or moved aside
//! and named — never read as empty and written back over the people it held.

use bisa_store::{MemoryKeyStore, ProblemKind, StoreError, Workspace};

fn open(dir: &tempfile::TempDir) -> Result<Workspace, StoreError> {
    Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default()))
}

/// A `members.json` that is there and does not parse does not stop the
/// open: it is moved under `quarantine/` with its bytes exactly as they were
/// — never written over — the owner alone is a member again, and the open
/// names the file. The alternative was a workspace that opened, read nobody,
/// and wrote the owner alone over every hosted member and their roles; the
/// one before that, a workspace that would not open at all.
#[test]
fn a_garbled_member_file_is_moved_aside_whole_and_the_owner_is_a_member_again() {
    let dir = tempfile::tempdir().unwrap();
    let path = {
        let ws = open(&dir).unwrap();
        ws.paths().members_file()
    };
    let garbled = b"{\"members\": [ not json";
    std::fs::write(&path, garbled).unwrap();

    let ws = open(&dir).expect("a member file this build cannot read is moved aside");
    let problems = ws.problems();
    let [problem] = problems.as_slice() else {
        panic!("one problem, naming the file: {problems:?}");
    };
    assert_eq!(problem.kind, ProblemKind::Recreated);
    assert_eq!(problem.path, path.display().to_string());
    let moved = std::path::PathBuf::from(problem.quarantined.as_deref().expect("moved"));
    assert!(moved.starts_with(ws.paths().quarantine_dir()));
    assert_eq!(
        std::fs::read(&moved).unwrap(),
        garbled,
        "the bytes are moved, never changed"
    );
    assert_eq!(ws.members().unwrap().len(), 1, "the owner alone");
    // The strict read is what every other caller gets: a file this build
    // cannot read is still refused by name where nobody repairs it.
    std::fs::write(&path, garbled).unwrap();
    match ws.members().unwrap_err() {
        StoreError::Unreadable { what, .. } => assert_eq!(what, "member file"),
        other => panic!("refused for another reason: {other}"),
    }
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
