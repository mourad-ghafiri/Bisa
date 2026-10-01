//! Listening and a run's way in, as the store keeps them: a workflow turned
//! On and a save that would break its public hook, a goal that listens with
//! a library workflow, what goes with a host that is gone, a closed goal's
//! queue, the start a run begins at, the signal that made it, and a standing
//! goal's run history. Every scenario is a temporary directory and an
//! in-memory keystore; nothing of the machine's is touched.

use bisa_core::event::JournalPayload;
use bisa_core::{
    Chain, ClosureReason, Flow, Join, ListenerHost, ListenerKey, Listening, OnFail, RunEntry,
    RunScope, Signal, SignalScope, SignalSource, StartOn, Step, StepId, StepKind, StepState, Tags,
    WorkflowId, WorkflowOrigin, DEFAULT_MAX_VISITS, GOAL_RUNS_KEPT,
};
use bisa_store::{
    ListenerRuntime, MemoryKeyStore, NewGoal, NewWorkflow, Paths, SignalState, StoreError,
    Workspace,
};
use std::collections::BTreeMap;

fn ws() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    (dir, ws)
}

fn sid(s: &str) -> StepId {
    StepId::new(s).unwrap()
}

fn step(id: &str, kind: StepKind, then: &[&str]) -> Step {
    Step {
        id: sid(id),
        name: id.into(),
        kind,
        then: then.iter().map(|t| Flow::to(sid(t))).collect(),
        boundaries: vec![],
        join: Join::All,
        on_fail: OnFail::Fail,
        retries: 0,
        max_visits: DEFAULT_MAX_VISITS,
        position: None,
    }
}

fn start(id: &str, on: StartOn, then: &[&str]) -> Step {
    step(
        id,
        StepKind::Start {
            on,
            inputs: BTreeMap::new(),
            guard: Default::default(),
        },
        then,
    )
}

fn post() -> Step {
    step(
        "post",
        StepKind::Notify {
            scope: None,
            template: "heard".into(),
            mentions: vec![],
            author: None,
        },
        &["end"],
    )
}

fn end() -> Step {
    step(
        "end",
        StepKind::End {
            finish: bisa_core::Finish::Path,
        },
        &[],
    )
}

fn workflow(name: &str, steps: Vec<Step>) -> NewWorkflow {
    NewWorkflow {
        name: name.into(),
        description: String::new(),
        inputs: vec![],
        steps,
        tags: Tags::default(),
        decision_making: false,
    }
}

/// By hand, and on a call — `public` says whether from outside.
fn hooked(public: bool) -> NewWorkflow {
    workflow(
        "Answer the hook",
        vec![
            start("start", StartOn::Manual, &["post"]),
            start("ticket", StartOn::Hook { public }, &["post"]),
            post(),
            end(),
        ],
    )
}

/// Only on a call: nothing runs it by hand.
fn event_only() -> NewWorkflow {
    workflow(
        "Only on a call",
        vec![
            start("ticket", StartOn::Hook { public: false }, &["post"]),
            post(),
            end(),
        ],
    )
}

fn on(since: u64) -> Listening {
    Listening {
        inputs: BTreeMap::new(),
        budget: None,
        since,
        paused: None,
    }
}

fn library(wf: WorkflowId) -> ListenerHost {
    ListenerHost::Workspace { workflow: wf }
}

fn key(host: ListenerHost, step: &str) -> ListenerKey {
    ListenerKey {
        host,
        step: sid(step),
    }
}

fn signal(id: &str, listener: Option<ListenerKey>, scope: SignalScope) -> Signal {
    Signal {
        id: id.into(),
        listener,
        source: SignalSource::Hook,
        name: None,
        at: 10,
        payload: serde_json::json!({ "ticket": "the printer is on fire" }),
        scope,
        chain: Chain::default(),
        dedupe_key: Some(format!("delivery-{id}")),
    }
}

fn without_ticket(wf: &bisa_core::Workflow) -> NewWorkflow {
    let mut body = NewWorkflow::from(wf);
    body.steps.retain(|s| s.id.as_str() != "ticket");
    body
}

