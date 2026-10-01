//! What one broken thing costs: one row, never a list, a page or the
//! process. A failed run, a run file this build cannot read, a goal
//! snapshot from another shape of the code, an activity row the feed cannot
//! type — each is read back through `/inbox`, `/pulse`, `/goals` and
//! `/goals/{id}` and none of them takes the screen down. The reported bug
//! was exactly the other thing: a run failed and the inbox and the pulse
//! never loaded again. A listening workflow this build cannot read is the
//! same cost on `/listeners`: its own listeners, never the list.

use crate::node::Node;
use bisa_core::activity::{ActivityConcept, ActivityFact, ActivitySource, ActivitySourceKind};
use bisa_core::event::JournalPayload;
use bisa_core::{AgentId, AskKind, GoalId};
use bisa_store::{Paths, SessionRow, SessionStatus};
use serde_json::{json, Value};
use std::time::Duration;

/// The goal's snapshot file, as the store lays it out — the one file a test
/// overwrites to stand in for another build's shape.
fn goal_snapshot(node: &Node, goal: GoalId) -> std::path::PathBuf {
    Paths::new(node.data_dir())
        .state_dir(&Paths::ns_goal(goal))
        .join(format!("{}-{goal}.json", bisa_core::kind::KIND_GOAL))
}

/// A workflow a call begins, held open.
fn hook_workflow(name: &str) -> Value {
    json!({
        "name": name,
        "steps": [
            {"id": "ticket", "name": "A ticket arrives", "kind": "start",
             "on": {"event": "hook"}, "then": ["hold"]},
            {"id": "hold", "name": "Hold", "kind": "wait", "until": {"until": "release"}},
        ]
    })
}

fn run_snapshot(node: &Node, goal: GoalId, run: &str) -> std::path::PathBuf {
    Paths::new(node.data_dir())
        .state_dir(&Paths::ns_goal(goal))
        .join(format!("{}-{run}.json", bisa_core::kind::KIND_WORKFLOW_RUN))
}

/// The one line that stands in for a snapshot another build wrote: a signed
/// event whose content this build refuses. Overwriting the file with text
/// that is not an event is the same refusal one step earlier.
fn poison(path: &std::path::Path) {
    std::fs::write(path, "{not an event this build can read").expect("overwrite the snapshot");
}

