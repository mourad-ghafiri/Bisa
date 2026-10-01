//! The daemon ends abruptly in the middle of a step — a crash, a power cut —
//! and starts again over the same workspace: the run is picked up where it
//! stood, on the item it already had, and comes to its end.

use super::sealed::{Sealed, AGENT_HARNESS};
use serde_json::{json, Value};

/// A worker that is still at work the first time it is prompted, and yields
/// the time after.
fn script() -> Value {
    json!({
        "turns": [
            { "scope": "work_item", "times": 1, "say": ["Working on it."], "hold": true },
            {
                "scope": "work_item",
                "tools": [{
                    "name": "yield_result",
                    "arguments": { "output": { "built": true } },
                }],
                "say": ["Built."],
            },
        ],
    })
}

fn workflow() -> Value {
    json!({
        "name": "Build it",
        "steps": [{
            "id": "build",
            "name": "Build",
            "kind": "agent",
            "instructions": "Build what the goal asks for.",
            "harness": [AGENT_HARNESS],
        }],
    })
}

#[test]
fn a_run_cut_off_in_the_middle_of_a_step_is_picked_up_when_the_daemon_starts_again() {
    let mut ws = Sealed::with_script(&script());
    let definition = ws.file("build.json", &workflow().to_string());
    let recorded = ws.json(&["workflow", "new", "--from", &definition.to_string_lossy()]);
    let workflow = recorded["workflow"]["id"]
        .as_str()
        .expect("the workflow")
        .to_string();

    ws.start();
    let captured = ws.json(&["new", "a thing to build", "--workflow", &workflow]);
    let goal = captured["goal"].as_str().expect("the goal").to_string();

    // The worker is in the middle of its turn, and the step is running.
    ws.until("the worker to be at work", || {
        (!ws.recorded("holding").is_empty()).then_some(())
    });
    let before = ws.json(&["status", &goal]);
    assert_eq!(before["run"]["steps"]["build"]["state"]["state"], "running");
    let run = before["run"]["id"].as_str().expect("the run").to_string();
    let item = before["run"]["steps"]["build"]["work_item"]
        .as_str()
        .expect("the step's work item")
        .to_string();

    // Cut off: no goodbye, and the worker goes with its daemon.
    ws.crash();
    ws.until("the worker to end with its daemon", || {
        (ws.recorded("ended").len() == ws.recorded("started").len()).then_some(())
    });

    // Started again over the same files: the run goes on, and is done.
    ws.start();
    let after = ws.until("the run to be done after the restart", || {
        let status = ws.json(&["status", &goal]);
        (status["run"]["outcome"] == "done").then_some(status)
    });
    assert_eq!(
        after["run"]["id"],
        run.as_str(),
        "the same run, not another"
    );
    let step = &after["run"]["steps"]["build"];
    assert_eq!(step["state"]["state"], "done");
    assert_eq!(step["output"], json!({ "built": true }));
    assert_eq!(
        step["work_item"],
        item.as_str(),
        "on the item it already had — the work is not started over on a new one"
    );
    assert_eq!(
        after["work_items"].as_array().map(Vec::len),
        Some(1),
        "and no second item was made: {}",
        after["work_items"]
    );

    // The goal's journal says what the restart found.
    let log = ws.json(&["log", &goal]);
    let said = log["events"].to_string();
    assert!(
        said.contains("a restart interrupted 1 running step"),
        "the interruption is written down: {said}"
    );
    ws.stop();
}
