//! The Workflow Agent's own ops, over the real intake socket: reading the
//! template library, validating a definition, proposing one for a goal, and
//! proposing an amendment to a live run. It proposes; the person adopts.

use crate::common;

use bisa_core::{AgentId, Gate, RunOutcome, StepKind, WaitFor};
use bisa_engine::{Engine, EnginePayload, SubmitRequest};
use bisa_store::CatalogKind;
use common::*;
use serde_json::{json, Value};

const DRIVER: &str = AgentId::WORKFLOW;

fn engine_on(dir: &tempfile::TempDir) -> Engine {
    engine_with(dir, vec![yielding("mock", json!({"ok": true}))])
}

fn draft(name: &str, harness: &str) -> Value {
    json!({
        "name": name,
        "description": "from the driver",
        "steps": [{"id": "do", "name": "Do it", "kind": "agent",
                   "instructions": "do {goal.statement}", "harness": [harness]}]
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn list_workflow_templates_lists_catalog_and_local_workflows() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let local = engine
        .create_workflow(new_workflow("mine", vec![agent_step("go", "mock")]))
        .unwrap();
    engine
        .workspace()
        .install(CatalogKind::Workflow, "weekly-review")
        .unwrap();

    let reply = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "list_workflow_templates", "agent": DRIVER}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    let templates = reply["templates"].as_array().unwrap();
    assert_eq!(templates.len(), 13);
    let weekly = templates
        .iter()
        .find(|t| t["slug"] == json!("weekly-review"))
        .unwrap();
    assert_eq!(weekly["installed"], json!(true));
    assert!(weekly["description"].is_string());
    let workflows = reply["workflows"].as_array().unwrap();
    assert_eq!(
        workflows.len(),
        2,
        "the local one and the installed template"
    );
    let mine = workflows
        .iter()
        .find(|w| w["id"] == json!(local.id.to_string()))
        .unwrap();
    assert_eq!(mine["origin"], json!("workspace"));
    assert_eq!(mine["steps"], json!(1));
    assert!(workflows
        .iter()
        .any(|w| w["origin"] == json!({"catalog": "weekly-review"})));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn validate_workflow_reports_problems_by_kind_without_writing() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let bad = json!({
        "name": "Broken",
        "steps": [
            {"id": "a", "name": "A", "kind": "agent", "instructions": "{inputs.missing}",
             "assignee": {"agent": "nobody"}, "then": ["b", "b"]},
            {"id": "b", "name": "B", "kind": "end", "finish": "done", "then": ["a"]}
        ]
    });
    let reply = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "validate_workflow", "agent": DRIVER, "workflow": bad}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    let kinds: Vec<&str> = reply["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["kind"].as_str().unwrap())
        .collect();
    for expected in ["unknown_assignee", "unknown_input", "end_with_successors"] {
        assert!(kinds.contains(&expected), "{kinds:?}");
    }
    assert!(
        engine.workspace().list_workflows().unwrap().is_empty(),
        "nothing written"
    );

    let ok = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "validate_workflow", "agent": DRIVER, "workflow": draft("Fine", "mock")}),
    )
    .await;
    assert_eq!(ok["problems"], json!([]));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn propose_workflow_installs_nothing_sets_the_goal_and_opens_the_adopt_gate() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let mut rx = engine.events();
    let goal = engine.submit_goal(guided("sell more socks")).unwrap();
    let before = engine.workspace().list_agents().unwrap().len();

    let reply = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "propose_workflow", "agent": DRIVER, "goal": goal.id.to_string(),
               "workflow": draft("Socks", "mock")}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    let wf: bisa_core::WorkflowId = reply["workflow"].as_str().unwrap().parse().unwrap();
    assert_eq!(reply["revision"], json!(1));
    let gate_id = reply["gate"].as_str().unwrap().to_string();

    assert_eq!(
        engine.workspace().list_agents().unwrap().len(),
        before,
        "installs nothing"
    );
    let g = engine.workspace().get_goal(goal.id).unwrap();
    assert_eq!(g.workflow, Some(wf));
    assert!(g.run.is_none());
    let gate = engine.gate(&gate_id).unwrap();
    assert_eq!(gate.gate, Gate::Approval);
    assert_eq!(gate.subject, format!("adopt:{wf}@1"));
    wait_for(&mut rx, "the proposal event", |e| {
        matches!(&e.payload, EnginePayload::WorkflowProposed { workflow, .. } if *workflow == wf)
    })
    .await;
    assert!(notes(&engine, goal.id)
        .iter()
        .any(|n| n.contains("proposed workflow")));

    // The proposal is the goal's design, not the library's: origin names
    // the goal, and the library scope hides it.
    let stored = engine.workspace().get_workflow(wf).unwrap();
    assert_eq!(
        stored.origin,
        bisa_core::WorkflowOrigin::Goal { goal: goal.id },
        "a proposal is the goal's design"
    );
    assert!(
        engine
            .workspace()
            .list_workflows_in(bisa_store::WorkflowScope::Library)
            .unwrap()
            .iter()
            .all(|w| w.id != wf),
        "the library never shows a proposal"
    );

    // A second proposal revises the goal's own design in place — the stored
    // origin, not a usage heuristic, is what says it may.
    let again = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "propose_workflow", "agent": DRIVER, "goal": goal.id.to_string(),
               "workflow": draft("Socks v2", "mock")}),
    )
    .await;
    assert_eq!(again["ok"], json!(true), "{again}");
    assert_eq!(again["workflow"], json!(wf.to_string()));
    assert_eq!(again["revision"], json!(2));
    assert!(
        engine.gate(&gate_id).is_none(),
        "the earlier adoption was withdrawn"
    );
    assert_eq!(engine.inbox().len(), 1);

    // A declined adoption starts nothing and deletes nothing: the design
    // stays the goal's, still pointed at, and a note says the person said no.
    let gate2 = engine.inbox()[0].id.clone();
    engine
        .decide(&gate2, false, Some("not this"), None, None)
        .unwrap();
    let after = engine.workspace().get_goal(goal.id).unwrap();
    assert!(after.run.is_none());
    assert_eq!(after.workflow, Some(wf), "the design stays on the goal");
    engine.workspace().get_workflow(wf).unwrap();
    assert!(engine.inbox().is_empty());
    assert!(notes(&engine, goal.id)
        .iter()
        .any(|n| n.contains("adoption declined")));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn amend_workflow_is_gated_and_refused_without_a_run() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let goal = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..guided("amend me")
        })
        .unwrap();
    let held = json!({"name": "Held", "steps": [
        {"id": "hold", "name": "Hold", "kind": "wait", "until": {"until": "release"}, "then": ["old"]},
        {"id": "old", "name": "Old", "kind": "agent", "instructions": "old way", "harness": ["mock"]}
    ]});
    // No run yet: refused.
    let early = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "amend_workflow", "agent": DRIVER, "goal": goal.id.to_string(), "workflow": held}),
    )
    .await;
    assert_eq!(early["ok"], json!(false), "{early}");
    assert!(early["errors"][0].as_str().unwrap().contains("no run"));

    let (_, run) = run_on(
        &engine,
        "amend me too",
        new_workflow(
            "Held",
            chain(vec![
                step(
                    "hold",
                    StepKind::Wait {
                        until: WaitFor::Release,
                    },
                ),
                agent_step("old", "mock"),
            ]),
        ),
    );
    let goal = run.scope.goal().expect("a goal's run");
    let amendment = json!({"name": "Held", "steps": [
        {"id": "hold", "name": "Hold", "kind": "wait", "until": {"until": "release"}, "then": ["new"]},
        {"id": "new", "name": "New", "kind": "agent", "instructions": "new way", "harness": ["mock"]}
    ]});
    let reply = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "amend_workflow", "agent": DRIVER, "goal": goal.to_string(), "workflow": amendment}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    let gate_id = reply["gate"].as_str().unwrap().to_string();
    let gate = engine.gate(&gate_id).unwrap();
    assert!(
        gate.subject.starts_with(&format!("amend:{}@", run.id)),
        "{}",
        gate.subject
    );
    // Nothing moved yet: the run still runs the old steps.
    assert!(current_run(&engine, goal).steps.contains_key(&sid("old")));
    // An amendment that touches the started step is refused up front.
    let touching = json!({"name": "Held", "steps": [
        {"id": "hold", "name": "Hold", "kind": "approval", "prompt": "?", "then": ["new"]},
        {"id": "new", "name": "New", "kind": "agent", "instructions": "x", "harness": ["mock"]}
    ]});
    let refused = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "amend_workflow", "agent": DRIVER, "goal": goal.to_string(), "workflow": touching}),
    )
    .await;
    assert_eq!(refused["ok"], json!(false), "{refused}");

    // The held copy is the goal's own, never a library workflow.
    let held_copies: Vec<_> = engine
        .workspace()
        .list_workflows()
        .unwrap()
        .into_iter()
        .filter(|w| w.name.ends_with(bisa_engine::ops::AMENDMENT_SUFFIX))
        .collect();
    assert_eq!(held_copies.len(), 1);
    assert_eq!(
        held_copies[0].origin,
        bisa_core::WorkflowOrigin::Goal { goal }
    );
    assert!(engine
        .workspace()
        .list_workflows_in(bisa_store::WorkflowScope::Library)
        .unwrap()
        .iter()
        .all(|w| !w.name.ends_with(bisa_engine::ops::AMENDMENT_SUFFIX)));

    engine.decide(&gate_id, true, None, None, None).unwrap();
    let amended = current_run(&engine, goal);
    assert!(amended.steps.contains_key(&sid("new")));
    assert!(!amended.steps.contains_key(&sid("old")));
    assert_eq!(amended.workflow.revision, run.workflow.revision + 1);
    // The held copy was dropped once applied.
    assert!(engine
        .workspace()
        .list_workflows()
        .unwrap()
        .iter()
        .all(|w| !w.name.ends_with("amendment")));
    engine
        .release_step(current_run(&engine, goal).id, &sid("hold"), None)
        .unwrap();
    assert_eq!(
        finished_run(&engine, goal).await.outcome,
        Some(RunOutcome::Done)
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn get_workflow_reads_by_id_or_slug() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let local = engine
        .create_workflow(new_workflow("mine", vec![agent_step("go", "mock")]))
        .unwrap();

    let by_id = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "get_workflow", "agent": DRIVER, "workflow": local.id.to_string()}),
    )
    .await;
    assert_eq!(by_id["ok"], json!(true), "{by_id}");
    assert_eq!(by_id["installed"], json!(true));
    assert_eq!(by_id["workflow"]["name"], json!("mine"));
    assert_eq!(by_id["problems"], json!([]));

    // An uninstalled template reads from the catalog; its agent steps name
    // agents that are not here yet, and the problems say so.
    let by_slug = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "get_workflow", "agent": DRIVER, "workflow": "bug-fix"}),
    )
    .await;
    assert_eq!(by_slug["ok"], json!(true), "{by_slug}");
    assert_eq!(by_slug["installed"], json!(false));
    assert!(by_slug["workflow"]["steps"].as_array().unwrap().len() > 1);
    assert!(by_slug["problems"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["kind"] == json!("unknown_assignee")));

    engine
        .workspace()
        .install(CatalogKind::Workflow, "bug-fix")
        .unwrap();
    let installed = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "get_workflow", "agent": DRIVER, "workflow": "bug-fix"}),
    )
    .await;
    assert_eq!(installed["installed"], json!(true));
    assert_eq!(installed["problems"], json!([]));

    let missing = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "get_workflow", "agent": DRIVER, "workflow": "no-such-thing"}),
    )
    .await;
    assert_eq!(missing["ok"], json!(false));
    engine.shutdown().await;
}

