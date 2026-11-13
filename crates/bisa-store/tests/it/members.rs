//! The member file is truth (08 — Persistence): read whole, or moved aside
//! and named — never read as empty and written back over the people it held.

use bisa_core::MemberRole;
use bisa_store::{Admission, MemoryKeyStore, ProblemKind, StoreError, Workspace};

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

/// The people's other two lists are repaired at open as the member file is.
/// An `invites.json` or a `held.json` that is there and does not parse is
/// moved under `quarantine/`, bytes unchanged, and named; the list starts
/// again empty and takes the next write. Left where it was, each failed
/// every read and every write for good: no invitation could be made and
/// every claim was refused; no message from outside could be held — so it
/// was never read and never heard — and nobody could leave or be removed.
/// The check finds both before any open does.
#[test]
fn an_invitations_file_or_a_held_list_that_does_not_read_is_moved_aside_and_the_next_is_made() {
    let dir = tempfile::tempdir().unwrap();
    let (invites, held) = {
        let ws = open(&dir).unwrap();
        ws.create_invite(MemberRole::Guest, vec![], None, 3600, "a".repeat(64))
            .unwrap();
        (ws.paths().invites_file(), ws.paths().held_file())
    };
    let garbled_invites = b"{\"invites\": [ not json".as_slice();
    let garbled_held = b"{\"held\": [ torn".as_slice();
    std::fs::write(&invites, garbled_invites).unwrap();
    std::fs::write(&held, garbled_held).unwrap();

    let found = bisa_store::check_files(&bisa_store::Paths::new(dir.path())).unwrap();
    for path in [&invites, &held] {
        assert!(
            found
                .iter()
                .any(|f| f.kind == bisa_store::FindingKind::Unparseable
                    && f.path == path.display().to_string()),
            "the check names {}: {found:?}",
            path.display()
        );
    }

    let ws = open(&dir).expect("neither list stops an open");
    let problems = ws.problems();
    for (path, bytes) in [(&invites, garbled_invites), (&held, garbled_held)] {
        let problem = problems
            .iter()
            .find(|p| p.path == path.display().to_string())
            .unwrap_or_else(|| panic!("{} is named: {problems:?}", path.display()));
        assert_eq!(problem.kind, ProblemKind::Quarantined);
        let moved = std::path::PathBuf::from(problem.quarantined.as_deref().expect("moved"));
        assert!(moved.starts_with(ws.paths().quarantine_dir()));
        assert_eq!(
            std::fs::read(&moved).unwrap(),
            bytes,
            "the bytes are moved, never changed"
        );
    }
    let held_words = problems
        .iter()
        .find(|p| p.path == held.display().to_string())
        .map(|p| p.text.to_string())
        .unwrap();
    assert!(
        held_words.contains("nothing is held back now"),
        "what it held is held no longer, and the problem says so: {held_words}"
    );

    assert!(ws.invites().unwrap().is_empty());
    assert!(ws.held_messages().unwrap().is_empty());
    ws.create_invite(MemberRole::Member, vec![], None, 3600, "b".repeat(64))
        .unwrap();
    let bob = bisa_core::PrincipalId::new("c".repeat(64)).unwrap();
    ws.hold_message("e1", "general", bob, bisa_store::HeldReason::Pending)
        .unwrap();
    assert_eq!(ws.invites().unwrap().len(), 1, "the next is made");
    assert_eq!(ws.held_messages().unwrap().len(), 1, "the next is held");
}

/// The members' mirror says what the file says, whole. After a member file
/// that would not read is moved aside, the people it named are in the file
/// no longer — and no longer in the mirror either, which is what a roster,
/// a direct channel and a mention ask. Added to and never emptied, the
/// mirror went on answering *a member* for somebody the file had lost.
#[test]
fn the_mirror_forgets_who_a_moved_file_named() {
    let dir = tempfile::tempdir().unwrap();
    let bob = bisa_core::PrincipalId::new("b".repeat(64)).unwrap();
    let path = {
        let ws = open(&dir).unwrap();
        ws.add_member(bob.clone(), MemberRole::Member, Admission::default())
            .unwrap();
        assert!(ws.is_member(&bob).unwrap());
        ws.paths().members_file()
    };
    std::fs::write(&path, b"{\"members\": [ not json").unwrap();
    let ws = open(&dir).unwrap();
    assert_eq!(ws.member(&bob).unwrap(), None, "the file lost them");
    assert!(
        !ws.is_member(&bob).unwrap(),
        "and the mirror says what the file says"
    );
    let refused = ws
        .open_dm(std::slice::from_ref(&bob))
        .expect_err("nobody opens a direct channel with somebody who is not here");
    assert!(!refused.to_string().is_empty());
}
