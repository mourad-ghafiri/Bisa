//! Spawned children and the steps that wait on them: each child paired with
//! the step that spawned it — through a restart as well, whatever the order
//! the children end in — a child the crash left unmade spawned at boot, a
//! child whose start refuses closed with the reason, and a release of a
//! waiting spawn that forgets its pairing.

use crate::common;
use bisa_core::{
    ClosureReason, Goal, GoalId, GoalOrigin, RunOutcome, StepKind, WaitFor, WorkflowId,
};
use bisa_engine::Engine;
use bisa_store::NewProject;
use common::*;
use serde_json::json;
use std::collections::BTreeMap;

/// A workflow held open by a wait a person releases.
fn held() -> bisa_store::NewWorkflow {
    new_workflow(
        "held",
        vec![step(
            "hold",
            StepKind::Wait {
                until: WaitFor::Release,
            },
        )],
    )
}

fn spawn_step(id: &str, child: WorkflowId) -> bisa_core::Step {
    step(
        id,
        StepKind::Spawn {
            statement_template: format!("{id} of {{goal.statement}}"),
            workflow: Some(child),
            assignees: vec![],
            inputs: Default::default(),
            wait: true,
        },
    )
}

/// The child the step spawned: its origin names the parent and the step.
fn spawned_by(engine: &Engine, parent: GoalId, step: &str) -> Option<Goal> {
    engine
        .workspace()
        .list_goals(None)
        .ok()?
        .into_iter()
        .find(|g| {
            matches!(&g.origin, GoalOrigin::Spawned { parent: p, step: Some(s) }
                if *p == parent && s.as_str() == step)
        })
}

/// Let the child's held step go: its run ends done.
fn release(engine: &Engine, child: GoalId) {
    let run = engine
        .workspace()
        .get_goal(child)
        .unwrap()
        .run
        .expect("the child runs");
    engine.release_step(run, &sid("hold"), None).unwrap();
}

fn child_of_output(run: &bisa_core::WorkflowRun, step: &str) -> String {
    run.steps[&sid(step)]
        .output
        .as_ref()
        .and_then(|o| o["child"].as_str())
        .unwrap_or_default()
        .to_string()
}

/// Two spawn steps waiting side by side on two children: after a restart
/// each child still releases its own step — the one that spawned it — and
/// the other goes on waiting, whichever child ends first.
#[tokio::test(flavor = "multi_thread")]
async fn two_spawn_steps_waiting_on_two_children_each_release_their_own_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let child_wf = engine.create_workflow(held()).unwrap();
    let mut fan = step("fan", StepKind::Parallel);
    fan.then = vec![
        bisa_core::Flow::to(sid("s1")),
        bisa_core::Flow::to(sid("s2")),
    ];
    let (goal, _) = run_on(
        &engine,
        "two children",
        new_workflow(
            "twins",
            vec![
                fan,
                spawn_step("s1", child_wf.id),
                spawn_step("s2", child_wf.id),
            ],
        ),
    );
    step_in_state(&engine, goal.id, "s1", "waiting").await;
    step_in_state(&engine, goal.id, "s2", "waiting").await;
    let by_s1 = until("the first step's child", || {
        spawned_by(&engine, goal.id, "s1")
    })
    .await;
    let by_s2 = until("the second step's child", || {
        spawned_by(&engine, goal.id, "s2")
    })
    .await;
    assert_ne!(by_s1.id, by_s2.id);
    engine.shutdown().await;

    let engine = engine_with(&dir, vec![]);
    // The second step's child ends first: its own step completes with it,
    // and the first step goes on waiting for its own.
    release(&engine, by_s2.id);
    let after = step_in_state(&engine, goal.id, "s2", "done").await;
    assert_eq!(child_of_output(&after, "s2"), by_s2.id.to_string());
    assert_eq!(after.steps[&sid("s1")].state.as_str(), "waiting");
    release(&engine, by_s1.id);
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert_eq!(child_of_output(&done, "s1"), by_s1.id.to_string());
    engine.shutdown().await;
}

