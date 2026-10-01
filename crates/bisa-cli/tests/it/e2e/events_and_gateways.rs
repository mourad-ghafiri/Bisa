//! Events and gateways, through the binary: a workflow turned On that begins
//! one run for each thing it hears — a call to its hook, a named signal, a
//! message a person posts — each run fanned out by a `parallel` gateway and
//! joined again; a signal written down while no node ran, heard once when
//! one is back; a boundary event that diverts a step nobody answered.

use super::sealed::{Sealed, AGENT_HARNESS};
use serde_json::{json, Value};

/// A worker that notes what it was told and yields; and, spoken to in a
/// conversation, an agent that answers in a sentence.
fn worker() -> Value {
    json!({
        "turns": [
            {
                "scope": "work_item",
                "tools": [{
                    "name": "yield_result",
                    "arguments": { "output": { "noted": true } },
                }],
                "say": ["Noted."],
            },
            { "scope": "conversation", "say": ["The desk has it."] },
        ],
    })
}

/// A desk that begins on four things — by hand, a hook, a signal, a message
/// — and does two things at once with what it was given.
fn desk() -> Value {
    let four_at_once = json!({ "overlap": { "parallel": 4 } });
    json!({
        "name": "Desk",
        "inputs": [{ "name": "subject", "label": "Subject", "kind": "text", "required": true }],
        "steps": [
            { "id": "by_hand", "name": "By hand", "kind": "start",
              "on": { "event": "manual" }, "then": ["fan"] },
            { "id": "ticket", "name": "A ticket arrives", "kind": "start",
              "on": { "event": "hook" },
              "inputs": { "subject": "{event.payload.subject}" },
              "guard": four_at_once, "then": ["fan"] },
            { "id": "ready", "name": "A report is ready", "kind": "start",
              "on": { "event": "signal", "name": "report.ready" },
              "inputs": { "subject": "{event.payload.subject}" },
              "guard": four_at_once, "then": ["fan"] },
            { "id": "asked", "name": "Somebody asks", "kind": "start",
              "on": { "event": "message", "in": "general", "contains": "desk:" },
              "inputs": { "subject": "{event.payload.text}" },
              "guard": four_at_once, "then": ["fan"] },
            { "id": "fan", "name": "Both at once", "kind": "parallel",
              "then": ["draft", "tell"] },
            { "id": "draft", "name": "Draft", "kind": "agent",
              "instructions": "One line about {inputs.subject}.",
              "harness": [AGENT_HARNESS], "then": ["wrap"] },
            { "id": "tell", "name": "Tell", "kind": "notify",
              "template": "heard: {inputs.subject}", "then": ["wrap"] },
            { "id": "wrap", "name": "Wrap up", "kind": "end", "finish": "done" },
        ],
    })
}

/// A decision nobody makes in time, and where the run goes then.
fn review_with_a_timeout() -> Value {
    json!({
        "name": "Review",
        "steps": [
            { "id": "review", "name": "Ship it?", "kind": "approval", "prompt": "Ship it?",
              "boundaries": [
                  { "name": "late", "on": { "event": "after", "secs": 1 }, "act": "divert" },
              ],
              "then": ["ship", { "to": "escalate", "branch": "late" }] },
            { "id": "ship", "name": "Ship", "kind": "notify", "template": "shipped" },
            { "id": "escalate", "name": "Escalate", "kind": "notify",
              "template": "nobody decided in time" },
        ],
    })
}

