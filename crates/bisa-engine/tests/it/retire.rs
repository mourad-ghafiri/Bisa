//! Retiring a goal or a workflow with what came from it: the preview's
//! facts, the plan carried out in order, and what is refused.

use crate::common;
use bisa_core::workitem::WorkItemState;
use bisa_core::{CancelCause, ClosureReason, ProjectOrigin, ProjectRoot, RunStatus, StepKind, Vcs};
use bisa_engine::retire::{Fate, GoalPlan, WorkflowPlan};
use bisa_engine::{Engine, EngineConfig, EnginePayload, SubmitRequest};
use bisa_harness::mock::MockAdapter;
use bisa_store::{MemoryKeyStore, NewProject, WorkflowScope, Workspace};
use common::{agent_step, engine_with, new_workflow, run_on, step, step_in_state, until, wait_for};

fn engine(dir: &tempfile::TempDir) -> Engine {
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    Engine::start(
        ws,
        common::catalog_with(vec![]),
        EngineConfig {
            design_enabled: false,
            ..Default::default()
        },
    )
    .unwrap()
}

fn project(
    ws: &Workspace,
    slug: &str,
    origin: ProjectOrigin,
    root: ProjectRoot,
) -> bisa_core::Project {
    ws.create_project(NewProject {
        origin,
        slug: slug.parse().unwrap(),
        name: Some(slug.to_string()),
        root,
        vcs: Vcs::None,
        assignees: vec![],
        publish: Default::default(),
        tags: Default::default(),
    })
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_goal_retired_by_archiving_closes_it_puts_its_born_projects_away_and_leaves_the_attached_alone(
) {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let ws = engine.workspace();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..SubmitRequest::captured("ship it")
        })
        .unwrap()
        .id;
    let born = project(
        ws,
        "born",
        ProjectOrigin::from_goal(goal),
        ProjectRoot::Managed,
    );
    let adopted_dir = dir.path().join("elsewhere");
    std::fs::create_dir_all(&adopted_dir).unwrap();
    let adopted = project(
        ws,
        "adopted",
        ProjectOrigin::from_goal(goal),
        ProjectRoot::External {
            path: adopted_dir.display().to_string(),
        },
    );
    let foreign = project(
        ws,
        "foreign",
        ProjectOrigin::Workspace,
        ProjectRoot::Managed,
    );
    for p in [&born, &adopted, &foreign] {
        ws.attach(goal, p.id).unwrap();
    }

    let preview = engine.preview_goal_retirement(goal).unwrap();
    assert_eq!(
        preview
            .projects_born
            .iter()
            .map(|p| (p.slug.as_str(), p.adopted))
            .collect::<Vec<_>>(),
        [("born", false), ("adopted", true)]
    );
    assert_eq!(
        preview
            .projects_attached
            .iter()
            .map(|p| p.slug.as_str())
            .collect::<Vec<_>>(),
        ["foreign"]
    );
    assert_eq!((preview.agents, preview.harnesses), (0, 0));
    assert!(preview.run.is_none(), "no run is going");
    assert!(preview.refusal.is_none(), "nothing refuses a deletion");
    assert!(preview.used_by.is_empty());

    let mut bus = engine.events();
    let done = engine
        .retire_goal(
            goal,
            GoalPlan {
                goal: Fate::Archive,
                projects: Fate::Archive,
                tree: false,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        done.projects.iter().map(|p| p.fate).collect::<Vec<_>>(),
        [Fate::Archive, Fate::Archive]
    );
    assert_eq!(
        done.workstreams_retired.len(),
        2,
        "each born project's primary was put away with it"
    );
    let g = ws.get_goal(goal).unwrap();
    assert!(g.is_closed(), "archiving an open goal closes it first");
    assert!(g.is_archived());
    assert!(ws.get_project(born.id).unwrap().is_archived());
    assert!(ws.get_project(adopted.id).unwrap().is_archived());
    assert!(
        !ws.get_project(foreign.id).unwrap().is_archived(),
        "an attached project is not the goal's to put away"
    );
    assert!(adopted_dir.is_dir(), "nothing on disk moved");
    assert!(ws.list_goals(None).unwrap().is_empty());
    assert_eq!(
        ws.list_projects().unwrap().len(),
        1,
        "only the foreign project is still listed"
    );
    let ev = common::wait_for(&mut bus, "goal.archived", |e| {
        matches!(
            &e.payload,
            EnginePayload::GoalArchived { archived: true, .. }
        )
    })
    .await;
    assert_eq!(ev.goal, Some(goal));

    // One move back, for each.
    engine.unarchive_goal(goal).unwrap();
    engine.archive_project(born.id, false).unwrap();
    let g = ws.get_goal(goal).unwrap();
    assert!(
        !g.is_archived() && g.is_closed(),
        "unarchiving does not reopen"
    );
    assert_eq!(ws.list_projects().unwrap().len(), 2);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_goal_retired_by_deleting_forgets_its_born_projects_detaches_the_rest_and_never_touches_an_adopted_folder(
) {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let ws = engine.workspace();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..SubmitRequest::captured("ship it")
        })
        .unwrap()
        .id;
    let born = project(
        ws,
        "born",
        ProjectOrigin::from_goal(goal),
        ProjectRoot::Managed,
    );
    let adopted_dir = dir.path().join("elsewhere");
    std::fs::create_dir_all(&adopted_dir).unwrap();
    let adopted = project(
        ws,
        "adopted",
        ProjectOrigin::from_goal(goal),
        ProjectRoot::External {
            path: adopted_dir.display().to_string(),
        },
    );
    let foreign = project(
        ws,
        "foreign",
        ProjectOrigin::Workspace,
        ProjectRoot::Managed,
    );
    for p in [&born, &adopted, &foreign] {
        ws.attach(goal, p.id).unwrap();
    }
    let born_dir = ws.project_paths(&born).dir().to_path_buf();
    assert!(born_dir.is_dir());

    let err = engine
        .retire_goal(
            goal,
            GoalPlan {
                goal: Fate::Keep,
                projects: Fate::Keep,
                tree: false,
            },
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("keep is not a fate"), "{err}");

    // No Trash in a test: the records go, the folders stay.
    let mut bus = engine.events();
    let done = engine
        .retire_goal(
            goal,
            GoalPlan {
                goal: Fate::Delete,
                projects: Fate::Delete,
                tree: false,
            },
        )
        .await
        .unwrap();
    assert_eq!(done.projects.len(), 2);
    assert!(ws.get_goal(goal).is_err(), "the goal is gone");
    assert!(
        ws.get_project(born.id).is_err() && ws.get_project(adopted.id).is_err(),
        "the born projects' records went"
    );
    assert!(
        born_dir.is_dir() && adopted_dir.is_dir(),
        "without the tree, every folder stays"
    );
    let foreign_now = ws.get_project(foreign.id).unwrap();
    assert!(!foreign_now.is_archived());
    assert!(
        ws.goals_of_project(foreign.id).unwrap().is_empty(),
        "the attached project is detached, never deleted"
    );
    common::wait_for(&mut bus, "goal.deleted", |e| {
        matches!(&e.payload, EnginePayload::GoalDeleted { .. })
    })
    .await;
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_used_workflow_cannot_be_deleted_but_can_be_archived_and_then_refuses_a_run() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let ws = engine.workspace();
    let wf = ws
        .create_workflow(
            new_workflow("release", vec![agent_step("work", "mock")]),
            bisa_core::WorkflowOrigin::Workspace,
        )
        .unwrap();
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            workflow: Some(wf.id),
            ..SubmitRequest::captured("ship it")
        })
        .unwrap()
        .id;

    let preview = engine.preview_workflow_retirement(wf.id).unwrap();
    assert_eq!(preview.used_by.len(), 1, "the goal holds it");
    let err = engine
        .retire_workflow(
            wf.id,
            WorkflowPlan {
                workflow: Fate::Delete,
                projects: Fate::Keep,
                tree: false,
            },
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("archive it"), "{err}");

    engine
        .retire_workflow(
            wf.id,
            WorkflowPlan {
                workflow: Fate::Archive,
                projects: Fate::Keep,
                tree: false,
            },
        )
        .await
        .unwrap();
    assert!(ws.get_workflow(wf.id).unwrap().is_archived());
    assert!(
        ws.list_workflows_in(WorkflowScope::Library)
            .unwrap()
            .is_empty(),
        "out of the library"
    );
    let err = engine
        .start_run(goal, Default::default())
        .unwrap_err()
        .to_string();
    assert!(err.contains("archived"), "{err}");

    engine.archive_workflow(wf.id, false).unwrap();
    assert_eq!(
        ws.list_workflows_in(WorkflowScope::Library).unwrap().len(),
        1
    );
    // Closed and archived goals are told apart from open ones in the lists.
    engine
        .close_goal(goal, ClosureReason::Abandoned { rationale: None })
        .unwrap();
    engine
        .retire_goal(
            goal,
            GoalPlan {
                goal: Fate::Archive,
                projects: Fate::Keep,
                tree: false,
            },
        )
        .await
        .unwrap();
    assert_eq!(ws.list_archived_goals().unwrap().len(), 1);
    engine.shutdown().await;
}