/// A crash between the snapshot that entered a `spawn` and the effect that
/// makes its child leaves the step waiting on nobody: the restart spawns
/// the child, and the step waits on it.
#[tokio::test(flavor = "multi_thread")]
async fn a_spawn_waiting_with_no_child_at_the_crash_gets_its_child_at_boot() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let child_wf = engine.create_workflow(held()).unwrap();
    let (goal, _) = run_on(
        &engine,
        "orphaned wait",
        new_workflow("one child", vec![spawn_step("s1", child_wf.id)]),
    );
    step_in_state(&engine, goal.id, "s1", "waiting").await;
    let child = until("the child", || spawned_by(&engine, goal.id, "s1")).await;
    engine.shutdown().await;
    // What the crash leaves: the step waiting, and no child.
    workspace(&dir).delete_goal(child.id).unwrap();

    let engine = engine_with(&dir, vec![]);
    let fresh = until("a child spawned at boot", || {
        spawned_by(&engine, goal.id, "s1").filter(|g| g.id != child.id)
    })
    .await;
    release(&engine, fresh.id);
    let done = finished_run(&engine, goal.id).await;
    assert_eq!(done.outcome, Some(RunOutcome::Done));
    assert_eq!(child_of_output(&done, "s1"), fresh.id.to_string());
    engine.shutdown().await;
}

/// A child whose start refuses — its agent step has nowhere unambiguous to
/// run, the parent having two projects — fails the step that spawned it,
/// and is closed with the reason rather than left a draft nobody will run.
#[tokio::test(flavor = "multi_thread")]
async fn a_child_whose_start_refuses_is_closed_with_the_reason() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![yielding("mock", json!({"ok": true}))]);
    let ws = engine.workspace();
    let parent = engine
        .submit_goal(bisa_engine::SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..guided("two places")
        })
        .unwrap();
    for slug in ["web", "api"] {
        let project = ws
            .create_project(NewProject::managed(slug).unwrap())
            .unwrap();
        ws.attach(parent.id, project.id).unwrap();
    }
    let child_wf = engine
        .create_workflow(new_workflow("unplaced", vec![agent_step("work", "mock")]))
        .unwrap();
    let wf = engine
        .create_workflow(new_workflow("spawns", vec![spawn_step("s1", child_wf.id)]))
        .unwrap();
    engine.set_workflow(parent.id, Some(wf.id)).unwrap();
    engine.start_run(parent.id, BTreeMap::new()).unwrap();
    let failed = finished_run(&engine, parent.id).await;
    assert_eq!(failed.outcome, Some(RunOutcome::Failed));
    let error = failed.steps[&sid("s1")].error.clone().unwrap_or_default();
    assert!(error.contains("project"), "{error}");
    let child = until("the child", || spawned_by(&engine, parent.id, "s1")).await;
    assert!(child.is_closed(), "closed, not a draft: {child:?}");
    let rationale = match &child.closed.as_ref().unwrap().reason {
        ClosureReason::Abandoned { rationale } => rationale.clone().unwrap_or_default(),
        other => panic!("{other:?}"),
    };
    assert!(rationale.contains("project"), "{rationale}");
    engine.shutdown().await;
}