/// The Workflow Agent's one write to a library workflow: from the
/// conversation about it, whole, at the revision it read — announced as its
/// hand (`designed`), so the designer beside the conversation and the Inbox
/// hear it. Nothing else earns the write: a stale revision, a definition
/// with problems, another agent, a session in no such conversation, another
/// workflow named, an archived workflow.
#[tokio::test(flavor = "multi_thread")]
async fn save_workflow_writes_the_conversations_workflow_at_its_revision_and_nothing_else() {
    use bisa_core::ConversationOrigin;
    use bisa_store::NewConversation;
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let mut rx = engine.events();
    let ws = engine.workspace();
    let wf = engine
        .create_workflow(new_workflow("Release", vec![agent_step("go", "mock")]))
        .unwrap();
    let other = engine
        .create_workflow(new_workflow("Other", vec![agent_step("go", "mock")]))
        .unwrap();
    let about = ws
        .create_conversation(NewConversation {
            origin: ConversationOrigin::Workflow { id: wf.id },
            title: None,
            mode: bisa_core::ConversationMode::Auto,
        })
        .unwrap();
    let scope = about.id.to_string();
    let save = |scope: &str, revision: u64, agent: &str, name: &str| {
        json!({"op": "save_workflow", "agent": agent, "scope": scope, "revision": revision,
               "definition": draft(name, "mock")})
    };

    // The write: the next revision, the body changed, the agent's hand said.
    let ok = intake_roundtrip(
        engine.socket_path(),
        save(&scope, 1, DRIVER, "Release, reviewed"),
    )
    .await;
    assert_eq!(ok["ok"], json!(true), "{ok}");
    assert_eq!(ok["workflow"], json!(wf.id.to_string()));
    assert_eq!(ok["revision"], json!(2));
    let stored = ws.get_workflow(wf.id).unwrap();
    assert_eq!(stored.name, "Release, reviewed");
    assert_eq!(stored.revision, 2);
    assert_eq!(
        stored.origin,
        bisa_core::WorkflowOrigin::Workspace,
        "a library workflow stays the library's"
    );
    wait_for(&mut rx, "the agent's hand on the change", |e| {
        matches!(&e.payload, EnginePayload::WorkflowChanged { workflow, revision: 2, designed: true } if *workflow == wf.id)
    })
    .await;

    // The revision it read has moved: refused, nothing written.
    let moved = intake_roundtrip(engine.socket_path(), save(&scope, 1, DRIVER, "Stale")).await;
    assert_eq!(moved["ok"], json!(false), "{moved}");
    assert!(
        moved["errors"][0]
            .as_str()
            .unwrap()
            .contains("moved to revision 2"),
        "{moved}"
    );
    assert_eq!(ws.get_workflow(wf.id).unwrap().name, "Release, reviewed");

    // Problems are refused with the problems, never saved.
    let broken = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "save_workflow", "agent": DRIVER, "scope": scope, "revision": 2,
               "definition": {"name": "Broken", "steps": [
                   {"id": "a", "name": "A", "kind": "agent", "instructions": "x",
                    "harness": ["mock"], "then": ["nowhere"]}]}}),
    )
    .await;
    assert_eq!(broken["ok"], json!(false), "{broken}");
    assert!(
        broken["problems"].as_array().is_some_and(|p| !p.is_empty()),
        "{broken}"
    );
    assert_eq!(ws.get_workflow(wf.id).unwrap().revision, 2);

    // Another agent never writes a workflow.
    let developer =
        intake_roundtrip(engine.socket_path(), save(&scope, 2, "developer", "Theirs")).await;
    assert_eq!(developer["ok"], json!(false), "{developer}");

    // A session in no conversation about a workflow — a goal's cycle, a
    // goal's thread — is told what writes a goal's design instead.
    let goal = engine.submit_goal(guided("ship it")).unwrap();
    let off = intake_roundtrip(
        engine.socket_path(),
        save(&goal.id.to_string(), 2, DRIVER, "From a goal"),
    )
    .await;
    assert_eq!(off["ok"], json!(false), "{off}");
    assert!(
        off["errors"][0]
            .as_str()
            .unwrap()
            .contains("propose_workflow"),
        "{off}"
    );

    // Another workflow named: this conversation is about one workflow.
    let named = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "save_workflow", "agent": DRIVER, "scope": scope, "revision": 1,
               "workflow": other.id.to_string(), "definition": draft("Other, edited", "mock")}),
    )
    .await;
    assert_eq!(named["ok"], json!(false), "{named}");
    assert!(
        named["errors"][0].as_str().unwrap().contains("is another"),
        "{named}"
    );
    assert_eq!(ws.get_workflow(other.id).unwrap().revision, 1);

    // The conversation's own, named explicitly, is the same write.
    let explicit = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "save_workflow", "agent": DRIVER, "scope": scope, "revision": 2,
               "workflow": wf.id.to_string(), "definition": draft("Release, twice", "mock")}),
    )
    .await;
    assert_eq!(explicit["ok"], json!(true), "{explicit}");
    assert_eq!(explicit["revision"], json!(3));

    // Put away, it is out of the library and not edited.
    engine.archive_workflow(wf.id, true).unwrap();
    let archived =
        intake_roundtrip(engine.socket_path(), save(&scope, 3, DRIVER, "Archived")).await;
    assert_eq!(archived["ok"], json!(false), "{archived}");
    assert!(
        archived["errors"][0].as_str().unwrap().contains("archived"),
        "{archived}"
    );
    engine.shutdown().await;
}

