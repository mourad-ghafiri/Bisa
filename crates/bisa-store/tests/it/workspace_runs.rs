//! A run of the workspace: a workflow run with no goal behind it. It starts
//! the moment it is made, beside any other run of its workflow; it is its own
//! home — a folder under `workflows/runs/<RunId>/` holding its snapshot, its
//! journal, its work items, its ledger and its scratch; it spends against
//! the ceiling its scope carries; and it is its workflow's history, going
//! with the workflow when that is deleted.
//!
//! Every scenario is a workspace in a temporary directory.

use bisa_core::event::{JournalPayload, RunFact};
use bisa_core::{
    ActivityConcept, Budget, CancelCause, FileScope, Flow, Gate, Home, Join, OnFail, ProblemKind,
    RunEvent, RunScope, RunStatus, Step, StepId, StepKind, Tags, ToolTier, ValueRef, WaitFor,
    WorkItemId, WorkItemSpec, WorkItemState, Workflow, WorkflowOrigin, WorkflowRun,
    DEFAULT_MAX_VISITS,
};
use bisa_store::{
    approval_subject, EntryKind, MemoryKeyStore, NewGoal, NewWorkflow, StoreError, Workspace,
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

fn end() -> Step {
    step(
        "end",
        StepKind::End {
            finish: bisa_core::Finish::Done,
        },
        &[],
    )
}

fn workflow(name: &str, first: Step) -> NewWorkflow {
    NewWorkflow {
        name: name.into(),
        description: String::new(),
        inputs: vec![],
        steps: vec![first, end()],
        tags: Tags::default(),
        decision_making: false,
    }
}

/// wait a minute → end: a run of it is live, holding one armed wait.
fn waiting(ws: &Workspace, name: &str) -> Workflow {
    ws.create_workflow(
        workflow(
            name,
            step(
                "hold",
                StepKind::Wait {
                    until: WaitFor::Delay {
                        secs: ValueRef::Fixed(60),
                    },
                },
                &["end"],
            ),
        ),
        WorkflowOrigin::Workspace,
    )
    .unwrap()
}

/// approval → end: a run of it waits on its gate.
fn gated(ws: &Workspace) -> Workflow {
    ws.create_workflow(
        workflow(
            "Gated",
            step(
                "ship",
                StepKind::Approval {
                    prompt: "Ship?".into(),
                },
                &["end"],
            ),
        ),
        WorkflowOrigin::Workspace,
    )
    .unwrap()
}

fn in_the_workspace() -> RunScope {
    RunScope::Workspace {
        budget: Budget::default(),
    }
}

fn start(ws: &Workspace, wf: &Workflow) -> WorkflowRun {
    ws.create_run(
        in_the_workspace(),
        wf.id,
        BTreeMap::new(),
        bisa_core::RunEntry::by_hand(),
        None,
    )
    .unwrap()
    .0
}

fn item(run: &WorkflowRun, text: &str) -> WorkItemSpec {
    WorkItemSpec {
        id: WorkItemId::from_ulid(ulid::Ulid::from_datetime(std::time::SystemTime::now())),
        home: run.home(),
        run: Some(run.id),
        step: None,
        instructions: text.into(),
        state: WorkItemState::Open,
        project: None,
        harness_candidates: vec!["mock".into()],
        model: None,
        effort: None,
        output_schema: None,
        budget: Default::default(),
        assignees: vec![],
        tier_ceiling: ToolTier::Write,
        agent: None,
        spawn_allowlist: vec![],
        depth_budget: 0,
        result_attempts: 0,
        interruptions: 0,
    }
}

fn ids(runs: Vec<WorkflowRun>) -> Vec<bisa_core::RunId> {
    runs.iter().map(|r| r.id).collect()
}

#[test]
fn a_workspace_run_starts_at_once_in_a_folder_of_its_own_with_no_goal() {
    let (_dir, ws) = ws();
    let wf = waiting(&ws, "Hold");
    let (run, effects) = ws
        .create_run(
            in_the_workspace(),
            wf.id,
            BTreeMap::new(),
            bisa_core::RunEntry::by_hand(),
            None,
        )
        .unwrap();
    assert!(!effects.is_empty(), "the start's effects: {effects:?}");
    assert_eq!(run.status(), RunStatus::Waiting, "started, not queued");
    assert!(run.started_at.is_some());
    assert_eq!(run.scope.goal(), None);
    assert_eq!(run.home(), Home::Run { run: run.id });

    let folder = ws.paths().home(&run.home());
    assert!(
        folder.dir().starts_with(ws.paths().workspace_runs_dir()),
        "{}",
        folder.dir().display()
    );
    assert!(folder.run_snapshot(run.id).is_file(), "its snapshot");
    assert!(folder.journal().is_file(), "its journal");
    assert!(ws.list_goals(None).unwrap().is_empty(), "no goal was made");
    assert_eq!(ws.get_run(run.id).unwrap(), run);
    assert_eq!(ws.list_armed_waits().unwrap()[0].0, run.id);
}

#[test]
fn two_runs_of_one_workflow_go_at_once_and_neither_queues() {
    let (_dir, ws) = ws();
    let wf = waiting(&ws, "Hold");
    let first = start(&ws, &wf);
    let second = start(&ws, &wf);
    assert_ne!(first.id, second.id);
    assert!(first.is_live() && second.is_live());
    assert_eq!(
        ids(ws.live_workspace_runs(Some(wf.id)).unwrap()),
        vec![first.id, second.id]
    );
    // A goal's run of the same workflow is the goal's, and queues as ever.
    let goal = ws.create_goal(NewGoal::captured("on a goal")).unwrap();
    let (on_goal, _) = ws
        .create_run(
            RunScope::Goal { goal: goal.id },
            wf.id,
            BTreeMap::new(),
            bisa_core::RunEntry::by_hand(),
            None,
        )
        .unwrap();
    let (queued, _) = ws
        .create_run(
            RunScope::Goal { goal: goal.id },
            wf.id,
            BTreeMap::new(),
            bisa_core::RunEntry::by_hand(),
            None,
        )
        .unwrap();
    assert_eq!(queued.status(), RunStatus::Queued);
    let listed = ids(ws.list_workflow_runs(wf.id).unwrap());
    assert_eq!(listed, vec![first.id, second.id]);
    assert!(!listed.contains(&on_goal.id) && !listed.contains(&queued.id));
    // Stopping one leaves the other going.
    ws.record_run_event(
        first.id,
        RunEvent::Cancel {
            cause: CancelCause::Stopped { rationale: None },
        },
    )
    .unwrap();
    assert_eq!(
        ids(ws.live_workspace_runs(Some(wf.id)).unwrap()),
        vec![second.id]
    );
    assert_eq!(
        ids(ws.list_workflow_runs(wf.id).unwrap()),
        vec![first.id, second.id],
        "a stopped run is history"
    );
}

#[test]
fn a_workspace_runs_facts_are_its_own_under_its_coordinate() {
    let (_dir, ws) = ws();
    let wf = waiting(&ws, "Hold");
    let run = start(&ws, &wf);
    let home = run.home();
    let facts: Vec<String> = ws
        .journal(&home)
        .unwrap()
        .into_iter()
        .filter_map(|je| {
            assert_eq!(je.home, home, "every fact names its home");
            match je.payload {
                JournalPayload::Run { event, .. } => Some(format!("run:{}", event.as_str())),
                JournalPayload::Step { step, event, .. } => {
                    Some(format!("{step}:{}", event.as_str()))
                }
                _ => None,
            }
        })
        .collect();
    assert_eq!(facts, vec!["run:started", "hold:waiting"]);

    // On disk, each line is addressed to the run: `33413:<owner>:<run>`.
    let want = format!(
        "{}:{}:{}",
        bisa_core::kind::KIND_WORKFLOW_RUN,
        ws.owner_principal().as_hex(),
        run.id
    );
    let raw = std::fs::read_to_string(ws.paths().home(&home).journal()).unwrap();
    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        let event: nostr::event::Event = serde_json::from_str(line).unwrap();
        assert!(
            event.tags.iter().any(|t| {
                let t = t.as_slice();
                t.len() >= 2 && t[0] == "a" && t[1] == want
            }),
            "{line}"
        );
    }

    // A note an agent leaves lands on the run too — and nowhere else.
    ws.append_journal(
        &home,
        JournalPayload::Note {
            text: "checked the feed".into(),
        },
        ws.owner_keys(),
        None,
    )
    .unwrap();
    assert_eq!(ws.journal(&home).unwrap().len(), 3);
    let ghost = Home::Run {
        run: bisa_core::RunId::from_ulid(ulid::Ulid::from_parts(9, 9)),
    };
    assert!(matches!(
        ws.append_journal(
            &ghost,
            JournalPayload::Note {
                text: "nobody's".into()
            },
            ws.owner_keys(),
            None
        ),
        Err(StoreError::RunNotFound(_))
    ));
}

