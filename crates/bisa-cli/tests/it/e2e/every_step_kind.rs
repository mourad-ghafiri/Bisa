//! One workflow that enters every kind of step a run can take by itself —
//! seventeen of the eighteen; a `connector` step needs an outside platform
//! and is `connectors`' journey — run through the binary from its start by
//! hand to its end: the events (`start`, `wait`, `emit`, `end`), the gateways
//! (`decide`, `if`, `switch`, `judge`, `parallel`), the loops (`for_each`,
//! `while`) and the tasks (`agent`, `human`, `approval`, `check`, `notify`,
//! `spawn`). What is asserted is where every step stands at the end, what
//! each produced, and what the run left behind it: words in `general`, a
//! signal, a goal born of the run.

use super::sealed::{Sealed, AGENT_HARNESS};
use serde_json::{json, Value};

/// A worker that yields what the workflow's later steps read, and a judge
/// that is sure.
fn worker_and_judge() -> Value {
    json!({
        "turns": [
            {
                "scope": "work_item",
                "tools": [{
                    "name": "yield_result",
                    "arguments": { "output": {
                        "line": "tea is leaves and patience",
                        "kind": "drink",
                        "parts": ["leaves", "water"],
                    } },
                }],
                "say": ["Written."],
            },
            {
                "scope": "none",
                "say": [json!({ "answers": { "branch": {
                    "type": "choice",
                    "choice": "keep",
                    "probabilities": { "keep": 0.97, "drop": 0.03 },
                    "confidence": 0.94,
                } } }).to_string()],
            },
        ],
    })
}

/// What the run's `spawn` step captures a goal on.
fn child() -> Value {
    json!({
        "name": "A child's work",
        "steps": [
            { "id": "say", "name": "Say so", "kind": "notify", "template": "the child is done" },
        ],
    })
}

fn every_kind(child: &str) -> Value {
    json!({
        "name": "Every kind",
        "description": "one of each, from a start by hand to an end",
        "inputs": [{ "name": "topic", "label": "Topic", "kind": "text", "required": true }],
        "steps": [
            { "id": "begin", "name": "By hand", "kind": "start",
              "on": { "event": "manual" }, "then": ["draft"] },
            { "id": "draft", "name": "Draft", "kind": "agent",
              "instructions": "One line about {inputs.topic}, what kind of thing it is, and its parts.",
              "harness": [AGENT_HARNESS],
              "output_schema": { "type": "object", "required": ["line", "kind", "parts"] },
              "then": ["sound"] },
            { "id": "sound", "name": "Is there a line?", "kind": "check",
              "check": { "check": "command", "command": "test -n {steps.draft.output.line}" },
              "then": ["sized"] },
            { "id": "sized", "name": "About tea?", "kind": "if",
              "when": { "condition": "output_matches", "step": "draft", "path": "line", "contains": "tea" },
              "then": [{ "to": "kind_of", "branch": "yes" }, { "to": "wrong", "branch": "no" }] },
            { "id": "kind_of", "name": "What kind?", "kind": "switch",
              "on": "{steps.draft.output.kind}",
              "cases": [{ "value": "drink", "branch": "drink" }],
              "otherwise": "other",
              "then": [{ "to": "route", "branch": "drink" }, { "to": "wrong", "branch": "other" }] },
            { "id": "route", "name": "Which way?", "kind": "decide",
              "rules": [{ "when": { "condition": "input_equals", "input": "topic", "value": "tea" },
                          "branch": "tea" }],
              "otherwise": "plain",
              "then": [{ "to": "fan", "branch": "tea" }, { "to": "wrong", "branch": "plain" }] },
            { "id": "fan", "name": "Both at once", "kind": "parallel",
              "then": ["each", "ask"] },

            { "id": "each", "name": "For each part", "kind": "for_each",
              "items": "{steps.draft.output.parts}", "max_iterations": 5,
              "then": [{ "to": "say_part", "branch": "each" }, { "to": "again", "branch": "done" }] },
            { "id": "say_part", "name": "Say the part", "kind": "notify",
              "template": "part {steps.each.output.index}: {steps.each.output.item}",
              "then": ["each"] },
            { "id": "again", "name": "Until it ticked", "kind": "while",
              "when": { "condition": "not", "of": {
                  "condition": "output_equals", "step": "tick", "path": "signal", "value": "loop.ticked" } },
              "max_iterations": 3,
              "then": [{ "to": "tick", "branch": "loop" }, { "to": "told", "branch": "done" }] },
            { "id": "tick", "name": "Tick", "kind": "emit",
              "signal": "loop.ticked", "payload": { "about": "{inputs.topic}" },
              "then": ["again"] },

            { "id": "ask", "name": "Keep it?", "kind": "human", "prompt": "Keep the line about {inputs.topic}?",
              "options": [{ "id": "keep", "label": "Keep it" }, { "id": "drop", "label": "Drop it" }],
              "then": ["approve"] },
            { "id": "approve", "name": "Publish?", "kind": "approval", "prompt": "Publish it?",
              "then": ["hold"] },
            { "id": "hold", "name": "A moment", "kind": "wait",
              "until": { "until": "delay", "secs": 1 }, "then": ["child"] },
            { "id": "child", "name": "A goal of its own", "kind": "spawn",
              "statement_template": "A child of {inputs.topic}",
              "workflow": child, "wait": true, "then": ["verdict"] },
            { "id": "verdict", "name": "Worth keeping?", "kind": "judge",
              "state": "{steps.draft.output.line}",
              "instructions": "Is this line worth keeping?",
              "options": [
                  { "branch": "keep", "meaning": "it says something true in few words" },
                  { "branch": "drop", "meaning": "it says nothing" },
              ],
              "otherwise": "unsure", "min_confidence": 0.8,
              "then": [
                  { "to": "told", "branch": "keep" },
                  { "to": "wrong", "branch": "drop" },
                  { "to": "wrong", "branch": "unsure" },
              ] },

            { "id": "told", "name": "Say it all", "kind": "notify",
              "template": "all of it, about {inputs.topic}", "then": ["finish"] },
            { "id": "finish", "name": "Done", "kind": "end", "finish": "done" },
            { "id": "wrong", "name": "Not what was expected", "kind": "end", "finish": "failed" },
        ],
    })
}