/// A goal cannot run another goal's design; the library copy from `promote`
/// can be used anywhere.
#[tokio::test(flavor = "multi_thread")]
async fn another_goals_design_is_refused_until_promoted() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let a = engine.submit_goal(guided("mine")).unwrap();
    let b = engine.submit_goal(guided("theirs")).unwrap();
    let reply = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "propose_workflow", "agent": DRIVER, "goal": a.id.to_string(),
               "workflow": draft("A's design", "mock")}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    let wf: bisa_core::WorkflowId = reply["workflow"].as_str().unwrap().parse().unwrap();

    let err = engine.set_workflow(b.id, Some(wf)).unwrap_err();
    assert!(
        err.to_string().contains("promote it to the library"),
        "{err}"
    );

    let copy = engine.promote_workflow(wf).unwrap();
    assert_eq!(copy.origin, bisa_core::WorkflowOrigin::Workspace);
    engine.set_workflow(b.id, Some(copy.id)).unwrap();
    assert_eq!(
        engine.workspace().get_goal(b.id).unwrap().workflow,
        Some(copy.id)
    );
    engine.shutdown().await;
}

/// A person's own design on the goal's tab is the goal's too — recorded,
/// pointed at, and startable without an adoption gate.
#[tokio::test(flavor = "multi_thread")]
async fn a_designed_workflow_is_the_goals_and_needs_no_adoption() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let goal = engine.submit_goal(guided("drawn by hand")).unwrap();
    let (wf, problems) = engine
        .design_workflow(
            goal.id,
            new_workflow("hand drawn", vec![agent_step("go", "mock")]),
            None,
        )
        .unwrap();
    assert!(problems.is_empty());
    assert_eq!(wf.origin, bisa_core::WorkflowOrigin::Goal { goal: goal.id });
    let g = engine.workspace().get_goal(goal.id).unwrap();
    assert_eq!(g.workflow, Some(wf.id));
    assert!(engine.inbox().is_empty(), "your own design opens no gate");

    // A second save names the revision it edited; a stale one is a conflict.
    let (again, _) = engine
        .design_workflow(
            goal.id,
            new_workflow("hand drawn v2", vec![agent_step("go", "mock")]),
            Some(wf.revision),
        )
        .unwrap();
    assert_eq!(again.id, wf.id);
    assert_eq!(again.revision, wf.revision + 1);
    let err = engine
        .design_workflow(
            goal.id,
            new_workflow("stale", vec![agent_step("go", "mock")]),
            Some(wf.revision),
        )
        .unwrap_err();
    assert!(
        matches!(
            err,
            bisa_engine::EngineError::Store(bisa_store::StoreError::RevisionConflict { .. })
        ),
        "{err:?}"
    );
    let err = engine
        .design_workflow(
            goal.id,
            new_workflow("no revision", vec![agent_step("go", "mock")]),
            None,
        )
        .unwrap_err();
    assert!(err.to_string().contains("send the revision"), "{err}");
    engine.shutdown().await;
}