/// A public hook is a door someone outside was handed, with its secret: a
/// save — or a draft — that removes it, renames it or makes it local while
/// the workflow is On is refused, naming the start. Any other edit lands, a
/// local hook may go, and once Off the public one may too.
#[test]
fn a_save_that_breaks_a_listened_public_hook_is_refused() {
    let (_d, ws) = ws();
    let wf = ws
        .create_workflow(hooked(true), WorkflowOrigin::Workspace)
        .unwrap();
    ws.set_listening(&library(wf.id), Some(on(1))).unwrap();

    let refused = |body: NewWorkflow, revision: u64| {
        let err = ws
            .update_workflow(wf.id, body.clone(), revision)
            .unwrap_err();
        assert!(matches!(err, StoreError::StillUsed(_)), "{err:?}");
        assert!(err.is_refusal());
        assert!(err.to_string().contains("ticket"), "{err}");
        assert!(matches!(
            ws.save_workflow_draft(wf.id, body, revision),
            Err(StoreError::StillUsed(_))
        ));
    };
    refused(without_ticket(&wf), wf.revision);
    let mut local = NewWorkflow::from(&wf);
    local.steps[1].kind = StepKind::Start {
        on: StartOn::Hook { public: false },
        inputs: BTreeMap::new(),
        guard: Default::default(),
    };
    refused(local, wf.revision);
    let mut renamed = NewWorkflow::from(&wf);
    renamed.steps[1].id = sid("tickets");
    refused(renamed, wf.revision);
    assert_eq!(
        ws.get_workflow(wf.id).unwrap().revision,
        wf.revision,
        "nothing was written"
    );

    let mut described = NewWorkflow::from(&wf);
    described.description = "answers the help desk".into();
    let wf = ws.update_workflow(wf.id, described, wf.revision).unwrap();

    ws.set_listening(&library(wf.id), None).unwrap();
    let wf = ws
        .update_workflow(wf.id, without_ticket(&wf), wf.revision)
        .unwrap();
    assert!(wf.steps.iter().all(|s| s.id.as_str() != "ticket"));

    // A local hook's caller is this machine: it may go while On.
    let local = ws
        .create_workflow(hooked(false), WorkflowOrigin::Workspace)
        .unwrap();
    ws.set_listening(&library(local.id), Some(on(2))).unwrap();
    ws.update_workflow(local.id, without_ticket(&local), local.revision)
        .unwrap();
}

/// A goal that listens with a library workflow holds its public hook the
/// same way; the goal's own switch, not the workflow's, is what counts.
#[test]
fn a_goal_listening_with_a_library_workflow_holds_its_public_hook() {
    let (_d, ws) = ws();
    let wf = ws
        .create_workflow(hooked(true), WorkflowOrigin::Workspace)
        .unwrap();
    let goal = ws
        .create_goal(NewGoal::captured("answer the desk"))
        .unwrap();
    ws.set_goal_workflow(goal.id, Some(wf.id)).unwrap();
    let host = ListenerHost::Goal { goal: goal.id };
    ws.set_listening(&host, Some(on(3))).unwrap();
    assert_eq!(ws.list_listening().unwrap(), vec![(host, on(3))]);
    assert!(matches!(
        ws.update_workflow(wf.id, without_ticket(&wf), wf.revision),
        Err(StoreError::StillUsed(_))
    ));
    ws.set_listening(&host, None).unwrap();
    ws.update_workflow(wf.id, without_ticket(&wf), wf.revision)
        .unwrap();
}

/// A workflow deleted takes what it listened with: its record, its
/// listeners' memories, its hook secrets, its signals in the index.
#[test]
fn deleting_a_workflow_forgets_what_it_listened_with() {
    let (dir, ws) = ws();
    let wf = ws
        .create_workflow(hooked(true), WorkflowOrigin::Workspace)
        .unwrap();
    let host = library(wf.id);
    ws.set_listening(&host, Some(on(1))).unwrap();
    let ticket = key(host, "ticket");
    assert!(ws.ensure_hook_secret(&ticket).unwrap().is_some());
    let memory = ListenerRuntime {
        last_dispatch_at: Some(9),
        ..ListenerRuntime::default()
    };
    ws.put_listener_runtime(&ticket, &memory).unwrap();
    ws.enqueue_signal(&signal("s1", Some(ticket.clone()), SignalScope::Workspace))
        .unwrap();
    assert_eq!(ws.list_signals(Some(&host), 10).unwrap().len(), 1);

    ws.delete_workflow(wf.id).unwrap();
    assert!(!Paths::new(dir.path()).listening_file(wf.id).exists());
    assert!(!ws.has_hook_secret(&ticket).unwrap());
    assert_eq!(ws.listener_runtime(&ticket), ListenerRuntime::default());
    assert!(ws.list_signals(Some(&host), 10).unwrap().is_empty());
    assert!(ws.list_listening().unwrap().is_empty());
    ws.rebuild_index().unwrap();
    assert!(
        ws.list_signals(Some(&host), 10).unwrap().is_empty(),
        "a rebuild does not bring a gone host's signals back"
    );
}

