//! Runs of the workspace over HTTP: a workflow run from the library with no
//! goal behind it — started, listed, read, answered and decided through the
//! run, stopped and restarted one by one or by its workflow — and the goal's
//! own run left to its goal. The Inbox files a run's asks on its workflow's
//! row.
//!
//! A run body names where the run begins: nothing, or the start by hand, is
//! a run by hand; an event start is a test run, begun there as if the sample
//! it carries had happened. A run an occurrence began is `events.rs`'s.
//!
//! Every scenario is a node over a workspace in a temporary directory.

use crate::node::{human_workflow, Node};
use bisa_core::{GoalId, RunId};
use serde_json::{json, Value};

/// A workflow that waits for a person to release it: a run that stays open.
fn held_workflow() -> Value {
    json!({
        "name": "Hold",
        "description": "held until released",
        "steps": [{"id": "hold", "name": "Hold", "kind": "wait", "until": {"until": "release"}}]
    })
}

/// A workflow whose one step reads the goal it serves: it runs on a goal only.
fn goal_reading_workflow() -> Value {
    json!({
        "name": "Reads the goal",
        "steps": [{"id": "say", "name": "Say", "kind": "notify",
                   "template": "working on {goal.statement}"}]
    })
}

/// A workflow with two ways in — by hand, and on a call that may say what
/// it is about — held open so its runs stay live. Both inputs are read: the
/// call's start maps `subject`, a gateway asks who it is for — an input no
/// step reads is a problem, and a workflow with one does not run.
fn two_ways_in() -> Value {
    json!({
        "name": "Two ways in",
        "inputs": [
            {"name": "subject", "label": "Subject", "kind": "text"},
            {"name": "who", "label": "Who", "kind": "text"},
        ],
        "steps": [
            {"id": "by-hand", "name": "By hand", "kind": "start",
             "on": {"event": "manual"}, "then": ["for-somebody"]},
            {"id": "ticket", "name": "A ticket arrives", "kind": "start",
             "on": {"event": "hook"},
             "inputs": {"subject": "{event.payload.subject}"},
             "then": ["for-somebody"]},
            {"id": "for-somebody", "name": "For somebody?", "kind": "if",
             "when": {"condition": "not", "of":
                 {"condition": "input_equals", "input": "who", "value": "nobody"}},
             "then": [{"to": "hold", "branch": "yes"}, {"to": "shelve", "branch": "no"}]},
            {"id": "hold", "name": "Hold", "kind": "wait", "until": {"until": "release"}},
            {"id": "shelve", "name": "Shelve", "kind": "wait", "until": {"until": "release"}},
        ]
    })
}

/// A workflow only a call begins, and only one that says what it is about.
fn called_only() -> Value {
    json!({
        "name": "Called only",
        "inputs": [{"name": "subject", "label": "Subject", "kind": "text", "required": true}],
        "steps": [
            {"id": "ticket", "name": "A ticket arrives", "kind": "start",
             "on": {"event": "hook"},
             "inputs": {"subject": "{event.payload.subject}"},
             "then": ["hold"]},
            {"id": "hold", "name": "Hold", "kind": "wait", "until": {"until": "release"}},
        ]
    })
}

async fn start(node: &Node, wf: impl std::fmt::Display) -> RunId {
    let v = node.post(&format!("/workflows/{wf}/runs"), json!({})).await;
    serde_json::from_value(v["run"]["id"].clone()).unwrap_or_else(|_| panic!("no run: {v}"))
}

fn run_home(run: RunId) -> Value {
    json!({"home": "run", "run": run.to_string()})
}