/// Editing a proposed design makes it the person's own: the
/// agent's adoption gate named the revision they changed, so it is withdrawn,
/// and nothing runs until an explicit start.
#[tokio::test(flavor = "multi_thread")]
async fn editing_a_proposed_design_withdraws_the_adopt_gate() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let goal = engine.submit_goal(guided("edit before it runs")).unwrap();

    let reply = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "propose_workflow", "agent": DRIVER, "goal": goal.id.to_string(),
               "workflow": draft("Socks", "mock")}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    let wf: bisa_core::WorkflowId = reply["workflow"].as_str().unwrap().parse().unwrap();
    let gate_id = reply["gate"].as_str().unwrap().to_string();
    assert!(engine.gate(&gate_id).is_some(), "the adopt gate is open");
    assert_eq!(engine.inbox().len(), 1, "one thing waits: the adoption");

    // The person edits the proposed plan (the door's PUT with a definition).
    let (again, _) = engine
        .design_workflow(
            goal.id,
            new_workflow("Socks, edited", vec![agent_step("go", "mock")]),
            Some(1),
        )
        .unwrap();
    assert_eq!(again.id, wf, "the same design, a new revision");
    assert_eq!(again.revision, 2);
    assert!(
        engine.gate(&gate_id).is_none(),
        "the stale adoption gate is withdrawn"
    );
    assert!(
        engine.inbox().is_empty(),
        "nothing waits — the plan is the person's now"
    );
    assert!(
        engine.workspace().get_goal(goal.id).unwrap().run.is_none(),
        "and nothing has run"
    );
    engine.shutdown().await;
}