/// A goal deleted takes its listeners too: the secret of its design's hook
/// start, its queued signals.
#[test]
fn deleting_a_goal_forgets_its_listeners() {
    let (_d, ws) = ws();
    let goal = ws
        .create_goal(NewGoal::captured("answer the desk"))
        .unwrap();
    let design = ws
        .create_workflow(hooked(true), WorkflowOrigin::Goal { goal: goal.id })
        .unwrap();
    ws.set_goal_workflow(goal.id, Some(design.id)).unwrap();
    let host = ListenerHost::Goal { goal: goal.id };
    ws.set_listening(&host, Some(on(4))).unwrap();
    let ticket = key(host, "ticket");
    ws.ensure_hook_secret(&ticket).unwrap();
    ws.enqueue_signal(&signal(
        "s1",
        Some(ticket.clone()),
        SignalScope::Goal { goal: goal.id },
    ))
    .unwrap();

    ws.delete_goal(goal.id).unwrap();
    assert!(!ws.has_hook_secret(&ticket).unwrap());
    assert!(ws.list_signals(Some(&host), 10).unwrap().is_empty());
    assert!(ws.list_listening().unwrap().is_empty());
}

/// A closed goal hears nothing more: its listening ends with the close, what
/// its events queued settles `skipped`, saying why, and it cannot listen
/// again.
#[test]
fn closing_a_goal_ends_its_listening_and_settles_its_queue() {
    let (_d, ws) = ws();
    let wf = ws
        .create_workflow(hooked(false), WorkflowOrigin::Workspace)
        .unwrap();
    let goal = ws
        .create_goal(NewGoal::captured("answer the desk"))
        .unwrap();
    ws.set_goal_workflow(goal.id, Some(wf.id)).unwrap();
    let host = ListenerHost::Goal { goal: goal.id };
    ws.set_listening(&host, Some(on(5))).unwrap();
    let ticket = key(host, "ticket");
    for id in ["s1", "s2"] {
        ws.enqueue_signal(&signal(
            id,
            Some(ticket.clone()),
            SignalScope::Goal { goal: goal.id },
        ))
        .unwrap();
    }
    assert_eq!(ws.pending_signals(&ticket).unwrap().len(), 2);

    let (closed, _) = ws
        .set_goal_closed(goal.id, ClosureReason::Abandoned { rationale: None })
        .unwrap();
    assert_eq!(closed.listening, None);
    assert!(ws.list_listening().unwrap().is_empty());
    assert!(ws.pending_signals(&ticket).unwrap().is_empty());
    for id in ["s1", "s2"] {
        let q = ws.signal(id).unwrap().unwrap();
        assert_eq!(q.state, SignalState::Skipped);
        assert_eq!(q.note.as_deref(), Some("the goal is closed"));
    }
    assert!(
        ws.set_listening(&host, Some(on(6))).is_err(),
        "a closed goal cannot listen"
    );
}