fn recorded(ws: &Sealed, name: &str, definition: &Value) -> String {
    let file = ws.file(name, &definition.to_string());
    let made = ws.json(&["workflow", "new", "--from", &file.to_string_lossy()]);
    assert_eq!(
        made["problems"],
        json!([]),
        "{}: {made}",
        definition["name"]
    );
    made["workflow"]["id"].as_str().expect("its id").to_string()
}

fn said(ws: &Sealed, scope: &str) -> Vec<String> {
    let mut all: Vec<(u64, String)> = ws.json(&["msgs", scope, "--limit", "50"])["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|m| {
            (
                m["created_at"].as_u64().unwrap_or_default(),
                m["content"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    all.sort();
    all.into_iter().map(|(_, text)| text).collect()
}

#[test]
fn a_run_enters_every_kind_of_step_and_each_does_what_it_is_for() {
    let mut ws = Sealed::with_script(&worker_and_judge());
    for (key, value) in [
        ("decisions.harness.id", AGENT_HARNESS),
        ("decisions.harness.model", ""),
    ] {
        ws.ok(&["settings", "set", "machine", key, value]);
    }
    let child = recorded(&ws, "child.json", &child());
    let workflow = recorded(&ws, "every.json", &every_kind(&child));
    // The definition holds seventeen kinds, each once or more.
    let shown = ws.json(&["workflow", "show", &workflow]);
    let mut kinds: Vec<&str> = shown["workflow"]["steps"]
        .as_array()
        .expect("its steps")
        .iter()
        .filter_map(|step| step["kind"].as_str())
        .collect();
    kinds.sort_unstable();
    kinds.dedup();
    assert_eq!(
        kinds,
        vec![
            "agent", "approval", "check", "decide", "emit", "end", "for_each", "human", "if",
            "judge", "notify", "parallel", "spawn", "start", "switch", "wait", "while",
        ]
    );
    ws.start();

    // By hand, to the first thing a person owes it.
    let started = ws.json(&["workflow", "run", &workflow, "--input", "topic=tea"]);
    assert_eq!(started["status"], "waiting", "{started}");
    let run = started["run"].as_str().expect("the run").to_string();
    let at_the_question = ws.json(&["status", &run])["run"]["steps"].clone();
    // One path of the fan-out ran to its end without anybody; the other waits.
    for (step, state) in [
        ("begin", "done"),
        ("draft", "done"),
        ("sound", "done"),
        ("fan", "done"),
        ("each", "done"),
        ("say_part", "done"),
        ("again", "done"),
        ("tick", "done"),
        ("ask", "waiting"),
        ("told", "pending"),
    ] {
        assert_eq!(
            at_the_question[step]["state"]["state"], state,
            "{step}: {at_the_question}"
        );
    }
    assert_eq!(
        at_the_question["sized"]["state"]["branches"],
        json!(["yes"])
    );
    assert_eq!(
        at_the_question["kind_of"]["state"]["branches"],
        json!(["drink"])
    );
    assert_eq!(
        at_the_question["kind_of"]["output"],
        json!({ "value": "drink" })
    );
    assert_eq!(
        at_the_question["route"]["state"]["branches"],
        json!(["tea"])
    );
    assert_eq!(
        at_the_question["each"]["state"]["branches"],
        json!(["done"])
    );
    assert_eq!(
        at_the_question["each"]["output"],
        json!({ "index": 2, "count": 2 }),
        "both parts were taken"
    );
    assert_eq!(
        at_the_question["each"]["visits"], 1,
        "iterations are no visits"
    );
    assert_eq!(
        at_the_question["again"]["state"]["branches"],
        json!(["done"])
    );
    assert_eq!(
        at_the_question["again"]["output"],
        json!({ "index": 1 }),
        "it looped once"
    );
    assert_eq!(at_the_question["tick"]["output"]["signal"], "loop.ticked");

    // A person answers, then approves; the wait, the child and the judge
    // follow by themselves.
    let answered = ws.json(&["step", "answer", &run, "ask", "-o", "keep"]);
    assert_eq!(
        answered["status"], "waiting",
        "the approval is next: {answered}"
    );
    ws.ok(&["approve", &run]);
    let done = ws.until("the run to come to its end", || {
        let status = ws.json(&["status", &run]);
        (status["status"] == "done").then_some(status)
    });
    let steps = &done["run"]["steps"];
    for (id, record) in steps.as_object().expect("the steps") {
        let expected = if id == "wrong" { "skipped" } else { "done" };
        assert_eq!(record["state"]["state"], expected, "{id}: {record}");
        assert_eq!(record.get("error"), None, "{id}: {record}");
    }
    assert_eq!(steps["ask"]["answer"]["selected"], json!(["keep"]));
    assert!(
        steps["approve"]["gate"].is_string(),
        "the decision that passed it"
    );
    assert_eq!(steps["verdict"]["state"]["branches"], json!(["keep"]));
    assert_eq!(steps["verdict"]["output"]["choice"], "keep");
    assert_eq!(steps["verdict"]["output"]["judged"], true);

    // What it left behind: its words, in order and once each…
    let general = said(&ws, "general");
    assert_eq!(
        general,
        vec![
            "part 0: leaves".to_string(),
            "part 1: water".to_string(),
            "all of it, about tea".to_string(),
        ]
    );
    // …its signal…
    let signals = ws.json(&["signal", "list", "--limit", "20"]);
    assert_eq!(
        signals["signals"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|s| s["name"] == "loop.ticked")
            .count(),
        1,
        "{signals}"
    );
    // …and a goal born of the run, which ran the workflow it was given.
    let goals = ws.json(&["search"]);
    let born = goals["matches"]
        .as_array()
        .and_then(|all| all.iter().find(|g| g["statement"] == "A child of tea"))
        .unwrap_or_else(|| panic!("the goal the run captured: {goals}"));
    let child_goal = born["goal"].as_str().expect("its id");
    let its = ws.json(&["status", child_goal]);
    assert_eq!(
        its["goal"]["origin"],
        json!({ "origin": "run", "run": run, "step": "child" }),
        "it says which run it was born of, and at which step: {its}"
    );
    assert_eq!(its["run"]["outcome"], "done");
    assert_eq!(said(&ws, child_goal), vec!["the child is done".to_string()]);
    assert_eq!(
        steps["child"]["output"],
        json!({ "child": child_goal, "outcome": "done" }),
        "the step that waited says what its child came to"
    );

    // One worker and one judge were asked, and both have ended.
    ws.until("every session to end", || {
        (ws.recorded("ended").len() == ws.recorded("started").len()).then_some(())
    });
    let scopes: Vec<String> = ws
        .recorded("session_new")
        .iter()
        .filter_map(|s| s["scope"].as_str().map(str::to_string))
        .collect();
    assert_eq!(scopes, vec!["work_item".to_string(), "none".to_string()]);
    ws.stop();

    // With no node, the run reads the same from its files.
    let kept = ws.json(&["status", &run]);
    assert_eq!(kept["run"]["steps"], *steps);
}