/// A design is a draft: it is kept with its problems and the goal points at
/// it, and only a start refuses them. But a goal with a live run takes no
/// design at all — nothing is written.
#[tokio::test(flavor = "multi_thread")]
async fn design_keeps_a_draft_with_problems_and_a_live_run_takes_none() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let goal = engine.submit_goal(guided("draft me")).unwrap();
    let mut dangling = agent_step("go", "mock");
    dangling.then = vec![bisa_core::Flow::to(sid("nowhere"))];
    let (wf, problems) = engine
        .design_workflow(goal.id, new_workflow("half drawn", vec![dangling]), None)
        .unwrap();
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].kind, bisa_core::ProblemKind::UnknownStep);
    assert_eq!(
        engine.workspace().get_goal(goal.id).unwrap().workflow,
        Some(wf.id)
    );
    let err = engine
        .start_run(goal.id, std::collections::BTreeMap::new())
        .unwrap_err();
    assert!(err.to_string().contains("nowhere"), "{err}");

    // A goal whose run is live: the design is refused before anything is written.
    let (busy, run) = run_on(
        &engine,
        "busy",
        new_workflow(
            "held",
            vec![step(
                "hold",
                StepKind::Wait {
                    until: WaitFor::Release,
                },
            )],
        ),
    );
    let before = engine.workspace().list_workflows().unwrap().len();
    let err = engine
        .design_workflow(
            busy.id,
            new_workflow("late design", vec![agent_step("go", "mock")]),
            None,
        )
        .unwrap_err();
    assert!(err.to_string().contains("amend the live one"), "{err}");
    assert_eq!(
        engine.workspace().list_workflows().unwrap().len(),
        before,
        "nothing was written"
    );
    assert_eq!(
        engine.workspace().get_goal(busy.id).unwrap().workflow,
        Some(run.workflow.id)
    );
    engine.shutdown().await;
}

fn held_amendment() -> serde_json::Value {
    json!({"name": "Held", "steps": [
        {"id": "hold", "name": "Hold", "kind": "wait", "until": {"until": "release"}, "then": ["new"]},
        {"id": "new", "name": "New", "kind": "agent", "instructions": "new way", "harness": ["mock"]}
    ]})
}

fn held_workflow() -> bisa_store::NewWorkflow {
    new_workflow(
        "Held",
        chain(vec![
            step(
                "hold",
                StepKind::Wait {
                    until: WaitFor::Release,
                },
            ),
            agent_step("old", "mock"),
        ]),
    )
}