#[test]
fn a_workspace_runs_work_items_and_ledger_are_filed_in_its_folder() {
    let (_dir, ws) = ws();
    let wf = waiting(&ws, "Hold");
    let run = start(&ws, &wf);
    let home = run.home();
    let spec = item(&run, "gather the numbers");
    ws.put_work_item(&spec).unwrap();
    assert_eq!(ws.home_of_work_item(spec.id).unwrap(), home);
    assert_eq!(ws.list_work_items(&home).unwrap(), vec![spec.clone()]);
    assert_eq!(
        ws.work_item_root(spec.id).unwrap(),
        ws.paths().home(&home).scratch(),
        "with no workstream, an item works in its run's scratch"
    );
    ws.transition_work_item(&home, spec.id, &bisa_core::WorkItemTransition::Cancel)
        .unwrap();
    assert_eq!(
        ws.get_work_item(&home, spec.id).unwrap().state,
        WorkItemState::Cancelled
    );

    ws.add_spend(&home, 40, 2, 5).unwrap();
    ws.add_spend(&home, 10, 1, 1).unwrap();
    let spent = ws.spent(&home).unwrap();
    assert_eq!(
        (spent.tokens, spent.usd_cents, spent.wall_clock_secs),
        (50, 3, 6)
    );
    let ledger = std::fs::read_to_string(ws.paths().home(&home).ledger()).unwrap();
    assert_eq!(
        ledger.lines().count(),
        2,
        "one line per charge, in its folder"
    );

    // An item filed under a run that is not there is refused.
    let mut stray = item(&run, "nowhere");
    stray.home = Home::Run {
        run: bisa_core::RunId::from_ulid(ulid::Ulid::from_parts(9, 9)),
    };
    assert!(ws.put_work_item(&stray).is_err());
}