fn recorded(ws: &Sealed, name: &str, definition: &Value) -> String {
    let file = ws.file(name, &definition.to_string());
    let made = ws.json(&["workflow", "new", "--from", &file.to_string_lossy()]);
    assert_eq!(made["problems"], json!([]), "{made}");
    made["workflow"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the workflow's id: {made}"))
        .to_string()
}

/// The workflow's runs of the workspace, oldest first.
fn runs_of(ws: &Sealed, workflow: &str) -> Vec<Value> {
    let mut runs = ws.json(&["workflow", "runs", workflow])["runs"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    runs.sort_by_key(|run| run["number"].as_u64());
    runs
}

/// Wait until the workflow has `count` runs and every one is over.
fn all_over(ws: &Sealed, workflow: &str, count: usize) -> Vec<Value> {
    ws.until(&format!("{count} runs of the workflow, all over"), || {
        let runs = runs_of(ws, workflow);
        (runs.len() == count && runs.iter().all(|run| run["status"] == "done")).then_some(runs)
    })
}

/// What was said in `general`, oldest first.
fn said_in_general(ws: &Sealed) -> Vec<String> {
    let mut said: Vec<(u64, String)> = ws.json(&["msgs", "general", "--limit", "50"])["messages"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|m| {
                    (
                        m["created_at"].as_u64().unwrap_or_default(),
                        m["content"].as_str().unwrap_or_default().to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    said.sort();
    said.into_iter().map(|(_, text)| text).collect()
}

#[test]
fn a_workflow_that_is_on_begins_one_run_for_each_thing_it_hears() {
    let mut ws = Sealed::with_script(&worker());
    let workflow = recorded(&ws, "desk.json", &desk());
    ws.start();
    let hook = format!("/workflows/{workflow}/hooks/ticket");
    let ticket = json!({ "subject": "the printer" });

    // Off: nothing is heard. The hook says so, and a signal is a quiet one.
    let (status, refused) = ws.call("POST", &hook, &[], Some(&ticket));
    assert_eq!(
        status, 409,
        "a workflow that is off takes no call: {refused}"
    );
    let quiet = ws.json(&[
        "signal",
        "emit",
        "report.ready",
        "--data",
        r#"{"subject":"too early"}"#,
    ]);
    assert_eq!(quiet["listeners"], json!([]), "{quiet}");
    assert_eq!(runs_of(&ws, &workflow), Vec::<Value>::new());

    // On: three events are armed, and the start by hand is none of them.
    ws.ok(&["workflow", "on", &workflow]);
    let listeners = ws.json(&["workflow", "listeners", &workflow]);
    let mut armed: Vec<&str> = listeners["listeners"]
        .as_array()
        .expect("the listeners")
        .iter()
        .filter_map(|l| l["event"].as_str())
        .collect();
    armed.sort_unstable();
    assert_eq!(armed, vec!["hook", "message", "signal"], "{listeners}");

    // A call to the hook, delivered twice under one key, is one occurrence.
    let once = [("idempotency-key", "ticket-1")];
    let (status, taken) = ws.call("POST", &hook, &once, Some(&ticket));
    assert_eq!(status, 202, "{taken}");
    let (status, again) = ws.call("POST", &hook, &once, Some(&ticket));
    assert_eq!(status, 202, "{again}");
    assert_eq!(
        again["signal"], taken["signal"],
        "a redelivery is the same signal"
    );
    // A step that is no hook of the workflow is not found; a caller with no
    // token is refused at the door.
    let (status, _) = ws.call(
        "POST",
        &format!("/workflows/{workflow}/hooks/ready"),
        &[],
        Some(&ticket),
    );
    assert_eq!(status, 404, "a signal start is no hook");
    let (status, _) = ws.call_with("POST", &hook, &[], Some(&ticket));
    assert_eq!(status, 401, "a local hook takes the control-plane token");
    all_over(&ws, &workflow, 1);

    // A named signal, raised by hand.
    let raised = ws.json(&[
        "signal",
        "emit",
        "report.ready",
        "--data",
        r#"{"subject":"the report"}"#,
    ]);
    assert_eq!(
        raised["listeners"].as_array().map(Vec::len),
        Some(1),
        "{raised}"
    );
    all_over(&ws, &workflow, 2);

    // A message a person posts from the command line: the node hears it.
    ws.ok(&["msg", "general", "desk: the stapler"]);
    // One the start does not ask for begins nothing.
    ws.ok(&["msg", "general", "nothing for the desk"]);
    let runs = all_over(&ws, &workflow, 3);

    // Each run says what began it, and was given what its event carried.
    let began: Vec<(String, String)> = runs
        .iter()
        .map(|run| {
            let id = run["id"].as_str().expect("a run's id");
            let subject = ws.json(&["status", id])["run"]["inputs"]["subject"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            (
                run["started_by"]["event"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                subject,
            )
        })
        .collect();
    assert_eq!(
        began,
        vec![
            ("hook".to_string(), "the printer".to_string()),
            ("signal".to_string(), "the report".to_string()),
            ("message".to_string(), "desk: the stapler".to_string()),
        ]
    );
    assert_eq!(runs[1]["started_by"]["detail"], "report.ready");

    // The gateway took both paths in every run: a worker drafted, the
    // platform told, and the end joined them.
    for run in &runs {
        let id = run["id"].as_str().expect("a run's id");
        let steps = ws.json(&["status", id])["run"]["steps"].clone();
        for step in ["fan", "draft", "tell", "wrap"] {
            assert_eq!(
                steps[step]["state"]["state"], "done",
                "{step} of {id}: {steps}"
            );
        }
        assert_eq!(steps["draft"]["output"], json!({ "noted": true }));
        for other in ["by_hand", "ticket", "ready", "asked"] {
            let entered = run["started_by"]["event"] == steps_event(other);
            let expected = if entered { "done" } else { "skipped" };
            assert_eq!(steps[other]["state"]["state"], expected, "{other} of {id}");
        }
    }
    let said = said_in_general(&ws);
    for heard in [
        "heard: the printer",
        "heard: the report",
        "heard: desk: the stapler",
    ] {
        assert_eq!(
            said.iter().filter(|text| text.as_str() == heard).count(),
            1,
            "{heard}, once: {said:?}"
        );
    }
    assert!(
        !said.iter().any(|text| text.contains("too early")),
        "what was raised while it was off began nothing: {said:?}"
    );
    // What a person says from the command line reaches the running engine
    // like what they say in the window: words that name nobody wake the
    // General Agent, and nobody else.
    let sessions = ws.until("the General Agent to be woken", || {
        let sessions = ws.recorded("session_new");
        sessions
            .iter()
            .any(|s| s["scope"] == "conversation")
            .then_some(sessions)
    });
    assert_eq!(
        sessions
            .iter()
            .filter(|s| s["scope"] == "work_item")
            .count(),
        3,
        "a worker a run: {sessions:#?}"
    );
    assert!(
        sessions
            .iter()
            .filter(|s| s["scope"] == "conversation")
            .all(|s| s["agent"] == "general-agent"),
        "{sessions:#?}"
    );

    // Off again: the hook is closed, and nothing is armed.
    ws.ok(&["workflow", "off", &workflow]);
    let (status, _) = ws.call("POST", &hook, &[], Some(&ticket));
    assert_eq!(status, 409);
    assert_eq!(
        ws.json(&["workflow", "listeners", &workflow])["listeners"],
        json!([])
    );
    ws.ok(&["msg", "general", "desk: anybody there?"]);
    ws.stop();
    assert_eq!(runs_of(&ws, &workflow).len(), 3, "and nothing more began");
}

/// The event a start of the desk begins on, by the step's id.
fn steps_event(step: &str) -> &'static str {
    match step {
        "ticket" => "hook",
        "ready" => "signal",
        "asked" => "message",
        _ => "manual",
    }
}

#[test]
fn a_signal_written_down_while_no_node_ran_begins_one_run_when_one_is_back() {
    let mut ws = Sealed::with_script(&worker());
    let workflow = recorded(&ws, "desk.json", &desk());
    ws.start();
    ws.ok(&["workflow", "on", &workflow]);
    ws.stop();

    // No node: the verb writes down what it is asked and leaves.
    let raised = ws.json(&[
        "signal",
        "emit",
        "report.ready",
        "--data",
        r#"{"subject":"while away"}"#,
    ]);
    let signal = raised["signal"].as_str().expect("the signal").to_string();
    assert_eq!(
        runs_of(&ws, &workflow),
        Vec::<Value>::new(),
        "nothing runs without a node"
    );

    // Back: what was written is heard — once, across a second restart too.
    ws.start();
    let runs = all_over(&ws, &workflow, 1);
    assert_eq!(runs[0]["started_by"]["event"], "signal");
    ws.stop();
    ws.start();
    ws.until("the queue to have settled", || {
        let listed = ws.json(&["signal", "list", "--limit", "20"]);
        let signals = listed["signals"].as_array()?;
        signals
            .iter()
            .all(|s| matches!(s["state"].as_str(), Some("done" | "skipped")))
            .then_some(())
    });
    assert_eq!(runs_of(&ws, &workflow).len(), 1, "one signal, one run");
    let listed = ws.json(&["signal", "list", "--limit", "20"]);
    assert!(
        listed["signals"]
            .as_array()
            .is_some_and(|all| all.iter().any(|s| s["id"] == signal.as_str())),
        "the signal raised by hand is kept: {listed}"
    );
    ws.stop();
}

#[test]
fn a_decision_nobody_makes_in_time_is_diverted_and_its_question_withdrawn() {
    let mut ws = Sealed::with_script(&worker());
    let workflow = recorded(&ws, "review.json", &review_with_a_timeout());
    ws.start();

    let started = ws.json(&["workflow", "run", &workflow]);
    let run = started["run"].as_str().expect("the run").to_string();
    // Nobody answers. The boundary fires on the node's own clock.
    let over = ws.until("the run to end by its boundary", || {
        let status = ws.json(&["status", &run]);
        (status["status"] == "done").then_some(status)
    });
    let steps = &over["run"]["steps"];
    assert_eq!(
        steps["review"]["state"],
        json!({ "state": "diverted", "by": "late" }),
        "{steps}"
    );
    assert_eq!(steps["escalate"]["state"]["state"], "done");
    assert_eq!(
        steps["ship"]["state"]["state"], "skipped",
        "the path not taken"
    );
    let said = said_in_general(&ws);
    assert!(
        said.contains(&"nobody decided in time".to_string()),
        "{said:?}"
    );
    assert!(!said.contains(&"shipped".to_string()), "{said:?}");

    // The question went with the step: nothing is owed, and an answer that
    // comes late is refused rather than taken.
    let inbox = ws.json(&["inbox"]);
    let owed = inbox["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["key"] == workflow.as_str())
        .flat_map(|row| row["needs_action"].as_array().cloned().unwrap_or_default())
        .count();
    assert_eq!(owed, 0, "{inbox}");
    let late = ws.bisa(&["approve", &run]);
    assert!(
        !late.status.success(),
        "a decision on a run that is over is refused"
    );
    ws.stop();
}