fn held_copies(engine: &bisa_engine::Engine) -> Vec<bisa_core::Workflow> {
    engine
        .workspace()
        .list_workflows()
        .unwrap()
        .into_iter()
        .filter(|w| w.name.ends_with(bisa_engine::ops::AMENDMENT_SUFFIX))
        .collect()
}

/// A held amendment belongs to its goal: closing the goal drops it and its
/// gate, so nothing is orphaned in the library.
#[tokio::test(flavor = "multi_thread")]
async fn held_amendment_is_the_goals_and_dropped_on_close() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let (_, run) = run_on(&engine, "close me", held_workflow());
    let goal = run.scope.goal().expect("a goal's run");
    let reply = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "amend_workflow", "agent": DRIVER, "goal": goal.to_string(), "workflow": held_amendment()}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    assert_eq!(held_copies(&engine).len(), 1);
    assert_eq!(
        engine
            .workspace()
            .list_workflows_in(bisa_store::WorkflowScope::Goal(goal))
            .unwrap()
            .len(),
        1,
        "the held copy is listed as the goal's"
    );
    engine
        .close_goal(
            goal,
            bisa_core::ClosureReason::Abandoned { rationale: None },
        )
        .unwrap();
    assert!(
        held_copies(&engine).is_empty(),
        "closing drops the held amendment"
    );
    assert!(
        engine.inbox().iter().all(|g| g.home.goal() != Some(goal)),
        "and withdraws its gate"
    );
    engine.shutdown().await;
}

/// When the run finishes with the amendment still undecided, the held copy
/// has nothing left to amend and is dropped.
#[tokio::test(flavor = "multi_thread")]
async fn held_amendment_dropped_when_run_finishes() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let (_, run) = run_on(&engine, "finish me", held_workflow());
    let goal = run.scope.goal().expect("a goal's run");
    let reply = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "amend_workflow", "agent": DRIVER, "goal": goal.to_string(), "workflow": held_amendment()}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    assert_eq!(held_copies(&engine).len(), 1);
    engine
        .release_step(current_run(&engine, goal).id, &sid("hold"), None)
        .unwrap();
    assert_eq!(
        finished_run(&engine, goal).await.outcome,
        Some(RunOutcome::Done)
    );
    until("the held amendment to be dropped", || {
        held_copies(&engine).is_empty().then_some(())
    })
    .await;
    assert!(engine.inbox().iter().all(|g| g.home.goal() != Some(goal)));
    engine.shutdown().await;
}

/// The amendment gate survives a restart as a journaled question under its
/// subject; deciding it durably applies the held copy.
#[tokio::test(flavor = "multi_thread")]
async fn amend_gate_decided_after_restart_applies() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let (_, run) = run_on(&engine, "restart me", held_workflow());
    let goal = run.scope.goal().expect("a goal's run");
    let reply = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "amend_workflow", "agent": DRIVER, "goal": goal.to_string(), "workflow": held_amendment()}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    engine.shutdown().await;

    // A fresh process: no live gate, only the journal and the held copy.
    let engine = engine_on(&dir);
    assert!(engine.inbox().is_empty());
    assert_eq!(
        held_copies(&engine).len(),
        1,
        "the held copy survived the restart"
    );
    let out = engine
        .decide_durable(
            &bisa_core::Home::from(goal),
            true,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
    assert_eq!(out.gate, bisa_core::Gate::Approval);
    let amended = current_run(&engine, goal);
    assert!(amended.steps.contains_key(&sid("new")));
    assert!(!amended.steps.contains_key(&sid("old")));
    assert!(held_copies(&engine).is_empty(), "applied and dropped");
    // The amendment is decided: the durable door now reaches the run's held
    // `wait`, whose one decision is its release.
    let out = engine
        .decide_durable(
            &bisa_core::Home::from(goal),
            true,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
    assert_eq!(out.gate, bisa_core::Gate::Escalation);
    let run = finished_run(&engine, goal).await;
    assert_eq!(run.steps[&sid("hold")].state, bisa_core::StepState::done());
    // Nothing waiting and nothing owed: refused.
    let err = engine
        .decide_durable(
            &bisa_core::Home::from(goal),
            true,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap_err();
    assert!(err.is_refusal(), "{err}");
    engine.shutdown().await;
}

/// An amendment approved after the run moved past it says so, and asks for a
/// fresh proposal; the held copy is dropped either way.
#[tokio::test(flavor = "multi_thread")]
async fn stale_amendment_at_approval_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    // hold → old (mock agent, which yields and finishes) → tail (release).
    let (_, run) = run_on(
        &engine,
        "outrun me",
        new_workflow(
            "Held",
            chain(vec![
                step(
                    "hold",
                    StepKind::Wait {
                        until: WaitFor::Release,
                    },
                ),
                step(
                    "tail",
                    StepKind::Wait {
                        until: WaitFor::Release,
                    },
                ),
                agent_step("old", "mock"),
            ]),
        ),
    );
    let goal = run.scope.goal().expect("a goal's run");
    // The amendment swaps `old` for `new`, keeping the two holds.
    let amendment = json!({"name": "Held", "steps": [
        {"id": "hold", "name": "Hold", "kind": "wait", "until": {"until": "release"}, "then": ["tail"]},
        {"id": "tail", "name": "Tail", "kind": "wait", "until": {"until": "release"}, "then": ["new"]},
        {"id": "new", "name": "New", "kind": "agent", "instructions": "new way", "harness": ["mock"]}
    ]});
    let reply = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "amend_workflow", "agent": DRIVER, "goal": goal.to_string(), "workflow": amendment}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    let gate_id = reply["gate"].as_str().unwrap().to_string();
    // Meanwhile the run moves past `tail` — with `old` now running, the
    // amendment would replace a started step.
    engine
        .release_step(current_run(&engine, goal).id, &sid("hold"), None)
        .unwrap();
    engine
        .release_step(current_run(&engine, goal).id, &sid("tail"), None)
        .unwrap();
    step_in_state(&engine, goal, "old", "running").await;
    let err = engine.decide(&gate_id, true, None, None, None).unwrap_err();
    assert!(
        err.to_string().contains("can no longer be applied"),
        "{err}"
    );
    assert!(err.to_string().contains("propose it again"), "{err}");
    assert!(held_copies(&engine).is_empty(), "dropped either way");
    engine.shutdown().await;
}