#[test]
fn a_workspace_run_spends_against_the_ceiling_its_scope_carries() {
    let (_dir, ws) = ws();
    let wf = waiting(&ws, "Hold");
    let (run, _) = ws
        .create_run(
            RunScope::Workspace {
                budget: Budget {
                    max_tokens: Some(100),
                    max_usd_cents: None,
                    max_wall_clock_secs: None,
                },
            },
            wf.id,
            BTreeMap::new(),
            bisa_core::RunEntry::by_hand(),
            None,
        )
        .unwrap();
    let home = run.home();
    assert_eq!(run.scope.budget().and_then(|b| b.max_tokens), Some(100));
    assert!(ws.budget_allows(&home).unwrap());
    ws.add_spend(&home, 60, 0, 0).unwrap();
    assert!(ws.budget_allows(&home).unwrap());
    ws.add_spend(&home, 60, 0, 0).unwrap();
    assert!(!ws.budget_allows(&home).unwrap(), "over its own ceiling");
    // Another run of the same workflow has its own ledger.
    let other = start(&ws, &wf);
    assert!(ws.budget_allows(&other.home()).unwrap());
    assert_eq!(ws.spent(&other.home()).unwrap().tokens, 0);
}

#[test]
fn a_workspace_runs_gate_is_decided_through_its_own_journal() {
    let (_dir, ws) = ws();
    let wf = gated(&ws);
    let run = start(&ws, &wf);
    assert_eq!(run.status(), RunStatus::Waiting);
    let home = run.home();
    let approval = ws
        .record_decision(
            &home,
            Gate::Approval,
            true,
            &approval_subject(run.id, &sid("ship")),
            None,
            None,
        )
        .unwrap();
    let (done, _) = ws
        .record_run_event(
            run.id,
            RunEvent::Decided {
                step: sid("ship"),
                approve: true,
                approval,
            },
        )
        .unwrap();
    assert_eq!(done.status(), RunStatus::Done);
    let decided = ws.decisions().unwrap();
    assert_eq!(decided.len(), 1);
    assert_eq!(
        decided[0].run_id.as_deref(),
        Some(run.id.to_string().as_str())
    );
    assert_eq!(decided[0].goal_id, None);
    assert!(ws.journal(&home).unwrap().iter().any(|je| matches!(
        je.payload,
        JournalPayload::Run {
            event: RunFact::Finished { .. },
            ..
        }
    )));

    // A decision recorded on another run is not this one's.
    let second = start(&ws, &wf);
    let elsewhere = ws
        .record_decision(
            &home,
            Gate::Approval,
            true,
            &approval_subject(second.id, &sid("ship")),
            None,
            None,
        )
        .unwrap();
    assert!(matches!(
        ws.record_run_event(
            second.id,
            RunEvent::Decided {
                step: sid("ship"),
                approve: true,
                approval: elsewhere,
            },
        ),
        Err(StoreError::GateDecisionInvalid(_))
    ));
}

