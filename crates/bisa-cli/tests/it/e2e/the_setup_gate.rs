//! The setup gate in the terminal: `bisa doctor` on a machine that has git
//! and nothing else says what is missing and exits 1; once a harness is
//! there and the three core agents are on it, it says all five are here and
//! exits 0. The platform installs nothing: the journey does what a person
//! does, and the verb only looks.

use super::sealed::{Sealed, AGENT_HARNESS};
use serde_json::{json, Value};

/// A harness that is there: it answers when the platform looks for it.
fn installed() -> Value {
    json!({ "turns": [] })
}

/// The checks of a readiness, by id: `(state, detail)`.
fn checks(readiness: &Value) -> Vec<(String, String)> {
    readiness["checks"]
        .as_array()
        .unwrap_or_else(|| panic!("the checks: {readiness}"))
        .iter()
        .map(|check| {
            (
                check["id"].as_str().unwrap_or_default().to_string(),
                check["state"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

fn state_of(readiness: &Value, id: &str) -> String {
    checks(readiness)
        .into_iter()
        .find(|(check, _)| check == id)
        .map(|(_, state)| state)
        .unwrap_or_else(|| panic!("no check `{id}`: {readiness}"))
}

/// `bisa doctor --json`: its exit code and what it printed.
fn doctor(ws: &Sealed) -> (Option<i32>, Value) {
    let out = ws.bisa(&["--json", "doctor"]);
    let printed = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "doctor did not print its readiness ({e}): {}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    });
    (out.status.code(), printed)
}

#[test]
fn doctor_says_what_is_missing_and_that_all_is_here_once_it_is() {
    let mut ws = Sealed::bare();

    // A machine with git and no harness: one check is ready.
    let (code, readiness) = doctor(&ws);
    assert_eq!(code, Some(1), "something is missing: {readiness}");
    assert_eq!(readiness["ready"], false);
    assert_eq!(
        checks(&readiness)
            .iter()
            .map(|(id, _)| id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "git",
            "harness",
            "decision_making_agent",
            "general_agent",
            "workflow_agent"
        ],
        "five checks, in the order the gate shows them"
    );
    assert_eq!(state_of(&readiness, "git"), "ready");
    assert_eq!(state_of(&readiness, "harness"), "missing");
    for unready in ["decision_making_agent", "general_agent", "workflow_agent"] {
        assert_eq!(state_of(&readiness, unready), "unready", "{unready}");
    }
    // What is shown is a line to copy and the page it came from: nothing
    // was installed, and nothing was run for the person.
    let harness = readiness["checks"]
        .as_array()
        .and_then(|all| all.iter().find(|c| c["id"] == "harness"))
        .cloned()
        .unwrap_or_default();
    assert!(
        harness["hint"]["url"]
            .as_str()
            .is_some_and(|url| url.starts_with("https://")),
        "{harness}"
    );
    assert!(
        harness["hint"]["commands"]
            .as_array()
            .is_some_and(|lines| !lines.is_empty()),
        "{harness}"
    );
    assert!(
        std::fs::read_dir(
            ws.data()
                .parent()
                .expect("the journey's folder")
                .join("agents")
        )
        .expect("the folder a harness would be in")
        .next()
        .is_none(),
        "the verb looked, and installed nothing"
    );
    // Said to a person too, a line a check.
    let said = ws.bisa(&["doctor"]);
    assert_eq!(said.status.code(), Some(1));
    let lines = String::from_utf8_lossy(&said.stdout).to_string();
    assert!(
        lines.contains('✓') && lines.contains('✗') && lines.contains('!'),
        "{lines}"
    );

    // A person installs a harness and puts the three agents on it.
    ws.install_agent(&installed());
    for agent in ["general-agent", "workflow-agent"] {
        ws.ok(&["agent", "edit", agent, "--harness", AGENT_HARNESS]);
    }
    for (key, value) in [
        ("decisions.provider", "harness"),
        ("decisions.harness.id", AGENT_HARNESS),
        ("decisions.harness.model", ""),
        ("decisions.enabled", "true"),
    ] {
        ws.ok(&["settings", "set", "workspace", key, value]);
    }
    let (code, readiness) = doctor(&ws);
    assert_eq!(
        checks(&readiness)
            .iter()
            .map(|(_, state)| state.as_str())
            .collect::<Vec<_>>(),
        vec!["ready"; 5],
        "{readiness}"
    );
    assert_eq!(readiness["ready"], true);
    assert_eq!(code, Some(0), "everything is here");

    // The node says the same as the embedded engine did.
    ws.start();
    let (code, through_the_node) = doctor(&ws);
    assert_eq!(code, Some(0));
    assert_eq!(checks(&through_the_node), checks(&readiness));
    let (status, asked) = ws.call("GET", "/readiness", &[], None);
    assert_eq!(status, 200);
    assert_eq!(checks(&asked), checks(&readiness));
    ws.stop();
}