/// A durable decision with two steps waiting names the step, or is refused.
#[tokio::test(flavor = "multi_thread")]
async fn decide_without_engine_refuses_when_two_steps_wait() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let mut fork = step(
        "fork",
        StepKind::Wait {
            until: WaitFor::Release,
        },
    );
    fork.then = vec![bisa_core::Flow::to(sid("a")), bisa_core::Flow::to(sid("b"))];
    let (_, run) = run_on(
        &engine,
        "two questions",
        new_workflow(
            "Two",
            vec![
                fork,
                step(
                    "a",
                    StepKind::Approval {
                        prompt: "A?".into(),
                    },
                ),
                step(
                    "b",
                    StepKind::Approval {
                        prompt: "B?".into(),
                    },
                ),
            ],
        ),
    );
    let goal = run.scope.goal().expect("a goal's run");
    engine
        .release_step(current_run(&engine, goal).id, &sid("fork"), None)
        .unwrap();
    step_in_state(&engine, goal, "b", "waiting").await;
    for gate in engine.inbox() {
        engine.inner().gates.withdraw(&gate.id);
    }
    let err = engine
        .decide_durable(
            &bisa_core::Home::from(goal),
            true,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap_err();
    assert!(err.to_string().contains("steps waiting"), "{err}");
    assert!(err.to_string().contains("name one"), "{err}");
    let wrong = sid("fork");
    let err = engine
        .decide_durable(
            &bisa_core::Home::from(goal),
            true,
            None,
            None,
            None,
            None,
            Some(&wrong),
        )
        .unwrap_err();
    assert!(err.to_string().contains("not waiting"), "{err}");
    let b = sid("b");
    engine
        .decide_durable(
            &bisa_core::Home::from(goal),
            true,
            None,
            None,
            None,
            None,
            Some(&b),
        )
        .unwrap();
    let after = current_run(&engine, goal);
    assert!(matches!(
        after.steps[&sid("b")].state,
        bisa_core::StepState::Done { .. }
    ));
    assert_eq!(after.steps[&sid("a")].state, bisa_core::StepState::Waiting);
    engine.shutdown().await;
}