#[test]
fn a_workspace_runs_facts_file_under_its_workflow_in_the_feed() {
    let (_dir, ws) = ws();
    let wf = waiting(&ws, "Hold");
    let run = start(&ws, &wf);
    let rows = ws
        .activity_page(Some(ActivityConcept::Workflows), None, 50)
        .unwrap();
    let mine: Vec<_> = rows
        .iter()
        .filter(|r| r.source_id == wf.id.to_string())
        .collect();
    assert!(!mine.is_empty(), "{rows:?}");
    assert!(mine.iter().all(|r| r.source_kind == "workflow"));
    assert!(mine.iter().any(|r| r.event.contains(&run.id.to_string())));
    assert!(ws
        .activity_page(Some(ActivityConcept::Goals), None, 50)
        .unwrap()
        .is_empty());
}

#[test]
fn a_workspace_run_refuses_a_goals_design_an_archived_workflow_and_a_goal_reading_one() {
    let (_dir, ws) = ws();
    let goal = ws.create_goal(NewGoal::captured("designed for")).unwrap();
    let design = ws
        .create_workflow(
            workflow(
                "Design",
                step(
                    "hold",
                    StepKind::Wait {
                        until: WaitFor::Delay {
                            secs: ValueRef::Fixed(60),
                        },
                    },
                    &["end"],
                ),
            ),
            WorkflowOrigin::Goal { goal: goal.id },
        )
        .unwrap();
    let err = ws
        .create_run(
            in_the_workspace(),
            design.id,
            BTreeMap::new(),
            bisa_core::RunEntry::by_hand(),
            None,
        )
        .unwrap_err();
    assert!(err.is_refusal(), "{err}");
    assert!(
        err.to_string().contains("promote it to the library"),
        "{err}"
    );

    let archived = waiting(&ws, "Old");
    ws.set_workflow_archived(archived.id, true).unwrap();
    let err = ws
        .create_run(
            in_the_workspace(),
            archived.id,
            BTreeMap::new(),
            bisa_core::RunEntry::by_hand(),
            None,
        )
        .unwrap_err();
    assert!(err.to_string().contains("archived"), "{err}");

    let reads_goal = ws
        .create_workflow(
            workflow(
                "Reads its goal",
                step(
                    "post",
                    StepKind::Notify {
                        scope: None,
                        template: "working on {goal.statement}".into(),
                        mentions: vec![],
                        author: None,
                    },
                    &["end"],
                ),
            ),
            WorkflowOrigin::Workspace,
        )
        .unwrap();
    let err = ws
        .create_run(
            in_the_workspace(),
            reads_goal.id,
            BTreeMap::new(),
            bisa_core::RunEntry::by_hand(),
            None,
        )
        .unwrap_err();
    let StoreError::WorkflowInvalid(problems) = &err else {
        panic!("{err:?}");
    };
    assert_eq!(
        problems.iter().map(|p| p.kind).collect::<Vec<_>>(),
        vec![ProblemKind::NeedsGoal]
    );
    // The same definition runs on a goal, which has one to read.
    ws.create_run(
        RunScope::Goal { goal: goal.id },
        reads_goal.id,
        BTreeMap::new(),
        bisa_core::RunEntry::by_hand(),
        None,
    )
    .unwrap();

    // None of the refusals wrote a run of the workspace.
    for wf in [design.id, archived.id, reads_goal.id] {
        assert!(ws.list_workflow_runs(wf).unwrap().is_empty());
    }
    let folders = match std::fs::read_dir(ws.paths().workspace_runs_dir()) {
        Ok(entries) => entries.count(),
        Err(_) => 0,
    };
    assert_eq!(folders, 0, "no folder was made for a refused run");
}