async fn wait_failed(node: &Node, goal: GoalId) -> Value {
    for _ in 0..100 {
        let v = node.get(&format!("/goals/{goal}")).await;
        if v["run"]["outcome"] == json!("failed") {
            return v;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!(
        "the run never failed: {}",
        node.get(&format!("/goals/{goal}")).await["run"]
    );
}

/// A workspace that cannot be written — the disk full, a folder made
/// read-only by somebody's hand — is the node's fault, not the caller's: a
/// 500 with the one sentence every 500 says (the path and the reason go to
/// the log, in full), and the node lives: the next read answers, and the
/// write goes through once the folder can be written again. Nothing is
/// half-made on the way. The folder's mode is set back before the test ends,
/// so the temporary directory can be cleaned up.
#[tokio::test(flavor = "multi_thread")]
#[cfg(unix)]
async fn a_workspace_that_cannot_be_written_answers_a_500_makes_nothing_and_the_node_lives() {
    use std::os::unix::fs::PermissionsExt;
    let node = Node::start().await;
    let goals = Paths::new(node.data_dir()).goals_dir();
    std::fs::create_dir_all(&goals).unwrap();
    let was = std::fs::metadata(&goals).unwrap().permissions();
    std::fs::set_permissions(&goals, std::fs::Permissions::from_mode(0o500)).unwrap();

    let (code, said) = node
        .req(
            "POST",
            "/goals",
            Some(json!({"statement": "write where nothing can be written", "mode": "manual"})),
        )
        .await;
    // Back to a folder that can be written before anything is asserted, so
    // a failed assertion leaves nothing behind that cannot be removed.
    std::fs::set_permissions(&goals, was).unwrap();
    assert_eq!(
        code, 500,
        "a write the disk refused is the node's fault: {said}"
    );
    // The one sentence every 500 says; the path and the reason are the log's.
    assert_eq!(said["text"]["id"], json!("error-node-internal"), "{said}");
    let words = said["error"].as_str().unwrap_or_default();
    assert!(
        !words.contains(goals.to_string_lossy().as_ref()),
        "the folder's path is the log's, not the person's: {said}"
    );
    assert!(
        node.get("/goals").await["goals"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "nothing half-made"
    );
    let (code, _) = node.req("GET", "/health", None).await;
    assert_eq!(code, 200, "the node lives");

    let (code, made) = node
        .req(
            "POST",
            "/goals",
            Some(json!({"statement": "write where it can be", "mode": "manual"})),
        )
        .await;
    assert_eq!(code, 200, "{made}");
    assert!(made["goal"]["id"].is_string(), "{made}");
    node.shutdown().await;
}

/// The reported bug, as a test: a run fails, and every screen still loads.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_run_still_reads_through_inbox_pulse_and_goals() {
    let node = Node::start().await;
    let goal = node.new_goal("fail and be read").await;
    let wf = node
        .workflow(json!({
            "name": "Doomed",
            "steps": [{
                "id": "work", "name": "Work", "kind": "agent",
                "instructions": "work", "harness": ["no-such-harness"]
            }]
        }))
        .await;
    node.start_run(goal, wf).await;
    let page = wait_failed(&node, goal).await;
    assert_eq!(page["status"], json!("failed"));
    assert_eq!(page["run_unreadable"], json!(false));

    // The failure is a notice on the goal's row: read here, opened there —
    // and never owed. The watermark that covers the conversation covers it.
    // The run's outcome is the store's the moment it settles; its notice is
    // the activity feed's, a bus hop behind — so the row is awaited.
    let mut row = Value::Null;
    for _ in 0..100 {
        let inbox = node.get("/inbox").await;
        assert!(inbox["rows"].is_array(), "{inbox}");
        if let Some(r) = inbox["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["key"] == json!(goal.to_string()))
        {
            row = r.clone();
            let has_notice = row["notices"]
                .as_array()
                .is_some_and(|ns| ns.iter().any(|n| n["notice"] == json!("run_failed")));
            if has_notice {
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(
        row["kind"],
        json!("goal"),
        "the failed goal earns a row: {row}"
    );
    assert_eq!(row["needs_action"], json!([]), "a failure is not owed");
    assert!(
        row["notices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["notice"] == json!("run_failed")),
        "the run's failure is a notice: {row}"
    );
    assert!(
        row["unread_notices"].as_u64().unwrap() >= 1,
        "unread until looked at: {row}"
    );
    assert_eq!(row["read"], json!(false));
    let first = &row["notices"][0];
    assert_eq!(
        first["source"]["kind"],
        json!("goal"),
        "a Pulse row, verbatim: {first}"
    );
    assert!(first["event"]["type"].is_string(), "{first}");
    assert!(
        node.get("/inbox?source=workflows").await["rows"]
            .as_array()
            .unwrap()
            .is_empty(),
        "a goal's failure is its goal's, never its workflow's"
    );
    node.post("/read", json!({"scope": goal.to_string()})).await;
    let read = node.get("/inbox").await;
    let row = read["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == json!(goal.to_string()))
        .expect("kept once read");
    assert_eq!(row["unread_notices"], json!(0), "{row}");
    assert_eq!(row["read"], json!(true));
    assert!(
        !row["notices"].as_array().unwrap().is_empty(),
        "the notice stays, read"
    );
    let goals = node.get("/goals").await;
    let row = goals["goals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["id"] == json!(goal.to_string()))
        .expect("the failed goal is still a row");
    assert_eq!(row["status"], json!("failed"));
    assert_eq!(row["holder"], json!("finished"));

    let pulse = node.get("/pulse").await;
    let rows = pulse["rows"].as_array().unwrap();
    assert!(
        rows.iter()
            .any(|r| r["source"]["id"] == json!(goal.to_string())
                && r["event"]["type"] == json!("step")
                && r["event"]["event"]["fact"] == json!("failed")),
        "the failed step is a pulse row: {pulse}"
    );
    node.shutdown().await;
}

/// One activity row the feed cannot type is left out of its page, said in
/// the log, and paged past — never the reason a page fails.
#[tokio::test(flavor = "multi_thread")]
async fn a_poisoned_activity_row_is_skipped_not_the_page() {
    let node = Node::start().await;
    let a = node.new_goal("first").await;
    let _b = node.new_goal("second").await;
    let before = node.get("/pulse").await["rows"].as_array().unwrap().len();
    assert!(before >= 2, "two goals are at least two rows");

    // A `message` row whose event is not a message.
    let seq = node
        .ws
        .record_activity(
            &ActivityFact {
                at: 4_102_444_800,
                concept: ActivityConcept::Goals,
                kind: "message".into(),
                source: ActivitySource::new(ActivitySourceKind::Goal, a.to_string()),
                author: None,
                event: json!("not a message"),
            },
            false,
        )
        .expect("record the poisoned row");

    // Newest first: the poisoned row is the whole first page of one.
    let page = node.get("/pulse?limit=1").await;
    assert_eq!(page["rows"], json!([]), "the row is skipped: {page}");
    assert_eq!(page["next"]["seq"], json!(seq), "and the cursor passes it");
    let after = node
        .get(&format!(
            "/pulse?limit=1&before={}&before_seq={}",
            page["next"]["at"], page["next"]["seq"]
        ))
        .await;
    assert_eq!(
        after["rows"].as_array().unwrap().len(),
        1,
        "the page after it is reachable"
    );

    let all = node.get("/pulse").await;
    let rows = all["rows"].as_array().unwrap();
    assert_eq!(rows.len(), before, "every good row is listed");
    assert!(rows.iter().all(|r| r["seq"] != json!(seq)));
    node.shutdown().await;
}

/// A goal whose snapshot another shape of the code wrote is left out of the
/// lists — the inbox, the goals — and refused on its own page; the other
/// goals are unaffected.
#[tokio::test(flavor = "multi_thread")]
async fn an_unreadable_goal_snapshot_is_skipped_by_the_lists() {
    let node = Node::start().await;
    let a = node.new_goal("readable").await;
    let b = node.new_goal("written elsewhere").await;
    poison(&goal_snapshot(&node, b));

    let goals = node.get("/goals").await;
    let ids: Vec<Value> = goals["goals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["id"].clone())
        .collect();
    assert_eq!(ids, vec![json!(a.to_string())], "{goals}");
    let inbox = node.get("/inbox").await;
    assert!(inbox["rows"].is_array());
    let (code, _) = node.req("GET", &format!("/goals/{b}"), None).await;
    assert_eq!(code, 404, "a single read refuses it");
    node.shutdown().await;
}

/// A goal whose run file cannot be read is a row that says so — drawn as a
/// goal with no run — and the list, the goal page and the inbox all answer.
#[tokio::test(flavor = "multi_thread")]
async fn an_unreadable_run_snapshot_degrades_the_goal_row() {
    let node = Node::start().await;
    let goal = node.new_goal("lose my run").await;
    node.ask(goal).await;
    let run = node.get(&format!("/goals/{goal}")).await["run"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    poison(&run_snapshot(&node, goal, &run));

    let goals = node.get("/goals").await;
    let row = goals["goals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["id"] == json!(goal.to_string()))
        .expect("the goal is still a row");
    assert_eq!(row["run_unreadable"], json!(true), "{row}");
    // No readable run: the row ghosts the chosen workflow's definition, every
    // step pending and none current — drawn as for a goal with no run.
    let steps = row["strip"]["steps"].as_array().unwrap();
    assert!(
        !steps.is_empty()
            && steps
                .iter()
                .all(|s| s["state"] == json!({"state": "pending"})),
        "ghosted, no run: {row}"
    );
    assert_eq!(row["strip"]["current"], json!([]));
    let page = node.get(&format!("/goals/{goal}")).await;
    assert_eq!(page["run"], Value::Null);
    assert_eq!(page["run_unreadable"], json!(true));
    let inbox = node.get("/inbox").await;
    assert!(
        inbox["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["key"] == json!(goal.to_string())
                && !r["needs_action"].as_array().unwrap().is_empty()),
        "the live gate's row is still there: {inbox}"
    );
    node.shutdown().await;
}

/// The amendment a run holds is an inbox row with its proposal, and still
/// one after a restart — rebuilt from the journal under its subject and
/// decidable by goal.
#[tokio::test(flavor = "multi_thread")]
async fn a_held_amendment_is_in_the_inbox_with_its_proposal_and_after_a_restart() {
    let node = Node::start().await;
    // Guided: an auto goal applies an amendment alone and asks nobody.
    let v = node
        .post(
            "/goals",
            json!({"statement": "amend me", "title": "T", "mode": "guided"}),
        )
        .await;
    let goal: GoalId = serde_json::from_value(v["goal"]["id"].clone()).expect("goal id");
    let wf = node
        .workflow(json!({
            "name": "Held",
            "steps": [
                {"id": "hold", "name": "Hold", "kind": "wait", "until": {"until": "release"}, "then": ["old"]},
                {"id": "old", "name": "Old", "kind": "agent", "instructions": "old way", "harness": ["no-such-harness"]}
            ]
        }))
        .await;
    let run = node.start_run(goal, wf).await;
    let reply = node
        .intake_op(json!({
            "op": "amend_workflow", "agent": AgentId::WORKFLOW, "goal": goal.to_string(),
            "workflow": {"name": "Held", "steps": [
                {"id": "hold", "name": "Hold", "kind": "wait", "until": {"until": "release"}, "then": ["new"]},
                {"id": "new", "name": "New", "kind": "agent", "instructions": "new way", "harness": ["no-such-harness"]}
            ]}
        }))
        .await;
    assert_eq!(reply["ok"], json!(true), "{reply}");

    let action_of = |inbox: &Value| -> Value {
        inbox["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["key"] == json!(goal.to_string()))
            .map(|r| {
                // The release ask stands beside the amendment; the row lists both.
                r["needs_action"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|a| {
                        a["subject"]
                            .as_str()
                            .is_some_and(|s| s.starts_with("amend:"))
                    })
                    .cloned()
                    .unwrap_or_else(|| panic!("no amendment on the row: {r}"))
            })
            .unwrap_or_else(|| panic!("no row for the goal: {inbox}"))
    };
    let live = action_of(&node.get("/inbox").await);
    let subject = live["subject"].as_str().unwrap().to_string();
    assert!(subject.starts_with(&format!("amend:{run}@")), "{live}");
    assert!(
        live["proposal"]["name"]
            .as_str()
            .unwrap()
            .ends_with("amendment"),
        "{live}"
    );
    assert_eq!(live["durable"], json!(false));

    let node = node.restart().await;
    let rebuilt = action_of(&node.get("/inbox").await);
    assert_eq!(rebuilt["subject"], json!(subject));
    assert_eq!(rebuilt["durable"], json!(true));
    assert!(rebuilt["gate_id"].is_null());
    assert!(
        rebuilt["proposal"]["name"]
            .as_str()
            .unwrap()
            .ends_with("amendment"),
        "{rebuilt}"
    );
    let page = node.get(&format!("/goals/{goal}")).await;
    // The release ask is open on the goal too; the amendment's question stands beside it.
    let amend_question = page["guidance"]["open_questions"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|q| q["subject"] == json!(subject))
        .cloned()
        .unwrap_or_else(|| panic!("the amendment's question is open on the goal: {page}"));
    assert_eq!(amend_question["subject"], json!(subject));

    node.post(&format!("/goals/{goal}/decide"), json!({"approve": true}))
        .await;
    let mut applied = false;
    for _ in 0..100 {
        let v = node.get(&format!("/goals/{goal}")).await;
        let steps = &v["run"]["steps"];
        if steps.get("new").is_some() && steps.get("old").is_none() {
            applied = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(applied, "the held copy replaced the steps not started");
    node.shutdown().await;
}

/// What a restart leaves on the surfaces, staged as the files a dead node
/// leaves: a session row still `Live` and a publish question nobody could
/// answer any more. The next node ends the row, withdraws the question
/// with the reason on the pulse, says so in a note, and lists no session —
/// so the desktop's roster, inbox and goal page agree with the store.
#[tokio::test(flavor = "multi_thread")]
async fn a_restart_ends_the_session_rows_and_withdraws_the_decision_it_lost() {
    let node = Node::start().await;
    let goal = node.new_goal("survive a restart").await;
    let owner = node.ws.owner_keys().clone();
    node.ws
        .append_journal(
            &bisa_core::Home::from(goal),
            JournalPayload::Question {
                work_item: None,
                gate: "workstream:ws-1".into(),
                text: "Publish: push main to origin?".into(),
                expects: AskKind::Decision,
            },
            &owner,
            None,
        )
        .expect("the question is a journal fact");
    node.ws
        .record_session(&SessionRow {
            id: "sess-dead".into(),
            adapter: "mock".into(),
            status: SessionStatus::Live,
            ..Default::default()
        })
        .expect("the row is written");
    assert_eq!(node.ws.list_live_sessions().unwrap().len(), 1);

    let node = node.restart().await;

    let row = node
        .ws
        .session_by_id("sess-dead")
        .unwrap()
        .expect("the row stays");
    assert_eq!(
        row.status,
        SessionStatus::Ended,
        "a live row at boot is a dead session"
    );
    assert!(row.ended_at.is_some());
    assert!(node.ws.list_live_sessions().unwrap().is_empty());
    let sessions = node.get("/sessions").await;
    assert_eq!(sessions["sessions"], json!([]), "{sessions}");

    let pulse = node.get("/pulse?limit=50").await;
    let rows = pulse["rows"].as_array().unwrap();
    let withdrawn = rows
        .iter()
        .find(|r| r["event"]["type"] == json!("withdrawn"))
        .unwrap_or_else(|| panic!("the withdrawal is a pulse row: {pulse}"));
    assert_eq!(withdrawn["event"]["subject"], json!("workstream:ws-1"));
    assert_eq!(
        withdrawn["event"]["reason"],
        json!("interrupted by a restart")
    );
    assert!(
        rows.iter().any(|r| r["event"]["type"] == json!("note")
            && r["event"]["text"]
                .as_str()
                .is_some_and(|t| t.contains("interrupted by a restart") && t.contains("Publish:"))),
        "the goal says which decision was lost: {pulse}"
    );
    let inbox = node.get("/inbox").await;
    let owed = inbox["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["key"] == json!(goal.to_string()))
        .flat_map(|r| r["needs_action"].as_array().cloned().unwrap_or_default())
        .count();
    assert_eq!(owed, 0, "a withdrawn question is owed by nobody: {inbox}");

    // Nothing is withdrawn twice: a second boot finds the withdrawal and passes.
    let node = node.restart().await;
    let pulse = node.get("/pulse?limit=50").await;
    let n = pulse["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["event"]["type"] == json!("withdrawn"))
        .count();
    assert_eq!(n, 1, "{pulse}");
    node.shutdown().await;
}

/// A listening workflow whose snapshot another shape of the code wrote costs
/// its own listeners, never the list of everybody's — and the others are
/// called as before.
#[tokio::test(flavor = "multi_thread")]
async fn an_unreadable_listening_workflow_costs_its_own_listeners_only() {
    let node = Node::start().await;
    let sound = node.workflow(hook_workflow("Sound")).await;
    let broken = node.workflow(hook_workflow("Written elsewhere")).await;
    for wf in [sound, broken] {
        let (code, v) = node
            .req(
                "PUT",
                &format!("/workflows/{wf}/listening"),
                Some(json!({})),
            )
            .await;
        assert_eq!(code, 200, "{v}");
    }
    let hosts = |listeners: &Value| -> Vec<String> {
        let mut hosts: Vec<String> = listeners
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["host"].as_str().unwrap().to_string())
            .collect();
        hosts.sort();
        hosts
    };
    let mut both = vec![format!("workspace:{sound}"), format!("workspace:{broken}")];
    both.sort();
    let all = node.get("/listeners").await;
    assert_eq!(hosts(&all), both, "{all}");

    poison(&Paths::new(node.data_dir()).library_workflow_snapshot(broken));

    let all = node.get("/listeners").await;
    assert_eq!(hosts(&all), vec![format!("workspace:{sound}")], "{all}");
    let (code, v) = node
        .req(
            "POST",
            &format!("/workflows/{sound}/hooks/ticket"),
            Some(json!({"subject": "still heard"})),
        )
        .await;
    assert_eq!(code, 202, "{v}");
    assert!(v["signal"].is_string(), "{v}");
    let (code, _) = node
        .req("GET", &format!("/workflows/{broken}/listeners"), None)
        .await;
    assert_ne!(code, 200, "a single read refuses it");
    node.shutdown().await;
}