/// A workflow that waits for a person to release it: a run that stays open.
fn held(name: &str) -> bisa_store::NewWorkflow {
    new_workflow(
        name,
        vec![step(
            "hold",
            StepKind::Wait {
                until: bisa_core::WaitFor::Release,
            },
        )],
    )
}

/// A workflow's runs of the workspace go with it: retiring it retires the
/// live ones first — cancelled, cause *retired*, whatever its fate — then an
/// archived workflow keeps them all as its history and a deleted one takes
/// them with it. A plain archive is refused while one of them goes.
#[tokio::test(flavor = "multi_thread")]
async fn retiring_a_workflow_retires_its_workspace_runs_and_keeps_or_takes_their_history() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(&dir);
    let ws = engine.workspace();
    let wf = engine.create_workflow(held("held")).unwrap();
    let live = engine
        .start_workspace_run(wf.id, Default::default())
        .unwrap();
    let done = engine
        .start_workspace_run(wf.id, Default::default())
        .unwrap();
    engine.stop_run(done.id, None).await.unwrap();

    let preview = engine.preview_workflow_retirement(wf.id).unwrap();
    assert_eq!(
        preview.runs.iter().map(|r| r.id).collect::<Vec<_>>(),
        vec![live.id],
        "the live run is named"
    );
    assert_eq!(preview.runs[0].status, RunStatus::Waiting);
    assert_eq!(preview.history, 2, "every run of it, finished or not");
    assert!(preview.used_by.is_empty(), "its own runs do not hold it");

    let err = engine.archive_workflow(wf.id, true).unwrap_err();
    assert!(
        err.is_refusal() && err.to_string().contains(&live.id.to_string()),
        "{err}"
    );
    assert!(!ws.get_workflow(wf.id).unwrap().is_archived());

    engine
        .retire_workflow(
            wf.id,
            WorkflowPlan {
                workflow: Fate::Archive,
                projects: Fate::Keep,
                tree: false,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        ws.get_run(live.id).unwrap().cancelled,
        Some(CancelCause::Retired)
    );
    assert!(ws.get_workflow(wf.id).unwrap().is_archived());
    assert_eq!(
        engine.workflow_runs(wf.id).unwrap().len(),
        2,
        "kept as its history"
    );
    let err = engine
        .start_workspace_run(wf.id, Default::default())
        .unwrap_err();
    assert!(err.to_string().contains("archived"), "{err}");

    engine.archive_workflow(wf.id, false).unwrap();
    let again = engine
        .start_workspace_run(wf.id, Default::default())
        .unwrap();
    engine
        .retire_workflow(
            wf.id,
            WorkflowPlan {
                workflow: Fate::Delete,
                projects: Fate::Keep,
                tree: false,
            },
        )
        .await
        .unwrap();
    assert!(ws.get_workflow(wf.id).is_err());
    for run in [live.id, done.id, again.id] {
        assert!(ws.get_run(run).is_err(), "run {run} went with it");
    }
    assert!(ws.live_workspace_runs(None).unwrap().is_empty());
    engine.shutdown().await;
}