#[test]
fn deleting_a_workflow_takes_its_finished_runs_and_is_refused_while_one_goes() {
    let (_dir, ws) = ws();
    let wf = waiting(&ws, "Hold");
    let done = start(&ws, &wf);
    ws.record_run_event(
        done.id,
        RunEvent::Cancel {
            cause: CancelCause::Stopped { rationale: None },
        },
    )
    .unwrap();
    let live = start(&ws, &wf);
    let spec = item(&live, "in flight");
    ws.put_work_item(&spec).unwrap();

    let err = ws.delete_workflow(wf.id).unwrap_err();
    assert!(matches!(err, StoreError::StillUsed(_)), "{err:?}");
    assert!(err.to_string().contains(&live.id.to_string()), "{err}");
    assert!(ws.get_workflow(wf.id).is_ok(), "nothing went");
    assert!(ws.paths().home(&done.home()).dir().is_dir());

    // Retired — what the engine does first — the delete takes them all.
    ws.record_run_event(
        live.id,
        RunEvent::Cancel {
            cause: CancelCause::Retired,
        },
    )
    .unwrap();
    assert_eq!(
        ws.get_run(live.id).unwrap().cancelled,
        Some(CancelCause::Retired)
    );
    ws.delete_workflow(wf.id).unwrap();
    for run in [&done, &live] {
        assert!(
            !ws.paths().home(&run.home()).dir().exists(),
            "its folder went"
        );
        assert!(matches!(
            ws.get_run(run.id),
            Err(StoreError::RunNotFound(_))
        ));
    }
    assert!(
        ws.home_of_work_item(spec.id).is_err(),
        "its items' rows went"
    );
    assert!(ws.live_workspace_runs(None).unwrap().is_empty());
}

