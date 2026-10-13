//! OpenCode 2 through the binary: found by its version, started as its ACP
//! server, and a goal's step run on it end to end — the scripted agent
//! installed as `opencode`, answering `--version` as OpenCode 2 does, so
//! the adapter chooses the wire OpenCode 2 is driven on.

use super::sealed::Sealed;
use serde_json::{json, Value};

/// OpenCode 2, as the scripted agent plays it: its version line, two models
/// with their levels, and one turn that yields.
fn opencode_two() -> Value {
    json!({
        "version": "opencode v2.0.26",
        "models": ["small", "large"],
        "model_efforts": {
            "small": ["low", "medium"],
            "large": ["low", "high", "max"],
        },
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

/// One step of work for `agent`.
fn one_step_for(ws: &Sealed, agent: &str) -> String {
    let definition = json!({
        "name": format!("One step for {agent}"),
        "steps": [{
            "id": "write",
            "name": "Write",
            "kind": "agent",
            "instructions": "Write it.",
            "assignee": { "agent": agent },
        }],
    });
    let file = ws.file(&format!("{agent}.json"), &definition.to_string());
    let made = ws.json(&["workflow", "new", "--from", &file.to_string_lossy()]);
    assert_eq!(made["problems"], json!([]), "{made}");
    made["workflow"]["id"].as_str().expect("its id").to_string()
}

#[test]
fn opencode_two_is_found_by_its_version_and_driven_through_its_acp_server() {
    let mut ws = Sealed::bare();
    ws.install_agent_as("opencode", &opencode_two());
    for agent in ["general-agent", "workflow-agent"] {
        ws.ok(&[
            "agent",
            "edit",
            agent,
            "--harness",
            "opencode",
            "--model",
            "large",
        ]);
    }

    // --- found: by the adapter's own probe, with OpenCode 2's version ---------
    let listed = ws.json(&["harness", "list"]);
    let row = listed["harnesses"]
        .as_array()
        .expect("the rows")
        .iter()
        .find(|row| row["id"] == "opencode")
        .cloned()
        .unwrap_or_else(|| panic!("no row for opencode: {listed}"));
    assert_eq!(row["probe"]["available"], true, "{row}");
    assert_eq!(row["probe"]["version"], "opencode v2.0.26", "{row}");
    assert_eq!(row["launch"]["program"], "opencode", "{row}");

    // --- driven: a step on it runs to its end through the ACP server ----------
    ws.start();
    ws.ok(&[
        "agent",
        "add",
        "--name",
        "Coder",
        "--harness",
        "opencode",
        "--model",
        "large",
        "--effort",
        "high",
        "--prompt",
        "You write what you are asked to.",
    ]);
    let workflow = one_step_for(&ws, "coder");
    let started = ws.json(&["workflow", "run", &workflow]);
    let run = started["run"].as_str().expect("the run");
    assert_eq!(
        started["status"],
        "done",
        "{}",
        ws.json(&["status", run])["run"]["steps"]
    );
    let starts = ws.recorded_by("opencode", "started");
    assert!(
        starts.iter().any(|s| s["args"] == json!(["acp"])),
        "started as its ACP server, with that one word: {starts:?}"
    );
    let set: Vec<(String, String)> = ws
        .recorded_by("opencode", "config")
        .iter()
        .map(|c| {
            (
                c["config_id"].as_str().unwrap_or_default().to_string(),
                c["value"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    assert!(
        set.contains(&("model".into(), "large".into())),
        "the model, set the protocol's way: {set:?}"
    );
    assert!(
        set.contains(&("effort".into(), "high".into())),
        "the level, held to what `large` offers: {set:?}"
    );
}
