//! How hard a model works, through the binary: the first that names a level
//! decides — the step, then the model, then the agent, then the setting,
//! `high` when nobody does — and the level is fitted to what the model about
//! to run takes. What is asserted is what the harness was set to, as it
//! records it: the level a real agent would have run at.

use super::sealed::{Sealed, AGENT_HARNESS};
use serde_json::{json, Value};

/// A worker that yields, on a harness that offers `levels`.
fn worker_offering(levels: &[&str]) -> Value {
    json!({
        "efforts": levels,
        "turns": [{
            "scope": "work_item",
            "tools": [{
                "name": "yield_result",
                "arguments": { "output": { "written": true } },
            }],
            "say": ["Written."],
        }],
    })
}

/// One step of work for the writer — at `effort` when the step names one.
fn one_step(effort: Option<&str>) -> Value {
    let mut step = json!({
        "id": "write",
        "name": "Write",
        "kind": "agent",
        "instructions": "Write it.",
        "assignee": { "agent": "writer" },
    });
    if let Some(level) = effort {
        step["effort"] = json!(level);
    }
    json!({ "name": format!("One step at {}", effort.unwrap_or("no level")), "steps": [step] })
}

fn recorded(ws: &Sealed, name: &str, definition: &Value) -> String {
    let file = ws.file(name, &definition.to_string());
    let made = ws.json(&["workflow", "new", "--from", &file.to_string_lossy()]);
    assert_eq!(made["problems"], json!([]), "{made}");
    made["workflow"]["id"].as_str().expect("its id").to_string()
}

/// The writer: an agent of the workspace on the scripted harness.
fn the_writer(ws: &Sealed) {
    ws.ok(&[
        "agent",
        "add",
        "--name",
        "Writer",
        "--harness",
        AGENT_HARNESS,
        "--prompt",
        "You write what you are asked to.",
    ]);
    let listed = ws.json(&["agent", "list"]);
    assert!(
        listed["agents"]
            .as_array()
            .is_some_and(|all| all.iter().any(|a| a["id"] == "writer")),
        "{listed}"
    );
}

/// Run the workflow to its end and answer the level its session was set to
/// — `None` when the harness was set to nothing.
fn level_of_a_run(ws: &Sealed, workflow: &str) -> Option<String> {
    let before = ws.recorded("session_new").len();
    let started = ws.json(&["workflow", "run", workflow]);
    let run = started["run"].as_str().expect("the run");
    assert_eq!(
        started["status"],
        "done",
        "{}",
        ws.json(&["status", run])["run"]["steps"]
    );
    let sessions = ws.recorded("session_new");
    assert_eq!(sessions.len(), before + 1, "one session a run");
    let session = sessions[before]["session"].clone();
    ws.recorded("config")
        .iter()
        .rev()
        .find(|set| set["session"] == session)
        .map(|set| {
            assert_eq!(set["config_id"], "effort", "{set}");
            set["value"].as_str().unwrap_or_default().to_string()
        })
}

#[test]
fn the_first_that_names_a_level_decides_how_hard_a_model_works() {
    let mut ws = Sealed::with_script(&worker_offering(&[
        "minimal", "low", "medium", "high", "xhigh", "max",
    ]));
    the_writer(&ws);
    let plain = recorded(&ws, "plain.json", &one_step(None));
    let pinned = recorded(&ws, "pinned.json", &one_step(Some("xhigh")));
    ws.start();

    // Nobody named one.
    assert_eq!(level_of_a_run(&ws, &plain).as_deref(), Some("high"));

    // The setting: everyone who says nothing.
    ws.ok(&["settings", "set", "workspace", "agents.effort", "low"]);
    assert_eq!(level_of_a_run(&ws, &plain).as_deref(), Some("low"));

    // The agent, whichever model of its plan runs.
    ws.ok(&["agent", "edit", "writer", "--effort", "max"]);
    assert_eq!(level_of_a_run(&ws, &plain).as_deref(), Some("max"));

    // One model of its plan.
    ws.ok(&["agent", "edit", "writer", "--model", "its-one-model@medium"]);
    assert_eq!(level_of_a_run(&ws, &plain).as_deref(), Some("medium"));

    // The step, over them all.
    assert_eq!(level_of_a_run(&ws, &pinned).as_deref(), Some("xhigh"));

    // Said nothing again, the agent leaves it to its model — and with the
    // model saying nothing too, to the setting.
    ws.ok(&["agent", "edit", "writer", "--effort", "inherit"]);
    assert_eq!(level_of_a_run(&ws, &plain).as_deref(), Some("medium"));
    ws.ok(&["agent", "edit", "writer", "--model", "its-one-model"]);
    assert_eq!(level_of_a_run(&ws, &plain).as_deref(), Some("low"));

    // An agent's card says what it was given, as it was typed.
    let shown = ws.json(&["agent", "show", "writer"]);
    assert_eq!(shown["agent"]["models"]["effort"], Value::Null, "{shown}");
    ws.stop();
}

#[test]
fn a_level_is_fitted_to_what_the_model_takes_and_nothing_is_sent_to_one_that_takes_none() {
    let mut ws = Sealed::with_script(&worker_offering(&["low", "medium", "high"]));
    the_writer(&ws);
    let most = recorded(&ws, "most.json", &one_step(Some("max")));
    let least = recorded(&ws, "least.json", &one_step(Some("minimal")));
    let middle = recorded(&ws, "middle.json", &one_step(Some("medium")));
    ws.start();

    assert_eq!(
        level_of_a_run(&ws, &most).as_deref(),
        Some("high"),
        "the nearest below"
    );
    assert_eq!(
        level_of_a_run(&ws, &least).as_deref(),
        Some("low"),
        "none below: the lowest above"
    );
    assert_eq!(
        level_of_a_run(&ws, &middle).as_deref(),
        Some("medium"),
        "the level itself"
    );
    ws.stop();

    // A harness that offers no level is set to nothing, whatever was asked.
    let mut none = Sealed::with_script(&worker_offering(&[]));
    the_writer(&none);
    let asked = recorded(&none, "most.json", &one_step(Some("max")));
    none.start();
    assert_eq!(level_of_a_run(&none, &asked), None);
    none.stop();
}
