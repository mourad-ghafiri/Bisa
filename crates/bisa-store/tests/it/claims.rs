//! A work item is claimed once, whoever races for it (B4 / I7), and a move
//! of its state is never written over by another made at the same moment.
//!
//! The engine lock makes one process the invariant; this is the invariant
//! inside that process: two claimers arriving together cannot both read
//! `open`, so exactly one claim fact reaches the journal — and a cancel that
//! arrives while the executor settles the item is the state it ends in.

use bisa_core::event::JournalPayload;
use bisa_core::{
    Home, PrincipalId, SessionId, WorkItemId, WorkItemSpec, WorkItemState, WorkItemTransition,
};
use bisa_store::{MemoryKeyStore, Workspace};
use std::sync::Arc;

fn open_item(ws: &Workspace) -> WorkItemSpec {
    let goal = ws
        .create_goal(bisa_store::NewGoal::captured("race for it"))
        .unwrap();
    let spec = WorkItemSpec {
        id: WorkItemId::from_ulid(ulid::Ulid::from_datetime(std::time::SystemTime::now())),
        home: Home::Goal { goal: goal.id },
        run: None,
        step: None,
        instructions: "build the thing".into(),
        state: WorkItemState::Open,
        project: None,
        harness_candidates: vec!["mock".into()],
        model: None,
        effort: None,
        output_schema: None,
        budget: Default::default(),
        assignees: vec![],
        tier_ceiling: bisa_core::ToolTier::Write,
        agent: None,
        spawn_allowlist: vec![],
        depth_budget: 0,
        result_attempts: 0,
        interruptions: 0,
    };
    ws.put_work_item(&spec).unwrap();
    spec
}

#[test]
fn eight_claimers_one_claim() {
    let dir = tempfile::tempdir().unwrap();
    let ws = Arc::new(
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap(),
    );
    let spec = open_item(&ws);
    let home = spec.home;
    let id = spec.id;

    let barrier = Arc::new(std::sync::Barrier::new(8));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let ws = Arc::clone(&ws);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                let keys = nostr::key::Keys::generate();
                let session =
                    SessionId::from_ulid(ulid::Ulid::from_datetime(std::time::SystemTime::now()));
                barrier.wait();
                ws.claim_work_item(&home, id, "mock", session, &keys, None)
                    .is_ok()
            })
        })
        .collect();
    let won: usize = handles
        .into_iter()
        .map(|h| usize::from(h.join().unwrap()))
        .sum();
    assert_eq!(won, 1, "exactly one claimer wins");

    let claims = ws
        .journal(&home)
        .unwrap()
        .into_iter()
        .filter(|e| matches!(e.payload, JournalPayload::Claim { .. }))
        .count();
    assert_eq!(claims, 1, "one claim fact, however many tried");
    assert!(matches!(
        ws.get_work_item(&home, id).unwrap().state,
        WorkItemState::Claimed { .. }
    ));
}

#[test]
fn a_second_claim_after_the_first_is_refused_and_journals_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    let spec = open_item(&ws);
    let session = || SessionId::from_ulid(ulid::Ulid::from_datetime(std::time::SystemTime::now()));
    ws.claim_work_item(
        &spec.home,
        spec.id,
        "mock",
        session(),
        &nostr::key::Keys::generate(),
        None,
    )
    .expect("first claim");
    let before = ws.journal(&spec.home).unwrap().len();
    let err = ws
        .claim_work_item(
            &spec.home,
            spec.id,
            "mock",
            session(),
            &nostr::key::Keys::generate(),
            None,
        )
        .expect_err("a claimed item is not open");
    assert!(err.is_refusal(), "{err}");
    assert_eq!(
        ws.journal(&spec.home).unwrap().len(),
        before,
        "a refused claim leaves no fact"
    );
}

/// A cancel is never lost to a move made at the same moment. The executor
/// blocks and unblocks an item while a boundary event cancels it: each move
/// is a read, a check and a write, and without one writer at a time the
/// slower of two would write the state it read before the other's — a
/// cancelled item back in progress, its session gone.
#[test]
fn a_cancel_racing_other_moves_is_the_state_the_item_ends_in() {
    let dir = tempfile::tempdir().unwrap();
    let ws = Arc::new(
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap(),
    );
    let by = PrincipalId::new(nostr::key::Keys::generate().public_key().to_hex()).unwrap();
    for round in 0..24 {
        let spec = open_item(&ws);
        let (home, id) = (spec.home, spec.id);
        ws.transition_work_item(&home, id, &WorkItemTransition::Claim { by: by.clone() })
            .unwrap();
        ws.transition_work_item(&home, id, &WorkItemTransition::Start)
            .unwrap();

        let barrier = Arc::new(std::sync::Barrier::new(4));
        let movers: Vec<_> = (0..3)
            .map(|_| {
                let (ws, barrier) = (Arc::clone(&ws), Arc::clone(&barrier));
                std::thread::spawn(move || {
                    barrier.wait();
                    for _ in 0..6 {
                        // Refused once the item is cancelled: nothing moves then.
                        let blocked = ws.transition_work_item(
                            &home,
                            id,
                            &WorkItemTransition::Block {
                                reason: "session aborted".into(),
                            },
                        );
                        if blocked.is_ok() {
                            let _refused_once_cancelled =
                                ws.transition_work_item(&home, id, &WorkItemTransition::Unblock);
                        }
                    }
                })
            })
            .collect();
        let canceller = {
            let (ws, barrier) = (Arc::clone(&ws), Arc::clone(&barrier));
            std::thread::spawn(move || {
                barrier.wait();
                ws.transition_work_item(&home, id, &WorkItemTransition::Cancel)
            })
        };
        canceller
            .join()
            .unwrap()
            .expect("a live item can be cancelled");
        for mover in movers {
            mover.join().unwrap();
        }
        assert_eq!(
            ws.get_work_item(&home, id).unwrap().state,
            WorkItemState::Cancelled,
            "round {round}: the cancel was written over"
        );
    }
}