/// A run begins at one of its starts: the one an event names — the others
/// skipped, the event kept, the signal that made it named once — or, by
/// hand, the manual one. A step that is not a start is no way in.
#[test]
fn a_run_begins_at_one_of_its_starts_and_one_signal_makes_one_run() {
    let (_d, ws) = ws();
    let wf = ws
        .create_workflow(hooked(false), WorkflowOrigin::Workspace)
        .unwrap();
    let scope = || RunScope::Workspace {
        budget: Default::default(),
    };

    let err = ws
        .create_run(
            scope(),
            wf.id,
            BTreeMap::new(),
            RunEntry::at(sid("post"), None),
            None,
        )
        .unwrap_err();
    assert!(err.is_refusal());
    assert!(err.to_string().contains("post"), "{err}");

    let event = signal(
        "s1",
        Some(key(library(wf.id), "ticket")),
        SignalScope::Workspace,
    );
    let (run, _) = ws
        .create_run(
            scope(),
            wf.id,
            BTreeMap::new(),
            RunEntry::at(sid("ticket"), Some(event.clone())),
            Some("s1".into()),
        )
        .unwrap();
    assert_eq!(run.start, Some(sid("ticket")));
    assert_eq!(run.event, Some(event));
    assert_eq!(run.dispatched.as_deref(), Some("s1"));
    assert_eq!(run.steps[&sid("start")].state, StepState::Skipped);
    assert!(matches!(
        run.steps[&sid("ticket")].state,
        StepState::Done { .. }
    ));
    let journal = ws.journal(&run.home()).unwrap();
    assert!(
        journal.iter().any(|je| matches!(
            &je.payload,
            JournalPayload::Signal { signal, .. } if signal == "s1"
        )),
        "the occurrence that began the run is a fact on its home"
    );

    // The same signal dispatched again — a replay after a crash — is
    // refused, naming the run it already began.
    match ws.create_run(
        scope(),
        wf.id,
        BTreeMap::new(),
        RunEntry::at(sid("ticket"), None),
        Some("s1".into()),
    ) {
        Err(StoreError::AlreadyDispatched { signal, run: made }) => {
            assert_eq!(signal, "s1");
            assert_eq!(made, run.id.to_string());
        }
        other => panic!("{other:?}"),
    }

    let (by_hand, _) = ws
        .create_run(scope(), wf.id, BTreeMap::new(), RunEntry::by_hand(), None)
        .unwrap();
    assert_eq!(by_hand.start, Some(sid("start")));
    assert_eq!(by_hand.event, None);
    assert_eq!(by_hand.steps[&sid("ticket")].state, StepState::Skipped);
    assert!(ws
        .journal(&by_hand.home())
        .unwrap()
        .iter()
        .all(|je| !matches!(je.payload, JournalPayload::Signal { .. })));
}

/// A workflow only events begin has no way in by hand; a test run names one
/// of its starts.
#[test]
fn an_event_only_workflow_is_begun_at_a_start_it_names() {
    let (_d, ws) = ws();
    let wf = ws
        .create_workflow(event_only(), WorkflowOrigin::Workspace)
        .unwrap();
    let scope = || RunScope::Workspace {
        budget: Default::default(),
    };
    let err = ws
        .create_run(scope(), wf.id, BTreeMap::new(), RunEntry::by_hand(), None)
        .unwrap_err();
    assert!(err.is_refusal());
    assert!(err.to_string().contains("Only on a call"), "{err}");
    let (run, _) = ws
        .create_run(
            scope(),
            wf.id,
            BTreeMap::new(),
            RunEntry::at(sid("ticket"), None),
            None,
        )
        .unwrap();
    assert_eq!(run.start, Some(sid("ticket")));
}

/// A standing goal runs for a year: its snapshot keeps the newest runs, and
/// every run it ever had stays on disk for its history.
#[test]
fn a_goal_keeps_its_newest_runs_and_its_folder_keeps_every_one() {
    let (_d, ws) = ws();
    let wf = ws
        .create_workflow(
            workflow(
                "Quick",
                vec![start("start", StartOn::Manual, &["end"]), end()],
            ),
            WorkflowOrigin::Workspace,
        )
        .unwrap();
    let goal = ws.create_goal(NewGoal::captured("tick")).unwrap();
    let mut made = Vec::new();
    for _ in 0..GOAL_RUNS_KEPT + 2 {
        let (run, _) = ws
            .create_run(
                RunScope::Goal { goal: goal.id },
                wf.id,
                BTreeMap::new(),
                RunEntry::by_hand(),
                None,
            )
            .unwrap();
        assert!(run.is_finished(), "a start straight into an end finishes");
        made.push(run.id);
    }
    let goal = ws.get_goal(goal.id).unwrap();
    assert_eq!(goal.runs.len(), GOAL_RUNS_KEPT);
    assert_eq!(goal.runs, made[2..].to_vec(), "the newest, oldest first");
    assert_eq!(goal.run, made.last().copied());
    assert_eq!(ws.list_runs(goal.id).unwrap().len(), GOAL_RUNS_KEPT + 2);
}
