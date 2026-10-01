//! Retiring over HTTP: the preview's facts, the plan carried out, the
//! archived lists shown only when asked, and what is refused.

use crate::node::Node;
use serde_json::{json, Value};

async fn project(node: &Node, slug: &str) -> String {
    let made = node
        .post("/projects", json!({"kind": "new", "slug": slug}))
        .await;
    made["project"]["id"]
        .as_str()
        .expect("a project")
        .to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_goal_is_previewed_retired_by_archiving_and_hidden_until_asked() {
    let node = Node::start().await;
    let goal = node.new_goal("ship it").await;
    let foreign = project(&node, "foreign").await;
    node.post(
        &format!("/projects/{foreign}/attach"),
        json!({"goal": goal.to_string()}),
    )
    .await;

    let preview = node.get(&format!("/goals/{goal}/retirement")).await;
    assert_eq!(
        preview["retirement"]["projects_attached"][0]["slug"],
        json!("foreign"),
        "{preview}"
    );
    assert_eq!(preview["retirement"]["projects_born"], json!([]));
    assert_eq!(preview["retirement"]["agents"], json!(0));
    assert_eq!(preview["retirement"]["harnesses"], json!(0));
    assert!(
        preview["retirement"]["run"].is_null(),
        "no run is going: {preview}"
    );
    assert!(preview["retirement"]["refusal"].is_null(), "{preview}");
    assert_eq!(
        preview["retirement"]["projects_attached"][0]["workstream_ids"]
            .as_array()
            .map(|w| w.len()),
        Some(1),
        "the primary workstream is named, so the desktop can count its terminals there: {preview}"
    );

    let done = node
        .post(
            &format!("/goals/{goal}/retire"),
            json!({"goal": "archive", "projects": "keep"}),
        )
        .await;
    assert_eq!(done["retired"]["stopped_sessions"], json!(0), "{done}");
    assert_eq!(done["retired"]["unsettled_sessions"], json!(0), "{done}");
    let row = node.get(&format!("/goals/{goal}")).await;
    assert_eq!(
        row["status"],
        json!("closed"),
        "archiving closes first: {row}"
    );
    assert!(row["goal"]["archived"]["at"].is_number(), "{row}");

    let listed = node.get("/goals").await;
    assert!(
        listed["goals"]
            .as_array()
            .unwrap()
            .iter()
            .all(|g| g["id"] != json!(goal.to_string())),
        "hidden from the list"
    );
    let listed = node.get("/goals?archived=true").await;
    let mine: Vec<&Value> = listed["goals"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|g| g["id"] == json!(goal.to_string()))
        .collect();
    assert_eq!(mine.len(), 1, "shown when asked");
    assert!(mine[0]["archived"]["at"].is_number());

    let back = node
        .post(
            &format!("/goals/{goal}/archive"),
            json!({"archived": false}),
        )
        .await;
    assert!(back["goal"]["archived"].is_null(), "{back}");
    assert_eq!(
        back["goal"]["closed"]["reason"],
        json!("abandoned"),
        "unarchiving does not reopen"
    );
    let projects = node.get("/projects").await;
    assert_eq!(
        projects["projects"].as_array().unwrap().len(),
        1,
        "the attached project was left alone"
    );
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_used_workflow_refuses_deletion_with_its_holders_and_archives_instead() {
    let node = Node::start().await;
    let wf = node.workflow(json!({"name": "release", "steps": []})).await;
    let goal = node.new_goal("ship it").await;
    node.put(
        &format!("/goals/{goal}/workflow"),
        json!({"workflow": wf.to_string()}),
    )
    .await;

    let preview = node.get(&format!("/workflows/{wf}/retirement")).await;
    assert_eq!(
        preview["retirement"]["used_by"].as_array().unwrap().len(),
        1,
        "{preview}"
    );
    let (code, body) = node
        .req(
            "POST",
            &format!("/workflows/{wf}/retire"),
            Some(json!({"workflow": "delete", "projects": "keep"})),
        )
        .await;
    assert_eq!(code, 409, "{body}");
    assert!(
        body["error"].as_str().unwrap().contains("archive it"),
        "{body}"
    );

    let done = node
        .post(
            &format!("/workflows/{wf}/retire"),
            json!({"workflow": "archive", "projects": "keep"}),
        )
        .await;
    assert_eq!(done["workflow"], json!(wf.to_string()), "{done}");
    let library = node.get("/workflows").await;
    assert!(
        library["workflows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|w| w["workflow"]["id"] != json!(wf.to_string())),
        "out of the library"
    );
    let with = node.get("/workflows?archived=true").await;
    assert!(with["workflows"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w["workflow"]["id"] == json!(wf.to_string())
            && w["workflow"]["archived"]["at"].is_number()));
    let (code, body) = node
        .req(
            "POST",
            &format!("/goals/{goal}/run"),
            Some(json!({"inputs": {}})),
        )
        .await;
    assert_eq!(
        code, 400,
        "a run on an archived workflow is refused: {body}"
    );

    let back = node
        .post(
            &format!("/workflows/{wf}/archive"),
            json!({"archived": false}),
        )
        .await;
    assert!(back["workflow"]["workflow"]["archived"].is_null(), "{back}");
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_project_is_archived_hidden_refused_for_attachment_and_taken_back_out() {
    let node = Node::start().await;
    let pid = project(&node, "site").await;
    let goal = node.new_goal("ship it").await;

    let row = node
        .post(
            &format!("/projects/{pid}/archive"),
            json!({"archived": true}),
        )
        .await;
    assert!(
        row["project"]["project"]["archived"]["at"].is_number(),
        "{row}"
    );
    assert_eq!(
        node.get("/projects").await["projects"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        node.get("/projects?archived=true").await["projects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let (code, body) = node
        .req(
            "POST",
            &format!("/projects/{pid}/attach"),
            Some(json!({"goal": goal.to_string()})),
        )
        .await;
    assert_eq!(code, 400, "{body}");

    node.post(
        &format!("/projects/{pid}/archive"),
        json!({"archived": false}),
    )
    .await;
    assert_eq!(
        node.get("/projects").await["projects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    node.post(
        &format!("/projects/{pid}/attach"),
        json!({"goal": goal.to_string()}),
    )
    .await;
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_workflow_deleted_or_archived_is_announced_as_such_on_the_bus() {
    let node = Node::start().await;
    let made = node
        .post("/workflows", json!({"name": "release", "steps": []}))
        .await;
    let id = made["workflow"]["id"]
        .as_str()
        .expect("a workflow")
        .to_string();
    let mut bus = node.events();

    node.post(
        &format!("/workflows/{id}/archive"),
        json!({"archived": true}),
    )
    .await;
    let archived = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let e = bus.recv().await.expect("the bus");
            if let bisa_engine::EnginePayload::WorkflowArchived { workflow, archived } = e.payload {
                break (workflow.to_string(), archived);
            }
        }
    })
    .await
    .expect("workflow_archived on the bus");
    assert_eq!(archived, (id.clone(), true));

    node.post(
        &format!("/workflows/{id}/archive"),
        json!({"archived": false}),
    )
    .await;
    node.delete(&format!("/workflows/{id}")).await;
    let deleted = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let e = bus.recv().await.expect("the bus");
            if let bisa_engine::EnginePayload::WorkflowDeleted { workflow } = &e.payload {
                break workflow.to_string();
            }
        }
    })
    .await
    .expect("workflow_deleted on the bus");
    assert_eq!(deleted, id);
    let (code, _) = node.req("GET", &format!("/workflows/{id}"), None).await;
    assert_eq!(code, 404);
    node.shutdown().await;
}