/// While staff is installed and enabled, an agent step that names nobody is
/// refused at the Workflow Agent's door — validation reports it, a proposal
/// is refused with it — and one that names a disabled or absent agent too.
#[tokio::test(flavor = "multi_thread")]
async fn validate_and_propose_refuse_an_unstaffed_agent_step_when_staff_exists() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    engine
        .workspace()
        .install(CatalogKind::Agent, "developer")
        .unwrap();
    let goal = engine.submit_goal(guided("staff it")).unwrap();

    let unstaffed = draft("Nobody", "mock");
    let v = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "validate_workflow", "agent": DRIVER, "workflow": unstaffed}),
    )
    .await;
    assert_eq!(v["ok"], json!(true), "{v}");
    assert!(
        v["problems"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["kind"] == json!("unstaffed_step") && p["step"] == json!("do")),
        "{v}"
    );
    let refused = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "propose_workflow", "agent": DRIVER, "goal": goal.id.to_string(), "workflow": unstaffed}),
    )
    .await;
    assert_eq!(refused["ok"], json!(false), "{refused}");
    assert!(
        refused["problems"][0]["kind"] == json!("unstaffed_step"),
        "{refused}"
    );
    assert!(
        engine
            .workspace()
            .get_goal(goal.id)
            .unwrap()
            .workflow
            .is_none(),
        "nothing was written"
    );

    // Naming an agent nobody installed is refused too.
    let mut stranger = draft("Stranger", "mock");
    stranger["steps"][0]["assignee"] = json!({"agent": "nobody-here"});
    let refused = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "propose_workflow", "agent": DRIVER, "goal": goal.id.to_string(), "workflow": stranger}),
    )
    .await;
    assert_eq!(refused["ok"], json!(false), "{refused}");
    assert!(
        refused["problems"][0]["kind"] == json!("unknown_assignee"),
        "{refused}"
    );

    // Staffed with the installed agent — or with an input of kind assignee — it lands.
    let mut staffed = draft("Staffed", "mock");
    staffed["steps"][0]["assignee"] = json!({"agent": "developer"});
    let ok = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "propose_workflow", "agent": DRIVER, "goal": goal.id.to_string(), "workflow": staffed}),
    )
    .await;
    assert_eq!(ok["ok"], json!(true), "{ok}");
    let mut by_input = draft("By input", "mock");
    by_input["inputs"] =
        json!([{"name": "who", "label": "Who", "kind": "assignee", "required": true}]);
    by_input["steps"][0]["assignee"] = json!({"input": "who"});
    let v = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "validate_workflow", "agent": DRIVER, "workflow": by_input}),
    )
    .await;
    assert_eq!(v["problems"], json!([]), "{v}");
    engine.shutdown().await;
}

/// With nobody installed, an unassigned agent step is the only kind that can
/// run, and it is accepted; `list_staff` says the roster is empty.
#[tokio::test(flavor = "multi_thread")]
async fn an_unstaffed_step_is_accepted_when_nobody_is_installed() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let goal = engine.submit_goal(guided("no staff")).unwrap();
    let ok = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "propose_workflow", "agent": DRIVER, "goal": goal.id.to_string(), "workflow": draft("Alone", "mock")}),
    )
    .await;
    assert_eq!(ok["ok"], json!(true), "{ok}");
    let staff = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "list_staff", "agent": DRIVER}),
    )
    .await;
    assert_eq!(staff["ok"], json!(true), "{staff}");
    assert!(
        staff["text"]
            .as_str()
            .unwrap()
            .contains("nobody is installed"),
        "{staff}"
    );
    assert_eq!(staff["agents"], json!([]));
    engine.shutdown().await;
}

/// On an auto goal the Workflow Agent's amendment applies at once: no held
/// copy, no gate, the run continues on the amended steps.
#[tokio::test(flavor = "multi_thread")]
async fn an_auto_goals_amendment_applies_without_a_gate() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let wf = engine
        .create_workflow(new_workflow(
            "Held",
            chain(vec![
                step(
                    "hold",
                    StepKind::Wait {
                        until: WaitFor::Release,
                    },
                ),
                agent_step("old", "mock"),
            ]),
        ))
        .unwrap();
    let goal = engine
        .submit_goal(SubmitRequest {
            workflow: Some(wf.id),
            ..SubmitRequest::captured("amend me alone")
        })
        .unwrap();
    assert_eq!(goal.mode, bisa_core::GoalMode::Auto);
    let run = engine.start_run(goal.id, Default::default()).unwrap();
    let amendment = json!({"name": "Held", "steps": [
        {"id": "hold", "name": "Hold", "kind": "wait", "until": {"until": "release"}, "then": ["new"]},
        {"id": "new", "name": "New", "kind": "agent", "instructions": "new way", "harness": ["mock"]}
    ]});
    let reply = intake_roundtrip(
        engine.socket_path(),
        json!({"op": "amend_workflow", "agent": DRIVER, "goal": goal.id.to_string(), "workflow": amendment}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    assert!(reply["gate"].is_null(), "{reply}");
    let amended = current_run(&engine, goal.id);
    assert!(amended.steps.contains_key(&sid("new")));
    assert!(!amended.steps.contains_key(&sid("old")));
    assert_eq!(amended.workflow.revision, run.workflow.revision + 1);
    assert!(engine
        .inbox()
        .iter()
        .all(|g| g.home.goal() != Some(goal.id)));
    assert!(
        engine
            .workspace()
            .list_workflows()
            .unwrap()
            .iter()
            .all(|w| !w.name.ends_with(bisa_engine::ops::AMENDMENT_SUFFIX)),
        "nothing was held"
    );
    let notes = notes(&engine, goal.id);
    assert!(
        notes
            .iter()
            .any(|n| n.contains("applied") && n.contains("auto mode")),
        "{notes:?}"
    );
    engine
        .release_step(current_run(&engine, goal.id).id, &sid("hold"), None)
        .unwrap();
    assert_eq!(
        finished_run(&engine, goal.id).await.outcome,
        Some(RunOutcome::Done)
    );
    engine.shutdown().await;
}