/// A held workflow that also begins on a call.
fn held_on_call(name: &str) -> bisa_store::NewWorkflow {
    let mut ticket = step(
        "ticket",
        StepKind::Start {
            on: bisa_core::StartOn::Hook { public: true },
            inputs: Default::default(),
            guard: Default::default(),
        },
    );
    ticket.then = vec![bisa_core::Flow::to(common::sid("hold"))];
    let mut draft = held(name);
    draft.steps.insert(0, ticket);
    draft
}

/// A workflow that listens stops listening when it is retired, whatever its
/// fate: put away, its record goes and taking it back out does not turn it
/// on again; deleted, what it listened with — its memory, its hook's secret
/// — goes with it.
#[tokio::test(flavor = "multi_thread")]
async fn retiring_a_workflow_that_listens_turns_it_off_first() {
    let dir = tempfile::tempdir().unwrap();
    // The listening runtime is driven by hand here: a worker of its own
    // would begin the queued signal's run before the workflow is put away.
    let engine = engine_with(&dir, vec![]);
    let ws = engine.workspace();
    let wf = engine.create_workflow(held_on_call("on call")).unwrap();
    let host = bisa_core::ListenerHost::Workspace { workflow: wf.id };
    let ticket = bisa_core::ListenerKey {
        host,
        step: common::sid("ticket"),
    };
    engine
        .set_listening(host, Default::default(), None)
        .unwrap();
    let queued = engine
        .call_hook(
            &ticket,
            serde_json::json!({}),
            None,
            bisa_engine::HookDoor::Local,
        )
        .unwrap();
    let mut rx = engine.events();

    // Put away — the plain archive, no run going: it stops listening.
    engine.archive_workflow(wf.id, true).unwrap();
    assert_eq!(ws.listening(&host).unwrap(), None);
    let settled = ws.signal(&queued).unwrap().unwrap();
    assert_eq!(settled.state, bisa_store::SignalState::Skipped);
    assert_eq!(settled.note.as_deref(), Some("not listening"));
    wait_for(&mut rx, "the workflow to say it stopped listening", |e| {
        matches!(
            &e.payload,
            EnginePayload::ListeningChanged { host: h, on: false } if *h == host
        )
    })
    .await;
    assert!(engine.armed_listeners().armed.is_empty());

    // Taken back out, it is off until a person turns it on.
    engine.archive_workflow(wf.id, false).unwrap();
    assert_eq!(ws.listening(&host).unwrap(), None);
    assert!(engine.armed_listeners().armed.is_empty());
    assert!(
        ws.has_hook_secret(&ticket).unwrap(),
        "its secret is kept across off and on"
    );
    engine
        .set_listening(host, Default::default(), None)
        .unwrap();
    assert_eq!(engine.armed_listeners().armed.len(), 1);

    // Deleted: everything it listened with goes with it.
    engine
        .retire_workflow(
            wf.id,
            WorkflowPlan {
                workflow: Fate::Delete,
                projects: Fate::Keep,
                tree: false,
            },
        )
        .await
        .unwrap();
    assert!(ws.get_workflow(wf.id).is_err());
    assert!(ws.list_listening().unwrap().is_empty());
    assert!(!ws.has_hook_secret(&ticket).unwrap());
    assert!(engine.armed_listeners().armed.is_empty());
    assert_eq!(
        engine.call_hook(
            &ticket,
            serde_json::json!({}),
            None,
            bisa_engine::HookDoor::Local
        ),
        Err(bisa_engine::HookRefusal::NotListening(host.to_string()))
    );
    engine.shutdown().await;
}

