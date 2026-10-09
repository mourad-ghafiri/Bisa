//! Listening and signals from the command line, beyond one workflow's: every
//! listener of the workspace at once, a hook start that was minted no
//! secret, and a signal raised on a goal rather than in the workspace.

use super::sealed::Sealed;
use serde_json::{json, Value};

/// A workflow called from this machine, held open.
fn called() -> Value {
    json!({
        "name": "Called",
        "description": "a call holds it open",
        "steps": [
            { "id": "ticket", "name": "A ticket arrives", "kind": "start", "on": { "event": "hook" }, "then": ["hold"] },
            { "id": "hold", "name": "Hold", "kind": "wait", "until": { "until": "release" } }
        ]
    })
}

/// A workflow that hears a signal on its goal.
fn hears() -> Value {
    json!({
        "name": "Hears",
        "description": "a signal begins it",
        "steps": [
            { "id": "ready", "name": "Ready", "kind": "start", "on": { "event": "signal", "name": "report.ready" }, "then": ["hold"] },
            { "id": "hold", "name": "Hold", "kind": "wait", "until": { "until": "release" } }
        ]
    })
}

fn recorded(ws: &Sealed, name: &str, definition: &Value) -> String {
    let file = ws.file(name, &definition.to_string());
    let made = ws.json(&["workflow", "new", "--from", &file.to_string_lossy()]);
    made["workflow"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the workflow's id: {made}"))
        .to_string()
}

#[test]
fn every_listener_at_once_a_hook_without_a_secret_and_a_signal_on_a_goal() {
    let mut ws = Sealed::bare();
    let called = recorded(&ws, "called.json", &called());
    let hears = recorded(&ws, "hears.json", &hears());
    ws.start();
    ws.ok(&["workflow", "on", &called]);
    let goal = ws.json(&["new", "a report to hear", "--workflow", &hears])["goal"]
        .as_str()
        .expect("the goal")
        .to_string();
    // A goal whose workflow begins on an event alone is turned on by the
    // run verb: there is no run to make, only listening to begin.
    ws.ok(&["run", &goal]);

    // Every listener of the workspace, the library's and the goal's.
    let all = ws.json(&["workflow", "listeners"]);
    let hosts: Vec<String> = all["listeners"]
        .as_array()
        .unwrap_or_else(|| panic!("the listeners: {all}"))
        .iter()
        .filter_map(|l| l["host"].as_str().or(l["listener"].as_str()))
        .map(str::to_string)
        .collect();
    assert!(hosts.iter().any(|h| h.contains(&called)), "{all}");
    assert!(hosts.iter().any(|h| h.contains(&goal)), "{all}");

    // A hook start called from this machine alone was minted no secret.
    let secret = ws.ok(&["workflow", "hook-secret", "workflow", &called, "ticket"]);
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&secret.stdout),
        String::from_utf8_lossy(&secret.stderr)
    );
    assert!(
        said.to_lowercase().contains("no secret") || said.to_lowercase().contains("none"),
        "{said}"
    );

    // A signal raised on the goal begins its run.
    let raised = ws.json(&["signal", "emit", "report.ready", "--goal", &goal]);
    assert!(
        raised.get("signal").is_some() || raised.get("id").is_some(),
        "{raised}"
    );
    ws.until("the goal's run to begin", || {
        (ws.json(&["status", &goal])["status"] == "waiting").then_some(())
    });
    ws.stop();
}
