//! The platform's own agents' ops, over the real intake socket.
//!
//! The General Agent and the Workflow Agent see the workspace and browse the catalog; the General
//! Agent installs, assigns and captures standing goals; the Workflow Agent
//! has a file of its own (`workflow_agent.rs`). Each op is driven here through the
//! same door a harness session uses — the unix socket, one JSON object per
//! line — because the wire shape is the contract the MCP client is written
//! against.
//!
//! The refusal is tested op by op for the same reason. The MCP server offers
//! these tools only to a core agent's session; the socket is a path, and any
//! session that knows it can write to it. If the boundary is anywhere, it is
//! here.

use crate::common;

use bisa_core::{AgentId, Assignee, GoalId, GoalOrigin, RosterPolicy, Slug, TeamId};
use bisa_engine::{Engine, SubmitRequest};
use bisa_harness::mock::MockAdapter;
use bisa_store::{CatalogKind, NewProject, Workspace, CATALOG};
use common::*;
use serde_json::{json, Value};

const GENERAL: &str = AgentId::GENERAL;
const WORKFLOW: &str = AgentId::WORKFLOW;

fn aid(s: &str) -> AgentId {
    AgentId::new(s).unwrap()
}

fn tid(s: &str) -> TeamId {
    TeamId::new(s).unwrap()
}

fn attached_project(ws: &Workspace, goal: GoalId, slug: &str) -> bisa_core::Project {
    let project = ws
        .create_project(NewProject::managed(slug).unwrap())
        .unwrap();
    ws.attach(goal, project.id).unwrap();
    project
}

fn engine_on(dir: &tempfile::TempDir) -> Engine {
    engine_with(dir, vec![MockAdapter::default()])
}

fn manual_goal(engine: &Engine, statement: &str) -> GoalId {
    engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            ..SubmitRequest::captured(statement)
        })
        .unwrap()
        .id
}

async fn intake(socket: &std::path::Path, req: Value) -> Value {
    intake_roundtrip(socket, req).await
}

// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn workspace_overview_answers_the_shape_of_the_whole_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let ws = engine.workspace();

    ws.install(CatalogKind::Team, "engineering").unwrap();
    let channel = ws
        .create_channel("design", None, RosterPolicy::default(), Default::default())
        .unwrap();
    let peer = ws
        .list_agents()
        .unwrap()
        .into_iter()
        .find(|a| !a.origin.is_core())
        .unwrap()
        .pubkey;
    ws.open_dm(&[peer]).unwrap();
    let goal = manual_goal(&engine, "ship the thing");
    attached_project(ws, goal, "storefront");
    let stuck = manual_goal(&engine, "waiting on a person");
    let asked = intake(
        engine.socket_path(),
        json!({"op": "ask_human", "goal": stuck.to_string(),
               "question": "which directory?", "expects": "answer"}),
    )
    .await;
    assert_eq!(asked["ok"], json!(true));
    // A library workflow that listens: on, its schedule armed by one tick.
    let mut hourly = step(
        "hourly",
        bisa_core::StepKind::Start {
            on: bisa_core::StartOn::Schedule {
                schedule: bisa_core::Schedule::every(3600),
            },
            inputs: Default::default(),
            guard: Default::default(),
        },
    );
    hourly.then = vec![bisa_core::Flow::to(sid("go"))];
    let wf = engine
        .create_workflow(new_workflow(
            "local one",
            vec![hourly, agent_step("go", "mock")],
        ))
        .unwrap();
    ws.install(CatalogKind::Workflow, "weekly-review").unwrap();
    engine
        .set_listening(
            bisa_core::ListenerHost::Workspace { workflow: wf.id },
            Default::default(),
            None,
        )
        .unwrap();
    engine.tick_listeners_at(1_700_000_000 - 3600).await;

    for caller in [GENERAL, WORKFLOW] {
        let reply = intake(
            engine.socket_path(),
            json!({"op": "workspace_overview", "agent": caller}),
        )
        .await;
        assert_eq!(reply["ok"], json!(true), "{caller}: {reply}");

        assert_eq!(reply["agents"]["core"], json!([GENERAL, WORKFLOW]));
        let installed: Vec<String> =
            serde_json::from_value(reply["agents"]["installed"].clone()).unwrap();
        assert!(
            !installed.iter().any(|a| AgentId::is_core_str(a)),
            "{installed:?}"
        );
        let expected_agents = ws.list_agents().unwrap();
        assert_eq!(reply["agents"]["total"], json!(expected_agents.len()));
        assert_eq!(installed.len(), expected_agents.len() - 2);

        let team = ws.get_team(&tid("engineering")).unwrap();
        let teams: Vec<Value> = serde_json::from_value(reply["teams"].clone()).unwrap();
        let engineering = teams
            .iter()
            .find(|t| t["id"] == json!("engineering"))
            .unwrap();
        assert_eq!(
            engineering["agents"],
            json!(team
                .members
                .iter()
                .filter(|m| m.as_agent().is_some())
                .count())
        );
        let channels: Vec<Value> = serde_json::from_value(reply["channels"].clone()).unwrap();
        let channel_ids: Vec<Value> = channels.iter().map(|c| c["id"].clone()).collect();
        assert_eq!(channel_ids, vec![json!("general"), json!(channel.id)]);

        assert_eq!(reply["skills"], json!(ws.list_skills().unwrap().len()));
        assert_eq!(reply["mcp_servers"]["total"], json!(0));
        assert_eq!(
            reply["listening"],
            json!({
                "workflows": 1,
                "goals": 0,
                "paused": 0,
                "listeners": 1,
                "next_due": 1_700_000_000u64,
            }),
            "what listens, and the earliest thing that happens by itself"
        );

        let projects: Vec<Value> = serde_json::from_value(reply["projects"].clone()).unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0]["goals"], json!([goal.to_string()]));

        assert_eq!(reply["goals"]["by_status"]["draft"], json!(2));
        let waiting: Vec<String> =
            serde_json::from_value(reply["goals"]["waiting_on_human"].clone()).unwrap();
        assert_eq!(
            waiting,
            vec![stuck.to_string()],
            "an open question is 'waiting on a human'"
        );

        assert_eq!(reply["workflows"]["total"], json!(2));
        assert_eq!(reply["workflows"]["catalog"], json!(1));
        assert_eq!(reply["running"], json!([]));
        for kind in CatalogKind::ALL.iter().copied() {
            assert_eq!(
                reply["catalog"][kind.as_str()]["total"],
                json!(CATALOG.entries(kind).len()),
                "{kind}: {reply}"
            );
        }
        assert_eq!(reply["catalog"]["team"]["installed"], json!(1));
        assert_eq!(reply["catalog"]["workflow"]["installed"], json!(1));
    }
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn list_catalog_lists_the_whole_catalog_or_one_kind() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    engine
        .workspace()
        .install(CatalogKind::Agent, "developer")
        .unwrap();

    let all = intake(
        engine.socket_path(),
        json!({"op": "list_catalog", "agent": GENERAL}),
    )
    .await;
    assert_eq!(all["ok"], json!(true), "{all}");
    let entries: Vec<Value> = serde_json::from_value(all["entries"].clone()).unwrap();
    let total: usize = CatalogKind::ALL
        .iter()
        .map(|k| CATALOG.entries(*k).len())
        .sum();
    assert_eq!(entries.len(), total);

    let workflows = intake(
        engine.socket_path(),
        json!({"op": "list_catalog", "agent": WORKFLOW, "kind": "workflow"}),
    )
    .await;
    let entries: Vec<Value> = serde_json::from_value(workflows["entries"].clone()).unwrap();
    assert_eq!(
        entries.len(),
        CATALOG.entries(CatalogKind::Workflow).len(),
        "every workflow template, and nothing of another kind"
    );
    let bug_fix = entries
        .iter()
        .find(|e| e["slug"] == json!("bug-fix"))
        .unwrap();
    assert_eq!(bug_fix["kind"], json!("workflow"));
    assert!(bug_fix["requires"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r == "developer"));
    assert_eq!(bug_fix["installed"], json!(false));

    let bogus = intake(
        engine.socket_path(),
        json!({"op": "list_catalog", "agent": GENERAL, "kind": "wizard"}),
    )
    .await;
    assert_eq!(bogus["ok"], json!(false));
    engine.shutdown().await;
}