/// The bound on a workflow's history: the oldest finished runs beyond
/// `workflow.runs.keep` go — folder and rows, oldest first — and a run that
/// is going is neither counted nor touched. Another workflow's history is
/// another workflow's. The bound is never read below one.
#[test]
fn the_oldest_finished_runs_beyond_the_bound_are_put_away_and_a_live_one_never_is() {
    let (_dir, ws) = ws();
    let wf = waiting(&ws, "Hold");
    let other = waiting(&ws, "Other");
    let stop = |run: &WorkflowRun| {
        ws.record_run_event(
            run.id,
            RunEvent::Cancel {
                cause: CancelCause::Stopped { rationale: None },
            },
        )
        .unwrap();
    };
    // Three finished, oldest first, then one live; one finished run of
    // another workflow.
    let mut finished = Vec::new();
    for _ in 0..3 {
        let run = start(&ws, &wf);
        stop(&run);
        finished.push(run);
    }
    let live = start(&ws, &wf);
    let spec = item(&live, "in flight");
    ws.put_work_item(&spec).unwrap();
    let elsewhere = start(&ws, &other);
    stop(&elsewhere);

    assert_eq!(ws.workspace_runs_kept().unwrap(), 500, "the default");
    assert!(
        ws.forget_finished_workspace_runs_beyond(wf.id, 3)
            .unwrap()
            .is_empty(),
        "three finished, three kept: nothing goes — the live one is not counted"
    );

    ws.set_setting(
        bisa_core::settings::Scope::Workspace,
        None,
        WorkflowRun::SETTING_KEPT,
        serde_json::json!(1),
    )
    .unwrap();
    let keep = ws.workspace_runs_kept().unwrap();
    assert_eq!(keep, 1);
    let gone = ws
        .forget_finished_workspace_runs_beyond(wf.id, keep)
        .unwrap();
    assert_eq!(
        gone,
        vec![finished[0].id, finished[1].id],
        "the two oldest, oldest first"
    );
    for run in &finished[..2] {
        assert!(
            !ws.paths().home(&run.home()).dir().exists(),
            "its folder went"
        );
        assert!(matches!(
            ws.get_run(run.id),
            Err(StoreError::RunNotFound(_))
        ));
    }
    assert_eq!(
        ids(ws.list_workflow_runs(wf.id).unwrap()),
        vec![finished[2].id, live.id],
        "the newest finished one and the live one stay"
    );
    assert!(ws.paths().home(&live.home()).dir().is_dir());
    assert_eq!(ws.home_of_work_item(spec.id).unwrap(), live.home());
    assert_eq!(
        ids(ws.list_workflow_runs(other.id).unwrap()),
        vec![elsewhere.id],
        "another workflow's history is untouched"
    );
    // Asked for a bound of nothing, one is still kept.
    assert!(ws
        .forget_finished_workspace_runs_beyond(other.id, 0)
        .unwrap()
        .is_empty());
    // A rebuild finds no trace of what went.
    ws.rebuild_index().unwrap();
    assert_eq!(
        ids(ws.list_workflow_runs(wf.id).unwrap()),
        vec![finished[2].id, live.id]
    );
}

#[test]
fn archiving_a_workflow_is_refused_while_one_of_its_runs_goes_and_keeps_the_rest() {
    let (_dir, ws) = ws();
    let wf = waiting(&ws, "Hold");
    let live = start(&ws, &wf);

    let err = ws.set_workflow_archived(wf.id, true).unwrap_err();
    assert!(matches!(err, StoreError::StillUsed(_)), "{err:?}");
    assert!(err.to_string().contains(&live.id.to_string()), "{err}");
    assert!(!ws.get_workflow(wf.id).unwrap().is_archived());

    // Retired first — what the engine does — the archive goes through, and
    // the run stays as the workflow's history.
    ws.record_run_event(
        live.id,
        RunEvent::Cancel {
            cause: CancelCause::Retired,
        },
    )
    .unwrap();
    assert!(ws.set_workflow_archived(wf.id, true).unwrap().is_archived());
    assert_eq!(ids(ws.list_workflow_runs(wf.id).unwrap()), vec![live.id]);
    assert!(ws.paths().home(&live.home()).dir().is_dir());
    // Taking it back out is never refused.
    assert!(!ws
        .set_workflow_archived(wf.id, false)
        .unwrap()
        .is_archived());
}