/// A person's release of a waiting `spawn` lets the step go and forgets
/// the pairing: the child goes on, and its end moves nothing of the parent's.
#[tokio::test(flavor = "multi_thread")]
async fn a_release_of_a_waiting_spawn_forgets_the_child_pairing() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_with(&dir, vec![]);
    let child_wf = engine.create_workflow(held()).unwrap();
    // The parent goes on after the spawn, so the run holds whatever the
    // release left behind.
    let mut s1 = spawn_step("s1", child_wf.id);
    s1.then = vec![bisa_core::Flow::to(sid("later"))];
    let later = step(
        "later",
        StepKind::Wait {
            until: WaitFor::Release,
        },
    );
    let (goal, run) = run_on(
        &engine,
        "released early",
        new_workflow("one child, then more", vec![s1, later]),
    );
    step_in_state(&engine, goal.id, "s1", "waiting").await;
    let child = until("the child", || spawned_by(&engine, goal.id, "s1")).await;
    assert!(engine.inner().waits.awaits(child.id));
    engine.release_step(run.id, &sid("s1"), None).unwrap();
    step_in_state(&engine, goal.id, "later", "waiting").await;
    assert!(
        !engine.inner().waits.awaits(child.id),
        "the pairing goes with the release"
    );
    engine.release_step(run.id, &sid("later"), None).unwrap();
    assert_eq!(
        finished_run(&engine, goal.id).await.outcome,
        Some(RunOutcome::Done)
    );
    release(&engine, child.id);
    assert_eq!(
        finished_run(&engine, child.id).await.outcome,
        Some(RunOutcome::Done)
    );
    assert_eq!(run_of(&engine, run.id).outcome, Some(RunOutcome::Done));
    engine.shutdown().await;
}

/// A child a build before this one made — its origin names the parent and no
/// step — is paired with the first waiting `spawn` step at a restart, and
/// counted so the recovery walk spawns no second child for it; the parent's
/// step completes when that child's run ends.
#[tokio::test(flavor = "multi_thread")]
async fn a_child_with_no_step_named_is_paired_with_the_first_waiting_spawn_at_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let ws = workspace(&dir);
    let child_wf = ws
        .create_workflow(held(), bisa_core::WorkflowOrigin::Workspace)
        .unwrap();
    let parent = ws
        .create_goal(bisa_store::NewGoal::captured("a parent from before"))
        .unwrap()
        .id;
    let parent_wf = ws
        .create_workflow(
            new_workflow("spawns", vec![spawn_step("child", child_wf.id)]),
            bisa_core::WorkflowOrigin::Workspace,
        )
        .unwrap();
    ws.set_goal_workflow(parent, Some(parent_wf.id)).unwrap();
    let (parent_run, _) = ws
        .create_run(
            bisa_core::RunScope::Goal { goal: parent },
            parent_wf.id,
            BTreeMap::new(),
            bisa_core::RunEntry::by_hand(),
            None,
        )
        .unwrap();
    assert_eq!(
        parent_run.steps[&sid("child")].state.as_str(),
        "waiting",
        "the spawn step waits, as the build before left it"
    );
    // The child as that build wrote it: open, its run held, no step named.
    let child = ws
        .create_goal(bisa_store::NewGoal {
            origin: bisa_core::GoalOrigin::Spawned { parent, step: None },
            mode: bisa_core::GoalMode::Manual,
            ..bisa_store::NewGoal::captured("the child, from before")
        })
        .unwrap();
    ws.set_goal_workflow(child.id, Some(child_wf.id)).unwrap();
    let (child_run, _) = ws
        .create_run(
            bisa_core::RunScope::Goal { goal: child.id },
            child_wf.id,
            BTreeMap::new(),
            bisa_core::RunEntry::by_hand(),
            None,
        )
        .unwrap();

    let engine = Engine::start(ws, catalog_with(vec![]), design_off_config()).unwrap();
    let spawned: Vec<Goal> = engine
        .workspace()
        .list_goals(None)
        .unwrap()
        .into_iter()
        .filter(|g| matches!(&g.origin, bisa_core::GoalOrigin::Spawned { parent: p, .. } if *p == parent))
        .collect();
    assert_eq!(
        spawned.len(),
        1,
        "no second child for the step: {spawned:?}"
    );
    engine
        .release_step(child_run.id, &sid("hold"), None)
        .unwrap();
    let done = finished_run(&engine, parent).await;
    assert_eq!(done.outcome, Some(bisa_core::RunOutcome::Done), "{done:?}");
    assert_eq!(
        done.steps[&sid("child")].state.as_str(),
        "done",
        "completed by the child it was paired with"
    );
    engine.shutdown().await;
}