/// The workflow's row in the Inbox, when it has one.
async fn workflow_row(node: &Node, wf: impl std::fmt::Display) -> Option<Value> {
    let v = node.get("/inbox").await;
    v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == json!(wf.to_string()))
        .cloned()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_workflow_runs_in_the_workspace_and_is_answered_through_its_run() {
    let node = Node::start().await;
    let wf = node.workflow(human_workflow()).await;

    let v = node.post(&format!("/workflows/{wf}/runs"), json!({})).await;
    assert_eq!(v["status"], json!("waiting"), "{v}");
    assert_eq!(v["run"]["scope"]["scope"], json!("workspace"), "{v}");
    let first: RunId = serde_json::from_value(v["run"]["id"].clone()).unwrap();
    // A second run goes beside the first: never queued.
    let second = start(&node, wf).await;
    assert_ne!(first, second);
    assert!(
        node.get("/goals").await["goals"]
            .as_array()
            .unwrap()
            .is_empty(),
        "no goal was captured"
    );

    // The workflow's runs, newest first, numbered among themselves.
    let v = node.get(&format!("/workflows/{wf}/runs")).await;
    let runs = v["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0]["id"], json!(second.to_string()));
    assert_eq!(runs[0]["number"], json!(2));
    assert_eq!(runs[1]["number"], json!(1));
    assert!(runs.iter().all(|r| r["scope"] == json!("workspace")));
    assert!(runs.iter().all(|r| r["status"] == json!("waiting")));
    assert!(runs.iter().all(|r| r.get("goal").is_none()));
    // The row counts them.
    let row = node.get(&format!("/workflows/{wf}")).await;
    assert_eq!(row["runs"]["live"], json!(2), "{row}");
    assert_eq!(row["runs"]["total"], json!(2));
    assert_eq!(row["runs"]["last"]["id"], json!(second.to_string()));
    assert_eq!(row["workspace_problems"], json!([]));

    // One run, whole: what it owes, decided through its own home.
    let v = node.get(&format!("/runs/{first}")).await;
    assert_eq!(v["run"]["id"], json!(first.to_string()));
    assert_eq!(v["summary"]["number"], json!(1));
    assert_eq!(v["holder"], json!("you"), "{v}");
    let ask = &v["needs_actions"][0];
    assert_eq!(ask["home"], run_home(first), "{v}");
    assert_eq!(ask["step"], json!("ask"));

    // The Inbox files both runs' asks on the workflow's row.
    let row = workflow_row(&node, wf).await.expect("the workflow's row");
    assert_eq!(row["kind"], json!("workflow"));
    assert_eq!(row["needs_action"].as_array().unwrap().len(), 2, "{row}");

    // Answered through the run; an option it never offered is refused.
    let (code, _) = node
        .req(
            "POST",
            &format!("/runs/{first}/steps/ask/answer"),
            Some(json!({"answer": {"selected": ["mysql"]}})),
        )
        .await;
    assert_eq!(code, 400);
    let v = node
        .post(
            &format!("/runs/{first}/steps/ask/answer"),
            json!({"answer": {"selected": ["sqlite"]}}),
        )
        .await;
    assert_eq!(v["run"]["outcome"], json!("done"), "{v}");

    // Decided through the run's home.
    let v = node
        .post(
            &format!("/runs/{second}/decide"),
            json!({"approve": true, "answer": {"selected": ["postgres"]}}),
        )
        .await;
    assert_eq!(v["home"], run_home(second), "{v}");
    assert_eq!(v["status"]["of"], json!("run"), "{v}");
    let v = node.get(&format!("/runs/{second}")).await;
    assert_eq!(v["run"]["outcome"], json!("done"), "{v}");
    assert_eq!(v["holder"], json!("finished"), "{v}");

    // Nothing is owed now, and a decision stands: the row is kept, handled.
    let row = workflow_row(&node, wf)
        .await
        .expect("the row is earned once and kept");
    assert_eq!(row["needs_action"], json!([]), "{row}");
    assert_eq!(row["handled"], json!(true), "{row}");
    let row = node.get(&format!("/workflows/{wf}")).await;
    assert_eq!(row["runs"]["live"], json!(0));
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_workspace_run_is_stopped_and_restarted_by_itself_or_by_its_workflow() {
    let node = Node::start().await;
    let wf = node.workflow(held_workflow()).await;
    let one = start(&node, wf).await;

    let v = node
        .post(
            &format!("/runs/{one}/stop"),
            json!({"rationale": "not now"}),
        )
        .await;
    assert_eq!(
        v["run"]["cancelled"],
        json!({"cause": "stopped", "rationale": "not now"}),
        "{v}"
    );
    // Stopping it again answers it as it is.
    let v = node.post(&format!("/runs/{one}/stop"), json!({})).await;
    assert_eq!(v["run"]["cancelled"]["cause"], json!("stopped"));
    // Restarting a finished run starts a new one.
    let v = node.post(&format!("/runs/{one}/restart"), json!({})).await;
    assert_eq!(v["status"], json!("waiting"), "{v}");
    let two: RunId = serde_json::from_value(v["run"]["id"].clone()).unwrap();
    assert_ne!(two, one);

    // The workflow's verbs: every live run of it, and those only.
    let three = start(&node, wf).await;
    let v = node.post(&format!("/workflows/{wf}/stop"), json!({})).await;
    let mut stopped: Vec<String> = serde_json::from_value(v["runs"].clone()).unwrap();
    stopped.sort();
    let mut expected = vec![two.to_string(), three.to_string()];
    expected.sort();
    assert_eq!(stopped, expected, "{v}");
    let v = node.post(&format!("/workflows/{wf}/stop"), json!({})).await;
    assert_eq!(v["runs"], json!([]), "nothing goes now");

    let four = start(&node, wf).await;
    let v = node
        .post(&format!("/workflows/{wf}/restart"), json!({}))
        .await;
    let restarted = v["runs"].as_array().unwrap();
    assert_eq!(restarted.len(), 1, "{v}");
    assert_ne!(restarted[0], json!(four.to_string()));
    let v = node.get(&format!("/runs/{four}")).await;
    assert_eq!(v["run"]["cancelled"], json!({"cause": "restarted"}));
    let v = node.get(&format!("/workflows/{wf}/runs")).await;
    assert_eq!(v["runs"].as_array().unwrap().len(), 5);

    // An unknown run and an unknown workflow are 404s.
    let ghost = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    for (method, path) in [
        ("GET", format!("/runs/{ghost}")),
        ("POST", format!("/runs/{ghost}/stop")),
        ("POST", format!("/runs/{ghost}/restart")),
        ("POST", format!("/runs/{ghost}/steps/hold/release")),
        ("POST", format!("/workflows/{ghost}/runs")),
        ("POST", format!("/workflows/{ghost}/stop")),
        ("POST", format!("/workflows/{ghost}/restart")),
    ] {
        let (code, v) = node.req(method, &path, Some(json!({}))).await;
        assert_eq!(code, 404, "{method} {path}: {v}");
    }
    node.shutdown().await;
}

/// A run that is there and cannot be read is said — what is wrong, and with
/// which file — never answered as a run that does not exist.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_that_cannot_be_read_says_so_and_never_poses_as_missing() {
    let node = Node::start().await;
    let wf = node.workflow(held_workflow()).await;
    let run = start(&node, wf).await;
    let (code, _) = node.req("GET", &format!("/runs/{run}"), None).await;
    assert_eq!(code, 200);

    // Written over by something that is no run — another shape of the code,
    // a disk that lost the end of the file.
    let snapshot = node
        .ws
        .paths()
        .home(&bisa_core::Home::Run { run })
        .dir()
        .join("state")
        .join(format!("33413-{run}.json"));
    assert!(snapshot.is_file(), "{}", snapshot.display());
    std::fs::write(&snapshot, b"{ \"kind\": 33413, \"content\": ").unwrap();

    for (method, path) in [
        ("GET", format!("/runs/{run}")),
        ("GET", format!("/runs/{run}/journal")),
        ("POST", format!("/runs/{run}/stop")),
        ("POST", format!("/runs/{run}/steps/hold/release")),
    ] {
        let (code, v) = node.req(method, &path, Some(json!({}))).await;
        let said = v["error"].as_str().unwrap_or_default();
        assert_eq!(code, 404, "{method} {path}: {v}");
        assert!(
            said.contains("33413-") && !said.contains("not found"),
            "{method} {path} names the file it could not read: {v}"
        );
    }
    // The run that names no file at all is the one that is not found.
    let ghost = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    let (code, v) = node.req("GET", &format!("/runs/{ghost}"), None).await;
    assert_eq!(code, 404);
    assert!(
        v["error"]
            .as_str()
            .unwrap_or_default()
            .contains("not found"),
        "{v}"
    );
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_goals_run_is_its_goals_and_its_steps_are_answered_by_run() {
    let node = Node::start().await;
    let wf = node.workflow(held_workflow()).await;
    let goal: GoalId = node.new_goal("hold on a goal").await;
    let goals_run = node.start_run(goal, wf).await;
    let workspace_run = start(&node, wf).await;

    // Stop and restart are the goal's: a conflict that says so.
    for verb in ["stop", "restart"] {
        let (code, v) = node
            .req(
                "POST",
                &format!("/runs/{goals_run}/{verb}"),
                Some(json!({})),
            )
            .await;
        assert_eq!(code, 409, "{verb}: {v}");
        assert!(
            v["error"].as_str().unwrap().contains("from its goal"),
            "{v}"
        );
    }
    // The workflow's verbs never touch it.
    let v = node.post(&format!("/workflows/{wf}/stop"), json!({})).await;
    assert_eq!(v["runs"], json!([workspace_run.to_string()]));
    let v = node.get(&format!("/runs/{goals_run}")).await;
    assert_eq!(v["summary"]["status"], json!("waiting"), "{v}");
    assert_eq!(v["summary"]["goal"], json!(goal.to_string()));
    assert_eq!(v["summary"]["scope"], json!("goal"));

    // A goal's run's step is released by its run too.
    let v = node
        .post(&format!("/runs/{goals_run}/steps/hold/release"), json!({}))
        .await;
    assert_eq!(v["run"]["outcome"], json!("done"), "{v}");
    assert_eq!(
        node.get(&format!("/goals/{goal}")).await["status"],
        json!("done")
    );
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_workflow_that_reads_the_goal_runs_on_a_goal_only() {
    let node = Node::start().await;
    let wf = node.workflow(goal_reading_workflow()).await;
    let row = node.get(&format!("/workflows/{wf}")).await;
    assert_eq!(row["problems"], json!([]), "it can start on a goal: {row}");
    let kinds: Vec<&str> = row["workspace_problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, vec!["needs_goal"], "{row}");

    let (code, v) = node
        .req("POST", &format!("/workflows/{wf}/runs"), Some(json!({})))
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(
        v["problems"]
            .as_array()
            .is_some_and(|ps| ps.iter().any(|p| p["kind"] == json!("needs_goal"))),
        "{v}"
    );
    assert!(node.get(&format!("/workflows/{wf}/runs")).await["runs"]
        .as_array()
        .unwrap()
        .is_empty());
    node.shutdown().await;
}

/// What began a run is read off the run: a person, by hand — with no start
/// named or the start by hand named — or a test, at the event start a person
/// named, as if the sample had happened.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_begins_by_hand_or_as_a_test_at_the_start_it_names() {
    let node = Node::start().await;
    let wf = node.workflow(two_ways_in()).await;
    let runs = format!("/workflows/{wf}/runs");

    // By hand, three times over: no body at all, no start named, and the
    // start by hand named.
    let (code, v) = node.req_raw("POST", &runs, None, Vec::new()).await;
    assert_eq!(code, 200, "a run body may be left out: {v}");
    assert_eq!(v["run"]["start"], json!("by-hand"), "{v}");
    for body in [json!({}), json!({"start": "by-hand"})] {
        let v = node.post(&runs, body.clone()).await;
        assert_eq!(v["status"], json!("waiting"), "{body}: {v}");
        assert_eq!(v["run"]["start"], json!("by-hand"), "{body}: {v}");
        assert!(v["run"].get("event").is_none(), "{body}: {v}");
    }

    // A test run: begun at the event start, its sample read by the start's
    // mapping and laid beside the inputs given. No signal was queued for it.
    let v = node
        .post(
            &runs,
            json!({
                "start": "ticket",
                "event": {"subject": "a sample"},
                "inputs": {"who": "the team"},
            }),
        )
        .await;
    assert_eq!(v["status"], json!("waiting"), "{v}");
    assert_eq!(v["run"]["start"], json!("ticket"), "{v}");
    assert_eq!(v["run"]["event"]["source"], json!("test"), "{v}");
    assert_eq!(
        v["run"]["event"]["payload"],
        json!({"subject": "a sample"}),
        "the event is the sample itself: {v}"
    );
    assert_eq!(v["run"]["inputs"]["subject"], json!("a sample"), "{v}");
    assert_eq!(v["run"]["inputs"]["who"], json!("the team"), "{v}");
    assert!(v["run"].get("dispatched").is_none(), "{v}");
    let tested: RunId = serde_json::from_value(v["run"]["id"].clone()).unwrap();

    // No sample given is an empty one: what the mapping cannot read is left
    // to the input's default.
    let v = node.post(&runs, json!({"start": "ticket"})).await;
    assert_eq!(v["run"]["event"]["source"], json!("test"), "{v}");
    assert_eq!(v["run"]["event"]["payload"], json!({}), "{v}");
    assert!(v["run"]["inputs"].get("subject").is_none(), "{v}");

    // Every surface says who started which, newest first.
    let by_test = json!({"by": "test", "event": "hook"});
    let by_you = json!({"by": "you"});
    let listed = node.get(&runs).await;
    let started: Vec<&Value> = listed["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| &r["started_by"])
        .collect();
    assert_eq!(
        started,
        vec![&by_test, &by_test, &by_you, &by_you, &by_you],
        "{listed}"
    );
    let v = node.get(&format!("/runs/{tested}")).await;
    assert_eq!(v["summary"]["started_by"], by_test, "{v}");
    let row = node.get(&format!("/workflows/{wf}")).await;
    assert_eq!(row["runs"]["last"]["started_by"], by_test, "{row}");
    assert_eq!(row["listening"], Value::Null, "a test run arms nothing");
    assert_eq!(
        node.get(&format!("/workflows/{wf}/listeners")).await,
        json!([])
    );
    node.shutdown().await;
}

/// A run body that names no way in is refused, and nothing runs.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_body_that_names_no_way_in_is_refused() {
    let node = Node::start().await;
    let wf = node.workflow(two_ways_in()).await;
    let runs = format!("/workflows/{wf}/runs");

    for (body, says) in [
        // An event with no start to read it.
        (json!({"event": {"subject": "a sample"}}), "start"),
        // An event for the start that reads none.
        (
            json!({"start": "by-hand", "event": {"subject": "a sample"}}),
            "by-hand",
        ),
        // A step that is no start, and a step nobody wrote.
        (json!({"start": "hold"}), "hold"),
        (json!({"start": "nowhere"}), "nowhere"),
        // A key nobody knows: refused in the one shape every refusal has.
        (json!({"start": "ticket", "sample": {}}), "sample"),
    ] {
        let (code, v) = node.req("POST", &runs, Some(body.clone())).await;
        assert_eq!(code, 400, "{body}: {v}");
        assert!(
            v["error"].as_str().is_some_and(|e| e.contains(says)),
            "{body} is refused in words that name `{says}`: {v}"
        );
    }
    assert_eq!(node.get(&runs).await["runs"], json!([]), "nothing ran");

    // A workflow only events begin: by hand it is refused, and the refusal
    // says how it is tried; a sample that lacks what the run needs never
    // starts it half-filled.
    let wf = node.workflow(called_only()).await;
    let runs = format!("/workflows/{wf}/runs");
    let row = node.get(&format!("/workflows/{wf}")).await;
    assert_eq!(row["event_only"], json!(true), "{row}");
    let (code, v) = node.req("POST", &runs, Some(json!({}))).await;
    assert_eq!(code, 400, "{v}");
    assert!(
        v["error"].as_str().is_some_and(|e| e.contains("by hand")),
        "{v}"
    );
    let (code, v) = node
        .req("POST", &runs, Some(json!({"start": "ticket", "event": {}})))
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(
        v["error"].as_str().is_some_and(|e| e.contains("subject")),
        "{v}"
    );
    assert_eq!(node.get(&runs).await["runs"], json!([]), "nothing ran");
    let v = node
        .post(
            &runs,
            json!({"start": "ticket", "event": {"subject": "a sample"}}),
        )
        .await;
    assert_eq!(v["run"]["inputs"]["subject"], json!("a sample"), "{v}");
    node.shutdown().await;
}

/// A goal's run body reads `start` and `event` the same way: a test run is a
/// run on the goal, and it arms nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_goals_test_run_is_a_run_on_the_goal() {
    let node = Node::start().await;
    let wf = node.workflow(two_ways_in()).await;
    let goal: GoalId = node.new_goal("try the ticket start").await;
    let (code, v) = node
        .req(
            "PUT",
            &format!("/goals/{goal}/workflow"),
            Some(json!({"workflow": wf.to_string()})),
        )
        .await;
    assert_eq!(code, 200, "{v}");

    let v = node
        .post(
            &format!("/goals/{goal}/run"),
            json!({"start": "ticket", "event": {"subject": "a sample"}}),
        )
        .await;
    assert_eq!(v["status"], json!("waiting"), "{v}");
    assert_eq!(v["run"]["scope"]["scope"], json!("goal"), "{v}");
    assert_eq!(v["run"]["start"], json!("ticket"), "{v}");
    assert_eq!(v["run"]["event"]["source"], json!("test"), "{v}");
    assert_eq!(
        v["run"]["event"]["scope"],
        json!({"scope": "goal", "goal": goal.to_string()}),
        "{v}"
    );
    assert_eq!(v["run"]["inputs"]["subject"], json!("a sample"), "{v}");

    let page = node.get(&format!("/goals/{goal}")).await;
    assert_eq!(page["listening"], Value::Null, "a test run arms nothing");
    assert_eq!(page["run"]["id"], v["run"]["id"], "{page}");
    let listed = node.get(&format!("/goals/{goal}/runs")).await;
    assert_eq!(
        listed["runs"][0]["started_by"],
        json!({"by": "test", "event": "hook"}),
        "{listed}"
    );

    // The same refusals as a workflow's run body.
    for body in [
        json!({"event": {"subject": "a sample"}}),
        json!({"start": "by-hand", "event": {}}),
        json!({"start": "hold"}),
    ] {
        let (code, v) = node
            .req("POST", &format!("/goals/{goal}/run"), Some(body.clone()))
            .await;
        assert_eq!(code, 400, "{body}: {v}");
    }
    node.shutdown().await;
}
