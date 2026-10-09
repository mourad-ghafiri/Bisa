//! The workflow verbs beyond running one, through the daemon and with the
//! daemon gone: a definition validated, edited and removed; the library
//! listed by scope; a workflow's runs stopped together; a goal's design made
//! from a file and promoted to the library offline; a step released and a
//! question marked done by hand, offline; a decision asked of a goal that
//! owes none; and what each verb refuses when handed an id that is no id.

use super::sealed::Sealed;
use serde_json::{json, Value};

/// A workflow held until a person releases it.
fn held() -> Value {
    json!({
        "name": "Held",
        "description": "waits for a person",
        "steps": [{ "id": "hold", "name": "Hold", "kind": "wait", "until": { "until": "release" } }]
    })
}

/// A workflow that asks one question.
fn asks() -> Value {
    json!({
        "name": "Asks",
        "description": "one question",
        "steps": [{
            "id": "ask", "name": "Which?", "kind": "human", "prompt": "Which one?",
            "options": [{ "id": "a", "label": "A" }, { "id": "b", "label": "B" }]
        }]
    })
}

fn recorded(ws: &Sealed, name: &str, definition: &Value) -> (String, String) {
    let file = ws.file(name, &definition.to_string());
    let path = file.to_string_lossy().to_string();
    let made = ws.json(&["workflow", "new", "--from", &path]);
    let id = made["workflow"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the workflow's id: {made}"))
        .to_string();
    (id, path)
}

fn run_of(ws: &Sealed, workflow: &str) -> String {
    let started = ws.json(&["workflow", "run", workflow]);
    started["run"]
        .as_str()
        .unwrap_or_else(|| panic!("the run's id: {started}"))
        .to_string()
}

#[test]
fn the_workflow_verbs_through_the_daemon_and_with_it_gone() {
    let mut ws = Sealed::bare();
    ws.start();

    // Validated from a file, recorded, edited from another file.
    let (held_id, held_path) = recorded(&ws, "held.json", &held());
    let validated = ws.json(&["workflow", "validate", "--from", &held_path]);
    assert_eq!(validated["problems"], json!([]), "{validated}");
    let mut longer = held();
    longer["description"] = json!("waits for a person, patiently");
    let longer_path = ws
        .file("held-2.json", &longer.to_string())
        .to_string_lossy()
        .to_string();
    let saved = ws.json(&["workflow", "edit", &held_id, "--from", &longer_path]);
    assert_eq!(
        saved["workflow"]["description"], "waits for a person, patiently",
        "{saved}"
    );

    // The library by scope: the library alone, everything, one goal's.
    let goal = ws.json(&["new", "a shelf for the hall"])["goal"]
        .as_str()
        .expect("the goal")
        .to_string();
    let design = ws.json(&["workflow", "use", &goal, "--from", &held_path]);
    let design_id = design["workflow"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the design's id: {design}"))
        .to_string();
    let library = ws.json(&["workflow", "list"]);
    let ids = |v: &Value| -> Vec<String> {
        v["workflows"]
            .as_array()
            .unwrap_or_else(|| panic!("a list: {v}"))
            .iter()
            .filter_map(|w| w["workflow"]["id"].as_str().or(w["id"].as_str()))
            .map(str::to_string)
            .collect()
    };
    assert!(ids(&library).contains(&held_id), "{library}");
    assert!(
        !ids(&library).contains(&design_id),
        "a goal's design is not the library's"
    );
    let everything = ws.json(&["workflow", "list", "--all"]);
    assert!(ids(&everything).contains(&design_id), "{everything}");
    let the_goals = ws.json(&["workflow", "list", "--goal", &goal]);
    assert_eq!(ids(&the_goals), vec![design_id.clone()], "{the_goals}");

    // Two runs, stopped together by their workflow.
    let first = run_of(&ws, &held_id);
    let second = run_of(&ws, &held_id);
    let stopped = ws.json(&["workflow", "stop", &held_id]);
    let mut stopped_runs: Vec<String> = stopped["runs"]
        .as_array()
        .unwrap_or_else(|| panic!("the runs: {stopped}"))
        .iter()
        .filter_map(|r| r.as_str())
        .map(str::to_string)
        .collect();
    stopped_runs.sort();
    let mut both = vec![first.clone(), second.clone()];
    both.sort();
    assert_eq!(stopped_runs, both);

    // What is no id is refused by name, before anything is read.
    for refused in [
        ws.refused(&["workflow", "edit", "not-an-id", "--from", &held_path]),
        ws.refused(&["workflow", "rm", "not-an-id"]),
        ws.refused(&["workflow", "promote", "not-an-id"]),
        ws.refused(&["workflow", "stop", &held_id, "--run", "not-an-id"]),
        ws.refused(&["workflow", "restart", &held_id, "--run", "not-an-id"]),
    ] {
        assert!(refused.contains("not-an-id"), "{refused}");
    }
    // A run of another workflow is not this one's to stop.
    let (asks_id, _) = recorded(&ws, "asks.json", &asks());
    let asked = run_of(&ws, &asks_id);
    let not_its = ws.refused(&["workflow", "stop", &held_id, "--run", &asked]);
    assert!(not_its.contains(&asked), "{not_its}");

    // Removed through the daemon.
    let (spare_id, _) = recorded(&ws, "spare.json", &held());
    let removed = ws.json(&["workflow", "rm", &spare_id]);
    assert_eq!(removed["ok"], json!(true), "{removed}");
    assert!(!ids(&ws.json(&["workflow", "list"])).contains(&spare_id));

    // With the daemon gone: a step released by hand, a question marked done
    // by hand, a design promoted, and a decision nobody owes.
    let held_run = run_of(&ws, &held_id);
    ws.stop();
    let released = ws.json(&["step", "release", &held_run, "hold"]);
    assert_eq!(released["status"], "done", "{released}");
    assert_eq!(ws.json(&["status", &held_run])["run"]["outcome"], "done");
    let done = ws.json(&["step", "done", &asked, "ask"]);
    assert_eq!(done["status"], "done", "{done}");
    let promoted = ws.json(&["workflow", "promote", &design_id]);
    let promoted_id = promoted["workflow"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the promoted workflow: {promoted}"))
        .to_string();
    assert_ne!(promoted_id, design_id);
    let nothing_owed = ws.refused(&["approve", &goal]);
    assert!(nothing_owed.contains("no pending"), "{nothing_owed}");
}
