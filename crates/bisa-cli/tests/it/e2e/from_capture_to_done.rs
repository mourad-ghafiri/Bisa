//! A goal, from the sentence a person typed to the result on disk: captured,
//! designed by the Workflow Agent, adopted by the person, run by a worker,
//! done — and read back once the daemon is gone.

use super::sealed::{Sealed, AGENT_HARNESS};
use serde_json::{json, Value};

/// What the two agents do: the designer proposes a workflow of one step, the
/// worker yields that step's result.
fn script() -> Value {
    json!({
        "turns": [
            {
                "scope": "goal",
                "tools": [{
                    "name": "propose_workflow",
                    "arguments": { "workflow": {
                        "name": "One line",
                        "description": "write the line, then done",
                        "steps": [{
                            "id": "write",
                            "name": "Write the line",
                            "kind": "agent",
                            "instructions": "Put it in one line: {goal.statement}",
                            "harness": [AGENT_HARNESS],
                        }],
                    } },
                }],
                "say": ["I proposed a workflow of one step."],
            },
            {
                "scope": "work_item",
                "tools": [{
                    "name": "yield_result",
                    "arguments": { "output": { "line": "tea is leaves and patience" } },
                }],
                "say": ["Written."],
            },
        ],
    })
}

/// The inbox row of `goal`, when it has one.
fn inbox_row(ws: &Sealed, goal: &str) -> Option<Value> {
    ws.json(&["inbox"])["rows"]
        .as_array()?
        .iter()
        .find(|row| row["key"] == goal)
        .cloned()
}

/// A snapshot on disk, as the store wrote it: the signed event's content.
fn snapshot(path: &std::path::Path) -> Value {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("no snapshot at {}: {e}", path.display()));
    let event: Value = serde_json::from_str(&text).expect("a signed event");
    serde_json::from_str(event["content"].as_str().expect("its content")).expect("the object")
}

#[test]
fn a_goal_is_captured_designed_adopted_run_and_read_back_from_disk() {
    let mut ws = Sealed::with_script(&script());
    ws.start();

    // Captured: a sentence, and the Workflow Agent is woken to design it.
    let captured = ws.json(&["new", "what tea is", "--mode", "guided"]);
    let goal = captured["goal"]
        .as_str()
        .expect("the goal's id")
        .to_string();
    assert_eq!(captured["mode"], "guided");
    assert_eq!(
        captured["run"],
        Value::Null,
        "nothing runs before a person adopts"
    );

    // Designed: the proposal waits on the person, in the inbox.
    let asked = ws.until("the Adopt gate in the inbox", || {
        inbox_row(&ws, &goal)?["needs_action"]
            .as_array()?
            .iter()
            .find(|ask| {
                ask["subject"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("adopt:"))
            })
            .cloned()
    });
    assert_eq!(asked["gate_kind"], "approval");
    assert_eq!(asked["proposal"]["name"], "One line");
    assert_eq!(asked["proposal"]["steps"][0]["id"], "write");

    // What the designer was handed: the platform's own server for this goal,
    // as the Workflow Agent, in the goal's own folder — and no token.
    let designer = &ws.recorded("session_new")[0];
    assert_eq!(designer["scope"], "goal");
    assert_eq!(designer["agent"], "workflow-agent");
    let server = &designer["servers"][0];
    assert_eq!(server["name"], "bisa");
    let args: Vec<&str> = server["args"]
        .as_array()
        .expect("arguments")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(args[0], "mcp");
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--goal", goal.as_str()]),
        "{args:?}"
    );
    let scratch = ws.data().join("goals").join(&goal).join("scratch");
    assert_eq!(
        std::fs::canonicalize(designer["cwd"].as_str().expect("where it ran")).ok(),
        std::fs::canonicalize(&scratch).ok(),
        "the design session stands in the goal's scratch folder"
    );
    let handed = designer.to_string();
    assert!(
        !handed.contains("BISA_API_TOKEN") && !handed.contains("BISA_SESSION_SECRET"),
        "no credential of the control plane reaches a harness: {handed}"
    );

    // Adopted: the person's decision starts the run.
    let adopted = ws.json(&["approve", &goal]);
    assert_eq!(adopted["approve"], true);

    // Run, and done: the worker yielded, the step took its result.
    let status = ws.until("the run to be done", || {
        let status = ws.json(&["status", &goal]);
        (status["run"]["outcome"] == "done").then_some(status)
    });
    assert_eq!(status["status"], "done");
    let run = status["run"]["id"]
        .as_str()
        .expect("the run's id")
        .to_string();
    let step = &status["run"]["steps"]["write"];
    assert_eq!(step["state"]["state"], "done");
    assert_eq!(
        step["output"],
        json!({ "line": "tea is leaves and patience" })
    );
    let item = status["work_items"][0]["id"]
        .as_str()
        .expect("the work item")
        .to_string();
    assert_eq!(status["work_items"][0]["state"]["state"], "accepted");

    // The worker was told what the step says, with the goal's words in it.
    let prompts = ws.recorded("prompt");
    let told = prompts
        .iter()
        .find(|p| p["scope"] == "work_item")
        .expect("the worker's prompt");
    assert!(
        told["text"]
            .as_str()
            .is_some_and(|t| t.starts_with("Put it in one line: what tea is")),
        "{told}"
    );
    // Every session the daemon started has ended: no harness is left running.
    ws.until("both sessions to end", || {
        (ws.recorded("ended").len() == ws.recorded("started").len()).then_some(())
    });
    assert_eq!(ws.recorded("started").len(), 2, "a designer and a worker");

    // The daemon is asked to stop, says goodbye and leaves no socket.
    ws.stop();

    // Read back with no daemon: the verbs answer from the files alone.
    let after = ws.json(&["status", &goal]);
    assert_eq!(after["status"], "done");
    assert_eq!(after["run"]["steps"]["write"]["output"], step["output"]);

    // And the files are the truth: the goal, its design, its run, its item.
    let state = ws.data().join("goals").join(&goal).join("state");
    let on_disk = snapshot(&state.join(format!("33413-{run}.json")));
    assert_eq!(on_disk["outcome"], "done");
    assert_eq!(on_disk["scope"], json!({ "scope": "goal", "goal": goal }));
    assert_eq!(on_disk["steps"]["write"]["work_item"], item.as_str());
    assert_eq!(
        snapshot(&state.join(format!("33400-{goal}.json")))["statement"],
        "what tea is"
    );
    assert_eq!(
        snapshot(&state.join(format!("33402-{item}.json")))["state"]["state"],
        "accepted"
    );
    let journal =
        std::fs::read_to_string(ws.data().join("goals").join(&goal).join("journal.jsonl"))
            .expect("the goal's journal");
    assert!(
        journal.lines().count() >= 4,
        "what happened is written down, a fact a line:\n{journal}"
    );
}