/// A workflow template pulls in the agents its steps name, and their skills.
#[tokio::test(flavor = "multi_thread")]
async fn installing_a_catalog_entry_is_transitive_idempotent_and_journalled() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let goal = manual_goal(&engine, "needs a shape");

    let reply = intake(
        engine.socket_path(),
        json!({"op": "install_catalog_entry", "agent": GENERAL,
               "kind": "workflow", "slug": "bug-fix", "goal": goal.to_string()}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    let agents: Vec<String> = serde_json::from_value(reply["installed"]["agents"].clone()).unwrap();
    assert!(
        !agents.is_empty(),
        "a template installs the agents it names: {reply}"
    );
    let workflows = reply["installed"]["workflows"].as_array().unwrap();
    assert_eq!(workflows.len(), 1);
    assert_eq!(workflows[0][0], json!("bug-fix"));
    let installed = engine
        .workspace()
        .workflow_for_slug("bug-fix")
        .unwrap()
        .unwrap();
    assert_eq!(
        engine.workspace().validate_workflow(&installed).unwrap(),
        vec![]
    );
    for agent in &agents {
        assert!(engine.workspace().get_agent(&aid(agent)).is_ok(), "{agent}");
    }
    let note = notes(&engine, goal)
        .into_iter()
        .find(|t| t.contains("bug-fix"))
        .expect("the install is on the record");
    assert!(note.contains("workflows:"), "{note}");

    let again = intake(
        engine.socket_path(),
        json!({"op": "install_catalog_entry", "agent": GENERAL,
               "kind": "workflow", "slug": "bug-fix", "goal": goal.to_string()}),
    )
    .await;
    assert_eq!(again["ok"], json!(true), "{again}");
    assert_eq!(again["installed"]["workflows"], json!([]));
    assert!(notes(&engine, goal)
        .iter()
        .any(|t| t.contains("already installed")));

    let unknown = intake(
        engine.socket_path(),
        json!({"op": "install_catalog_entry", "agent": GENERAL, "kind": "agent", "slug": "wizard"}),
    )
    .await;
    assert_eq!(unknown["ok"], json!(false), "{unknown}");
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn assign_adds_then_replaces_and_leaves_a_record() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let ws = engine.workspace();
    ws.install(CatalogKind::Agent, "developer").unwrap();
    ws.install(CatalogKind::Agent, "qa-engineer").unwrap();
    ws.install(CatalogKind::Team, "engineering").unwrap();
    let goal = manual_goal(&engine, "ship the thing");
    let project = attached_project(ws, goal, "storefront");

    let added = intake(
        engine.socket_path(),
        json!({"op": "assign", "agent": GENERAL, "goal": goal.to_string(),
               "assignees": ["agent:developer"], "replace": false}),
    )
    .await;
    assert_eq!(added["ok"], json!(true), "{added}");
    assert_eq!(added["assignees"], json!(["agent:developer"]));

    let more = intake(
        engine.socket_path(),
        json!({"op": "assign", "agent": GENERAL, "goal": goal.to_string(),
               "assignees": ["team:engineering", "agent:developer"], "replace": false}),
    )
    .await;
    assert_eq!(
        more["assignees"],
        json!(["agent:developer", "team:engineering"])
    );
    let swapped = intake(
        engine.socket_path(),
        json!({"op": "assign", "agent": GENERAL, "goal": goal.to_string(),
               "assignees": ["agent:qa-engineer"], "replace": true}),
    )
    .await;
    assert_eq!(swapped["assignees"], json!(["agent:qa-engineer"]));
    assert_eq!(
        ws.get_goal(goal).unwrap().assignees,
        vec![Assignee::Agent("qa-engineer".into())]
    );
    let on_project = intake(
        engine.socket_path(),
        json!({"op": "assign", "agent": GENERAL, "goal": goal.to_string(),
               "project": project.id.to_string(), "assignees": ["agent:developer"], "replace": false}),
    )
    .await;
    assert_eq!(on_project["assignees"], json!(["agent:developer"]));
    assert_eq!(
        ws.get_project(project.id).unwrap().assignees,
        vec![Assignee::Agent("developer".into())]
    );

    let record = notes(&engine, goal);
    let assignments: Vec<&String> = record
        .iter()
        .filter(|n| n.contains("assigned") || n.contains("reassigned"))
        .collect();
    assert_eq!(assignments.len(), 4, "{record:?}");
    assert!(assignments[2].starts_with("reassigned"));
    assert!(assignments[3].contains("storefront"));

    let ghost = intake(
        engine.socket_path(),
        json!({"op": "assign", "agent": GENERAL, "goal": goal.to_string(), "assignees": ["agent:nobody"]}),
    )
    .await;
    assert_eq!(ghost["ok"], json!(false));
    let ungrammatical = intake(
        engine.socket_path(),
        json!({"op": "assign", "agent": GENERAL, "goal": goal.to_string(), "assignees": ["developer"]}),
    )
    .await;
    assert_eq!(ungrammatical["ok"], json!(false));
    engine.shutdown().await;
}

/// `capture_goal`: work that recurs becomes a standing goal — captured for
/// the Workflow Agent to design with the start event its statement names,
/// with a note saying who captured it. It listens once its design is
/// adopted; capturing arms nothing.
#[tokio::test(flavor = "multi_thread")]
async fn capture_goal_captures_a_standing_goal_for_the_workflow_agent_to_design() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let ws = engine.workspace();
    let statement = "Every Monday at 09:00, post a digest of last week's work in #general";

    let reply = intake(
        engine.socket_path(),
        json!({"op": "capture_goal", "agent": GENERAL, "statement": statement,
               "title": "Weekly digest"}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    let id: GoalId = reply["goal"].as_str().unwrap().parse().unwrap();
    let goal = ws.get_goal(id).unwrap();
    assert_eq!(goal.statement, statement);
    assert_eq!(goal.title.as_deref(), Some("Weekly digest"));
    assert_eq!(goal.origin, GoalOrigin::Captured);
    assert_eq!(
        goal.workflow, None,
        "its workflow is the designer's to make"
    );
    assert_eq!(goal.listening, None, "capturing arms nothing");
    assert!(
        notes(&engine, id)
            .iter()
            .any(|n| n.contains("captured by the General Agent")),
        "{:?}",
        notes(&engine, id)
    );
    assert!(engine.armed_listeners().armed.is_empty());

    // A blank title is no title.
    let untitled = intake(
        engine.socket_path(),
        json!({"op": "capture_goal", "agent": GENERAL,
               "statement": "Whenever someone posts in #support, triage it", "title": "  "}),
    )
    .await;
    assert_eq!(untitled["ok"], json!(true), "{untitled}");
    let id: GoalId = untitled["goal"].as_str().unwrap().parse().unwrap();
    assert_eq!(ws.get_goal(id).unwrap().title, None);

    // A goal needs a statement.
    let empty = intake(
        engine.socket_path(),
        json!({"op": "capture_goal", "agent": GENERAL, "statement": "  "}),
    )
    .await;
    assert_eq!(empty["ok"], json!(false), "{empty}");
    assert_eq!(ws.list_goals(None).unwrap().len(), 2);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn create_project_makes_a_repository_attached_to_the_goal() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let ws = engine.workspace();
    let goal = manual_goal(&engine, "needs somewhere to work");

    let reply = intake(
        engine.socket_path(),
        json!({"op": "create_project", "agent": GENERAL, "goal": goal.to_string(), "slug": "storefront"}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    let path = std::path::PathBuf::from(reply["path"].as_str().unwrap());
    assert!(path.is_dir());
    assert!(path.join(".git").is_dir());
    let projects = ws.projects_for(goal).unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(reply["attached_to"], json!(goal.to_string()));
    // The one creation path gives it a root commit, so its first workstream
    // is a branch of its own — or, on a machine with no commit identity, a
    // note on the goal saying why there is none yet. Read with the engine's
    // own git, the one that decided: the free function reads this machine's
    // global config, which the engine's isolated git never sees.
    match engine.inner().git().identity(&path).unwrap().source {
        bisa_vcs::IdentitySource::None => assert!(
            common::notes(&engine, goal)
                .iter()
                .any(|n| n.contains("no commit identity")),
            "{:?}",
            common::notes(&engine, goal)
        ),
        _ => assert!(
            bisa_vcs::git::status(&path).unwrap().oid.is_some(),
            "a root commit"
        ),
    }

    let reply = intake(
        engine.socket_path(),
        json!({"op": "create_project", "agent": GENERAL, "goal": goal.to_string(), "slug": "../escape"}),
    )
    .await;
    assert_eq!(reply["ok"], json!(false), "{reply}");
    assert_eq!(ws.projects_for(goal).unwrap().len(), 1);
    engine.shutdown().await;
}

/// **The boundary.** Every core-agent op refuses a session that is not
/// allowed it, and says who is.
#[tokio::test(flavor = "multi_thread")]
async fn every_core_agent_op_refuses_any_other_session() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let ws = engine.workspace();
    ws.install(CatalogKind::Agent, "developer").unwrap();
    let goal = manual_goal(&engine, "not yours to staff");
    let draft =
        json!({"name": "x", "steps": [{"id": "a", "name": "A", "kind": "end", "finish": "done"}]});

    // (op body, the callers it refuses)
    let general_only = vec![
        json!({"op": "install_catalog_entry", "kind": "agent", "slug": "qa-engineer"}),
        json!({"op": "assign", "goal": goal.to_string(), "assignees": ["agent:developer"]}),
        json!({"op": "capture_goal", "statement": "every Monday, post the digest"}),
    ];
    let workflow_only = vec![
        json!({"op": "list_workflow_templates"}),
        json!({"op": "get_workflow", "workflow": "bug-fix"}),
        json!({"op": "validate_workflow", "workflow": draft}),
        json!({"op": "propose_workflow", "goal": goal.to_string(), "workflow": draft}),
        json!({"op": "amend_workflow", "goal": goal.to_string(), "workflow": draft}),
    ];
    let shared = vec![
        json!({"op": "workspace_overview"}),
        json!({"op": "list_catalog", "kind": "agent"}),
    ];
    let refuse = |mut req: Value, caller: &str, names: &str| {
        let socket = engine.socket_path().to_path_buf();
        let names = names.to_string();
        let caller = caller.to_string();
        async move {
            req["agent"] = json!(caller);
            let op = req["op"].as_str().unwrap().to_string();
            let reply = intake(&socket, req).await;
            assert_eq!(
                reply["ok"],
                json!(false),
                "{op} must refuse {caller}: {reply}"
            );
            let errors = reply["errors"].to_string();
            assert!(
                errors.contains(&names),
                "{op}: the refusal names who may call it: {errors}"
            );
        }
    };
    for req in &general_only {
        refuse(req.clone(), "developer", GENERAL).await;
        refuse(req.clone(), WORKFLOW, GENERAL).await;
    }
    for req in &workflow_only {
        refuse(req.clone(), "developer", WORKFLOW).await;
        refuse(req.clone(), GENERAL, WORKFLOW).await;
    }
    for req in &shared {
        refuse(req.clone(), "developer", GENERAL).await;
        let mut ok = req.clone();
        ok["agent"] = json!(WORKFLOW);
        assert_eq!(intake(engine.socket_path(), ok).await["ok"], json!(true));
    }

    // And none of them did anything on the way to being refused.
    assert!(ws.get_agent(&aid("qa-engineer")).is_err());
    assert!(ws.get_goal(goal).unwrap().assignees.is_empty());
    assert_eq!(
        ws.list_goals(None).unwrap().len(),
        1,
        "nobody captured a goal on the way"
    );
    assert!(ws.list_workflows().unwrap().is_empty());
    assert!(ws.get_goal(goal).unwrap().workflow.is_none());
    engine.shutdown().await;
}

/// A work-item session can create a project: the runner is looked up from
/// the item and signs the journal note.
#[tokio::test(flavor = "multi_thread")]
async fn a_work_item_session_can_create_a_project() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let ws = engine.workspace();
    ws.install(CatalogKind::Agent, "developer").unwrap();
    let mut dev = ws.get_agent(&aid("developer")).unwrap();
    dev.harness = "mock".into();
    ws.update_agent(dev).unwrap();
    // The item exists because a step asked for it, and the developer took it.
    let (goal, _) = run_on(
        &engine,
        "build me a calculator",
        new_workflow("w", vec![assigned_agent_step("write", "mock", "developer")]),
    );
    finished_run(&engine, goal.id).await;
    let item = items_of(&engine, goal.id).remove(0);
    assert_eq!(item.agent.as_deref(), Some("developer"));

    let reply = intake(
        engine.socket_path(),
        json!({"op": "create_project", "work_item": item.id.to_string(),
               "goal": goal.id.to_string(), "slug": "calc"}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    let projects = ws.projects_for(goal.id).unwrap();
    assert_eq!(projects.len(), 1);
    let author = ws
        .journal(&bisa_core::Home::from(goal.id))
        .unwrap()
        .iter()
        .rev()
        .find_map(|e| match &e.payload {
            bisa_core::event::JournalPayload::Note { text } if text.contains("project") => {
                Some(e.author.clone())
            }
            _ => None,
        })
        .expect("journalled");
    assert_eq!(author, ws.get_agent(&aid("developer")).unwrap().pubkey);
    engine.shutdown().await;
}

/// `get_goal` names the projects this goal is attached to, and where they
/// are — the attachments and nothing else.
#[tokio::test(flavor = "multi_thread")]
async fn get_goal_says_where_files_go() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let ws = engine.workspace();
    let parent = manual_goal(&engine, "the programme");
    let child = engine
        .submit_goal(SubmitRequest {
            mode: bisa_core::GoalMode::Manual,
            origin: GoalOrigin::Spawned { parent },
            ..SubmitRequest::captured("one slice of it")
        })
        .unwrap()
        .id;
    let elsewhere = manual_goal(&engine, "somebody else's");
    attached_project(ws, child, "site");
    attached_project(ws, parent, "platform");
    let shared = attached_project(ws, elsewhere, "shared-lib");
    attached_project(ws, elsewhere, "not-yours");
    ws.attach(child, shared.id).unwrap();

    let reply = intake(
        engine.socket_path(),
        json!({"op": "get_goal", "goal": child.to_string()}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    assert_eq!(reply["goal"]["origin"]["origin"], json!("spawned"));
    let by_slug: std::collections::HashMap<&str, &Value> = reply["projects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["slug"].as_str().unwrap(), p))
        .collect();
    assert_eq!(by_slug.len(), 2, "{reply}");
    for slug in ["site", "shared-lib"] {
        assert!(by_slug.contains_key(slug));
    }
    for slug in ["platform", "not-yours"] {
        assert!(!by_slug.contains_key(slug));
    }
    let expected =
        ws.project_root_path(&ws.get_project_by_slug(&Slug::new("site").unwrap()).unwrap());
    assert_eq!(
        by_slug["site"]["path"].as_str(),
        Some(expected.display().to_string().as_str())
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn any_agent_may_create_a_project() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_on(&dir);
    let ws = engine.workspace();
    ws.install(CatalogKind::Agent, "developer").unwrap();
    let goal = manual_goal(&engine, "build me a calculator");
    let reply = intake(
        engine.socket_path(),
        json!({"op": "create_project", "agent": "developer", "goal": goal.to_string(), "slug": "calc"}),
    )
    .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");
    let projects = ws.projects_for(goal).unwrap();
    assert_eq!(projects.len(), 1);
    let root = ws.project_root_path(&projects[0]);
    assert!(root.join(".git").exists());
    let author = ws
        .journal(&bisa_core::Home::from(goal))
        .unwrap()
        .iter()
        .rev()
        .find_map(|e| match &e.payload {
            bisa_core::event::JournalPayload::Note { text } if text.contains("project") => {
                Some(e.author.clone())
            }
            _ => None,
        })
        .expect("journalled");
    assert_eq!(author, ws.get_agent(&aid("developer")).unwrap().pubkey);
    engine.shutdown().await;
}