/// A harness that starts and then says nothing: the item stays in progress
/// until something stops it — the shape of a deletion mid-run.
fn quiet_harness() -> MockAdapter {
    MockAdapter {
        script: Some(vec![bisa_harness::SessionEvent::Lifecycle(
            bisa_harness::LifecycleEvent::Started,
        )]),
        ..Default::default()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn deleting_a_running_goal_aborts_its_session_before_the_folder_goes() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![quiet_harness()]);
    let mut bus = engine.events();
    let (goal, run) = run_on(
        &engine,
        "delete me mid-run",
        new_workflow("running", vec![agent_step("work", "mock")]),
    );
    step_in_state(&engine, goal.id, "work", "running").await;
    until("the session to be on the roster", || {
        engine
            .inner()
            .presence
            .snapshot()
            .iter()
            .any(|s| s.goal == Some(goal.id) && s.state.is_live())
            .then_some(())
    })
    .await;

    let preview = engine.preview_goal_retirement(goal.id).unwrap();
    assert_eq!(preview.agents, 1, "one engine session on it");
    assert_eq!(preview.harnesses, 0);
    let facts = preview.run.expect("the run is going");
    assert_eq!(facts.id, run.id);
    assert_eq!(facts.status, RunStatus::Running);
    assert_eq!(facts.live_steps, 1);
    assert!(preview.refusal.is_none());

    let goal_dir = engine.workspace().paths().goal(goal.id).dir().to_path_buf();
    assert!(goal_dir.is_dir());
    let done = engine
        .retire_goal(
            goal.id,
            GoalPlan {
                goal: Fate::Delete,
                projects: Fate::Keep,
                tree: false,
            },
        )
        .await
        .unwrap();
    assert_eq!(done.stopped_sessions, 1, "the session was stopped");
    assert_eq!(
        done.unsettled_sessions, 0,
        "the harness was aborted on the spot, not on its next event"
    );
    assert!(
        !goal_dir.exists(),
        "the folder went after the session ended"
    );

    // The order on the bus: the item's cancellation, then the deletion —
    // scoped to the goal, so a screen open on it hears it.
    let mut cancelled_before_deleted = false;
    let mut deleted_scoped = false;
    loop {
        let e = wait_for(&mut bus, "the retirement's events", |e| {
            matches!(
                &e.payload,
                EnginePayload::ExecutionEnded { .. } | EnginePayload::GoalDeleted { .. }
            )
        })
        .await;
        match &e.payload {
            EnginePayload::ExecutionEnded { .. } if !deleted_scoped => {
                cancelled_before_deleted = true;
            }
            EnginePayload::GoalDeleted { goal: g } => {
                assert_eq!(*g, goal.id);
                deleted_scoped = e.goal == Some(goal.id);
                break;
            }
            _ => {}
        }
    }
    assert!(
        cancelled_before_deleted,
        "the work was cancelled before the goal went"
    );
    assert!(deleted_scoped, "GoalDeleted names the goal on its envelope");

    // Nothing in memory names the goal any more.
    let inner = engine.inner();
    assert!(
        !inner
            .inflight
            .iter()
            .any(|e| e.value().home.goal() == Some(goal.id)),
        "no reservation of the goal's"
    );
    assert!(
        !inner
            .active_items
            .iter()
            .any(|e| e.value().goal() == Some(goal.id)),
        "no active item of the goal's"
    );
    assert!(!inner.waits.awaits(goal.id), "no parent waits on it");
    assert!(
        inner
            .presence
            .snapshot()
            .iter()
            .all(|s| s.goal != Some(goal.id)),
        "its roster rows are forgotten"
    );
    assert!(engine.workspace().get_goal(goal.id).is_err());
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_goal_whose_design_is_used_elsewhere_is_refused_before_anything_stops() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![quiet_harness()]);
    let ws = engine.workspace();
    let (goal, _) = run_on(
        &engine,
        "keep running",
        new_workflow("running", vec![agent_step("work", "mock")]),
    );
    step_in_state(&engine, goal.id, "work", "running").await;
    // The goal's own design, held by another goal: deleting the first would
    // take the design from under the second.
    let design = ws
        .create_workflow(
            new_workflow("the goal's design", vec![agent_step("work", "mock")]),
            bisa_core::WorkflowOrigin::Goal { goal: goal.id },
        )
        .unwrap();
    // No goal can point at another goal's design (the store refuses it); a
    // library workflow's `spawn` step can name it, and this one does.
    let borrower = engine
        .create_workflow(new_workflow(
            "borrow the design",
            vec![step(
                "child",
                StepKind::Spawn {
                    statement_template: "borrow {goal.statement}".into(),
                    workflow: Some(design.id),
                    assignees: vec![],
                    inputs: Default::default(),
                    wait: true,
                },
            )],
        ))
        .unwrap();

    let preview = engine.preview_goal_retirement(goal.id).unwrap();
    let refusal = preview.refusal.expect("the deletion is refused up front");
    assert!(refusal.contains(&borrower.id.to_string()), "{refusal}");
    assert_eq!(preview.agents, 1);

    let err = engine
        .retire_goal(
            goal.id,
            GoalPlan {
                goal: Fate::Delete,
                projects: Fate::Keep,
                tree: false,
            },
        )
        .await
        .unwrap_err();
    assert!(err.is_refusal(), "{err}");
    let g = ws.get_goal(goal.id).unwrap();
    assert!(!g.is_closed(), "nothing was closed");
    assert!(
        engine
            .inner()
            .presence
            .snapshot()
            .iter()
            .any(|s| s.goal == Some(goal.id) && s.state.is_live()),
        "nothing was stopped"
    );
    assert!(
        engine
            .workspace()
            .list_work_items(&bisa_core::Home::from(goal.id))
            .unwrap()
            .iter()
            .all(|i| !matches!(i.state, WorkItemState::Cancelled)),
        "no item was cancelled"
    );

    // Archiving is still open to it, and does stop the work.
    let done = engine
        .retire_goal(
            goal.id,
            GoalPlan {
                goal: Fate::Archive,
                projects: Fate::Keep,
                tree: false,
            },
        )
        .await
        .unwrap();
    assert_eq!(done.stopped_sessions, 1);
    assert!(ws.get_goal(goal.id).unwrap().is_archived());
    engine.shutdown().await;
}