#[test]
fn a_rebuild_finds_every_workspace_run_again_from_its_folder() {
    let (_dir, ws) = ws();
    let wf = waiting(&ws, "Hold");
    let live = start(&ws, &wf);
    let done = start(&ws, &wf);
    ws.record_run_event(
        done.id,
        RunEvent::Cancel {
            cause: CancelCause::Stopped { rationale: None },
        },
    )
    .unwrap();
    let home = live.home();
    let spec = item(&live, "kept");
    ws.put_work_item(&spec).unwrap();
    ws.add_spend(&home, 7, 1, 2).unwrap();
    let gated_wf = gated(&ws);
    let gate_run = start(&ws, &gated_wf);
    ws.record_decision(
        &gate_run.home(),
        Gate::Approval,
        true,
        &approval_subject(gate_run.id, &sid("ship")),
        None,
        None,
    )
    .unwrap();

    let history = ids(ws.list_workflow_runs(wf.id).unwrap());
    let spent = ws.spent(&home).unwrap();
    let armed = ws.list_armed_waits().unwrap();
    let feed = ws.activity_page(None, None, 200).unwrap().len();
    ws.rebuild_index().unwrap();
    assert_eq!(ids(ws.list_workflow_runs(wf.id).unwrap()), history);
    assert_eq!(
        ids(ws.live_workspace_runs(Some(wf.id)).unwrap()),
        vec![live.id]
    );
    assert_eq!(ws.spent(&home).unwrap(), spent);
    assert_eq!(ws.list_armed_waits().unwrap(), armed);
    assert_eq!(ws.home_of_work_item(spec.id).unwrap(), home);
    assert_eq!(ws.decisions().unwrap().len(), 1);
    assert_eq!(
        ws.decisions().unwrap()[0].run_id.as_deref(),
        Some(gate_run.id.to_string().as_str())
    );
    assert_eq!(
        ws.activity_page(None, None, 200).unwrap().len(),
        feed,
        "the journal's rows replay once each"
    );
}

#[test]
fn a_workspace_runs_folder_is_a_file_scope_of_its_own() {
    let (_dir, ws) = ws();
    let wf = waiting(&ws, "Hold");
    let run = start(&ws, &wf);
    let id = run.id.to_string();
    assert_eq!(
        ws.file_root(FileScope::Run, &id).unwrap(),
        ws.paths().home(&run.home()).dir().to_path_buf()
    );
    let tree = ws.list_tree(FileScope::Run, &id, "", Some(2)).unwrap();
    let kind_of = |name: &str| {
        tree.entries
            .iter()
            .find(|e| e.name == name)
            .map(|e| e.kind)
            .unwrap_or_else(|| panic!("{name} is listed: {:?}", tree.entries))
    };
    assert_eq!(kind_of("journal.jsonl"), EntryKind::Journal);
    assert_eq!(kind_of("state"), EntryKind::State);

    // A goal's run keeps its files in its goal's folder.
    let goal = ws.create_goal(NewGoal::captured("filed with it")).unwrap();
    let (on_goal, _) = ws
        .create_run(
            RunScope::Goal { goal: goal.id },
            wf.id,
            BTreeMap::new(),
            bisa_core::RunEntry::by_hand(),
            None,
        )
        .unwrap();
    let err = ws
        .file_root(FileScope::Run, &on_goal.id.to_string())
        .unwrap_err();
    assert!(err.to_string().contains("that goal's"), "{err}");
    assert!(ws.file_root(FileScope::Run, "not-a-run").is_err());
}
