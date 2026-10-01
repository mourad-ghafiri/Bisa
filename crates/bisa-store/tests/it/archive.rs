//! Archived — a mark on a goal, a workflow or a project: hidden from every
//! list that does not ask, refused for the moves that would start work on
//! it, kept through a rebuild, and one move back.

use bisa_core::{GoalStatus, ProjectRoot, RunScope, Vcs, WorkflowOrigin};
use bisa_store::{MemoryKeyStore, NewGoal, NewProject, NewWorkflow, WorkflowScope, Workspace};

fn ws() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    (dir, ws)
}

fn library_workflow(ws: &Workspace, name: &str) -> bisa_core::Workflow {
    ws.create_workflow(
        NewWorkflow {
            name: name.into(),
            description: String::new(),
            inputs: vec![],
            steps: vec![bisa_core::Step {
                id: bisa_core::StepId::new("end").unwrap(),
                name: "End".into(),
                kind: bisa_core::StepKind::End {
                    finish: bisa_core::Finish::Done,
                },
                then: vec![],
                boundaries: vec![],
                join: bisa_core::Join::All,
                on_fail: bisa_core::OnFail::Fail,
                retries: 0,
                max_visits: bisa_core::DEFAULT_MAX_VISITS,
                position: None,
            }],
            tags: Default::default(),
            decision_making: false,
        },
        WorkflowOrigin::Workspace,
    )
    .unwrap()
}

#[test]
fn a_goal_is_archived_only_once_closed_hidden_from_the_list_and_taken_back_out() {
    let (_dir, ws) = ws();
    let goal = ws.create_goal(NewGoal::captured("ship it")).unwrap().id;
    let err = ws.set_goal_archived(goal, true).unwrap_err().to_string();
    assert!(err.contains("close it before archiving"), "{err}");

    ws.set_goal_closed(
        goal,
        bisa_core::ClosureReason::Abandoned { rationale: None },
    )
    .unwrap();
    let archived = ws.set_goal_archived(goal, true).unwrap();
    assert!(archived.is_archived());
    assert!(
        archived.is_closed(),
        "archived is a mark on a closed goal, never a status"
    );
    assert_eq!(archived.status(None), GoalStatus::Closed);
    assert!(
        ws.list_goals(None).unwrap().is_empty(),
        "hidden from the list"
    );
    assert!(ws.list_goals(Some(GoalStatus::Closed)).unwrap().is_empty());
    assert_eq!(
        ws.list_archived_goals().unwrap().len(),
        1,
        "shown only when asked"
    );

    let mut edited = ws.get_goal(goal).unwrap();
    edited.archived = None;
    let err = ws.update_goal(edited).unwrap_err().to_string();
    assert!(
        err.contains("set_goal_archived"),
        "an edit cannot unarchive: {err}"
    );

    let back = ws.set_goal_archived(goal, false).unwrap();
    assert!(!back.is_archived());
    assert!(back.is_closed(), "unarchiving does not reopen");
    assert_eq!(ws.list_goals(None).unwrap().len(), 1);
    assert!(ws.list_archived_goals().unwrap().is_empty());
}

#[test]
fn an_archived_workflow_leaves_the_library_and_refuses_a_goal_and_a_run() {
    let (_dir, ws) = ws();
    let wf = library_workflow(&ws, "release");
    let goal = ws.create_goal(NewGoal::captured("ship it")).unwrap().id;

    let archived = ws.set_workflow_archived(wf.id, true).unwrap();
    assert!(archived.is_archived());
    assert_eq!(
        archived.revision,
        wf.revision + 1,
        "a new revision, like any write"
    );
    assert!(ws
        .list_workflows_in(WorkflowScope::Library)
        .unwrap()
        .is_empty());
    assert_eq!(
        ws.list_archived_workflows_in(WorkflowScope::Library)
            .unwrap()
            .len(),
        1
    );
    let err = ws
        .set_goal_workflow(goal, Some(wf.id))
        .unwrap_err()
        .to_string();
    assert!(err.contains("archived"), "{err}");
    let err = ws
        .create_run(
            RunScope::Goal { goal },
            wf.id,
            Default::default(),
            bisa_core::RunEntry::by_hand(),
            None,
        )
        .unwrap_err()
        .to_string();
    assert!(err.contains("archived"), "{err}");

    ws.set_workflow_archived(wf.id, false).unwrap();
    assert_eq!(
        ws.list_workflows_in(WorkflowScope::Library).unwrap().len(),
        1
    );
    ws.set_goal_workflow(goal, Some(wf.id)).unwrap();
}

#[test]
fn an_archived_project_leaves_the_list_refuses_an_attachment_and_survives_a_rebuild() {
    let (dir, ws) = ws();
    let project = ws
        .create_project(NewProject {
            origin: bisa_core::ProjectOrigin::Workspace,
            slug: "site".parse().unwrap(),
            name: Some("Site".into()),
            root: ProjectRoot::Managed,
            vcs: Vcs::None,
            assignees: vec![],
            publish: Default::default(),
            tags: Default::default(),
        })
        .unwrap();
    let goal = ws.create_goal(NewGoal::captured("ship it")).unwrap().id;
    ws.attach(goal, project.id).unwrap();

    let archived = ws.set_project_archived(project.id, true).unwrap();
    assert!(archived.is_archived());
    assert!(
        ws.list_projects().unwrap().is_empty(),
        "hidden from the list"
    );
    assert_eq!(ws.list_archived_projects().unwrap().len(), 1);
    assert_eq!(
        ws.projects_for(goal).unwrap().len(),
        1,
        "an attachment made before stands"
    );
    let other = ws.create_goal(NewGoal::captured("another")).unwrap().id;
    let err = ws.attach(other, project.id).unwrap_err().to_string();
    assert!(err.contains("archived"), "{err}");
    drop(ws);

    let ws =
        Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
    ws.rebuild_index().unwrap();
    assert!(
        ws.list_projects().unwrap().is_empty(),
        "the mark is the record's, so a rebuild keeps it"
    );
    assert_eq!(ws.list_archived_projects().unwrap().len(), 1);
    ws.set_project_archived(project.id, false).unwrap();
    assert_eq!(ws.list_projects().unwrap().len(), 1);
}

#[test]
fn the_projects_attached_to_a_goal_but_not_born_of_it_are_told_apart() {
    let (_dir, ws) = ws();
    let goal = ws.create_goal(NewGoal::captured("ship it")).unwrap().id;
    let born = ws
        .create_project(NewProject {
            origin: bisa_core::ProjectOrigin::from_goal(goal),
            slug: "born".parse().unwrap(),
            name: Some("Born".into()),
            root: ProjectRoot::Managed,
            vcs: Vcs::None,
            assignees: vec![],
            publish: Default::default(),
            tags: Default::default(),
        })
        .unwrap();
    let foreign = ws
        .create_project(NewProject {
            origin: bisa_core::ProjectOrigin::Workspace,
            slug: "foreign".parse().unwrap(),
            name: Some("Foreign".into()),
            root: ProjectRoot::Managed,
            vcs: Vcs::None,
            assignees: vec![],
            publish: Default::default(),
            tags: Default::default(),
        })
        .unwrap();
    ws.attach(goal, born.id).unwrap();
    ws.attach(goal, foreign.id).unwrap();
    let only: Vec<_> = ws
        .projects_attached_only(goal)
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(only, vec![foreign.id]);
}
